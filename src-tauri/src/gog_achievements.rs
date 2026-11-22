//! Succès GOG, lus par le compte GOG déjà connecté (`gog_online`) : aucune connexion de plus.
//!
//! GOG Galaxy ne garde rien de lisible sur le disque pour les succès (base `galaxy-2.0.db` sans
//! table documentée, et Galaxy absent de beaucoup de PC) : la seule voie sans nouveau compte est
//! l'API « gameplay » que Galaxy et les jeux utilisent, `GET https://gameplay.gog.com/clients/
//! <id produit>/users/<id du compte>/achievements`, jeton du compte en `Authorization: Bearer`,
//! langue en `X-Gog-Lc` (forme relevée dans comet, réimplémentation libre du service de Galaxy, et
//! dans SuccessStory, extension de Playnite, qui y passe l'id du produit). Réponse : `{ total_count,
//! limit, page_token, items: [{ achievement_id, achievement_key, name, description, visible,
//! image_url_unlocked, image_url_locked, date_unlocked (RFC 3339 ou nul), rarity (% de joueurs),
//! rarity_level_slug }], achievements_mode }`.
//!
//! Règles : jeux de la source GOG seulement (id produit numérique), une requête à la fois et
//! espacée, redirections refusées (agent de `gog_online::api`), réponse lue sur 4 Mo au plus, erreur
//! sans URL. Le front ne reçoit jamais de jeton ni d'id de compte : seulement des succès nettoyés
//! et des icônes sur le CDN d'images de GOG, adresse reconstruite ici (`image_url`). Un 401, 403 ou
//! 404 de l'API ne touche pas au compte (pas « à reconnecter ») : le jeu est noté sans succès.
//!
//! Cache `achievements-gog.json` (dossier de données de l'app) : liste par produit, relue après
//! 24 h, ou si le jeu a été joué depuis, ou si la langue de Ludex a changé ; une réponse vide
//! n'efface pas une liste connue. Résumé posé sur `Game.achievements` au scan (`apply`), détail lu
//! dans le cache pour la fiche, la vitrine, le journal et la story.

use crate::gog_online::api::{self, Failure};
use crate::gog_online::vault::Session;
use crate::library::{AchievementStore, Achievement, Achievements, Game, Platform};
use crate::{diagnostic, lock, AppState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// Événement `game-achievements` : `{ id, achievements }` quand le résumé d'un jeu change.
pub const EVENT: &str = "game-achievements";
const FILE: &str = "achievements-gog.json";
/// À augmenter quand le contenu du cache change : tout est alors redemandé.
const CACHE_VERSION: u32 = 1;
const REFRESH_AFTER: u64 = 24 * 3600;
/// Relecture forcée depuis la fiche : pas plus d'une fois par 10 minutes.
const REFRESH_MIN: u64 = 600;
const GAMEPLAY: &str = "https://gameplay.gog.com";
const GAP: Duration = Duration::from_millis(300);
const MAX_BODY: u64 = 4 * 1024 * 1024;
const MAX_ITEMS: usize = 2_000;

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Langue demandée à GOG (noms et descriptions), et gardée avec la liste.
fn language() -> &'static str {
    if crate::i18n::english() { "en-US" } else { "fr-FR" }
}

/// Un succès tel que gardé dans le cache, déjà nettoyé.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub visible: bool,
    pub unlocked: bool,
    #[serde(default)]
    pub unlocked_at: Option<u64>,
    /// Part des joueurs GOG qui l'ont débloqué, en %.
    #[serde(default)]
    pub rarity: Option<f32>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub icon_locked: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Entry {
    checked: u64,
    lang: String,
    /// `None` : GOG n'a rien donné (jeu sans succès, ou accès refusé).
    #[serde(default)]
    items: Option<Vec<Item>>,
}

/// Cache des succès GOG, par id de produit.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Cache {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    games: BTreeMap<u64, Entry>,
    #[serde(skip)]
    path: PathBuf,
}

