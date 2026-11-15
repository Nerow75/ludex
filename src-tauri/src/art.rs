//! Jaquettes des jeux qui n'en ont pas (EA, Battle.net, Autres, Riot…) : le jeu est cherché par son
//! titre dans la recherche publique de la boutique Steam (aucun compte), puis ses images viennent du
//! CDN Steam, comme pour les jeux Steam. Les jeux récents n'ont plus leurs images à l'adresse
//! classique du CDN : leurs chemins (hachés) sont lus dans l'API publique `IStoreBrowseService`. La correspondance doit être stricte : mieux vaut une
//! jaquette générée qu'une image d'un autre jeu.
//!
//! Les réponses sont gardées dans `art-cache.json` (dossier de données de l'app) : un titre trouvé
//! n'est jamais recherché de nouveau, un titre introuvable l'est au bout de 30 jours.

use crate::library::{Game, Platform};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const RETRY_AFTER: u64 = 30 * 24 * 3600;
/// À augmenter quand les règles de correspondance ou le contenu du cache changent : tous les
/// titres sont alors cherchés de nouveau.
const MATCHING_VERSION: u32 = 3;
const SEARCH: &str = "https://store.steampowered.com/api/storesearch/";
const CDN: &str = "https://cdn.cloudflare.steamstatic.com/steam/apps";
const ITEMS: &str = "https://api.steampowered.com/IStoreBrowseService/GetItems/v1/";
const ASSETS: &str = "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps";

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Entry {
    /// Appid Steam, `None` si le jeu n'y est pas.
    appid: Option<u32>,
    /// Date de la recherche (secondes Unix).
    checked: u64,
    /// Chemins des images dans `store_item_assets` (« <hash>/library_capsule_2x.jpg »), quand l'API
    /// les donne ; sinon l'adresse classique du CDN.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cover: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hero: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ArtCache {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    titles: BTreeMap<String, Entry>,
    #[serde(skip)]
    path: PathBuf,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Titre comparable : minuscules, lettres et chiffres seulement (« The Sims™ 4 » → « thesims4 »).
fn compact(title: &str) -> String {
    title.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Mentions d'édition retirées avant de comparer (« … Complete Edition »).
const EDITIONS: &[&str] = &[
    "gameoftheyearedition", "goty", "completeedition", "definitiveedition", "deluxeedition", "goldedition",
    "standardedition", "ultimateedition", "remastered", "directorscut", "nextstop",
];

fn without_edition(name: &str) -> &str {
    EDITIONS.iter().find_map(|e| name.strip_suffix(e)).unwrap_or(name)
}

fn digits(s: &str) -> String {
    s.chars().filter(char::is_ascii_digit).collect()
}

fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + usize::from(ca != cb)).min(row[j] + 1).min(row[j + 1] + 1);
            prev = cur;
        }
    }
    row[b.len()]
}

/// Le nom trouvé sur Steam est-il bien ce jeu ?
/// - égalité (aux symboles et à la casse près, mention d'édition retirée) ;
/// - nom Steam qui finit par le titre (« EA SPORTS™ FIFA 21 » pour « FIFA 21 ») ;
/// - faute de frappe (2 lettres au plus, titre long) si les nombres sont identiques : « Football
///   Manager 2022 » ne doit jamais prendre l'image de 2023.
pub fn same_game(title: &str, steam_name: &str) -> bool {
    let (t, s) = (compact(title), compact(steam_name));
    let s = without_edition(&s);
    if t.len() < 3 {
        return false;
    }
    t == s || (t.len() >= 6 && s.ends_with(&t)) || (t.len() >= 10 && digits(&t) == digits(s) && distance(&t, s) <= 2)
}

/// Jeux retirés de la vente sur Steam : la recherche ne les trouve plus, mais le CDN garde leurs images.
const DELISTED: &[(&str, u32)] = &[
    ("EA SPORTS FC 24", 2195250),
    ("EA SPORTS FC 25", 2669320),
    ("FIFA 23", 1811260),
    ("FIFA 22", 1506830),
    ("FIFA 21", 1313860),
    ("Football Manager 2024", 2252570),
    ("Football Manager 2023", 1904540),
    ("Football Manager 2022", 1569040),
    ("Football Manager 2021", 1263850),
    ("Assassin's Creed Unity", 289650),
    ("Crusader Kings III", 1158310),
];

fn delisted(title: &str) -> Option<u32> {
    DELISTED.iter().find(|(name, _)| same_game(title, name)).map(|&(_, appid)| appid)
}

#[derive(Deserialize)]
struct SearchResult {
    #[serde(default)]
    items: Vec<SearchItem>,
}

