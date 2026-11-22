//! Jeux Steam possédés pas installés (section « Pas installés »), sans compte par défaut.
//!
//! - Liste locale, toujours disponible : appids que `localconfig.vdf` connaît pour le compte de ce
//!   PC (jeux déjà lancés ou ouverts sur ce compte, installés ou non), triés par le type que
//!   `appcache/appinfo.vdf` donne (`game` seulement : ni DLC, ni outils, ni bandes originales).
//!   Incomplète par nature : un jeu acheté jamais lancé sur ce compte n'y est pas.
//! - Liste complète, facultative : clé Web API Steam personnelle collée dans les Paramètres
//!   (« Comptes »), `IPlayerService/GetOwnedGames` pour le SteamID64 du compte de ce PC. La clé est
//!   chiffrée par DPAPI dans le coffre partagé (`gog_online::vault`, `steam_key`), jamais envoyée
//!   au front, jamais dans le journal (erreurs sans URL : l'URL porte la clé), le Diagnostic,
//!   `user-data.json` ni l'export. « Retirer la clé » l'efface avec le cache.
//! - Installation : `steam://install/<appid>` reconstruite ici depuis un appid numérique de la
//!   liste ; Steam demande le dossier et télécharge. Ludex ne télécharge rien pour Steam.

pub mod appinfo;

use crate::gog_online::vault::{self, SteamKey};
use crate::gog_online::AccountState;
use crate::{blocking, lock, AppState};
use appinfo::AppInfo;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

/// Événement : état de la clé changé.
pub const KEY_EVENT: &str = "steam-key";
/// Cache de la liste complète (aucune clé), effacé avec la clé.
const CACHE_FILE: &str = "steam-owned-cache.json";
const CACHE_VERSION: u32 = 1;
/// Liste complète relue après 6 h.
const OWNED_TTL: u64 = 6 * 3600;
const API_BASE: &str = "https://api.steampowered.com";
/// Page de Steam où l'utilisateur obtient sa clé (ouverte par Rust, jamais une adresse du front).
const KEY_PAGE: &str = "https://steamcommunity.com/dev/apikey";
/// Réponse lue sur 16 Mo au plus, 50 000 jeux au plus.
const MAX_BODY: u64 = 16 * 1024 * 1024;
const MAX_GAMES: usize = 50_000;
/// `appinfo.vdf` plus gros : non lu (jamais vu au-delà de quelques centaines de Mo).
const MAX_APPINFO: u64 = 1024 * 1024 * 1024;

fn now() -> u64 {
    SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Clé collée par l'utilisateur : 32 chiffres hexadécimaux (espaces autour ignorés), en majuscules.
pub fn key_ok(raw: &str) -> Option<String> {
    let key = raw.trim();
    (key.len() == 32 && key.bytes().all(|b| b.is_ascii_hexdigit())).then(|| key.to_ascii_uppercase())
}

/// État de la clé en une ligne, pour le rapport du Diagnostic (jamais la clé ni le SteamID).
pub fn state_text(state: AccountState) -> String {
    match state {
        AccountState::Off => tr!("absente (liste locale)", "none (local list)"),
        AccountState::Connected => tr!("enregistrée", "saved"),
        AccountState::Expired => tr!("refusée par Steam", "refused by Steam"),
    }
}

// ---------------------------------------------------------------------------------------------
// API Web de Steam
// ---------------------------------------------------------------------------------------------

/// Erreur d'une requête, sans l'URL (elle porte la clé) ni le corps.
#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
    /// 401 ou 403 : clé invalide, révoquée ou d'un autre compte.
    Refused,
    Status(u16),
    Network(String),
    Invalid,
}

impl Failure {
    pub fn message(&self) -> String {
        match self {
            Failure::Refused => tr!("Steam a refusé la clé Web API.", "Steam refused the Web API key."),
            Failure::Status(code) => tr!("Réponse {} de Steam.", "Response {} from Steam.", code),
            Failure::Network(kind) => tr!("Steam ne répond pas ({}).", "Steam isn't responding ({}).", kind),
            Failure::Invalid => tr!("Réponse illisible de Steam.", "Unreadable response from Steam."),
        }
    }
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(30))
        .redirects(0)
        .user_agent(concat!("Ludex/", env!("CARGO_PKG_VERSION"), " (lanceur de jeux)"))
        .build()
}

/// Jeu de la liste complète.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Owned {
    pub appid: u32,
    pub name: String,
}