impl Cache {
    pub fn load(dir: &Path) -> Cache {
        let path = dir.join(FILE);
        let mut cache: Cache = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        if cache.version != CACHE_VERSION {
            cache.games.clear();
            cache.version = CACHE_VERSION;
        }
        // Relecture prudente : un fichier modifié à la main repasse par les mêmes filtres.
        for entry in cache.games.values_mut() {
            if let Some(items) = entry.items.take() {
                entry.items = Some(items.into_iter().filter_map(Item::recheck).take(MAX_ITEMS).collect());
            }
        }
        cache.path = path;
        cache
    }

    pub fn save(&self) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }

    /// À relire : jamais lu, autre langue, plus de 24 h, ou joué depuis la dernière lecture.
    fn stale(&self, product: u64, last_played: Option<u64>, at: u64, lang: &str) -> bool {
        match self.games.get(&product) {
            None => true,
            Some(e) => e.lang != lang || at.saturating_sub(e.checked) > REFRESH_AFTER || last_played.is_some_and(|p| p > e.checked),
        }
    }

    fn record(&mut self, product: u64, items: Option<Vec<Item>>, at: u64, lang: &str) {
        let previous = self.games.get(&product).filter(|e| e.lang == lang).and_then(|e| e.items.clone());
        // Une liste connue n'est pas effacée par une réponse vide.
        let items = items.filter(|l| !l.is_empty()).or(previous);
        self.games.insert(product, Entry { checked: at, lang: lang.to_string(), items });
    }

    fn items(&self, product: u64) -> Option<&[Item]> {
        self.games.get(&product)?.items.as_deref().filter(|l| !l.is_empty())
    }

    /// Résumé d'un jeu, comme celui de Steam.
    pub fn summary(&self, product: u64) -> Option<Achievements> {
        let items = self.items(product)?;
        let mut unlock_times: Vec<u64> = items.iter().filter_map(|i| i.unlocked_at).collect();
        unlock_times.sort_unstable();
        Some(Achievements {
            store: AchievementStore::Gog,
            unlocked: items.iter().filter(|i| i.unlocked).count() as u32,
            total: items.len() as u32,
            last_unlocked: unlock_times.last().copied(),
            unlock_times,
        })
    }

    /// Liste complète, dans l'ordre de GOG, rareté comprise. Caché et pas encore débloqué : ni
    /// nom, ni description, ni icône (même règle que Steam).
    pub fn details(&self, product: u64) -> Option<Vec<Achievement>> {
        let items = self.items(product)?;
        Some(
            items
                .iter()
                .map(|i| {
                    let secret = !i.visible && !i.unlocked;
                    Achievement {
                        store: AchievementStore::Gog,
                        api_name: i.key.clone(),
                        name: if secret { String::new() } else { i.name.clone() },
                        description: if secret { None } else { i.description.clone() },
                        icon: if secret { None } else { i.icon.clone() },
                        icon_gray: if secret { None } else { i.icon_locked.clone().or_else(|| i.icon.clone()) },
                        hidden: !i.visible,
                        unlocked: i.unlocked,
                        unlocked_at: i.unlocked_at,
                        global_percent: i.rarity,
                    }
                })
                .collect(),
        )
    }

    /// Pose le résumé connu sur un jeu GOG (au scan).
    pub fn apply(&self, game: &mut Game) {
        if let Some(product) = product_of(game)
            && game.achievements.is_none()
        {
            game.achievements = self.summary(product);
        }
    }
}

/// Id de produit d'un jeu de la source GOG (numérique), sinon `None`.
pub fn product_of(game: &Game) -> Option<u64> {
    if game.platform != Platform::Gog || game.platform_id.is_empty() || game.platform_id.len() > 20 || !game.platform_id.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    game.platform_id.parse().ok().filter(|&id| id > 0)
}

impl Item {
    /// Mêmes filtres qu'à la lecture de l'API, pour un cache relu.
    fn recheck(self) -> Option<Item> {
        Some(Item {
            key: clean_key(&self.key)?,
            name: crate::library::manual::clean_text(&self.name, 120).unwrap_or_default(),
            description: self.description.and_then(|d| crate::library::manual::clean_text(&d, 400)),
            rarity: self.rarity.filter(|r| (0.0..=100.0).contains(r)),
            icon: self.icon.and_then(|u| image_url(&u)),
            icon_locked: self.icon_locked.and_then(|u| image_url(&u)),
            unlocked: self.unlocked || self.unlocked_at.is_some(),
            ..self
        })
    }
}