#[derive(Deserialize)]
struct SearchItem {
    #[serde(default)]
    r#type: String,
    name: String,
    id: u32,
}

/// Recherche publique de la boutique Steam (sans compte). `Ok(None)` : le jeu n'y est pas.
fn search(agent: &ureq::Agent, title: &str) -> Result<Option<u32>, String> {
    if let Some(appid) = delisted(title) {
        return Ok(Some(appid));
    }
    let result: SearchResult = agent
        .get(SEARCH)
        .query("term", title)
        .query("l", "french")
        .query("cc", "FR")
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())
        .and_then(|body| serde_json::from_str(&body).map_err(|e| e.to_string()))?;
    Ok(result.items.into_iter().find(|i| i.r#type == "app" && same_game(title, &i.name)).map(|i| i.id))
}

/// Chemins des images d'une appli (jaquette, image de fond) via l'API publique de la boutique.
fn assets(agent: &ureq::Agent, appid: u32) -> Result<(Option<String>, Option<String>), String> {
    let input = format!(r#"{{"ids":[{{"appid":{appid}}}],"context":{{"language":"french","country_code":"FR"}},"data_request":{{"include_assets":true}}}}"#);
    let body: serde_json::Value = agent
        .get(ITEMS)
        .query("input_json", &input)
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())
        .and_then(|b| serde_json::from_str(&b).map_err(|e| e.to_string()))?;
    let assets = &body["response"]["store_items"][0]["assets"];
    // Nom de fichier simple ou « <hash>/<fichier> » : jamais de « .. » ni d'adresse complète.
    let file = |key: &str| {
        assets[key]
            .as_str()
            .filter(|f| !f.is_empty() && !f.contains("..") && f.chars().all(|c| c.is_ascii_alphanumeric() || "/_-.".contains(c)))
            .map(String::from)
    };
    Ok((file("library_capsule_2x").or_else(|| file("library_capsule")), file("library_hero")))
}

/// Jeux à habiller : sans jaquette, hors Steam (dont les images viennent déjà de Steam).
pub fn needs_art(game: &Game) -> bool {
    game.cover_url.is_none() && game.platform != Platform::Steam
}

/// Images du CDN Steam, et appid retenu pour la fiche du jeu (`metadata.rs`).
fn set_urls(game: &mut Game, appid: u32, cover: Option<&str>, hero: Option<&str>) {
    game.steam_appid.get_or_insert(appid);
    let cdn = |file: &str| format!("{CDN}/{appid}/{file}");
    let asset = |file: &str| format!("{ASSETS}/{appid}/{file}");
    game.cover_url = Some(cover.map_or_else(|| cdn("library_600x900_2x.jpg"), asset));
    if game.hero_url.is_none() {
        game.hero_url = Some(hero.map_or_else(|| cdn("library_hero.jpg"), asset));
    }
    if game.logo_url.is_none() {
        game.logo_url = Some(cdn("logo.png"));
    }
}

impl ArtCache {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("art-cache.json");
        let mut cache: ArtCache = fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        if cache.version != MATCHING_VERSION {
            cache.titles.clear();
            cache.version = MATCHING_VERSION;
        }
        cache.path = path;
        cache
    }

    fn save(&self) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }

    /// Reporte les jaquettes déjà connues. Renvoie vrai si le jeu a reçu une jaquette.
    pub fn apply(&self, game: &mut Game) -> bool {
        if !needs_art(game) {
            return false;
        }
        if let Some(appid) = delisted(&game.title) {
            set_urls(game, appid, None, None);
            return true;
        }
        match self.titles.get(&compact(&game.title)) {
            Some(Entry { appid: Some(appid), cover, hero, .. }) => {
                set_urls(game, *appid, cover.as_deref(), hero.as_deref());
                true
            }
            _ => false,
        }
    }

    /// Titres jamais cherchés, ou introuvables depuis plus de 30 jours.
    pub fn to_search(&self, games: &[Game]) -> Vec<String> {
        let mut titles: Vec<String> = games
            .iter()
            .filter(|g| needs_art(g) && delisted(&g.title).is_none())
            .filter(|g| match self.titles.get(&compact(&g.title)) {
                None => true,
                Some(e) => e.appid.is_none() && now().saturating_sub(e.checked) > RETRY_AFTER,
            })
            .map(|g| g.title.clone())
            .collect();
        titles.sort();
        titles.dedup();
        titles
    }

    /// Cherche les titres un par un (pause entre deux requêtes) et enregistre au fur et à mesure.
    /// `on_found` reçoit chaque titre trouvé. Une erreur réseau arrête la recherche sans rien
    /// enregistrer pour les titres restants : ils seront cherchés au prochain scan.
    pub fn resolve(&mut self, titles: Vec<String>, mut on_found: impl FnMut(&str)) {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(8))
            .user_agent("Ludex (lanceur de jeux)")
            .build();
        for (i, title) in titles.iter().enumerate() {
            if i > 0 {
                std::thread::sleep(Duration::from_millis(350));
            }
            match search(&agent, title) {
                Ok(appid) => {
                    // Sans réponse de l'API des images : adresse classique du CDN.
                    let (cover, hero) = match appid {
                        Some(id) if delisted(title).is_none() => {
                            std::thread::sleep(Duration::from_millis(200));
                            assets(&agent, id).unwrap_or_default()
                        }
                        _ => (None, None),
                    };
                    self.titles.insert(compact(title), Entry { appid, checked: now(), cover, hero });
                    if let Err(e) = self.save() {
                        eprintln!("Cache des jaquettes non enregistré : {e}");
                    }
                    if appid.is_some() {
                        on_found(title);
                    }
                }
                Err(e) => {
                    eprintln!("Recherche de jaquettes interrompue : {e}");
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Launch;

    #[test]
    fn matches_only_the_same_game() {
        assert!(same_game("The Sims™ 4", "The Sims™ 4"));
        assert!(same_game("EA SPORTS FC 24", "EA SPORTS FC™ 24"));
        assert!(same_game("FIFA 21", "EA SPORTS™ FIFA 21"));
        assert!(same_game("Aassassins Creed Unity", "Assassin's Creed® Unity"));
        assert!(same_game("Bus Simulator 21", "Bus Simulator 21 Next Stop"));
        assert!(same_game("Sid Meier's Civilization VI", "Sid Meier’s Civilization® VI"));
        assert!(!same_game("Minecraft", "Minecraft Dungeons"));
        assert!(!same_game("Football Manager 2022", "Football Manager 2023"));
        assert!(!same_game("Crusader King", "Crusader Kings III"));
        assert!(!same_game("Dofus 3", "DOFUS"));
        assert!(!same_game("GO", "Rocket League GO"));
        assert!(DELISTED.iter().any(|(n, _)| same_game("Aassassins Creed Unity", n)));
    }

    #[test]
    fn applies_cached_art_and_lists_titles_to_search() {
        let mut cache = ArtCache::default();
        cache.titles.insert("frostpunk".into(), Entry { appid: Some(323190), checked: now(), cover: None, hero: None });
        cache.titles.insert("genshinimpact".into(), Entry { appid: None, checked: now(), cover: None, hero: None });
        cache.titles.insert("roblox".into(), Entry { appid: None, checked: 0, cover: None, hero: None });

        let game = |title: &str| Game::new(Platform::Other, title, title, Launch::Uri(String::new()));
        let mut frostpunk = game("Frostpunk");
        assert!(cache.apply(&mut frostpunk));
        assert!(frostpunk.cover_url.unwrap().ends_with("/323190/library_600x900_2x.jpg"));
        assert_eq!(frostpunk.steam_appid, Some(323190));
        cache.titles.insert(
            "escapefromtarkov".into(),
            Entry { appid: Some(3932890), checked: now(), cover: Some("df4e/library_capsule_2x.jpg".into()), hero: None },
        );
        let mut tarkov = game("Escape From Tarkov");
        assert!(cache.apply(&mut tarkov));
        assert_eq!(tarkov.cover_url.unwrap(), format!("{ASSETS}/3932890/df4e/library_capsule_2x.jpg"));
        assert_eq!(tarkov.hero_url.unwrap(), format!("{CDN}/3932890/library_hero.jpg"));

        let games = vec![game("Frostpunk"), game("Genshin Impact"), game("Roblox"), game("Cyberpunk 2077"), game("Cyberpunk 2077")];
        // Frostpunk : connu ; Genshin : introuvable récemment ; Roblox : introuvable il y a longtemps.
        assert_eq!(cache.to_search(&games), vec!["Cyberpunk 2077".to_string(), "Roblox".to_string()]);

        let mut steam = Game::new(Platform::Steam, "730", "Frostpunk", Launch::Uri(String::new()));
        assert!(!cache.apply(&mut steam)); // les jeux Steam ont déjà leurs images
    }

    /// Recherche réelle sur la boutique Steam : `cargo test steam_search_online -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn steam_search_online() {
        let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(8)).build();
        let games = crate::library::scan().games;
        let mut titles: Vec<String> = games.iter().filter(|g| needs_art(g)).map(|g| g.title.clone()).collect();
        titles.dedup();
        for title in titles {
            let found = search(&agent, &title);
            let art = found.clone().ok().flatten().map(|id| assets(&agent, id));
            println!("{title} → {found:?} {art:?}");
            std::thread::sleep(Duration::from_millis(300));
        }
    }
}
