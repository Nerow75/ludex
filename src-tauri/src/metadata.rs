//! Fiche des jeux dont on connaît l'appid Steam (jeux Steam, Rockstar, jeux retrouvés par `art.rs`) :
//! genres, tags, ambiances, développeurs, éditeurs, date de sortie, courte description et prise en
//! charge de la manette, lus dans l'API publique `IStoreBrowseService/GetItems` de la boutique Steam
//! (aucun compte). Elle accepte plusieurs appids par requête et donne des dates en secondes Unix ;
//! les étiquettes votées (tags) n'y sont que des numéros, nommés en français par
//! `IStoreService/GetTagList` (liste complète, relue tous les 30 jours).
//!
//! Les réponses brutes sont gardées dans `metadata-cache.json` (dossier de données de l'app) : un
//! appid n'est redemandé qu'au bout de 30 jours, et sa fiche reste affichée en attendant. Genres,
//! tags et ambiances (`moods.rs`) en sont tirés à chaque application : changer leurs tables ne
//! demande pas de tout relire.

use crate::library::Game;
use crate::moods::{Mood, moods};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const REFRESH_AFTER: u64 = 30 * 24 * 3600;
/// À augmenter quand le contenu du cache change : tout est alors redemandé.
const CACHE_VERSION: u32 = 2;
const ITEMS: &str = "https://api.steampowered.com/IStoreBrowseService/GetItems/v1/";
const TAG_LIST: &str = "https://api.steampowered.com/IStoreService/GetTagList/v1/?language=french";
/// Appids par requête.
const BATCH: usize = 40;
/// Étiquettes lues par jeu, parmi lesquelles on garde les genres et les tags affichés.
const TAG_COUNT: u32 = 20;
pub const MAX_GENRES: usize = 3;
/// Tags gardés dans la fiche, du plus voté au moins voté.
const MAX_TAGS: usize = 10;
/// Catégories « manette » de la boutique.
const FULL_CONTROLLER: u32 = 28;
const PARTIAL_CONTROLLER: u32 = 18;

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

/// Noms des genres que Ludex reconnaît, dans l'ordre de `GENRES` : seuls choix permis pour
/// corriger les genres d'un jeu (`corrections.rs`).
pub fn genre_names() -> impl Iterator<Item = &'static str> {
    GENRES.iter().map(|&(_, name)| name)
}

/// Prise en charge de la manette annoncée par la boutique.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Controller {
    Full,
    Partial,
}

/// Fiche d'un jeu, envoyée au front.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Details {
    /// Du plus voté au moins voté, trois au plus.
    pub genres: Vec<String>,
    /// Tags Steam en français, du plus voté au moins voté, dix au plus (genres compris).
    pub tags: Vec<String>,
    /// Chill, Intense, Histoire, Entre amis (`moods.rs`).
    pub moods: Vec<Mood>,
    /// `None` : la boutique n'en dit rien (le plus souvent, pas de prise en charge).
    pub controller: Option<Controller>,
    pub developers: Vec<String>,
    pub publishers: Vec<String>,
    /// Première sortie (secondes Unix) : la sortie d'origine pour un jeu arrivé plus tard sur Steam.
    pub release_date: Option<u64>,
    pub description: Option<String>,
}