#[derive(Deserialize)]
struct OwnedReply {
    response: OwnedBody,
}

#[derive(Deserialize)]
struct OwnedBody {
    #[serde(default)]
    games: Vec<OwnedRaw>,
}

#[derive(Deserialize)]
struct OwnedRaw {
    appid: u32,
    #[serde(default)]
    name: String,
}

/// Lit la réponse de `GetOwnedGames` : appids non nuls, sans doublon, noms nettoyés (120 caractères).
/// `response` sans `games` (profil privé, ou rien) = liste vide.
pub fn parse_owned(text: &str) -> Result<Vec<Owned>, Failure> {
    let reply: OwnedReply = serde_json::from_str(text).map_err(|_| Failure::Invalid)?;
    let mut seen = HashSet::new();
    Ok(reply
        .response
        .games
        .into_iter()
        .filter(|g| g.appid > 0 && seen.insert(g.appid))
        .take(MAX_GAMES)
        .map(|g| Owned { appid: g.appid, name: crate::library::manual::clean_text(&g.name, 120).unwrap_or_default() })
        .collect())
}

/// `GetOwnedGames` du compte `steamid` (jeux gratuits joués compris, noms demandés).
fn fetch_owned(agent: &ureq::Agent, base: &str, key: &SteamKey, steamid: u64) -> Result<Vec<Owned>, Failure> {
    let url = format!("{base}/IPlayerService/GetOwnedGames/v1/");
    let response = agent
        .get(&url)
        .query("key", &key.0)
        .query("steamid", &steamid.to_string())
        .query("include_appinfo", "1")
        .query("include_played_free_games", "1")
        .query("format", "json")
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(401 | 403, _) => Failure::Refused,
            ureq::Error::Status(code, _) => Failure::Status(code),
            ureq::Error::Transport(t) => Failure::Network(format!("{:?}", t.kind())),
        })?;
    if response.status() != 200 {
        return Err(Failure::Status(response.status()));
    }
    let mut text = String::new();
    response.into_reader().take(MAX_BODY).read_to_string(&mut text).map_err(|_| Failure::Invalid)?;
    parse_owned(&text)
}

// ---------------------------------------------------------------------------------------------
// Liste des jeux pas installés
// ---------------------------------------------------------------------------------------------

/// Jeu Steam possédé non installé, montré au front.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnedGame {
    pub id: u32,
    pub title: String,
    pub image_url: String,
}

/// D'où vient la liste.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// `localconfig.vdf` : jeux déjà lancés sur ce compte.
    Local,
    /// `GetOwnedGames` avec la clé.
    Key,
}

/// Liste locale : appids de `localconfig.vdf` dont `appinfo.vdf` dit `game` (type inconnu = écarté :
/// le fichier liste aussi outils, DLC et bandes originales), nom tiré d'`appinfo.vdf`.
pub fn local_candidates(apps: &[u32], info: &HashMap<u32, AppInfo>) -> Vec<Owned> {
    apps.iter().filter_map(|a| info.get(a).filter(|i| i.is_game()).map(|i| Owned { appid: *a, name: i.name.clone() })).collect()
}

/// Liste complète : jeux de `GetOwnedGames`, moins ceux qu'`appinfo.vdf` connaît sous un autre type
/// (type inconnu = gardé : l'API ne renvoie en principe que des jeux) ; nom de l'API, sinon d'`appinfo.vdf`.
pub fn key_candidates(owned: &[Owned], info: &HashMap<u32, AppInfo>) -> Vec<Owned> {
    owned
        .iter()
        .filter(|o| info.get(&o.appid).is_none_or(AppInfo::is_game))
        .map(|o| {
            let name = if o.name.is_empty() { info.get(&o.appid).map(|i| i.name.clone()).unwrap_or_default() } else { o.name.clone() };
            Owned { appid: o.appid, name }
        })
        .collect()
}

/// Candidats moins les jeux déjà sur ce PC (`installed` : appids des jeux Steam et appids Steam
/// retrouvés pour les autres sources), sans nom ni doublon, triés par titre.
pub fn not_installed(candidates: Vec<Owned>, installed: &HashSet<u32>) -> Vec<OwnedGame> {
    let mut seen = HashSet::new();
    let mut list: Vec<OwnedGame> = candidates
        .into_iter()
        .filter(|c| !installed.contains(&c.appid) && seen.insert(c.appid))
        .filter_map(|c| {
            let title = crate::library::manual::clean_text(&c.name, 120)?;
            Some(OwnedGame { id: c.appid, title, image_url: crate::wishlist::image_url(c.appid) })
        })
        .collect();
    list.sort_by_cached_key(|g| (g.title.to_lowercase(), g.id));
    list
}