/// Clé interne d'un succès : 1 à 128 caractères imprimables, sans espace.
fn clean_key(raw: &str) -> Option<String> {
    let key = raw.trim();
    (!key.is_empty() && key.len() <= 128 && key.chars().all(|c| c.is_ascii_graphic())).then(|| key.to_string())
}

/// Icône d'un succès : seulement le CDN d'images de GOG (`images.gog-statics.com`,
/// `images-<n>.gog-statics.com`, `images.gog.com`), en https, port par défaut, sans identifiants,
/// requête ni fragment, un seul nom de fichier (lettres, chiffres, `.`, `_`, `-`, sans `..`).
/// Adresse reconstruite par Ludex : `https://images.gog-statics.com/<fichier>` (ou `images.gog.com`).
pub fn image_url(raw: &str) -> Option<String> {
    let url = url::Url::parse(raw.trim()).ok()?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() || url.port().is_some() || url.query().is_some() || url.fragment().is_some() {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    let statics = host == "images.gog-statics.com"
        || host.strip_prefix("images-").and_then(|h| h.strip_suffix(".gog-statics.com")).is_some_and(|n| !n.is_empty() && n.len() <= 2 && n.bytes().all(|b| b.is_ascii_digit()));
    let canonical = if statics {
        "images.gog-statics.com"
    } else if host == "images.gog.com" {
        "images.gog.com"
    } else {
        return None;
    };
    let file = url.path().strip_prefix('/')?;
    let safe = !file.is_empty()
        && file.len() <= 200
        && !file.contains("..")
        && file.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    safe.then(|| format!("https://{canonical}/{file}"))
}

/// Date de déblocage : RFC 3339, ou `+0000` sans deux-points (forme vue chez GOG).
fn parse_date(raw: &str) -> Option<u64> {
    let raw = raw.trim();
    let parsed = chrono::DateTime::parse_from_rfc3339(raw)
        .or_else(|_| chrono::DateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%z"))
        .or_else(|_| chrono::DateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%.f%z"))
        .ok()?;
    u64::try_from(parsed.timestamp()).ok().filter(|&t| t > 0)
}

/// Lit la réponse de l'API. `None` : réponse illisible.
pub fn parse(text: &str) -> Option<Vec<Item>> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let list = v.get("items")?.as_array()?;
    let mut seen = std::collections::HashSet::new();
    let text_of = |item: &serde_json::Value, key: &str, max: usize| item.get(key).and_then(|s| s.as_str()).and_then(|s| crate::library::manual::clean_text(s, max));
    let items = list
        .iter()
        .filter_map(|item| {
            let key = item
                .get("achievement_key")
                .and_then(|k| k.as_str())
                .and_then(clean_key)
                .or_else(|| item.get("achievement_id").and_then(|k| k.as_str().map(String::from).or_else(|| k.as_u64().map(|n| n.to_string()))).as_deref().and_then(clean_key))?;
            if !seen.insert(key.clone()) {
                return None;
            }
            let unlocked_at = item.get("date_unlocked").and_then(|d| d.as_str()).and_then(parse_date);
            let unlocked = unlocked_at.is_some() || item.get("date_unlocked").is_some_and(|d| d.as_str().is_some_and(|s| !s.trim().is_empty()));
            let image = |key: &str| item.get(key).and_then(|u| u.as_str()).and_then(image_url);
            Some(Item {
                name: text_of(item, "name", 120).unwrap_or_else(|| key.clone()),
                description: text_of(item, "description", 400),
                visible: item.get("visible").and_then(|b| b.as_bool()).unwrap_or(true),
                unlocked,
                unlocked_at,
                rarity: item.get("rarity").and_then(|r| r.as_f64()).map(|r| r as f32).filter(|r| (0.0..=100.0).contains(r)),
                icon: image("image_url_unlocked"),
                icon_locked: image("image_url_locked"),
                key,
            })
        })
        .take(MAX_ITEMS)
        .collect();
    Some(items)
}

/// Réponse de l'API pour un jeu.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    Items(Vec<Item>),
    /// 401, 403 ou 404 : succès inaccessibles pour ce jeu (aucun, ou refusés).
    Refused(u16),
}

/// Une requête. `base` : `https://gameplay.gog.com` (serveur local dans les tests).
pub fn request(agent: &ureq::Agent, base: &str, user_id: &str, product: u64, token: &str, lang: &str) -> Result<Outcome, Failure> {
    if user_id.is_empty() || user_id.len() > 32 || !user_id.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Failure::Network(tr!("compte illisible", "unreadable account")));
    }
    let url = format!("{base}/clients/{product}/users/{user_id}/achievements");
    let response = match agent.get(&url).set("Authorization", &format!("Bearer {token}")).set("X-Gog-Lc", lang).call() {
        Ok(r) => r,
        Err(ureq::Error::Status(code @ (401 | 403 | 404), _)) => return Ok(Outcome::Refused(code)),
        Err(e) => return Err(api::failure(e)),
    };
    if response.status() != 200 {
        return Err(Failure::Status(response.status()));
    }
    let mut text = String::new();
    response
        .into_reader()
        .take(MAX_BODY)
        .read_to_string(&mut text)
        .map_err(|_| Failure::Network(tr!("réponse illisible", "unreadable response")))?;
    parse(&text).map(Outcome::Items).ok_or_else(|| Failure::Network(tr!("liste des succès illisible", "unreadable achievement list")))
}

