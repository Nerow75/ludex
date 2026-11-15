//! Fiche des jeux dont on connaît l'appid Steam (jeux Steam, Rockstar, jeux retrouvés par `art.rs`) :
//! genres, développeurs, éditeurs, date de sortie et courte description, lus dans l'API publique
//! `IStoreBrowseService/GetItems` de la boutique Steam (aucun compte). Elle accepte plusieurs appids
//! par requête et donne des dates en secondes Unix ; les genres sont les étiquettes de genre les
//! mieux votées du jeu.
//!
//! Les réponses sont gardées dans `metadata-cache.json` (dossier de données de l'app) : un appid
//! n'est redemandé qu'au bout de 30 jours, et sa fiche reste affichée en attendant.

use crate::library::Game;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const REFRESH_AFTER: u64 = 30 * 24 * 3600;
/// À augmenter quand le contenu du cache change : tout est alors redemandé.
const CACHE_VERSION: u32 = 1;
const ITEMS: &str = "https://api.steampowered.com/IStoreBrowseService/GetItems/v1/";
/// Appids par requête.
const BATCH: usize = 40;
/// Étiquettes lues par jeu, parmi lesquelles on garde les genres.
const TAG_COUNT: u32 = 20;
const MAX_GENRES: usize = 3;

/// Étiquettes Steam qui sont des genres (identifiant, nom français). Free-to-play, Accès anticipé
/// et Indépendant sont des modèles ou des origines, pas des genres : écartés.
const GENRES: &[(u32, &str)] = &[
    (19, "Action"),
    (21, "Aventure"),
    (9, "Stratégie"),
    (122, "RPG"),
    (599, "Simulation"),
    (701, "Sport"),
    (699, "Course"),
    (597, "Occasionnel"),
    (128, "Massivement multijoueur"),
];

