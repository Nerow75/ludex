//! Compte Epic Games facultatif, sur le modèle exact du compte GOG (`gog_online`) : connexion par
//! la page d'Epic dans une fenêtre dédiée, jeux possédés pas encore installés sur ce PC,
//! installation confiée au launcher Epic (Ludex ne télécharge rien pour Epic).
//!
//! - Connexion coupée par défaut, lancée depuis les Paramètres (« Comptes »). La fenêtre
//!   `epic-login` est créée par Rust, en navigation privée, sans capability Tauri (aucune commande
//!   de Ludex depuis les pages d'Epic), fenêtres surgissantes refusées, `https` seulement.
//! - Le code d'autorisation n'est pas dans une URL chez Epic : la page
//!   `www.epicgames.com/id/api/redirect` l'affiche dans un JSON. Seulement sur cette page exacte,
//!   Ludex lance un court script qui lit `authorizationCode` et le remet par une navigation vers
//!   `https://ludex-epic.invalid/code?code=…`, que Ludex annule (rien ne part sur le réseau). Le
//!   mot de passe est saisi chez Epic, jamais vu par Ludex.
//! - Client public du launcher Epic (comme legendary et Heroic, voir `api.rs`) : seule façon de
//!   lire la bibliothèque d'un compte sans inscription d'application chez Epic.
//! - Jetons chiffrés par DPAPI dans le coffre partagé (`gog_online::vault`, case `Epic`), jamais
//!   envoyés au front, jamais dans le journal, le Diagnostic, `user-data.json` ni l'export.
//! - Le front n'envoie qu'un `appName` d'Epic, vérifié contre la liste des jeux possédés non
//!   installés ; l'URI du launcher est reconstruite ici.

pub mod api;

use crate::gog_online::vault::{self, Session, Slot};
use crate::gog_online::{AccountState, AccountView};
use crate::{blocking, lock, AppState};
use api::{Endpoints, Failure, Grant, Handoff, Item, Record};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

/// Fenêtre de connexion (aucune capability ne la nomme).
pub const LOGIN_LABEL: &str = "epic-login";
/// Événement : état du compte changé.
pub const ACCOUNT_EVENT: &str = "epic-account";
/// Cache de la bibliothèque et des fiches (aucun jeton), effacé à la déconnexion.
const CACHE_FILE: &str = "epic-cache.json";
const CACHE_VERSION: u32 = 1;
/// Bibliothèque relue après 6 h ; fiches après 30 jours.
const OWNED_TTL: u64 = 6 * 3600;
const ITEM_TTL: u64 = 30 * 86_400;

/// État du compte en une ligne, pour le rapport du Diagnostic (jamais d'identifiant).
pub fn state_text(state: AccountState) -> String {
    match state {
        AccountState::Off => tr!("non connecté", "not signed in"),
        AccountState::Connected => tr!("connecté", "signed in"),
        AccountState::Expired => tr!("à reconnecter (jeton refusé par Epic)", "needs signing in again (token refused by Epic)"),
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedItem {
    item: Option<Item>,
    fetched: u64,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Cache {
    #[serde(default)]
    version: u32,
    /// Compte à qui appartient la liste (une autre connexion repart de zéro).
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    records: Vec<Record>,
    #[serde(default)]
    checked: Option<u64>,
    /// Fiches par `catalogItemId`.
    #[serde(default)]
    items: BTreeMap<String, CachedItem>,
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
            log::warn!("Cache des jeux Epic non enregistré");
        }
    }

    /// Entrées dont la fiche manque ou a plus de 30 jours, groupées par espace de noms.
    fn stale(&self, now: u64) -> BTreeMap<String, Vec<String>> {
        let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for r in &self.records {
            if self.items.get(&r.catalog_item_id).is_none_or(|c| now.saturating_sub(c.fetched) >= ITEM_TTL) {
                let ids = out.entry(r.namespace.clone()).or_default();
                if !ids.contains(&r.catalog_item_id) {
                    ids.push(r.catalog_item_id.clone());
                }
            }
        }
        out
    }
}

/// Jeu Epic possédé non installé, montré au front.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnedGame {
    /// `appName` d'Epic.
    pub id: String,
    pub title: String,
    pub image_url: Option<String>,
}