// ---------------------------------------------------------------------------------------------
// État
// ---------------------------------------------------------------------------------------------

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Cache {
    #[serde(default)]
    version: u32,
    /// Compte à qui appartient la liste (un autre compte Steam sur ce PC repart de zéro).
    #[serde(default)]
    steamid: Option<u64>,
    #[serde(default)]
    games: Vec<Owned>,
    #[serde(default)]
    checked: Option<u64>,
}

impl Cache {
    fn empty() -> Cache {
        Cache { version: CACHE_VERSION, ..Default::default() }
    }

    fn load(path: &Path) -> Cache {
        std::fs::read(path).ok().and_then(|b| serde_json::from_slice::<Cache>(&b).ok()).filter(|c| c.version == CACHE_VERSION).unwrap_or_else(Cache::empty)
    }

    fn save(&self, path: &Path) {
        let Ok(bytes) = serde_json::to_vec(self) else { return };
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, bytes).and_then(|_| std::fs::rename(&tmp, path)).is_err() {
            let _ = std::fs::remove_file(&tmp);
            log::warn!("Cache des jeux Steam possédés non enregistré");
        }
    }
}

/// `appinfo.vdf` lu, gardé tant que sa taille et sa date ne changent pas.
struct AppinfoCache {
    len: u64,
    modified: SystemTime,
    apps: Arc<HashMap<u32, AppInfo>>,
}

/// État de la clé, envoyé au front (jamais la clé).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyView {
    pub state: AccountState,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    pub key: KeyView,
    pub source: Source,
    pub games: Vec<OwnedGame>,
    /// Jeux connus de la liste (avant de retirer ceux qui sont installés).
    pub owned: usize,
    /// Dernière lecture de la liste complète (`null` pour la liste locale).
    pub checked: Option<u64>,
    pub error: Option<String>,
    /// Steam est installé et un compte y est connu (sinon ni liste locale ni installation).
    pub steam: bool,
}

/// Jeux Steam possédés, géré par Tauri.
pub struct SteamOwned {
    vault_path: PathBuf,
    cache_path: PathBuf,
    base: String,
    key: Mutex<Option<SteamKey>>,
    refused: AtomicBool,
    last_error: Mutex<Option<String>>,
    cache: Mutex<Cache>,
    appinfo: Mutex<Option<AppinfoCache>>,
    api_lock: Mutex<()>,
}

impl SteamOwned {
    pub fn new(data_dir: &Path) -> SteamOwned {
        let vault_path = vault::path_in(data_dir);
        let key = match vault::load_steam_key(&vault_path) {
            Ok(k) => k,
            Err(e) => {
                log::warn!("Clé Web API Steam : {e}");
                None
            }
        };
        SteamOwned {
            vault_path,
            cache_path: data_dir.join(CACHE_FILE),
            base: API_BASE.to_string(),
            key: Mutex::new(key),
            refused: AtomicBool::new(false),
            last_error: Mutex::new(None),
            cache: Mutex::new(Cache::load(&data_dir.join(CACHE_FILE))),
            appinfo: Mutex::new(None),
            api_lock: Mutex::new(()),
        }
    }

    pub fn state(&self) -> AccountState {
        match lock(&self.key) {
            Ok(k) if k.is_some() && self.refused.load(Ordering::SeqCst) => AccountState::Expired,
            Ok(k) if k.is_some() => AccountState::Connected,
            _ => AccountState::Off,
        }
    }

    fn view(&self) -> KeyView {
        KeyView { state: self.state(), error: lock(&self.last_error).ok().and_then(|e| e.clone()) }
    }

    fn set_error(&self, error: Option<String>) {
        if let Ok(mut e) = lock(&self.last_error) {
            *e = error;
        }
    }