/// Fiche d'un jeu, envoyée au front.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Details {
    /// Du plus voté au moins voté, trois au plus.
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub developers: Vec<String>,
    #[serde(default)]
    pub publishers: Vec<String>,
    /// Première sortie (secondes Unix) : la sortie d'origine pour un jeu arrivé plus tard sur Steam.
    #[serde(default)]
    pub release_date: Option<u64>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Entry {
    /// Date de la requête (secondes Unix).
    checked: u64,
    /// `None` : l'appid est inconnu de la boutique.
    #[serde(default)]
    details: Option<Details>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MetaCache {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    apps: BTreeMap<u32, Entry>,
    #[serde(skip)]
    path: PathBuf,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[derive(Deserialize)]
struct ItemsResult {
    #[serde(default)]
    response: ItemsBody,
}

#[derive(Default, Deserialize)]
struct ItemsBody {
    #[serde(default)]
    store_items: Vec<StoreItem>,
}

#[derive(Deserialize)]
struct StoreItem {
    #[serde(default)]
    id: u32,
    /// 1 : trouvé.
    #[serde(default)]
    success: u32,
    #[serde(default)]
    basic_info: Option<BasicInfo>,
    #[serde(default)]
    tags: Vec<Tag>,
    #[serde(default)]
    release: Option<Release>,
}

#[derive(Default, Deserialize)]
struct BasicInfo {
    #[serde(default)]
    short_description: String,
    #[serde(default)]
    publishers: Vec<Named>,
    #[serde(default)]
    developers: Vec<Named>,
}

#[derive(Deserialize)]
struct Named {
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
struct Tag {
    tagid: u32,
    #[serde(default)]
    weight: u32,
}

#[derive(Default, Deserialize)]
struct Release {
    #[serde(default)]
    steam_release_date: u64,
    #[serde(default)]
    original_release_date: u64,
}

/// Noms non vides, sans doublon, dans l'ordre.
fn names(list: &[Named]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for n in list.iter().map(|n| n.name.trim()).filter(|n| !n.is_empty()) {
        if !out.iter().any(|o| o == n) {
            out.push(n.to_string());
        }
    }
    out
}

/// Genres parmi les étiquettes, de la plus votée à la moins votée.
fn genres(tags: &[Tag]) -> Vec<String> {
    let mut tags: Vec<&Tag> = tags.iter().collect();
    tags.sort_by_key(|t| std::cmp::Reverse(t.weight));
    tags.iter()
        .filter_map(|t| GENRES.iter().find(|(id, _)| *id == t.tagid).map(|&(_, name)| name.to_string()))
        .take(MAX_GENRES)
        .collect()
}

/// Entités HTML courantes des descriptions de la boutique.
fn decode(text: &str) -> String {
    text.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", "\u{a0}")
        .replace("&amp;", "&")
}

fn details(item: StoreItem) -> Details {
    let info = item.basic_info.unwrap_or_default();
    let release = item.release.unwrap_or_default();
    let date = [release.original_release_date, release.steam_release_date].into_iter().find(|&d| d > 0);
    let description = decode(info.short_description.trim());
    Details {
        genres: genres(&item.tags),
        developers: names(&info.developers),
        publishers: names(&info.publishers),
        release_date: date,
        description: Some(description).filter(|d| !d.is_empty()),
    }
}

/// Lit la réponse de `GetItems` : fiche de chaque appid demandé, `None` si la boutique ne le connaît
/// pas (ou ne l'a pas renvoyé).
fn parse(body: &str, asked: &[u32]) -> Result<Vec<(u32, Option<Details>)>, String> {
    let result: ItemsResult = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let mut found: BTreeMap<u32, Details> = BTreeMap::new();
    for item in result.response.store_items {
        if item.success == 1 && item.id != 0 {
            found.insert(item.id, details(item));
        }
    }
    Ok(asked.iter().map(|&id| (id, found.remove(&id))).collect())
}

fn request(agent: &ureq::Agent, appids: &[u32]) -> Result<Vec<(u32, Option<Details>)>, String> {
    let ids: Vec<String> = appids.iter().map(|id| format!(r#"{{"appid":{id}}}"#)).collect();
    let input = format!(
        r#"{{"ids":[{}],"context":{{"language":"french","country_code":"FR"}},"data_request":{{"include_basic_info":true,"include_release":true,"include_tag_count":{TAG_COUNT}}}}}"#,
        ids.join(",")
    );
    let body = agent
        .get(ITEMS)
        .query("input_json", &input)
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    parse(&body, appids)
}

impl MetaCache {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("metadata-cache.json");
        let mut cache: MetaCache = fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        if cache.version != CACHE_VERSION {
            cache.apps.clear();
            cache.version = CACHE_VERSION;
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

    /// Reporte la fiche connue du jeu. Renvoie vrai si elle a changé.
    pub fn apply(&self, game: &mut Game) -> bool {
        let details = game.steam_appid.and_then(|id| self.apps.get(&id)).and_then(|e| e.details.clone());
        if details.is_none() || details == game.details {
            return false;
        }
        game.details = details;
        true
    }

    /// Appids jamais demandés, ou demandés il y a plus de 30 jours.
    pub fn to_fetch(&self, games: &[Game]) -> Vec<u32> {
        let mut ids: Vec<u32> = games
            .iter()
            .filter_map(|g| g.steam_appid)
            .filter(|id| self.apps.get(id).is_none_or(|e| now().saturating_sub(e.checked) > REFRESH_AFTER))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    fn record(&mut self, results: Vec<(u32, Option<Details>)>) {
        let checked = now();
        for (id, details) in results {
            // Une fiche déjà connue n'est pas effacée par une réponse vide.
            let details = details.or_else(|| self.apps.get(&id).and_then(|e| e.details.clone()));
            self.apps.insert(id, Entry { checked, details });
        }
    }

    /// Demande les fiches par lots (pause entre deux requêtes) et enregistre après chaque lot.
    /// `on_batch` reçoit le cache à jour après chaque lot. Une erreur réseau arrête tout sans rien
    /// enregistrer pour les appids restants : ils seront demandés au prochain scan. Renvoie faux
    /// dans ce cas.
    pub fn fetch(&mut self, appids: Vec<u32>, mut on_batch: impl FnMut(&MetaCache)) -> bool {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(10))
            .user_agent("Ludex (lanceur de jeux)")
            .build();
        for (i, batch) in appids.chunks(BATCH).enumerate() {
            if i > 0 {
                std::thread::sleep(Duration::from_secs(1));
            }
            match request(&agent, batch) {
                Ok(results) => {
                    self.record(results);
                    if let Err(e) = self.save() {
                        eprintln!("Cache des fiches non enregistré : {e}");
                    }
                    on_batch(self);
                }
                Err(e) => {
                    eprintln!("Lecture des fiches interrompue : {e}");
                    return false;
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{Launch, Platform};

    const SAMPLE: &str = r#"{"response":{"store_items":[
        {"item_type":0,"id":1222670,"success":1,"visible":true,"name":"The Sims™ 4","appid":1222670,
         "basic_info":{"short_description":"Créez &quot;vos&quot; Sims &amp; jouez.",
            "publishers":[{"name":"Electronic Arts","creator_clan_account_id":36135791}],
            "developers":[{"name":"Maxis"},{"name":"Maxis"},{"name":" "}]},
         "tags":[{"tagid":10235,"weight":631},{"tagid":113,"weight":600},{"tagid":597,"weight":540},
                 {"tagid":599,"weight":594},{"tagid":21,"weight":100},{"tagid":19,"weight":90}],
         "release":{"steam_release_date":1592506560,"original_release_date":1409641200}},
        {"item_type":0,"id":3932890,"success":1,"basic_info":{"short_description":"  "},
         "release":{"steam_release_date":1763204483}},
        {"item_type":0,"id":1,"success":15,"appid":1}
    ]}}"#;

    #[test]
    fn parses_store_items() {
        let results = parse(SAMPLE, &[1222670, 3932890, 1, 42]).unwrap();
        let sims = results[0].1.clone().unwrap();
        assert_eq!(sims.genres, vec!["Simulation", "Occasionnel", "Aventure"]);
        assert_eq!(sims.developers, vec!["Maxis"]);
        assert_eq!(sims.publishers, vec!["Electronic Arts"]);
        assert_eq!(sims.release_date, Some(1409641200)); // sortie d'origine, pas l'arrivée sur Steam
        assert_eq!(sims.description.as_deref(), Some("Créez \"vos\" Sims & jouez."));

        let tarkov = results[1].1.clone().unwrap();
        assert!(tarkov.genres.is_empty() && tarkov.developers.is_empty());
        assert_eq!(tarkov.release_date, Some(1763204483));
        assert_eq!(tarkov.description, None);

        assert_eq!(results[2], (1, None)); // inconnu de la boutique
        assert_eq!(results[3], (42, None)); // absent de la réponse
        assert!(parse("pas du json", &[1]).is_err());
    }

    fn game(appid: Option<u32>) -> Game {
        let mut g = Game::new(Platform::Other, "x", "X", Launch::Uri(String::new()));
        g.steam_appid = appid;
        g
    }

    #[test]
    fn applies_cache_and_lists_appids_to_fetch() {
        let mut cache = MetaCache::default();
        let cyberpunk = Details { genres: vec!["RPG".into()], developers: vec!["CD PROJEKT RED".into()], ..Details::default() };
        cache.apps.insert(1091500, Entry { checked: now(), details: Some(cyberpunk.clone()) });
        cache.apps.insert(1, Entry { checked: now(), details: None });
        cache.apps.insert(730, Entry { checked: 0, details: Some(Details::default()) });

        let mut g = game(Some(1091500));
        assert!(cache.apply(&mut g));
        assert_eq!(g.details.as_ref(), Some(&cyberpunk));
        assert!(!cache.apply(&mut g)); // déjà à jour
        assert!(!cache.apply(&mut game(Some(1))));
        assert!(!cache.apply(&mut game(None)));

        let games = [game(Some(1091500)), game(Some(1)), game(Some(730)), game(Some(271590)), game(Some(271590)), game(None)];
        // 1091500 et 1 : demandés récemment ; 730 : il y a plus de 30 jours ; 271590 : jamais.
        assert_eq!(cache.to_fetch(&games), vec![730, 271590]);

        // Une réponse vide n'efface pas une fiche connue.
        cache.record(vec![(1091500, None), (730, None)]);
        assert_eq!(cache.apps[&1091500].details.as_ref(), Some(&cyberpunk));
        assert!(cache.to_fetch(&games) == vec![271590]);
    }

    #[test]
    fn saves_and_reloads_the_cache() {
        let dir = std::env::temp_dir().join(format!("ludex-meta-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut cache = MetaCache::load(&dir);
        cache.record(parse(SAMPLE, &[1222670, 1]).unwrap());
        cache.save().unwrap();
        let reloaded = MetaCache::load(&dir);
        assert_eq!(reloaded.apps.len(), 2);
        assert_eq!(reloaded.apps[&1222670].details.as_ref().unwrap().developers, vec!["Maxis"]);
        assert!(reloaded.apps[&1].details.is_none());
        assert!(!dir.join("metadata-cache.json.tmp").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    /// Requête réelle à la boutique Steam : `cargo test steam_details_online -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn steam_details_online() {
        let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(10)).build();
        let results = request(&agent, &[1091500, 1222670, 3932890, 271590]).unwrap();
        for (id, d) in &results {
            println!("{id} → {d:?}");
        }
        assert!(results.iter().all(|(_, d)| d.as_ref().is_some_and(|d| !d.developers.is_empty() && d.release_date.is_some())));
    }

    /// Fiches de toute la bibliothèque de ce PC (jaquettes déjà en cache comprises), sans toucher
    /// aux caches de l'app : `cargo test details_this_pc -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn details_this_pc() {
        let mut games = crate::library::scan().games;
        if let Some(dir) = std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("com.ludex.app")) {
            let art = crate::art::ArtCache::load(&dir);
            games.iter_mut().for_each(|g| {
                art.apply(g);
            });
        }
        let tmp = std::env::temp_dir().join(format!("ludex-meta-pc-{}", std::process::id()));
        let mut cache = MetaCache::load(&tmp);
        let ids = cache.to_fetch(&games);
        assert!(cache.fetch(ids.clone(), |_| {}));
        let mut with = 0;
        for g in games.iter_mut() {
            cache.apply(g);
            if let Some(d) = &g.details {
                with += 1;
                let year = d.release_date.map(|s| 1970 + s / 31_556_952);
                println!("[{:?}] {} · {} · {} · {:?}", g.platform, g.title, d.genres.join(", "), d.developers.join(", "), year);
            }
        }
        println!("{with} jeux sur {} ont une fiche ({} appids demandés)", games.len(), ids.len());
        let _ = fs::remove_dir_all(&tmp);
    }
}