/// Jeux possédés non installés : les jeux pour Windows de la bibliothèque (fiche connue), moins
/// ceux que la source Epic voit déjà (`AppName` des manifestes), sans doublon de titre, par titre.
pub fn not_installed(records: &[Record], items: &BTreeMap<String, Item>, installed: &HashSet<String>) -> Vec<OwnedGame> {
    let mut titles = HashSet::new();
    let mut list: Vec<OwnedGame> = records
        .iter()
        .filter(|r| !installed.contains(&r.app_name))
        .filter_map(|r| items.get(&r.catalog_item_id).map(|i| (r, i)))
        .filter(|(_, i)| i.game && i.windows)
        .filter(|(_, i)| titles.insert(i.title.to_lowercase()))
        .map(|(r, i)| OwnedGame { id: r.app_name.clone(), title: i.title.clone(), image_url: i.image.as_ref().and_then(api::image_url) })
        .collect();
    list.sort_by_cached_key(|g| g.title.to_lowercase());
    list
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    pub account: AccountView,
    pub games: Vec<OwnedGame>,
    /// Entrées de la bibliothèque (DLC et contenus compris).
    pub owned: usize,
    pub checked: Option<u64>,
    pub error: Option<String>,
    /// Le launcher Epic est installé (sinon « Installer » ne peut rien ouvrir).
    pub launcher: bool,
}

/// État du compte Epic, géré par Tauri.
pub struct Epic {
    vault_path: PathBuf,
    cache_path: PathBuf,
    ends: Endpoints,
    session: Mutex<Option<Session>>,
    expired: AtomicBool,
    login_open: AtomicBool,
    exchanging: AtomicBool,
    last_error: Mutex<Option<String>>,
    cache: Mutex<Cache>,
    api_lock: Mutex<()>,
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl Epic {
    pub fn new(data_dir: &Path) -> Epic {
        let vault_path = vault::path_in(data_dir);
        let session = match vault::load_slot(&vault_path, Slot::Epic) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Compte Epic : {e}");
                None
            }
        };
        Epic {
            vault_path,
            cache_path: data_dir.join(CACHE_FILE),
            ends: Endpoints::default(),
            session: Mutex::new(session),
            expired: AtomicBool::new(false),
            login_open: AtomicBool::new(false),
            exchanging: AtomicBool::new(false),
            last_error: Mutex::new(None),
            cache: Mutex::new(Cache::load(&data_dir.join(CACHE_FILE))),
            api_lock: Mutex::new(()),
        }
    }

    pub fn state(&self) -> AccountState {
        match lock(&self.session) {
            Ok(s) if s.is_some() && self.expired.load(Ordering::SeqCst) => AccountState::Expired,
            Ok(s) if s.is_some() => AccountState::Connected,
            _ => AccountState::Off,
        }
    }

    fn view(&self) -> AccountView {
        AccountView {
            state: self.state(),
            username: lock(&self.session).ok().and_then(|s| s.as_ref().and_then(|s| s.username.clone())),
            login_open: self.login_open.load(Ordering::SeqCst),
            error: lock(&self.last_error).ok().and_then(|e| e.clone()),
        }
    }

    fn set_error(&self, error: Option<String>) {
        if let Ok(mut e) = lock(&self.last_error) {
            *e = error;
        }
    }

    fn store(&self, session: Option<Session>) -> Result<(), String> {
        vault::save_slot(&self.vault_path, Slot::Epic, session.as_ref())?;
        *lock(&self.session)? = session;
        Ok(())
    }

    /// Requête au compte avec un jeton valide : rafraîchi s'il expire, puis une seconde fois si
    /// Epic le refuse. Rafraîchissement refusé = « à reconnecter », sans boucle.
    fn with_token<T>(&self, app: &AppHandle, agent: &ureq::Agent, call: impl Fn(&Session) -> Result<T, Failure>) -> Result<T, String> {
        let _guard = self.api_lock.lock().unwrap_or_else(|e| e.into_inner());
        if self.expired.load(Ordering::SeqCst) {
            return Err(Failure::Unauthorized.message());
        }
        let mut session = lock(&self.session)?.clone().ok_or_else(|| tr!("Aucun compte Epic connecté.", "No Epic account signed in."))?;
        let mut refreshed = false;
        if api::needs_refresh(&session, now()) {
            session = self.refresh(app, agent, &session)?;
            refreshed = true;
        }
        match call(&session) {
            Err(Failure::Unauthorized) if !refreshed => {
                let session = self.refresh(app, agent, &session)?;
                call(&session).map_err(|e| self.failed(app, e))
            }
            other => other.map_err(|e| self.failed(app, e)),
        }
    }

    fn refresh(&self, app: &AppHandle, agent: &ureq::Agent, session: &Session) -> Result<Session, String> {
        match api::token(agent, &self.ends, Grant::Refresh(&session.refresh_token), now(), Some(session)) {
            Ok(fresh) => {
                self.store(Some(fresh.clone()))?;
                crate::diagnostic::ok(crate::diagnostic::Service::Epic);
                Ok(fresh)
            }
            Err(e) => Err(self.failed(app, e)),
        }
    }

    fn failed(&self, app: &AppHandle, e: Failure) -> String {
        let message = e.message();
        crate::diagnostic::failed(crate::diagnostic::Service::Epic, &message);
        if e == Failure::Unauthorized && !self.expired.swap(true, Ordering::SeqCst) {
            let _ = app.emit(ACCOUNT_EVENT, self.view());
        }
        message
    }
}