    /// Noms et types de `appinfo.vdf`, relu seulement si le fichier a changé ; vide s'il manque.
    fn appinfo(&self) -> Arc<HashMap<u32, AppInfo>> {
        let Some(path) = crate::library::steam_appinfo_file() else { return Arc::default() };
        let Some((len, modified)) = std::fs::metadata(&path).ok().and_then(|m| Some((m.len(), m.modified().ok()?))) else { return Arc::default() };
        let Ok(mut cached) = lock(&self.appinfo) else { return Arc::default() };
        if let Some(c) = cached.as_ref().filter(|c| c.len == len && c.modified == modified) {
            return c.apps.clone();
        }
        let apps = if len > MAX_APPINFO {
            log::warn!("appinfo.vdf trop gros ({len} octets) : types des jeux Steam inconnus");
            HashMap::new()
        } else {
            match std::fs::read(&path).ok().as_deref().and_then(appinfo::parse) {
                Some(apps) => apps,
                None => {
                    log::warn!("appinfo.vdf illisible (version inconnue ?) : types des jeux Steam inconnus");
                    HashMap::new()
                }
            }
        };
        let apps = Arc::new(apps);
        *cached = Some(AppinfoCache { len, modified, apps: apps.clone() });
        apps
    }
}

fn emit(app: &AppHandle, owned: &SteamOwned) {
    let _ = app.emit(KEY_EVENT, owned.view());
}

/// État de la clé pour le Diagnostic.
pub fn diag_state(app: &AppHandle) -> AccountState {
    app.try_state::<SteamOwned>().map(|s| s.state()).unwrap_or_default()
}

/// Appids déjà sur ce PC : jeux Steam (masqués compris) et appid Steam des jeux des autres sources
/// (un GTA V installé par Rockstar n'est pas proposé à l'installation sur Steam).
fn installed_appids(state: &AppState) -> HashSet<u32> {
    lock(&state.games)
        .map(|games| {
            games
                .iter()
                .filter_map(|g| if g.platform == crate::library::Platform::Steam { g.platform_id.parse().ok() } else { g.steam_appid })
                .collect()
        })
        .unwrap_or_default()
}

/// Liste à montrer : la complète si la clé est enregistrée et la liste lue pour ce compte, sinon la locale.
fn library_view(app: &AppHandle, owned_state: &SteamOwned, error: Option<String>) -> Result<Library, String> {
    let installed = installed_appids(&app.state::<AppState>());
    let account = crate::library::steam_local_account_apps();
    let info = owned_state.appinfo();
    let full = {
        let cache = lock(&owned_state.cache)?;
        let mine = account.as_ref().is_some_and(|(id, _)| cache.steamid == Some(*id));
        (owned_state.state() == AccountState::Connected && mine && cache.checked.is_some()).then(|| (cache.games.clone(), cache.checked))
    };
    let (source, candidates, checked) = match full {
        Some((games, checked)) => (Source::Key, key_candidates(&games, &info), checked),
        None => (Source::Local, account.as_ref().map(|(_, apps)| local_candidates(apps, &info)).unwrap_or_default(), None),
    };
    let owned = candidates.len();
    Ok(Library {
        key: owned_state.view(),
        source,
        games: not_installed(candidates, &installed),
        owned,
        checked,
        error,
        steam: account.is_some(),
    })
}