/// État des succès GOG, posé une fois au démarrage (`init`), à part d'`AppState`.
pub struct GogAchievements {
    cache: Mutex<Cache>,
    busy: AtomicBool,
}

impl GogAchievements {
    pub fn new(data_dir: &Path) -> GogAchievements {
        GogAchievements { cache: Mutex::new(Cache::load(data_dir)), busy: AtomicBool::new(false) }
    }

    pub fn busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }

    /// Lit le cache sous verrou (détail, résumé).
    pub fn with<T>(&self, read: impl FnOnce(&Cache) -> T) -> Option<T> {
        lock(&self.cache).ok().map(|c| read(&c))
    }
}

static STATE: OnceLock<GogAchievements> = OnceLock::new();

/// Charge le cache (dans `setup`, avant le premier scan).
pub fn init(data_dir: &Path) {
    let _ = STATE.set(GogAchievements::new(data_dir));
}

/// État des succès GOG ; `None` avant `init` (tests).
pub fn get() -> Option<&'static GogAchievements> {
    STATE.get()
}

/// Pose les résumés connus sur les jeux GOG d'un scan (rien avant `init`).
pub fn apply(games: &mut [Game]) {
    let Some(state) = get() else { return };
    let Ok(cache) = lock(&state.cache) else { return };
    games.iter_mut().for_each(|g| cache.apply(g));
}

/// Jeux GOG à relire : (id du jeu, id produit).
fn to_fetch(app: &AppHandle, force: Option<&str>) -> Vec<(String, u64)> {
    let (Some(state), Some(ours)) = (app.try_state::<AppState>(), get()) else { return Vec::new() };
    let Ok(games) = lock(&state.games) else { return Vec::new() };
    let Ok(cache) = lock(&ours.cache) else { return Vec::new() };
    let (at, lang) = (now(), language());
    games
        .iter()
        .filter(|g| !g.is_tool)
        .filter(|g| force.is_none_or(|id| g.id == id))
        .filter_map(|g| Some((g, product_of(g)?)))
        .filter(|(g, product)| match force {
            // Depuis la fiche : relu si plus de 10 minutes, ou jamais lu.
            Some(_) => cache.games.get(product).is_none_or(|e| e.lang != lang || at.saturating_sub(e.checked) > REFRESH_MIN),
            None => cache.stale(*product, g.last_played, at, lang),
        })
        .map(|(g, product)| (g.id.clone(), product))
        .collect()
}

