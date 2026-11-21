//! Jaquettes des jeux qui n'en ont pas (EA, Battle.net, Autres, Riot…) : le jeu est cherché par son
//! titre dans la recherche publique de la boutique Steam (aucun compte), puis ses images viennent du
//! CDN Steam, comme pour les jeux Steam. Les jeux récents n'ont plus leurs images à l'adresse
//! classique du CDN : leurs chemins (hachés) sont lus dans l'API publique `IStoreBrowseService`.
//!
//! Absent de Steam (League of Legends, VALORANT, Genshin Impact, Roblox…), le jeu est cherché dans le
//! catalogue public du Microsoft Store (`displaycatalog.mp.microsoft.com`, sans clé ni compte) :
//! affiche 2:3 (`Poster`) et image de fond (`SuperHeroArt`) sur `store-images.s-microsoft.com`.
//! Seul le nom de chaque image est gardé (jamais une adresse complète venue du réseau).
//!
//! La correspondance doit être stricte : mieux vaut une jaquette générée qu'une image d'un autre jeu.
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
const MATCHING_VERSION: u32 = 4;
const SEARCH: &str = "https://store.steampowered.com/api/storesearch/";
const CDN: &str = "https://cdn.cloudflare.steamstatic.com/steam/apps";
const ITEMS: &str = "https://api.steampowered.com/IStoreBrowseService/GetItems/v1/";
const ASSETS: &str = "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps";
const STORE_SUGGEST: &str = "https://displaycatalog.mp.microsoft.com/v7.0/productFamilies/autosuggest";
const STORE_PRODUCTS: &str = "https://displaycatalog.mp.microsoft.com/v7.0/products";
/// Images du Microsoft Store : `<STORE_IMAGES>/<nom>`, taille demandée par `?w=` (proportions gardées).
const STORE_IMAGES: &str = "https://store-images.s-microsoft.com/image";

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
    /// Jeu trouvé dans le Microsoft Store (seulement s'il n'est pas sur Steam).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    store: Option<StoreArt>,
}

/// Produit du Microsoft Store et noms de ses images.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct StoreArt {
    /// Identifiant du produit (« 9PB301R051CQ »).
    product: String,
    /// Affiche 2:3, nom de l'image (`store-images.s-microsoft.com/image/<nom>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cover: Option<String>,
    /// Image de fond sans titre.
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