/// Relit la liste complète (clé enregistrée, compte connu). Clé refusée = état « refusée », liste
/// gardée en cache mais plus montrée (la locale la remplace).
fn sync(app: &AppHandle, owned_state: &SteamOwned, steamid: u64) -> Result<(), String> {
    let _guard = owned_state.api_lock.lock().unwrap_or_else(|e| e.into_inner());
    let Some(key) = lock(&owned_state.key)?.clone() else { return Ok(()) };
    match fetch_owned(&agent(), &owned_state.base, &key, steamid) {
        Ok(games) => {
            crate::diagnostic::ok(crate::diagnostic::Service::Steam);
            owned_state.refused.store(false, Ordering::SeqCst);
            owned_state.set_error(None);
            let mut cache = lock(&owned_state.cache)?;
            *cache = Cache { version: CACHE_VERSION, steamid: Some(steamid), games, checked: Some(now()) };
            cache.save(&owned_state.cache_path);
            Ok(())
        }
        Err(e) => {
            let message = if e == Failure::Refused {
                owned_state.refused.store(true, Ordering::SeqCst);
                tr!("Steam refuse la clé enregistrée : remplace-la ou retire-la.", "Steam refuses the saved key: replace or remove it.")
            } else {
                e.message()
            };
            crate::diagnostic::failed(crate::diagnostic::Service::Steam, &message);
            owned_state.set_error(Some(message.clone()));
            emit(app, owned_state);
            Err(message)
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Commandes
// ---------------------------------------------------------------------------------------------

/// Jeux Steam possédés non installés. `refresh` : liste complète relue tout de suite ; sinon après 6 h.
#[tauri::command]
pub async fn steam_owned(app: AppHandle, refresh: bool) -> Result<Library, String> {
    blocking(move || {
        let owned_state = app.state::<SteamOwned>();
        let mut error = None;
        if owned_state.state() != AccountState::Off
            && let Some((steamid, _)) = crate::library::steam_local_account_apps()
        {
            let due = {
                let cache = lock(&owned_state.cache)?;
                refresh || cache.steamid != Some(steamid) || cache.checked.is_none_or(|c| now().saturating_sub(c) >= OWNED_TTL)
            };
            if due {
                error = sync(&app, &owned_state, steamid).err();
            }
        }
        library_view(&app, &owned_state, error)
    })
    .await
}

/// Enregistre la clé collée après un essai réussi auprès de Steam (rien d'enregistré sinon).
#[tauri::command]
pub async fn steam_key_save(app: AppHandle, key: String) -> Result<Library, String> {
    blocking(move || {
        crate::collections::check_input(&key)?;
        let key = key_ok(&key).map(SteamKey).ok_or_else(|| {
            tr!("Clé invalide : une clé Web API Steam compte 32 caractères (chiffres et lettres A à F).", "Invalid key: a Steam Web API key has 32 characters (digits and letters A to F).")
        })?;
        let (steamid, _) = crate::library::steam_local_account_apps()
            .ok_or_else(|| tr!("Aucun compte Steam trouvé sur ce PC : connecte-toi une fois dans Steam.", "No Steam account found on this PC: sign in to Steam once."))?;
        let owned_state = app.state::<SteamOwned>();
        let games = {
            let _guard = owned_state.api_lock.lock().unwrap_or_else(|e| e.into_inner());
            match fetch_owned(&agent(), &owned_state.base, &key, steamid) {
                Ok(games) => games,
                Err(e) => {
                    let message = if e == Failure::Refused {
                        tr!("Steam a refusé cette clé : vérifie-la sur la page de Steam.", "Steam refused this key: check it on the Steam page.")
                    } else {
                        e.message()
                    };
                    crate::diagnostic::failed(crate::diagnostic::Service::Steam, &message);
                    return Err(message);
                }
            }
        };
        crate::diagnostic::ok(crate::diagnostic::Service::Steam);
        if games.is_empty() {
            return Err(tr!(
                "Steam n'a renvoyé aucun jeu pour ce compte : clé d'un autre compte, ou détails de jeu privés. Clé non enregistrée.",
                "Steam returned no games for this account: key from another account, or private game details. Key not saved."
            ));
        }
        vault::save_steam_key(&owned_state.vault_path, Some(&key))?;
        *lock(&owned_state.key)? = Some(key);
        owned_state.refused.store(false, Ordering::SeqCst);
        owned_state.set_error(None);
        {
            let mut cache = lock(&owned_state.cache)?;
            *cache = Cache { version: CACHE_VERSION, steamid: Some(steamid), games, checked: Some(now()) };
            cache.save(&owned_state.cache_path);
        }
        emit(&app, &owned_state);
        library_view(&app, &owned_state, None)
    })
    .await
}

/// Efface la clé du coffre et le cache de la liste complète ; la liste locale reprend.
#[tauri::command]
pub async fn steam_key_remove(app: AppHandle) -> Result<Library, String> {
    blocking(move || {
        let owned_state = app.state::<SteamOwned>();
        let _guard = owned_state.api_lock.lock().unwrap_or_else(|e| e.into_inner());
        vault::save_steam_key(&owned_state.vault_path, None)?;
        *lock(&owned_state.key)? = None;
        owned_state.refused.store(false, Ordering::SeqCst);
        owned_state.set_error(None);
        *lock(&owned_state.cache)? = Cache::empty();
        let _ = std::fs::remove_file(&owned_state.cache_path);
        emit(&app, &owned_state);
        library_view(&app, &owned_state, None)
    })
    .await
}

/// État de la clé (Paramètres).
#[tauri::command]
pub fn steam_key(owned: tauri::State<'_, SteamOwned>) -> KeyView {
    owned.view()
}

/// Ouvre la page de Steam où l'utilisateur obtient sa clé.
#[tauri::command]
pub fn steam_key_page(app: AppHandle) -> Result<(), String> {
    app.opener().open_url(KEY_PAGE, None::<&str>).map_err(|e| tr!("Impossible d'ouvrir la page de Steam : {e}", "Couldn't open the Steam page: {e}"))
}

/// Installe un jeu de la liste par Steam (`steam://install/<appid>`), qui demande le dossier et
/// télécharge lui-même. Le jeu apparaît par la surveillance des bibliothèques Steam.
#[tauri::command]
pub async fn steam_install(app: AppHandle, appid: u32) -> Result<(), String> {
    blocking(move || {
        let owned_state = app.state::<SteamOwned>();
        let view = library_view(&app, &owned_state, None)?;
        if !view.steam {
            return Err(tr!("Steam n'est pas installé sur ce PC.", "Steam isn't installed on this PC."));
        }
        if appid == 0 || !view.games.iter().any(|g| g.id == appid) {
            return Err(tr!("Ce jeu n'est pas dans tes jeux Steam à installer.", "This game isn't in your Steam games to install."));
        }
        app.opener()
            .open_url(format!("steam://install/{appid}"), None::<&str>)
            .map_err(|e| tr!("Steam n'a pas répondu : {e}", "Steam didn't respond: {e}"))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};

    const SAMPLE: &str = include_str!("samples/owned.json");

    fn info(apps: &[(u32, &str, &str)]) -> HashMap<u32, AppInfo> {
        apps.iter().map(|(a, n, k)| (*a, AppInfo { name: n.to_string(), kind: k.to_string() })).collect()
    }

    #[test]
    fn keys_are_32_hex_digits() {
        assert_eq!(key_ok("  0123456789abcdef0123456789ABCDEF \n").as_deref(), Some("0123456789ABCDEF0123456789ABCDEF"));
        assert_eq!(key_ok("0123456789abcdef0123456789abcde"), None, "31 caractères");
        assert_eq!(key_ok("0123456789abcdef0123456789abcdeg"), None, "g");
        assert_eq!(key_ok("0123456789abcdef 123456789abcdef"), None, "espace dedans");
        assert_eq!(key_ok(""), None);
    }

    #[test]
    fn reads_owned_games_reply() {
        let games = parse_owned(SAMPLE).unwrap();
        assert_eq!(games.len(), 5);
        assert_eq!(games[0], Owned { appid: 105600, name: "Terraria".into() });
        assert_eq!(games[4].name, "Half-Life 2", "espaces et contrôles nettoyés");
        assert_eq!(parse_owned(r#"{"response":{}}"#).unwrap(), Vec::new(), "profil privé");
        assert_eq!(parse_owned(r#"{"response":{"games":[{"appid":0},{"appid":7,"name":"A"},{"appid":7,"name":"B"}]}}"#).unwrap().len(), 1);
        assert_eq!(parse_owned("<html>"), Err(Failure::Invalid));
    }

    #[test]
    fn local_list_keeps_known_games_only() {
        let info = info(&[(105600, "Terraria", "game"), (228980, "Steamworks Common Redistributables", "tool"), (1145360, "Hades", "game"), (2000, "Bande originale", "music")]);
        let picked = local_candidates(&[105600, 228980, 1145360, 2000, 999], &info);
        let ids: Vec<u32> = picked.iter().map(|o| o.appid).collect();
        assert_eq!(ids, vec![105600, 1145360], "outil, musique et type inconnu écartés");
    }

    #[test]
    fn full_list_drops_known_non_games_and_installed() {
        let owned = parse_owned(SAMPLE).unwrap();
        let info = info(&[(431960, "Wallpaper Engine", "application"), (620, "Portal 2", "game")]);
        let installed: HashSet<u32> = [105600].into();
        let list = not_installed(key_candidates(&owned, &info), &installed);
        let titles: Vec<&str> = list.iter().map(|g| g.title.as_str()).collect();
        assert_eq!(titles, vec!["Hades", "Half-Life 2", "Portal 2"], "installé et application écartés, type inconnu gardé, tri par titre");
        assert_eq!(list[0].image_url, crate::wishlist::image_url(1145360));
        // Nom absent de l'API : repris d'appinfo.vdf ; sans nom nulle part : écarté.
        let unnamed = [Owned { appid: 620, name: String::new() }, Owned { appid: 5, name: String::new() }];
        let list = not_installed(key_candidates(&unnamed, &info), &HashSet::new());
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "Portal 2");
    }

    /// Serveur HTTP local : une réponse, garde la ligne de requête.
    fn server(status: u16, body: &'static str) -> (String, std::thread::JoinHandle<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            reader.read_line(&mut first).unwrap();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
            }
            let _ = write!(stream, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            first
        });
        (base, handle)
    }

    #[test]
    fn owned_games_request_and_refusal_without_key_in_message() {
        let key = SteamKey("0123456789ABCDEF0123456789ABCDEF".into());
        let (base, srv) = server(200, SAMPLE);
        let games = fetch_owned(&agent(), &base, &key, 76_561_197_960_265_729).unwrap();
        assert_eq!(games.len(), 5);
        let line = srv.join().unwrap();
        assert!(line.starts_with("GET /IPlayerService/GetOwnedGames/v1/?"), "{line}");
        assert!(line.contains("key=0123456789ABCDEF0123456789ABCDEF") && line.contains("steamid=76561197960265729") && line.contains("include_appinfo=1"));

        for status in [401, 403] {
            let (base, srv) = server(status, "<html>Forbidden</html>");
            let err = fetch_owned(&agent(), &base, &key, 1).unwrap_err();
            srv.join().unwrap();
            assert_eq!(err, Failure::Refused);
            assert!(!err.message().contains(&key.0));
        }
        let (base, srv) = server(500, "");
        let err = fetch_owned(&agent(), &base, &key, 1).unwrap_err();
        srv.join().unwrap();
        assert_eq!(err, Failure::Status(500));
        assert!(!format!("{err:?} {}", err.message()).contains(&key.0));
        // Réseau coupé : le message ne contient ni l'adresse ni la clé.
        let err = fetch_owned(&agent(), "http://127.0.0.1:9", &key, 1).unwrap_err();
        assert!(matches!(err, Failure::Network(_)));
        assert!(!err.message().contains(&key.0) && !err.message().contains("127.0.0.1"));
    }

    #[test]
    fn cache_never_holds_the_key() {
        let dir = std::env::temp_dir().join(format!("ludex-steam-owned-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(CACHE_FILE);
        let cache = Cache { version: CACHE_VERSION, steamid: Some(42), games: parse_owned(SAMPLE).unwrap(), checked: Some(7) };
        cache.save(&path);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.to_ascii_lowercase().contains("key"));
        let back = Cache::load(&path);
        assert_eq!((back.steamid, back.games.len(), back.checked), (Some(42), 5, Some(7)));
        std::fs::write(&path, r#"{"version":99,"steamid":1}"#).unwrap();
        assert_eq!(Cache::load(&path).steamid, None, "autre version : repart de zéro");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Liste locale réelle de ce PC (lecture seule : ni coffre, ni cache, ni réseau ; aucun SteamID affiché).
    #[test]
    #[ignore]
    fn steam_owned_this_pc() {
        let start = std::time::Instant::now();
        let path = crate::library::steam_appinfo_file().expect("Steam absent");
        let bytes = std::fs::read(&path).expect("appinfo.vdf illisible");
        let read = start.elapsed();
        let info = appinfo::parse(&bytes).expect("version d'appinfo.vdf inconnue");
        println!("appinfo.vdf : {} Mo, version 0x{:02x}, {} applications ({} jeux), lu en {:?}, analysé en {:?}",
            bytes.len() / 1_048_576, bytes[0], info.len(), info.values().filter(|i| i.is_game()).count(), read, start.elapsed() - read);
        let mut kinds: HashMap<&str, usize> = HashMap::new();
        for i in info.values() {
            *kinds.entry(i.kind.as_str()).or_default() += 1;
        }
        let mut kinds: Vec<_> = kinds.into_iter().collect();
        kinds.sort_by_key(|a| std::cmp::Reverse(a.1));
        println!("types : {kinds:?}");
        let (_, apps) = crate::library::steam_local_account_apps().expect("aucun compte Steam");
        let candidates = local_candidates(&apps, &info);
        let installed: HashSet<u32> = crate::library::scan().games.iter().filter_map(|g| if g.platform == crate::library::Platform::Steam { g.platform_id.parse().ok() } else { g.steam_appid }).collect();
        let list = not_installed(candidates.clone(), &installed);
        println!("localconfig.vdf : {} appids, {} jeux, {} installés sur ce PC, {} pas installés", apps.len(), candidates.len(), installed.len(), list.len());
        for g in list.iter().take(25) {
            println!("  {} ({})", g.title, g.id);
        }
    }
}