/// Ce que la boutique dit d'un jeu, gardé tel quel dans le cache.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    /// Identifiants des tags, du plus voté au moins voté (`TAG_COUNT` au plus).
    #[serde(default)]
    tags: Vec<u32>,
    /// Catégories « joueurs » : solo, coop, écran partagé…
    #[serde(default)]
    players: Vec<u32>,
    /// Catégories « manette ».
    #[serde(default)]
    controller: Vec<u32>,
    #[serde(default)]
    developers: Vec<String>,
    #[serde(default)]
    publishers: Vec<String>,
    #[serde(default)]
    release_date: Option<u64>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Entry {
    /// Date de la requête (secondes Unix).
    checked: u64,
    /// `None` : l'appid est inconnu de la boutique.
    #[serde(default)]
    item: Option<Item>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaCache {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    apps: BTreeMap<u32, Entry>,
    /// Noms français des tags, par identifiant.
    #[serde(default)]
    tag_names: BTreeMap<u32, String>,
    /// Date de lecture de `tag_names` (secondes Unix).
    #[serde(default)]
    names_checked: u64,
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
    /// Renvoyées sans rien demander de plus.
    #[serde(default)]
    categories: Option<Categories>,
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

#[derive(Default, Deserialize)]
struct Categories {
    #[serde(default)]
    supported_player_categoryids: Vec<u32>,
    #[serde(default)]
    controller_categoryids: Vec<u32>,
}

#[derive(Deserialize)]
struct TagListResult {
    #[serde(default)]
    response: TagListBody,
}

#[derive(Default, Deserialize)]
struct TagListBody {
    #[serde(default)]
    tags: Vec<TagName>,
}

#[derive(Deserialize)]
struct TagName {
    tagid: u32,
    #[serde(default)]
    name: String,
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

/// Identifiants des étiquettes, de la plus votée à la moins votée, sans doublon.
fn ranked(tags: &[Tag]) -> Vec<u32> {
    let mut tags: Vec<&Tag> = tags.iter().collect();
    tags.sort_by_key(|t| std::cmp::Reverse(t.weight));
    let mut out: Vec<u32> = Vec::new();
    for t in tags {
        if !out.contains(&t.tagid) {
            out.push(t.tagid);
        }
    }
    out
}

/// Genres parmi les étiquettes déjà classées.
fn genres(tags: &[u32]) -> Vec<String> {
    tags.iter()
        .filter_map(|t| GENRES.iter().find(|(id, _)| id == t).map(|&(_, name)| name.to_string()))
        .take(MAX_GENRES)
        .collect()
}

/// Noms français des étiquettes déjà classées ; une étiquette sans nom connu est sautée.
fn tag_names(tags: &[u32], names: &BTreeMap<u32, String>) -> Vec<String> {
    tags.iter().filter_map(|t| names.get(t).cloned()).take(MAX_TAGS).collect()
}

fn controller(categories: &[u32]) -> Option<Controller> {
    if categories.contains(&FULL_CONTROLLER) {
        Some(Controller::Full)
    } else if categories.contains(&PARTIAL_CONTROLLER) {
        Some(Controller::Partial)
    } else {
        None
    }
}

impl Item {
    fn details(&self, names: &BTreeMap<u32, String>) -> Details {
        Details {
            genres: genres(&self.tags),
            tags: tag_names(&self.tags, names),
            moods: moods(&self.tags, &self.players),
            controller: controller(&self.controller),
            developers: self.developers.clone(),
            publishers: self.publishers.clone(),
            release_date: self.release_date,
            description: self.description.clone(),
        }
    }
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

fn item(store: StoreItem) -> Item {
    let info = store.basic_info.unwrap_or_default();
    let release = store.release.unwrap_or_default();
    let categories = store.categories.unwrap_or_default();
    let date = [release.original_release_date, release.steam_release_date].into_iter().find(|&d| d > 0);
    let description = decode(info.short_description.trim());
    Item {
        tags: ranked(&store.tags),
        players: categories.supported_player_categoryids,
        controller: categories.controller_categoryids,
        developers: names(&info.developers),
        publishers: names(&info.publishers),
        release_date: date,
        description: Some(description).filter(|d| !d.is_empty()),
    }
}

/// Lit la réponse de `GetItems` : données de chaque appid demandé, `None` si la boutique ne le
/// connaît pas (ou ne l'a pas renvoyé).
fn parse(body: &str, asked: &[u32]) -> Result<Vec<(u32, Option<Item>)>, String> {
    let result: ItemsResult = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let mut found: BTreeMap<u32, Item> = BTreeMap::new();
    for store in result.response.store_items {
        if store.success == 1 && store.id != 0 {
            found.insert(store.id, item(store));
        }
    }
    Ok(asked.iter().map(|&id| (id, found.remove(&id))).collect())
}

/// Lit la réponse de `GetTagList` : nom de chaque tag. Liste vide = erreur (rien à garder).
fn parse_tag_list(body: &str) -> Result<BTreeMap<u32, String>, String> {
    let result: TagListResult = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let names: BTreeMap<u32, String> = result
        .response
        .tags
        .into_iter()
        .map(|t| (t.tagid, decode(t.name.trim())))
        .filter(|(_, n)| !n.is_empty())
        .collect();
    if names.is_empty() { Err("liste des tags vide".into()) } else { Ok(names) }
}

fn request_tag_list(agent: &ureq::Agent) -> Result<BTreeMap<u32, String>, String> {
    let body = agent.get(TAG_LIST).call().map_err(|e| e.to_string())?.into_string().map_err(|e| e.to_string())?;
    parse_tag_list(&body)
}

fn request(agent: &ureq::Agent, appids: &[u32]) -> Result<Vec<(u32, Option<Item>)>, String> {
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
            cache.tag_names.clear();
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
        let details = game
            .steam_appid
            .and_then(|id| self.apps.get(&id))
            .and_then(|e| e.item.as_ref())
            .map(|item| item.details(&self.tag_names));
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

    fn record(&mut self, results: Vec<(u32, Option<Item>)>) {
        let checked = now();
        for (id, item) in results {
            // Une fiche déjà connue n'est pas effacée par une réponse vide.
            let item = item.or_else(|| self.apps.get(&id).and_then(|e| e.item.clone()));
            self.apps.insert(id, Entry { checked, item });
        }
    }

    /// Noms des tags jamais lus, ou lus il y a plus de 30 jours.
    fn names_stale(&self) -> bool {
        self.tag_names.is_empty() || now().saturating_sub(self.names_checked) > REFRESH_AFTER
    }

    /// Demande les noms des tags s'il le faut, puis les fiches par lots (pause entre deux requêtes),
    /// et enregistre après chaque lot. `on_batch` reçoit le cache à jour après chaque lot. Une erreur
    /// réseau arrête tout sans rien enregistrer pour les appids restants : ils seront demandés au
    /// prochain scan. Renvoie faux dans ce cas.
    pub fn fetch(&mut self, appids: Vec<u32>, mut on_batch: impl FnMut(&MetaCache)) -> bool {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(10))
            .user_agent("Ludex (lanceur de jeux)")
            .build();
        // Les noms d'abord : sans eux, les fiches arriveraient sans tags.
        if self.names_stale() {
            match request_tag_list(&agent) {
                Ok(names) => {
                    crate::diagnostic::ok(crate::diagnostic::Service::Details);
                    self.tag_names = names;
                    self.names_checked = now();
                }
                // Liste connue mais ancienne : on la garde, les tags restent nommés.
                Err(e) if !self.tag_names.is_empty() => {
                    crate::diagnostic::failed(crate::diagnostic::Service::Details, format!("noms des tags : {e}"));
                    eprintln!("Noms des tags non relus : {e}");
                }
                Err(e) => {
                    crate::diagnostic::failed(crate::diagnostic::Service::Details, format!("noms des tags : {e}"));
                    eprintln!("Lecture des fiches interrompue (noms des tags) : {e}");
                    return false;
                }
            }
        }
        for (i, batch) in appids.chunks(BATCH).enumerate() {
            if i > 0 {
                std::thread::sleep(Duration::from_secs(1));
            }
            match request(&agent, batch) {
                Ok(results) => {
                    crate::diagnostic::ok(crate::diagnostic::Service::Details);
                    self.record(results);
                    if let Err(e) = self.save() {
                        eprintln!("Cache des fiches non enregistré : {e}");
                    }
                    on_batch(self);
                }
                Err(e) => {
                    crate::diagnostic::failed(crate::diagnostic::Service::Details, &e);
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
         "categories":{"supported_player_categoryids":[2],"feature_categoryids":[22],"controller_categoryids":[18]},
         "basic_info":{"short_description":"Créez &quot;vos&quot; Sims &amp; jouez.",
            "publishers":[{"name":"Electronic Arts","creator_clan_account_id":36135791}],
            "developers":[{"name":"Maxis"},{"name":"Maxis"},{"name":" "}]},
         "tags":[{"tagid":10235,"weight":631},{"tagid":113,"weight":600},{"tagid":597,"weight":540},
                 {"tagid":599,"weight":594},{"tagid":21,"weight":100},{"tagid":19,"weight":90},{"tagid":10235,"weight":1}],
         "release":{"steam_release_date":1592506560,"original_release_date":1409641200}},
        {"item_type":0,"id":3932890,"success":1,"basic_info":{"short_description":"  "},
         "categories":{"supported_player_categoryids":[1,9],"controller_categoryids":[18,28,55]},
         "release":{"steam_release_date":1763204483}},
        {"item_type":0,"id":1,"success":15,"appid":1}
    ]}}"#;

    const TAG_LIST_SAMPLE: &str = r#"{"response":{"version_hash":"1","tags":[
        {"tagid":10235,"name":"Simulation de vie"},{"tagid":113,"name":"Free-to-play"},{"tagid":597,"name":"Casual"},
        {"tagid":599,"name":"Simulation"},{"tagid":21,"name":"Aventure"},{"tagid":19,"name":"Action"},
        {"tagid":4,"name":" "},{"tagid":5,"name":"Pointer &amp; cliquer"}]}}"#;

    fn names() -> BTreeMap<u32, String> {
        parse_tag_list(TAG_LIST_SAMPLE).unwrap()
    }

    #[test]
    fn parses_store_items() {
        let results = parse(SAMPLE, &[1222670, 3932890, 1, 42]).unwrap();
        let sims = results[0].1.clone().unwrap();
        assert_eq!(sims.tags, vec![10235, 113, 599, 597, 21, 19]); // par votes, sans doublon
        let sims = sims.details(&names());
        assert_eq!(sims.genres, vec!["Simulation", "Occasionnel", "Aventure"]);
        assert_eq!(sims.tags, vec!["Simulation de vie", "Free-to-play", "Simulation", "Casual", "Aventure", "Action"]);
        assert_eq!(sims.moods, vec![Mood::Chill]); // Simulation de vie, Casual ; solo
        assert_eq!(sims.controller, Some(Controller::Partial));
        assert_eq!(sims.developers, vec!["Maxis"]);
        assert_eq!(sims.publishers, vec!["Electronic Arts"]);
        assert_eq!(sims.release_date, Some(1409641200)); // sortie d'origine, pas l'arrivée sur Steam
        assert_eq!(sims.description.as_deref(), Some("Créez \"vos\" Sims & jouez."));

        let tarkov = results[1].1.clone().unwrap().details(&names());
        assert!(tarkov.genres.is_empty() && tarkov.developers.is_empty() && tarkov.tags.is_empty());
        assert_eq!(tarkov.moods, vec![Mood::Friends]); // catégorie coop
        assert_eq!(tarkov.controller, Some(Controller::Full)); // complète l'emporte sur partielle
        assert_eq!(tarkov.release_date, Some(1763204483));
        assert_eq!(tarkov.description, None);

        assert_eq!(results[2], (1, None)); // inconnu de la boutique
        assert_eq!(results[3], (42, None)); // absent de la réponse
        assert!(parse("pas du json", &[1]).is_err());
    }

    #[test]
    fn parses_tag_list_and_names_tags() {
        let names = names();
        assert_eq!(names.len(), 7); // le nom vide est écarté
        assert_eq!(names[&5], "Pointer & cliquer");
        assert!(parse_tag_list(r#"{"response":{"tags":[]}}"#).is_err());
        // Tag sans nom connu sauté ; dix au plus.
        let ids: Vec<u32> = [999].into_iter().chain((0..15).map(|_| 19)).collect();
        let tags = tag_names(&ids, &names);
        assert_eq!(tags.len(), MAX_TAGS);
        assert_eq!(tags[0], "Action");
        assert_eq!(controller(&[55, 57]), None);
    }

    fn game(appid: Option<u32>) -> Game {
        let mut g = Game::new(Platform::Other, "x", "X", Launch::Uri(String::new()));
        g.steam_appid = appid;
        g
    }

    #[test]
    fn applies_cache_and_lists_appids_to_fetch() {
        let mut cache = MetaCache { tag_names: names(), ..MetaCache::default() };
        let cyberpunk = Item { tags: vec![122, 19], developers: vec!["CD PROJEKT RED".into()], ..Item::default() };
        cache.apps.insert(1091500, Entry { checked: now(), item: Some(cyberpunk.clone()) });
        cache.apps.insert(1, Entry { checked: now(), item: None });
        cache.apps.insert(730, Entry { checked: 0, item: Some(Item::default()) });

        let mut g = game(Some(1091500));
        assert!(cache.apply(&mut g));
        let d = g.details.clone().unwrap();
        assert_eq!(d.genres, vec!["RPG", "Action"]);
        assert_eq!(d.tags, vec!["Action"]); // 122 sans nom dans l'exemple
        assert_eq!(d.developers, vec!["CD PROJEKT RED"]);
        assert!(!cache.apply(&mut g)); // déjà à jour
        assert!(!cache.apply(&mut game(Some(1))));
        assert!(!cache.apply(&mut game(None)));

        let games = [game(Some(1091500)), game(Some(1)), game(Some(730)), game(Some(271590)), game(Some(271590)), game(None)];
        // 1091500 et 1 : demandés récemment ; 730 : il y a plus de 30 jours ; 271590 : jamais.
        assert_eq!(cache.to_fetch(&games), vec![730, 271590]);

        // Une réponse vide n'efface pas une fiche connue.
        cache.record(vec![(1091500, None), (730, None)]);
        assert_eq!(cache.apps[&1091500].item.as_ref(), Some(&cyberpunk));
        assert!(cache.to_fetch(&games) == vec![271590]);
    }

    #[test]
    fn tag_names_go_stale() {
        let mut cache = MetaCache::default();
        assert!(cache.names_stale());
        cache.tag_names = names();
        cache.names_checked = now();
        assert!(!cache.names_stale());
        cache.names_checked = now() - REFRESH_AFTER - 1;
        assert!(cache.names_stale());
    }

    #[test]
    fn saves_and_reloads_the_cache() {
        let dir = std::env::temp_dir().join(format!("ludex-meta-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut cache = MetaCache::load(&dir);
        cache.tag_names = names();
        cache.names_checked = 12;
        cache.record(parse(SAMPLE, &[1222670, 1]).unwrap());
        cache.save().unwrap();
        let reloaded = MetaCache::load(&dir);
        assert_eq!(reloaded.apps.len(), 2);
        assert_eq!(reloaded.apps[&1222670].item.as_ref().unwrap().developers, vec!["Maxis"]);
        assert_eq!(reloaded.apps[&1222670].item.as_ref().unwrap().players, vec![2]);
        assert!(reloaded.apps[&1].item.is_none());
        assert_eq!(reloaded.tag_names.len(), 7);
        assert_eq!(reloaded.names_checked, 12);
        assert!(!dir.join("metadata-cache.json.tmp").exists());

        // Cache d'une ancienne version : tout est relu, noms des tags compris.
        let mut old = reloaded.clone();
        old.version = 1;
        old.save().unwrap();
        let fresh = MetaCache::load(&dir);
        assert!(fresh.apps.is_empty() && fresh.tag_names.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    /// Requête réelle à la boutique Steam : `cargo test steam_details_online -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn steam_details_online() {
        let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(10)).build();
        let names = request_tag_list(&agent).unwrap();
        println!("{} noms de tags", names.len());
        let results = request(&agent, &[1091500, 1222670, 3932890, 271590]).unwrap();
        for (id, item) in &results {
            println!("{id} → {:?}", item.as_ref().map(|i| i.details(&names)));
        }
        assert!(results.iter().all(|(_, i)| i.as_ref().is_some_and(|i| !i.developers.is_empty() && i.release_date.is_some())));
    }

    /// Fiches de toute la bibliothèque de ce PC (jaquettes déjà en cache comprises), sans toucher
    /// aux caches de l'app : `cargo test details_this_pc -- --ignored --nocapture`. Affiche aussi
    /// les tags les plus fréquents, la répartition des ambiances et de la manette.
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
        let mut tags: BTreeMap<String, usize> = BTreeMap::new();
        let mut moods: BTreeMap<String, usize> = BTreeMap::new();
        let mut pads: BTreeMap<String, usize> = BTreeMap::new();
        for g in games.iter_mut() {
            cache.apply(g);
            if let Some(d) = &g.details {
                with += 1;
                let year = d.release_date.map(|s| 1970 + s / 31_556_952);
                println!(
                    "[{:?}] {} · {} · {} · {:?}\n    tags : {}\n    ambiances : {:?} · manette : {:?}",
                    g.platform,
                    g.title,
                    d.genres.join(", "),
                    d.developers.join(", "),
                    year,
                    d.tags.join(", "),
                    d.moods,
                    d.controller
                );
                d.tags.iter().for_each(|t| *tags.entry(t.clone()).or_default() += 1);
                d.moods.iter().for_each(|m| *moods.entry(format!("{m:?}")).or_default() += 1);
                if d.moods.is_empty() {
                    *moods.entry("aucune".into()).or_default() += 1;
                }
                *pads.entry(format!("{:?}", d.controller)).or_default() += 1;
            }
        }
        let mut top: Vec<(String, usize)> = tags.into_iter().collect();
        top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        println!("\nTags les plus fréquents ({} différents) :", top.len());
        for (t, n) in top.iter().take(20) {
            println!("  {n:>3}  {t}");
        }
        println!("Ambiances : {moods:?}");
        println!("Manette : {pads:?}");
        println!("{with} jeux sur {} ont une fiche ({} appids demandés)", games.len(), ids.len());
        let _ = fs::remove_dir_all(&tmp);
    }
}