/// État du compte pour le Diagnostic.
pub fn diag_state(app: &AppHandle) -> AccountState {
    app.try_state::<Epic>().map(|e| e.state()).unwrap_or_default()
}

// ---------------------------------------------------------------------------------------------
// Connexion
// ---------------------------------------------------------------------------------------------

#[tauri::command]
pub fn epic_account(epic: tauri::State<'_, Epic>) -> AccountView {
    epic.view()
}

/// Ouvre la fenêtre de connexion d'Epic (ou la ramène si elle est déjà ouverte).
#[tauri::command]
pub fn epic_login(app: AppHandle) -> Result<AccountView, String> {
    let epic = app.state::<Epic>();
    if let Some(window) = app.get_webview_window(LOGIN_LABEL) {
        let _ = window.set_focus();
        return Ok(epic.view());
    }
    let url: tauri::Url = api::login_url().parse().map_err(|_| tr!("Adresse de connexion invalide.", "Invalid sign-in address."))?;
    let handle = app.clone();
    epic.exchanging.store(false, Ordering::SeqCst);
    epic.set_error(None);
    let mut builder = tauri::WebviewWindowBuilder::new(&app, LOGIN_LABEL, tauri::WebviewUrl::External(url))
        .title(tr!("Connexion à Epic Games", "Sign in to Epic Games"))
        .inner_size(520.0, 760.0)
        .min_inner_size(380.0, 520.0)
        .center()
        // Navigation privée : ni cookie ni cache gardés après la fermeture.
        .incognito(true)
        .on_navigation(move |url| {
            if let Some(handoff) = api::handoff(url) {
                received(&handle, handoff);
                return false;
            }
            crate::gog_online::api::navigation_allowed(url)
        })
        // Script lancé seulement sur la page exacte qui affiche le code (voir l'en-tête).
        .on_page_load(|window, payload| {
            if payload.event() == tauri::webview::PageLoadEvent::Finished && api::is_redirect_page(payload.url()) {
                let _ = window.eval(api::handoff_script());
            }
        })
        .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny);
    if let Some(main) = app.get_webview_window(crate::MAIN_WINDOW) {
        builder = builder.parent(&main).map_err(|e| tr!("Fenêtre de connexion impossible : {e}", "Couldn't open the sign-in window: {e}"))?;
    }
    builder.build().map_err(|e| tr!("Fenêtre de connexion impossible : {e}", "Couldn't open the sign-in window: {e}"))?;
    epic.login_open.store(true, Ordering::SeqCst);
    let view = epic.view();
    let _ = app.emit(ACCOUNT_EVENT, view.clone());
    Ok(view)
}