pub(crate) fn without_edition(name: &str) -> &str {
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

/// Mentions de plateforme retirées du nom d'un produit du Microsoft Store (« Roblox - Windows »).
const STORE_PLATFORMS: &[&str] = &["forwindows10", "forwindows", "windows10", "windows", "forpc", "pc"];

/// Le produit du Microsoft Store est-il bien ce jeu ? Égalité stricte (symboles, casse, mention
/// d'édition et de plateforme retirées), aucune faute tolérée : « StarCraft II : Collection de
/// campagnes » n'est pas « StarCraft II ».
fn same_store_game(title: &str, store_name: &str) -> bool {
    let (t, s) = (compact(title), compact(store_name));
    let s = STORE_PLATFORMS.iter().find_map(|p| s.strip_suffix(p)).unwrap_or(&s);
    t.len() >= 3 && t == without_edition(s)
}

/// Identifiant de produit du Microsoft Store : 12 lettres majuscules ou chiffres.
fn store_product_id(id: &str) -> bool {
    id.len() == 12 && id.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

/// Nom d'une image du Microsoft Store tiré de son adresse (`//store-images.s-microsoft.com/image/<nom>`) :
/// lettres, chiffres, `.` et `-` seulement, sinon rien.
fn store_image(uri: &str) -> Option<String> {
    let name = uri.strip_prefix("https:").unwrap_or(uri).strip_prefix("//store-images.s-microsoft.com/image/")?;
    let ok = !name.is_empty()
        && name.len() <= 200
        && !name.contains("..")
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.');
    ok.then(|| name.to_string())
}

/// Premier jeu (ni extension ni monnaie) des suggestions dont le nom correspond au titre.
fn parse_suggest(body: &serde_json::Value, title: &str) -> Option<String> {
    body["Results"].as_array()?.iter().flat_map(|r| r["Products"].as_array().into_iter().flatten()).find_map(|p| {
        let id = p["ProductId"].as_str().filter(|id| store_product_id(id))?;
        (p["Type"] == "Game" && same_store_game(title, p["Title"].as_str()?)).then(|| id.to_string())
    })
}

/// Affiche (`Poster`, sinon `BrandedKeyArt`, toujours plus haute que large) et image de fond
/// (`SuperHeroArt`, plus large que haute) d'un produit, s'il correspond toujours au titre.
fn parse_product(body: &serde_json::Value, product: &str, title: &str) -> Option<StoreArt> {
    let item = body["Products"].as_array()?.iter().find(|p| p["ProductId"] == product)?;
    let props = &item["LocalizedProperties"][0];
    if !same_store_game(title, props["ProductTitle"].as_str()?) {
        return None;
    }
    let images = props["Images"].as_array()?;
    let find = |purpose: &str, tall: bool| {
        images.iter().find_map(|i| {
            let (w, h) = (i["Width"].as_u64()?, i["Height"].as_u64()?);
            let fits = i["ImagePurpose"] == purpose && w > 0 && h > 0 && (h > w) == tall;
            if fits { store_image(i["Uri"].as_str()?) } else { None }
        })
    };
    let art = StoreArt {
        product: product.to_string(),
        cover: find("Poster", true).or_else(|| find("BrandedKeyArt", true)),
        hero: find("SuperHeroArt", false),
    };
    (art.cover.is_some() || art.hero.is_some()).then_some(art)
}

fn get_json(request: ureq::Request) -> Result<serde_json::Value, String> {
    request
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())
        .and_then(|b| serde_json::from_str(&b).map_err(|e| e.to_string()))
}

/// Catalogue public du Microsoft Store (sans clé ni compte). `Ok(None)` : le jeu n'y est pas.
fn store_search(agent: &ureq::Agent, title: &str) -> Result<Option<StoreArt>, String> {
    let suggest = agent
        .get(STORE_SUGGEST)
        .query("market", "FR")
        .query("languages", "fr-fr")
        .query("query", title)
        .query("productFamilyNames", "Games");
    let Some(product) = parse_suggest(&get_json(suggest)?, title) else { return Ok(None) };
    std::thread::sleep(Duration::from_millis(200));
    let details = agent.get(STORE_PRODUCTS).query("bigIds", &product).query("market", "FR").query("languages", "fr-fr");
    Ok(parse_product(&get_json(details)?, &product, title))
}

/// Images du Microsoft Store : affiche à 600 px de large (900 de haut pour une 2:3), fond à 1 920.
/// L'image de fond du launcher (FiveM, jeux Ankama…) reste. Renvoie vrai si le jeu a changé.
fn set_store_urls(game: &mut Game, art: &StoreArt) -> bool {
    let before = (game.cover_url.clone(), game.hero_url.clone());
    if let Some(cover) = &art.cover {
        game.cover_url = Some(format!("{STORE_IMAGES}/{cover}?w=600"));
    }
    if let (Some(hero), None) = (&art.hero, &game.hero_url) {
        game.hero_url = Some(format!("{STORE_IMAGES}/{hero}?w=1920"));
    }
    before != (game.cover_url.clone(), game.hero_url.clone())
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
            Some(Entry { store: Some(art), .. }) => set_store_urls(game, art),
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
                Some(e) => e.appid.is_none() && e.store.is_none() && now().saturating_sub(e.checked) > RETRY_AFTER,
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
                    // Pas sur Steam : le Microsoft Store. Une erreur réseau arrête aussi la recherche.
                    let store = match appid {
                        Some(_) => None,
                        None => {
                            std::thread::sleep(Duration::from_millis(200));
                            match store_search(&agent, title) {
                                Ok(store) => store,
                                Err(e) => {
                                    crate::diagnostic::failed(crate::diagnostic::Service::Art, format!("Microsoft Store : {e}"));
                                    log::warn!("Recherche de jaquettes interrompue (Microsoft Store) : {e}");
                                    return;
                                }
                            }
                        }
                    };
                    crate::diagnostic::ok(crate::diagnostic::Service::Art);
                    let found = appid.is_some() || store.is_some();
                    self.titles.insert(compact(title), Entry { appid, checked: now(), cover, hero, store });
                    if let Err(e) = self.save() {
                        log::warn!("Cache des jaquettes non enregistré : {e}");
                    }
                    if found {
                        on_found(title);
                    }
                }
                Err(e) => {
                    crate::diagnostic::failed(crate::diagnostic::Service::Art, &e);
                    log::warn!("Recherche de jaquettes interrompue : {e}");
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
        cache.titles.insert("frostpunk".into(), Entry { appid: Some(323190), checked: now(), cover: None, hero: None, store: None });
        cache.titles.insert("genshinimpact".into(), Entry { appid: None, checked: now(), cover: None, hero: None, store: None });
        cache.titles.insert("roblox".into(), Entry { appid: None, checked: 0, cover: None, hero: None, store: None });

        let game = |title: &str| Game::new(Platform::Other, title, title, Launch::Uri(String::new()));
        let mut frostpunk = game("Frostpunk");
        assert!(cache.apply(&mut frostpunk));
        assert!(frostpunk.cover_url.unwrap().ends_with("/323190/library_600x900_2x.jpg"));
        assert_eq!(frostpunk.steam_appid, Some(323190));
        cache.titles.insert(
            "escapefromtarkov".into(),
            Entry { appid: Some(3932890), checked: now(), cover: Some("df4e/library_capsule_2x.jpg".into()), hero: None, store: None },
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

    #[test]
    fn matches_only_the_same_store_product() {
        assert!(same_store_game("League of Legends", "League of Legends"));
        assert!(same_store_game("VALORANT", "VALORANT"));
        assert!(same_store_game("Roblox", "Roblox - Windows"));
        assert!(same_store_game("Genshin Impact", "Genshin Impact™"));
        assert!(same_store_game("Minecraft Launcher", "Minecraft Launcher"));
        assert!(!same_store_game("StarCraft II", "StarCraft II® : Collection de campagnes"));
        assert!(!same_store_game("Roblox", "Roblox - Xbox"));
        assert!(!same_store_game("Minecraft", "Minecraft Legends"));
        assert!(!same_store_game("Fortnite", "Fortnite Festival"));
        assert!(!same_store_game("Genshin Impac", "Genshin Impact")); // aucune faute tolérée
        assert!(!same_store_game("PC", "PC"));
        assert!(store_product_id("9PB301R051CQ"));
        assert!(!store_product_id("9pb301r051cq"));
        assert!(!store_product_id("9PB301R051CQ&x=1"));
    }

    /// Réponses du catalogue enregistrées (raccourcies) le 19 novembre 2026.
    fn suggest_body() -> serde_json::Value {
        serde_json::json!({ "Results": [
            { "ProductFamilyName": "Games", "Products": [
                { "ProductId": "BQ1TN1T79V9K", "Type": "Game", "Title": "Roblox - Xbox" },
                { "ProductId": "C3S80L2NKZCZ", "Type": "Consumable", "Title": "Roblox" },
                { "ProductId": "../../evil00", "Type": "Game", "Title": "Roblox" },
                { "ProductId": "9PMF91N3LZ3M", "Type": "Game", "Title": "Roblox - Windows" }
            ]}
        ]})
    }

    fn product_body() -> serde_json::Value {
        let uri = |n: &str| format!("//store-images.s-microsoft.com/image/apps.3683.142949.{n}");
        serde_json::json!({ "Products": [{ "ProductId": "9PMF91N3LZ3M", "LocalizedProperties": [{
            "ProductTitle": "Roblox - Windows",
            "Images": [
                { "ImagePurpose": "Logo", "Width": 300, "Height": 300, "Uri": uri("logo") },
                { "ImagePurpose": "BoxArt", "Width": 1080, "Height": 1080, "Uri": uri("box") },
                { "ImagePurpose": "SuperHeroArt", "Width": 1920, "Height": 1080, "Uri": "https://evil.com/a.jpg" },
                { "ImagePurpose": "SuperHeroArt", "Width": 1920, "Height": 1080, "Uri": uri("hero") },
                { "ImagePurpose": "Poster", "Width": 1440, "Height": 2160, "Uri": uri("poster") }
            ]
        }]}]})
    }

    #[test]
    fn reads_store_answers_strictly() {
        assert_eq!(parse_suggest(&suggest_body(), "Roblox").as_deref(), Some("9PMF91N3LZ3M"));
        assert_eq!(parse_suggest(&suggest_body(), "Roblox Studio"), None);
        assert_eq!(parse_suggest(&serde_json::json!({}), "Roblox"), None);

        let art = parse_product(&product_body(), "9PMF91N3LZ3M", "Roblox").unwrap();
        assert_eq!(art.cover.as_deref(), Some("apps.3683.142949.poster"));
        assert_eq!(art.hero.as_deref(), Some("apps.3683.142949.hero"));
        // Un autre produit ou un autre nom : rien.
        assert_eq!(parse_product(&product_body(), "9N7TFFRRZCC9", "Roblox"), None);
        assert_eq!(parse_product(&product_body(), "9PMF91N3LZ3M", "Roblox Studio"), None);

        assert_eq!(store_image("//store-images.s-microsoft.com/image/apps.1.2-a.b").as_deref(), Some("apps.1.2-a.b"));
        assert_eq!(store_image("https://store-images.s-microsoft.com/image/apps.1").as_deref(), Some("apps.1"));
        for bad in [
            "//evil.com/image/apps.1",
            "//store-images.s-microsoft.com/image/",
            "//store-images.s-microsoft.com/image/../x",
            "//store-images.s-microsoft.com/image/a/b",
            "//store-images.s-microsoft.com/image/a?w=1",
            "//store-images.s-microsoft.com.evil.com/image/a",
        ] {
            assert_eq!(store_image(bad), None, "{bad}");
        }
    }

    #[test]
    fn applies_store_art_without_replacing_a_launcher_hero() {
        let mut cache = ArtCache::default();
        let art = StoreArt { product: "9PB301R051CQ".into(), cover: Some("apps.1.poster".into()), hero: Some("apps.1.hero".into()) };
        cache.titles.insert(
            "leagueoflegends".into(),
            Entry { appid: None, checked: 0, cover: None, hero: None, store: Some(art) },
        );
        let mut lol = Game::new(Platform::Riot, "league_of_legends.live", "League of Legends", Launch::Uri(String::new()));
        assert!(cache.apply(&mut lol));
        let cover = lol.cover_url.clone().unwrap();
        assert_eq!(cover, format!("{STORE_IMAGES}/apps.1.poster?w=600"));
        assert!(crate::images::allowed(&cover));
        assert!(crate::images::allowed(lol.hero_url.as_deref().unwrap()));
        assert_eq!(lol.logo_url, None);
        assert_eq!(lol.steam_appid, None);
        // Trouvé dans le Microsoft Store, même il y a longtemps : pas recherché de nouveau.
        assert!(cache.to_search(&[lol]).is_empty());

        let mut other = Game::new(Platform::Other, "league", "League of Legends", Launch::Uri(String::new()));
        other.hero_url = Some("http://ludex.localhost/other/league/hero".into());
        assert!(cache.apply(&mut other));
        assert_eq!(other.hero_url.as_deref(), Some("http://ludex.localhost/other/league/hero"));
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

    /// Visuels hors Steam de ce PC, en lecture seule (aucun cache de l'app lu ni écrit) :
    /// `cargo test visuals_this_pc -- --ignored --nocapture`. Pour chaque jeu hors Steam : images
    /// locales du launcher, puis, sans jaquette, Steam puis le Microsoft Store, et les images du
    /// Store téléchargées en mémoire (taille, type, dimensions).
    #[test]
    #[ignore]
    fn visuals_this_pc() {
        let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(8)).build();
        let download = crate::images::agent();
        let games = crate::library::scan().games;
        let (mut local, mut store, mut none) = (0, 0, Vec::new());
        for g in games.iter().filter(|g| g.platform != Platform::Steam) {
            let art = &g.local_art;
            let kinds: Vec<&str> = ["cover", "hero", "logo"].into_iter().filter(|k| art.get(k).is_some()).collect();
            if !kinds.is_empty() {
                local += 1;
                println!("[{:?}] {} : images locales {kinds:?}", g.platform, g.title);
                for k in &kinds {
                    println!("    {k} = {}", art.get(k).unwrap().display());
                }
            }
            if !needs_art(g) {
                continue;
            }
            std::thread::sleep(Duration::from_millis(350));
            if let Ok(Some(appid)) = search(&agent, &g.title) {
                println!("[{:?}] {} : Steam {appid}", g.platform, g.title);
                continue;
            }
            std::thread::sleep(Duration::from_millis(200));
            match store_search(&agent, &g.title) {
                Ok(Some(found)) => {
                    store += 1;
                    println!("[{:?}] {} : Microsoft Store {}", g.platform, g.title, found.product);
                    let mut game = g.clone();
                    set_store_urls(&mut game, &found);
                    for (kind, url) in [("cover", &game.cover_url), ("hero", &game.hero_url)] {
                        let Some(url) = url.as_deref().filter(|u| u.contains(STORE_IMAGES)) else { continue };
                        match crate::images::fetch(&download, url) {
                            crate::images::Fetch::Image(bytes, ext) => {
                                let dims = crate::covers::dimensions(&bytes).map(|(w, h)| format!("{w}×{h}"));
                                println!("    {kind} : {} Ko, {ext}, {}", bytes.len() / 1024, dims.unwrap_or_default());
                            }
                            other => println!("    {kind} : {other:?}"),
                        }
                    }
                }
                Ok(None) => none.push(g.title.clone()),
                Err(e) => println!("[{:?}] {} : erreur {e}", g.platform, g.title),
            }
        }
        println!("Images locales : {local} jeux ; Microsoft Store : {store} ; rien : {} {none:?}", none.len());
    }
}