/// Lit un jeu, l'enregistre et prévient le front si son résumé change. `false` : arrêt (pas de
/// compte, réseau, jeton refusé).
fn fetch_one(app: &AppHandle, id: &str, product: u64) -> bool {
    let Some(gog) = app.try_state::<crate::gog_online::Gog>() else { return false };
    let Some(ours) = get() else { return false };
    let lang = language();
    let call = |agent: &ureq::Agent, session: &Session| request(agent, GAMEPLAY, &session.user_id, product, &session.access_token, lang);
    let items = match gog.call(app, call) {
        None => return false,
        Some(Err(e)) => {
            diagnostic::failed(diagnostic::Service::GogAchievements, &e);
            log::warn!("Succès GOG interrompus : {e}");
            return false;
        }
        Some(Ok(Outcome::Items(items))) => {
            diagnostic::ok(diagnostic::Service::GogAchievements);
            Some(items)
        }
        Some(Ok(Outcome::Refused(code))) => {
            // 404 : le plus souvent un jeu sans succès ; 401 ou 403 : accès refusé à ce jeu.
            if code != 404 {
                let message = tr!("GOG refuse l'accès aux succès ({}).", "GOG refused access to achievements ({}).", code);
                diagnostic::failed(diagnostic::Service::GogAchievements, &message);
            }
            None
        }
    };
    let summary = match lock(&ours.cache) {
        Ok(mut cache) => {
            cache.record(product, items, now(), lang);
            if let Err(e) = cache.save() {
                log::warn!("Succès GOG non enregistrés : {e}");
            }
            cache.summary(product)
        }
        Err(_) => return false,
    };
    publish(app, id, summary);
    true
}

/// Pose le résumé sur le jeu de l'état et envoie `game-achievements` s'il change.
fn publish(app: &AppHandle, id: &str, summary: Option<Achievements>) {
    let Some(state) = app.try_state::<AppState>() else { return };
    let changed = match lock(&state.games) {
        Ok(mut games) => match games.iter_mut().find(|g| g.id == id) {
            Some(g) if g.achievements != summary => {
                g.achievements = summary.clone();
                true
            }
            _ => false,
        },
        Err(_) => false,
    };
    if changed {
        let _ = app.emit(EVENT, serde_json::json!({ "id": id, "achievements": summary }));
    }
}

/// Après un scan : relit les jeux GOG dont la liste est vieille, un à la fois, sans tenir de verrou
/// pendant les requêtes. Rien sans compte GOG connecté.
pub fn resolve(app: AppHandle) {
    let Some(ours) = get() else { return };
    if crate::gog_online::diag_state(&app) != crate::gog_online::AccountState::Connected || to_fetch(&app, None).is_empty() || ours.busy.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        for (i, (id, product)) in to_fetch(&app, None).into_iter().enumerate() {
            if i > 0 {
                std::thread::sleep(GAP);
            }
            if !fetch_one(&app, &id, product) {
                break;
            }
        }
        ours.busy.store(false, Ordering::SeqCst);
    });
}

/// Compte GOG déconnecté : cache effacé, résumés retirés des jeux GOG (front prévenu).
pub fn forget(app: &AppHandle) {
    let Some(ours) = get() else { return };
    if let Ok(mut cache) = lock(&ours.cache) {
        cache.games.clear();
        let _ = std::fs::remove_file(&cache.path);
    }
    let ids: Vec<String> = app
        .try_state::<AppState>()
        .and_then(|state| lock(&state.games).ok().map(|games| games.iter().filter(|g| product_of(g).is_some() && g.achievements.is_some()).map(|g| g.id.clone()).collect()))
        .unwrap_or_default();
    for id in ids {
        publish(app, &id, None);
    }
}