/// Code remis par la page de redirection : fenêtre fermée, échange dans un fil (une seule fois).
fn received(app: &AppHandle, handoff: Handoff) {
    let epic = app.state::<Epic>();
    let code = match handoff {
        Handoff::Code(code) => code,
        Handoff::Missing => {
            // Page sans code (connexion pas finie, format changé) : la fenêtre reste ouverte.
            epic.set_error(Some(tr!("Epic n'a pas donné de code de connexion : réessaie.", "Epic didn't provide a sign-in code: try again.")));
            let _ = app.emit(ACCOUNT_EVENT, epic.view());
            return;
        }
    };
    if epic.exchanging.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        if let Some(window) = app.get_webview_window(LOGIN_LABEL) {
            let _ = window.destroy();
        }
        let epic = app.state::<Epic>();
        let agent = api::agent();
        let outcome = api::token(&agent, &epic.ends, Grant::Code(&code), now(), None)
            .map_err(|e| {
                let m = e.message();
                crate::diagnostic::failed(crate::diagnostic::Service::Epic, &m);
                m
            })
            .and_then(|session| epic.store(Some(session)));
        match outcome {
            Ok(()) => {
                crate::diagnostic::ok(crate::diagnostic::Service::Epic);
                epic.expired.store(false, Ordering::SeqCst);
                epic.set_error(None);
                log::info!("Compte Epic connecté");
            }
            Err(e) => {
                log::warn!("Connexion à Epic impossible : {e}");
                epic.set_error(Some(e));
            }
        }
        epic.login_open.store(false, Ordering::SeqCst);
        epic.exchanging.store(false, Ordering::SeqCst);
        let _ = app.emit(ACCOUNT_EVENT, epic.view());
    });
}

/// Fenêtre de connexion fermée (par l'utilisateur ou après le code).
pub fn login_closed(app: &AppHandle) {
    let Some(epic) = app.try_state::<Epic>() else { return };
    if !epic.exchanging.load(Ordering::SeqCst) {
        epic.login_open.store(false, Ordering::SeqCst);
        let _ = app.emit(ACCOUNT_EVENT, epic.view());
    }
}

/// Se déconnecter : session fermée chez Epic (au mieux), jetons et liste effacés.
#[tauri::command]
pub fn epic_logout(app: AppHandle) -> Result<AccountView, String> {
    let epic = app.state::<Epic>();
    if let Some(window) = app.get_webview_window(LOGIN_LABEL) {
        let _ = window.destroy();
    }
    let old = lock(&epic.session)?.clone();
    epic.store(None)?;
    epic.expired.store(false, Ordering::SeqCst);
    epic.set_error(None);
    *lock(&epic.cache)? = Cache::empty();
    let _ = std::fs::remove_file(&epic.cache_path);
    if let Some(old) = old {
        let ends = epic.ends.clone();
        std::thread::spawn(move || {
            let _ = api::kill_session(&api::agent(), &ends, &old);
        });
    }
    log::info!("Compte Epic déconnecté");
    let view = epic.view();
    let _ = app.emit(ACCOUNT_EVENT, view.clone());
    Ok(view)
}

// ---------------------------------------------------------------------------------------------
// Jeux possédés
// ---------------------------------------------------------------------------------------------

/// `AppName` des jeux Epic installés, vus par la source Epic (masqués compris).
fn installed_ids(state: &AppState) -> HashSet<String> {
    lock(&state.games).map(|g| g.iter().filter(|g| g.platform == crate::library::Platform::Epic).map(|g| g.platform_id.clone()).collect()).unwrap_or_default()
}

/// Le schéma `com.epicgames.launcher:` est enregistré (launcher installé).
#[cfg(windows)]
fn launcher_installed() -> bool {
    use winreg::enums::{HKEY_CLASSES_ROOT, KEY_READ};
    winreg::RegKey::predef(HKEY_CLASSES_ROOT).open_subkey_with_flags("com.epicgames.launcher", KEY_READ).is_ok()
}

#[cfg(not(windows))]
fn launcher_installed() -> bool {
    false
}

fn known_items(cache: &Cache) -> BTreeMap<String, Item> {
    cache.items.iter().filter_map(|(id, c)| c.item.clone().map(|i| (id.clone(), i))).collect()
}

fn library_view(app: &AppHandle, error: Option<String>) -> Result<Library, String> {
    let epic = app.state::<Epic>();
    let installed = installed_ids(&app.state::<AppState>());
    let cache = lock(&epic.cache)?;
    let connected = epic.state() != AccountState::Off;
    Ok(Library {
        account: epic.view(),
        games: if connected { not_installed(&cache.records, &known_items(&cache), &installed) } else { Vec::new() },
        owned: if connected { cache.records.len() } else { 0 },
        checked: cache.checked,
        error,
        launcher: launcher_installed(),
    })
}

/// Jeux Epic possédés non installés. `refresh` : relue tout de suite ; sinon après 6 h.
#[tauri::command]
pub async fn epic_library(app: AppHandle, refresh: bool) -> Result<Library, String> {
    blocking(move || {
        let epic = app.state::<Epic>();
        if epic.state() != AccountState::Connected {
            return library_view(&app, None);
        }
        let user = lock(&epic.session)?.as_ref().map(|s| s.user_id.clone());
        let due = {
            let mut cache = lock(&epic.cache)?;
            if cache.user_id != user {
                *cache = Cache { user_id: user.clone(), ..Cache::empty() };
            }
            refresh || cache.checked.is_none_or(|c| now().saturating_sub(c) >= OWNED_TTL)
        };
        let error = if due { sync_library(&app, &epic).err() } else { None };
        library_view(&app, error)
    })
    .await
}

/// Relit la bibliothèque puis les fiches manquantes (un espace de noms par requête, 200 ms entre deux).
fn sync_library(app: &AppHandle, epic: &Epic) -> Result<(), String> {
    let agent = api::agent();
    let records = epic.with_token(app, &agent, |s| api::library(&agent, &epic.ends, s))?;
    crate::diagnostic::ok(crate::diagnostic::Service::Epic);
    let stale = {
        let mut cache = lock(&epic.cache)?;
        cache.records = records;
        cache.checked = Some(now());
        cache.stale(now())
    };
    let mut result = Ok(());
    let mut first = true;
    'outer: for (namespace, ids) in stale {
        for batch in ids.chunks(api::CATALOG_BATCH) {
            if !first {
                std::thread::sleep(Duration::from_millis(200));
            }
            first = false;
            match epic.with_token(app, &agent, |s| api::catalog(&agent, &epic.ends, s, &namespace, batch)) {
                Ok(found) => {
                    let mut cache = lock(&epic.cache)?;
                    let fetched = now();
                    for id in batch {
                        let item = found.iter().find(|(i, _)| i == id).map(|(_, item)| item.clone());
                        let keep = item.or_else(|| cache.items.get(id).and_then(|c| c.item.clone()));
                        cache.items.insert(id.clone(), CachedItem { item: keep, fetched });
                    }
                }
                Err(e) => {
                    result = Err(e);
                    break 'outer;
                }
            }
        }
    }
    let mut cache = lock(&epic.cache)?;
    let wanted: HashSet<String> = cache.records.iter().map(|r| r.catalog_item_id.clone()).collect();
    cache.items.retain(|id, _| wanted.contains(id));
    cache.save(&epic.cache_path);
    result
}