/// Depuis la fiche (déjà hors du fil principal) : relit ce jeu si sa liste a plus de 10 minutes.
pub fn refresh_game(app: &AppHandle, id: &str) {
    if crate::gog_online::diag_state(app) != crate::gog_online::AccountState::Connected {
        return;
    }
    if let Some((id, product)) = to_fetch(app, Some(id)).into_iter().next() {
        fetch_one(app, &id, product);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Launch;
    use std::io::{BufRead, BufReader, Write};

    const SAMPLE: &str = include_str!("gog_achievements_samples/achievements.json");

    #[test]
    fn reads_the_recorded_shape() {
        let items = parse(SAMPLE).unwrap();
        // 6 entrées : une sans clé écartée, une en double écartée.
        assert_eq!(items.len(), 4);
        let first = &items[0];
        assert_eq!((first.key.as_str(), first.name.as_str()), ("FIRST_BLOOD", "Premier sang"));
        assert!(first.unlocked && first.unlocked_at == Some(1_571_842_525) && first.visible);
        assert_eq!(first.rarity, Some(62.5));
        assert_eq!(first.icon.as_deref(), Some("https://images.gog-statics.com/0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0_gac_60.png"));
        assert_eq!(first.icon_locked.as_deref(), Some("https://images.gog-statics.com/aaaa_gac_60.png"));
        // Date sans deux-points dans le décalage, rareté hors bornes écartée, icônes piégées écartées.
        let second = &items[1];
        assert_eq!(second.unlocked_at, Some(1_600_000_000));
        assert_eq!(second.rarity, None);
        assert_eq!((second.icon.as_deref(), second.icon_locked.as_deref()), (None, None));
        // Caché, pas débloqué ; nom nettoyé (contrôles et espaces).
        let secret = &items[2];
        assert!(!secret.visible && !secret.unlocked && secret.unlocked_at.is_none());
        assert_eq!(secret.name, "Le secret");
        // Id numérique à défaut de clé ; texte vide = clé comme nom.
        assert_eq!((items[3].key.as_str(), items[3].name.as_str()), ("12345", "12345"));
        assert_eq!(parse("{}"), None);
        assert_eq!(parse("<html>"), None);
        assert_eq!(parse(r#"{"items":[]}"#), Some(Vec::new()));
    }

    #[test]
    fn icons_stay_on_the_gog_cdn() {
        assert_eq!(image_url("https://images-3.gog-statics.com/abc_gac_60.png").as_deref(), Some("https://images.gog-statics.com/abc_gac_60.png"));
        assert_eq!(image_url("https://images.gog.com/abc.png").as_deref(), Some("https://images.gog.com/abc.png"));
        for bad in [
            "http://images.gog-statics.com/a.png",
            "https://images.gog-statics.com.evil.com/a.png",
            "https://evil.com/images.gog-statics.com/a.png",
            "https://images.gog-statics.com/a.png?x=1",
            "https://images.gog-statics.com/a.png#x",
            "https://images.gog-statics.com:8443/a.png",
            "https://user@images.gog-statics.com/a.png",
            "https://images.gog-statics.com/dir/a.png",
            "https://images.gog-statics.com/..a.png",
            "https://images.gog-statics.com/",
            "https://images-x.gog-statics.com/a.png",
            "https://images-123.gog-statics.com/a.png",
            "javascript:alert(1)",
            "",
        ] {
            assert_eq!(image_url(bad), None, "{bad}");
        }
    }

    #[test]
    fn summary_details_and_hidden_rule() {
        let dir = std::env::temp_dir().join(format!("ludex-gog-ach-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut cache = Cache::load(&dir);
        cache.record(7, parse(SAMPLE), 1_000, "fr-FR");
        let s = cache.summary(7).unwrap();
        assert_eq!((s.store, s.unlocked, s.total, s.last_unlocked), (AchievementStore::Gog, 2, 4, Some(1_600_000_000)));
        assert_eq!(s.unlock_times, vec![1_571_842_525, 1_600_000_000]);
        let list = cache.details(7).unwrap();
        assert!(list.iter().all(|a| a.store == AchievementStore::Gog));
        assert_eq!(list[0].global_percent, Some(62.5));
        let secret = &list[2];
        assert!(secret.hidden && !secret.unlocked && secret.name.is_empty() && secret.description.is_none() && secret.icon.is_none() && secret.icon_gray.is_none());
        assert_eq!(list[0].icon_gray.as_deref(), Some("https://images.gog-statics.com/aaaa_gac_60.png"));

        // Réponse vide plus tard : la liste connue reste. Autre jeu sans succès : rien.
        cache.record(7, Some(Vec::new()), 2_000, "fr-FR");
        assert_eq!(cache.summary(7).map(|s| s.total), Some(4));
        cache.record(8, None, 2_000, "fr-FR");
        assert_eq!(cache.summary(8), None);

        // Relecture : après 24 h, joué depuis, autre langue.
        assert!(!cache.stale(7, None, 2_000 + REFRESH_AFTER, "fr-FR"));
        assert!(cache.stale(7, None, 2_001 + REFRESH_AFTER, "fr-FR"));
        assert!(cache.stale(7, Some(2_500), 3_000, "fr-FR"));
        assert!(cache.stale(7, None, 2_000, "en-US"));
        assert!(cache.stale(9, None, 0, "fr-FR"));

        // Sauvegarde puis relecture, icône modifiée à la main écartée.
        cache.save().unwrap();
        let text = std::fs::read_to_string(dir.join(FILE)).unwrap().replace("https://images.gog-statics.com/aaaa_gac_60.png", "https://evil.example/x.png");
        std::fs::write(dir.join(FILE), text).unwrap();
        let reloaded = Cache::load(&dir);
        assert_eq!(reloaded.summary(7), cache.summary(7));
        assert_eq!(reloaded.details(7).unwrap()[0].icon_gray.as_deref(), Some(list[0].icon.as_deref().unwrap()));
        // Autre version : tout est oublié.
        std::fs::write(dir.join(FILE), r#"{"version":0,"games":{"7":{"checked":1,"lang":"fr-FR","items":[]}}}"#).unwrap();
        assert!(Cache::load(&dir).games.is_empty());
        let _ = std::fs::remove_dir_all(&dir);

        // Résumé posé sur un jeu GOG seulement.
        let mut gog = Game::new(Platform::Gog, "7", "Jeu", Launch::Uri(String::new()));
        cache.apply(&mut gog);
        assert_eq!(gog.achievements.map(|a| a.total), Some(4));
        let mut steam = Game::new(Platform::Steam, "7", "Jeu", Launch::Uri(String::new()));
        cache.apply(&mut steam);
        assert!(steam.achievements.is_none());
        let odd = Game::new(Platform::Gog, "7a", "Jeu", Launch::Uri(String::new()));
        assert_eq!(product_of(&odd), None);
    }

    /// Serveur local : une réponse par connexion, renvoie la première ligne et les en-têtes lus.
    fn server(replies: Vec<(u16, &'static str)>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for (status, body) in replies {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut lines = Vec::new();
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    lines.push(line.trim().to_string());
                }
                seen.push(lines.join(" | "));
                let _ = write!(stream, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
            seen
        });
        (base, handle)
    }

    #[test]
    fn requests_with_the_account_token() {
        let (base, handle) = server(vec![(200, SAMPLE), (404, "{}"), (401, "{}"), (500, "{}"), (200, "<html>"), (302, "")]);
        let agent = api::agent();
        let got = request(&agent, &base, "4242", 7, "jeton", "fr-FR").unwrap();
        assert!(matches!(got, Outcome::Items(ref l) if l.len() == 4));
        assert_eq!(request(&agent, &base, "4242", 7, "jeton", "fr-FR"), Ok(Outcome::Refused(404)));
        // 401 : refusé pour ce jeu, jamais « compte à reconnecter ».
        assert_eq!(request(&agent, &base, "4242", 7, "jeton", "fr-FR"), Ok(Outcome::Refused(401)));
        let server_error = request(&agent, &base, "4242", 7, "jeton", "fr-FR").unwrap_err();
        assert_eq!(server_error, Failure::Status(500));
        assert!(!server_error.message().contains("127.0.0.1") && !server_error.message().contains("jeton"));
        assert!(matches!(request(&agent, &base, "4242", 7, "jeton", "fr-FR"), Err(Failure::Network(_))));
        // Redirection jamais suivie.
        assert_eq!(request(&agent, &base, "4242", 7, "jeton", "fr-FR"), Err(Failure::Status(302)));
        // Compte piégé : aucune requête.
        assert!(request(&agent, &base, "42/../x", 7, "jeton", "fr-FR").is_err());
        let seen = handle.join().unwrap();
        assert!(seen[0].starts_with("GET /clients/7/users/4242/achievements HTTP/1.1"), "{}", seen[0]);
        let lower = seen[0].to_ascii_lowercase();
        assert!(lower.contains("authorization: bearer jeton") && lower.contains("x-gog-lc: fr-fr"));
    }
}