/// Installe un jeu possédé par le launcher Epic (`action=install`), qui demande le dossier et
/// télécharge lui-même. Le nouveau jeu apparaît par la surveillance des manifestes Epic.
#[tauri::command]
pub fn epic_install(app: AppHandle, id: String) -> Result<(), String> {
    let epic = app.state::<Epic>();
    if epic.state() != AccountState::Connected {
        return Err(tr!("Connecte ton compte Epic dans les Paramètres.", "Sign in to your Epic account in Settings."));
    }
    if !api::id_ok(&id) {
        return Err(tr!("Ce jeu n'est pas dans tes jeux Epic à installer.", "This game isn't in your Epic games to install."));
    }
    let uri = {
        let installed = installed_ids(&app.state::<AppState>());
        let cache = lock(&epic.cache)?;
        let listed = not_installed(&cache.records, &known_items(&cache), &installed).iter().any(|g| g.id == id);
        cache.records.iter().find(|r| listed && r.app_name == id).and_then(api::install_uri)
    }
    .ok_or_else(|| tr!("Ce jeu n'est pas dans tes jeux Epic à installer.", "This game isn't in your Epic games to install."))?;
    if !launcher_installed() {
        return Err(tr!("Le launcher Epic Games n'est pas installé sur ce PC.", "The Epic Games Launcher isn't installed on this PC."));
    }
    app.opener().open_url(&uri, None::<&str>).map_err(|e| tr!("Le launcher Epic n'a pas répondu : {e}", "The Epic launcher didn't respond: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read as _, Write as _};

    fn sample_items() -> BTreeMap<String, Item> {
        let mut items: BTreeMap<String, Item> = BTreeMap::new();
        for (text, ids) in [
            (include_str!("samples/catalog_rocket_league.json"), vec!["530145df28a24424923f5828cc9031a1".to_string(), "8eeee1bbc27442fd914a0b97b940869d".to_string()]),
            (include_str!("samples/catalog_fortnite.json"), vec!["4fe75bbc5a674f4f9b356b5c90567da5".to_string()]),
        ] {
            items.extend(api::parse_catalog(text, &ids).unwrap());
        }
        items
    }

    #[test]
    fn not_installed_keeps_windows_games_only() {
        let (mut records, _) = api::parse_library(include_str!("samples/library_page1.json")).unwrap();
        // DLC de Rocket League, possédé : jamais proposé.
        records.push(Record { app_name: "SugarDlc".into(), namespace: "9773aa1aa54f4f7b80e44bef04986cea".into(), catalog_item_id: "8eeee1bbc27442fd914a0b97b940869d".into() });
        let mut items = sample_items();
        // Jeu seulement pour Mac : écarté.
        items.insert("c04fd6a7cb4e4d06a40a8da1fe4b3e8d".into(), Item { title: "Jeu Mac".into(), game: true, windows: false, image: None });
        let installed: HashSet<String> = ["Fortnite".to_string()].into();
        let list = not_installed(&records, &items, &installed);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "Sugar");
        assert_eq!(list[0].title, "Rocket League®");
        assert!(list[0].image_url.as_deref().is_some_and(|u| u.starts_with("https://cdn1.epicgames.com/")));
        // Rien d'installé : Fortnite revient, trié par titre.
        let all = not_installed(&records, &items, &HashSet::new());
        assert_eq!(all.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(), ["Fortnite", "Sugar"]);
    }

    #[test]
    fn cache_groups_stale_items_by_namespace() {
        let (records, _) = api::parse_library(include_str!("samples/library_page1.json")).unwrap();
        let mut cache = Cache { records, ..Cache::empty() };
        cache.items.insert("4fe75bbc5a674f4f9b356b5c90567da5".into(), CachedItem { item: None, fetched: 1_000 });
        let stale = cache.stale(1_000 + 10);
        assert!(!stale.contains_key("fn"), "fiche récente");
        assert_eq!(stale.get("9773aa1aa54f4f7b80e44bef04986cea").map(Vec::len), Some(1));
        assert_eq!(cache.stale(1_000 + ITEM_TTL).get("fn").map(Vec::len), Some(1));
        let dir = std::env::temp_dir().join(format!("ludex-epic-cache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(CACHE_FILE);
        cache.save(&path);
        assert_eq!(Cache::load(&path).records.len(), 3);
        std::fs::write(&path, br#"{"version":99,"records":[]}"#).unwrap();
        assert!(Cache::load(&path).records.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Serveur HTTP local : une réponse par connexion, dans l'ordre ; garde la ligne de requête,
    /// l'en-tête `Authorization` et le corps.
    fn server(replies: Vec<(u16, &'static str)>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for (status, body) in replies {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                let (mut auth, mut length) = (String::new(), 0usize);
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    let lower = line.to_ascii_lowercase();
                    if lower.starts_with("authorization:") {
                        auth = line.trim().to_string();
                    }
                    if let Some(n) = lower.strip_prefix("content-length:") {
                        length = n.trim().parse().unwrap_or(0);
                    }
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                }
                let mut sent = vec![0; length];
                reader.read_exact(&mut sent).unwrap();
                seen.push(format!("{} | {} | {}", first.trim(), auth, String::from_utf8_lossy(&sent)));
                let _ = write!(stream, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
            seen
        });
        (base, handle)
    }

    #[test]
    fn code_exchange_then_paged_library_and_catalog() {
        let (base, server) = server(vec![
            (200, include_str!("samples/token.json")),
            (200, include_str!("samples/library_page1.json")),
            (200, include_str!("samples/library_page2.json")),
            (200, include_str!("samples/catalog_rocket_league.json")),
        ]);
        let ends = Endpoints { account: base.clone(), library: base.clone(), catalog: base };
        let agent = api::agent();
        let session = api::token(&agent, &ends, Grant::Code("CODE0123456789"), 10, None).unwrap();
        assert_eq!(session.user_id, "0123456789abcdef0123456789abcdef");
        assert_eq!(session.username.as_deref(), Some("JoueurEssai"));
        let records = api::library(&agent, &ends, &session).unwrap();
        assert_eq!(records.iter().map(|r| r.app_name.as_str()).collect::<Vec<_>>(), ["Sugar", "Fortnite", "9d2d0eb64d5c44529cece33fe2a46482", "Hades"]);
        let found = api::catalog(&agent, &ends, &session, "9773aa1aa54f4f7b80e44bef04986cea", &["530145df28a24424923f5828cc9031a1".into()]).unwrap();
        assert_eq!(found.len(), 1);
        let seen = server.join().unwrap();
        assert!(seen[0].starts_with("POST /account/api/oauth/token HTTP/1.1 | Authorization: basic MzRh"));
        assert!(seen[0].ends_with("grant_type=authorization_code&code=CODE0123456789&token_type=eg1"));
        assert!(seen[1].starts_with("GET /library/api/public/items?includeMetadata=true HTTP"));
        assert!(seen[1].contains("Authorization: bearer eg1~faux-jeton-d-acces"));
        assert!(seen[2].starts_with("GET /library/api/public/items?includeMetadata=true&cursor=curseur-page-2 HTTP"));
        assert!(seen[3].starts_with("GET /catalog/api/shared/namespace/9773aa1aa54f4f7b80e44bef04986cea/bulk/items?id=530145df28a24424923f5828cc9031a1&"));
    }

    #[test]
    fn refused_code_or_refresh_is_unauthorized_without_secret() {
        let (base, server) = server(vec![(400, r#"{"errorCode":"errors.com.epicgames.account.oauth.authorization_code_not_found"}"#), (401, "{}")]);
        let ends = Endpoints { account: base.clone(), library: base.clone(), catalog: base };
        let agent = api::agent();
        let err = api::token(&agent, &ends, Grant::Code("SECRETCODE123"), 0, None).unwrap_err();
        assert_eq!(err, Failure::Unauthorized);
        assert!(!err.message().contains("SECRETCODE"));
        let err = api::token(&agent, &ends, Grant::Refresh("SECRET-REFRESH"), 0, None).unwrap_err();
        assert_eq!(err, Failure::Unauthorized);
        server.join().unwrap();
    }
}
