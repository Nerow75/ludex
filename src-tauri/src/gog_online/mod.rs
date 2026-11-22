//! Compte GOG optionnel (seule exception au principe « aucune connexion à un compte ») :
//! connexion par la page de GOG dans une fenêtre dédiée, jeux possédés pas encore installés sur
//! ce PC, installation par l'installeur hors ligne officiel.
//!
//! - Connexion désactivée par défaut, lancée depuis les Paramètres (« Comptes »). La fenêtre
//!   `gog-login` est créée par Rust, en navigation privée (rien de gardé), sans capability Tauri
//!   (aucune commande accessible depuis la page de GOG), sans script injecté, fenêtres surgissantes
//!   refusées. Ludex ne lit que l'URL de redirection de GOG pour y prendre le code ; le mot de
//!   passe est saisi chez GOG.
//! - Jetons échangés et rafraîchis ici, chiffrés par DPAPI (`vault.rs`), jamais envoyés au front,
//!   jamais dans le journal, le Diagnostic, `user-data.json` ni l'export.
//! - Le front n'envoie qu'un id de produit GOG numérique, vérifié contre la liste des jeux possédés
//!   non installés : liens, chemins et arguments sont reconstruits ici.

pub mod api;
pub mod install;
pub mod vault;

use crate::{blocking, lock, AppState};
use api::{Endpoints, Failure, Grant, Kind, Product};
use install::{Control, Phase, Progress, Stop};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use vault::Session;

/// Fenêtre de connexion (aucune capability ne la nomme).
pub const LOGIN_LABEL: &str = "gog-login";
/// Événement : état du compte changé (connexion, déconnexion, jeton refusé).
pub const ACCOUNT_EVENT: &str = "gog-account";
/// Événement : progression d'une installation.
pub const INSTALL_EVENT: &str = "gog-install";
/// Cache des jeux possédés et de leurs fiches (aucun jeton), effacé à la déconnexion.
const CACHE_FILE: &str = "gog-cache.json";
const CACHE_VERSION: u32 = 1;
/// Liste des jeux possédés relue après 6 h ; fiches après 30 jours.
const OWNED_TTL: u64 = 6 * 3600;
const PRODUCT_TTL: u64 = 30 * 86_400;

/// État du compte, seul ce qui est montré.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountState {
    /// Aucun compte connecté (par défaut).
    #[default]
    Off,
    Connected,
    /// Jeton refusé par GOG : se reconnecter.
    Expired,
}

impl AccountState {
    /// État en une ligne, pour le rapport du Diagnostic (jamais d'identifiant).
    pub fn text(self) -> String {
        match self {
            AccountState::Off => tr!("non connecté", "not signed in"),
            AccountState::Connected => tr!("connecté", "signed in"),
            AccountState::Expired => tr!("à reconnecter (jeton refusé par GOG)", "needs signing in again (token refused by GOG)"),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountView {
    pub state: AccountState,
    /// Pseudo GOG, seulement pour la page des Paramètres.
    pub username: Option<String>,
    pub login_open: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedProduct {
    product: Option<Product>,
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
    owned: Vec<u64>,
    #[serde(default)]
    checked: Option<u64>,
    #[serde(default)]
    products: BTreeMap<u64, CachedProduct>,
}

impl Cache {
    fn load(path: &Path) -> Cache {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Cache>(&b).ok())
            .filter(|c| c.version == CACHE_VERSION)
            .unwrap_or(Cache { version: CACHE_VERSION, ..Default::default() })
    }

    fn save(&self, path: &Path) {
        let Ok(bytes) = serde_json::to_vec(self) else { return };
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, bytes).and_then(|_| std::fs::rename(&tmp, path)).is_err() {
            let _ = std::fs::remove_file(&tmp);
            log::warn!("Cache des jeux GOG non enregistré");
        }
    }

    /// Ids possédés dont la fiche manque ou a plus de 30 jours.
    fn stale_products(&self, now: u64) -> Vec<u64> {
        self.owned.iter().copied().filter(|id| self.products.get(id).is_none_or(|p| now.saturating_sub(p.fetched) >= PRODUCT_TTL)).collect()
    }
}

/// Un jeu possédé qu'on peut installer : jeu (pas DLC ni lot), pour Windows, installable.
fn installable(p: &Product) -> bool {
    p.kind == Kind::Game && p.windows && p.installable
}

/// Jeu possédé non installé, montré au front.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnedGame {
    pub id: u64,
    pub title: String,
    pub image_url: Option<String>,
}

/// Jeux possédés non installés sur ce PC : les jeux installables de la liste, moins ceux que la
/// source GOG voit déjà (id de produit = `gameID` du registre), triés par titre.
pub fn not_installed(owned: &[u64], products: &BTreeMap<u64, Product>, installed: &std::collections::HashSet<String>) -> Vec<OwnedGame> {
    let mut list: Vec<OwnedGame> = owned
        .iter()
        .filter(|id| !installed.contains(&id.to_string()))
        .filter_map(|id| products.get(id))
        .filter(|p| installable(p))
        .map(|p| OwnedGame { id: p.id, title: p.title.clone(), image_url: p.image.as_deref().and_then(api::image_url) })
        .collect();
    list.sort_by_cached_key(|g| g.title.to_lowercase());
    list
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    pub account: AccountView,
    pub games: Vec<OwnedGame>,
    /// Jeux possédés au total (DLC et lots compris).
    pub owned: usize,
    /// Dernière relecture de la liste (secondes Unix).
    pub checked: Option<u64>,
    pub error: Option<String>,
    /// Installation en cours (ou dernière finie), pour l'afficher après un rechargement.
    pub job: Option<Progress>,
}

/// Installation en cours.
struct Job {
    id: u64,
    control: Arc<Control>,
    progress: Mutex<Progress>,
    finished: AtomicBool,
}

/// État du compte GOG, géré par Tauri à côté d'`AppState`.
pub struct Gog {
    vault_path: PathBuf,
    cache_path: PathBuf,
    ends: Endpoints,
    session: Mutex<Option<Session>>,
    expired: AtomicBool,
    login_open: AtomicBool,
    /// Code reçu : échange en cours (la redirection peut être vue deux fois).
    exchanging: AtomicBool,
    last_error: Mutex<Option<String>>,
    cache: Mutex<Cache>,
    /// Tenu pendant une suite de requêtes au compte : un seul rafraîchissement à la fois.
    api_lock: Mutex<()>,
    job: Mutex<Option<Arc<Job>>>,
    /// Dernier dossier choisi pour installer (mémoire seulement).
    last_dir: Mutex<Option<PathBuf>>,
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl Gog {
    pub fn new(data_dir: &Path) -> Gog {
        let vault_path = vault::path_in(data_dir);
        let session = match vault::load(&vault_path) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Compte GOG : {e}");
                None
            }
        };
        Gog {
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
            job: Mutex::new(None),
            last_dir: Mutex::new(None),
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
        vault::save(&self.vault_path, session.as_ref())?;
        *lock(&self.session)? = session;
        Ok(())
    }

    /// Exécute une requête au compte avec un jeton valide : rafraîchi s'il expire, puis une
    /// seconde fois si GOG le refuse. Rafraîchissement refusé = « à reconnecter », sans boucle.
    fn with_token<T>(&self, app: &AppHandle, agent: &ureq::Agent, call: impl Fn(&Session) -> Result<T, Failure>) -> Result<T, String> {
        let _guard = self.api_lock.lock().unwrap_or_else(|e| e.into_inner());
        if self.expired.load(Ordering::SeqCst) {
            return Err(Failure::Unauthorized.message());
        }
        let mut session = lock(&self.session)?.clone().ok_or_else(|| tr!("Aucun compte GOG connecté.", "No GOG account signed in."))?;
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
                crate::diagnostic::ok(crate::diagnostic::Service::Gog);
                Ok(fresh)
            }
            Err(e) => Err(self.failed(app, e)),
        }
    }

    /// Note l'échec (Diagnostic) ; jeton refusé = compte « à reconnecter » et front prévenu.
    fn failed(&self, app: &AppHandle, e: Failure) -> String {
        let message = e.message();
        crate::diagnostic::failed(crate::diagnostic::Service::Gog, &message);
        if e == Failure::Unauthorized && !self.expired.swap(true, Ordering::SeqCst) {
            let _ = app.emit(ACCOUNT_EVENT, self.view());
        }
        message
    }

    fn current_job(&self) -> Option<Arc<Job>> {
        lock(&self.job).ok().and_then(|j| j.clone())
    }

    fn busy(&self) -> bool {
        self.current_job().is_some_and(|j| !j.finished.load(Ordering::SeqCst))
    }
}

/// État du compte pour le Diagnostic.
pub fn diag_state(app: &AppHandle) -> AccountState {
    app.try_state::<Gog>().map(|g| g.state()).unwrap_or_default()
}

// ---------------------------------------------------------------------------------------------
// Connexion
// ---------------------------------------------------------------------------------------------

#[tauri::command]
pub fn gog_account(gog: tauri::State<'_, Gog>) -> AccountView {
    gog.view()
}

/// Ouvre la fenêtre de connexion de GOG (ou la ramène si elle est déjà ouverte).
#[tauri::command]
pub fn gog_login(app: AppHandle) -> Result<AccountView, String> {
    let gog = app.state::<Gog>();
    if let Some(window) = app.get_webview_window(LOGIN_LABEL) {
        let _ = window.set_focus();
        return Ok(gog.view());
    }
    let url: tauri::Url = api::login_url().parse().map_err(|_| tr!("Adresse de connexion invalide.", "Invalid sign-in address."))?;
    let handle = app.clone();
    gog.exchanging.store(false, Ordering::SeqCst);
    gog.set_error(None);
    let mut builder = tauri::WebviewWindowBuilder::new(&app, LOGIN_LABEL, tauri::WebviewUrl::External(url))
        .title(tr!("Connexion à GOG", "Sign in to GOG"))
        .inner_size(500.0, 720.0)
        .min_inner_size(380.0, 520.0)
        .center()
        // Navigation privée : ni cookie ni cache gardés après la fermeture.
        .incognito(true)
        .on_navigation(move |url| {
            if let Some(code) = api::code_from_redirect(url) {
                received_code(&handle, code);
                return false;
            }
            api::navigation_allowed(url)
        })
        // Aucune fenêtre surgissante : tout reste dans cette fenêtre filtrée.
        .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny);
    if let Some(main) = app.get_webview_window(crate::MAIN_WINDOW) {
        builder = builder.parent(&main).map_err(|e| tr!("Fenêtre de connexion impossible : {e}", "Couldn't open the sign-in window: {e}"))?;
    }
    builder.build().map_err(|e| tr!("Fenêtre de connexion impossible : {e}", "Couldn't open the sign-in window: {e}"))?;
    gog.login_open.store(true, Ordering::SeqCst);
    let view = gog.view();
    let _ = app.emit(ACCOUNT_EVENT, view.clone());
    Ok(view)
}

/// Code lu dans la redirection : fenêtre fermée, échange dans un fil (une seule fois).
fn received_code(app: &AppHandle, code: String) {
    let gog = app.state::<Gog>();
    if gog.exchanging.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        if let Some(window) = app.get_webview_window(LOGIN_LABEL) {
            let _ = window.destroy();
        }
        let gog = app.state::<Gog>();
        let agent = api::agent();
        let result = api::token(&agent, &gog.ends, Grant::Code(&code), now(), None).map_err(|e| {
            let m = e.message();
            crate::diagnostic::failed(crate::diagnostic::Service::Gog, &m);
            m
        });
        let outcome = result.and_then(|mut session| {
            session.username = api::username(&agent, &gog.ends, &session).ok().flatten();
            gog.store(Some(session))
        });
        match outcome {
            Ok(()) => {
                crate::diagnostic::ok(crate::diagnostic::Service::Gog);
                gog.expired.store(false, Ordering::SeqCst);
                gog.set_error(None);
                log::info!("Compte GOG connecté");
            }
            Err(e) => {
                log::warn!("Connexion à GOG impossible : {e}");
                gog.set_error(Some(e));
            }
        }
        gog.login_open.store(false, Ordering::SeqCst);
        gog.exchanging.store(false, Ordering::SeqCst);
        let _ = app.emit(ACCOUNT_EVENT, gog.view());
    });
}

/// Fenêtre de connexion fermée (par l'utilisateur ou après le code).
pub fn login_closed(app: &AppHandle) {
    let Some(gog) = app.try_state::<Gog>() else { return };
    if !gog.exchanging.load(Ordering::SeqCst) {
        gog.login_open.store(false, Ordering::SeqCst);
        let _ = app.emit(ACCOUNT_EVENT, gog.view());
    }
}

/// Se déconnecter : jetons et liste des jeux effacés. Refusé pendant un téléchargement.
#[tauri::command]
pub fn gog_logout(app: AppHandle) -> Result<AccountView, String> {
    let gog = app.state::<Gog>();
    if gog.busy() {
        return Err(tr!("Une installation GOG est en cours : annule-la d'abord.", "A GOG install is in progress: cancel it first."));
    }
    if let Some(window) = app.get_webview_window(LOGIN_LABEL) {
        let _ = window.destroy();
    }
    gog.store(None)?;
    gog.expired.store(false, Ordering::SeqCst);
    gog.set_error(None);
    *lock(&gog.cache)? = Cache { version: CACHE_VERSION, ..Default::default() };
    let _ = std::fs::remove_file(&gog.cache_path);
    *lock(&gog.job)? = None;
    log::info!("Compte GOG déconnecté");
    let view = gog.view();
    let _ = app.emit(ACCOUNT_EVENT, view.clone());
    Ok(view)
}

// ---------------------------------------------------------------------------------------------
// Jeux possédés
// ---------------------------------------------------------------------------------------------

/// Ids des jeux GOG installés, vus par la source GOG (masqués compris).
fn installed_ids(state: &AppState) -> std::collections::HashSet<String> {
    lock(&state.games).map(|g| g.iter().filter(|g| g.platform == crate::library::Platform::Gog).map(|g| g.platform_id.clone()).collect()).unwrap_or_default()
}

fn library_view(app: &AppHandle, error: Option<String>) -> Result<Library, String> {
    let gog = app.state::<Gog>();
    let state = app.state::<AppState>();
    let installed = installed_ids(&state);
    let cache = lock(&gog.cache)?;
    let products: BTreeMap<u64, Product> = cache.products.iter().filter_map(|(id, c)| c.product.clone().map(|p| (*id, p))).collect();
    let connected = gog.state() != AccountState::Off;
    Ok(Library {
        account: gog.view(),
        games: if connected { not_installed(&cache.owned, &products, &installed) } else { Vec::new() },
        owned: if connected { cache.owned.len() } else { 0 },
        checked: cache.checked,
        error,
        job: gog.current_job().map(|j| j.progress.lock().map(|p| p.clone()).unwrap_or_else(|e| e.into_inner().clone())),
    })
}

/// Liste des jeux possédés non installés. `refresh` : relue tout de suite ; sinon après 6 h.
#[tauri::command]
pub async fn gog_library(app: AppHandle, refresh: bool) -> Result<Library, String> {
    blocking(move || {
        let gog = app.state::<Gog>();
        if gog.state() != AccountState::Connected {
            return library_view(&app, None);
        }
        let user = lock(&gog.session)?.as_ref().map(|s| s.user_id.clone());
        let due = {
            let mut cache = lock(&gog.cache)?;
            if cache.user_id != user {
                *cache = Cache { version: CACHE_VERSION, user_id: user.clone(), ..Default::default() };
            }
            refresh || cache.checked.is_none_or(|c| now().saturating_sub(c) >= OWNED_TTL)
        };
        let error = if due { sync_library(&app, &gog).err() } else { None };
        library_view(&app, error)
    })
    .await
}

/// Relit la liste des jeux possédés puis les fiches manquantes (lots de 50, 300 ms entre deux).
fn sync_library(app: &AppHandle, gog: &Gog) -> Result<(), String> {
    let agent = api::agent();
    let owned = gog.with_token(app, &agent, |s| api::owned(&agent, &gog.ends, s))?;
    crate::diagnostic::ok(crate::diagnostic::Service::Gog);
    let stale = {
        let mut cache = lock(&gog.cache)?;
        cache.owned = owned;
        cache.checked = Some(now());
        cache.stale_products(now())
    };
    let mut result = Ok(());
    for (i, batch) in stale.chunks(api::PRODUCTS_BATCH).enumerate() {
        if i > 0 {
            std::thread::sleep(Duration::from_millis(300));
        }
        match api::products(&agent, &gog.ends, batch) {
            Ok(found) => {
                let mut cache = lock(&gog.cache)?;
                let fetched = now();
                for id in batch {
                    // Produit absent de la réponse (retiré de la vente) : noté sans fiche.
                    let product = found.iter().find(|p| p.id == *id).cloned();
                    let keep = product.or_else(|| cache.products.get(id).and_then(|c| c.product.clone()));
                    cache.products.insert(*id, CachedProduct { product: keep, fetched });
                }
            }
            Err(e) => {
                let m = e.message();
                crate::diagnostic::failed(crate::diagnostic::Service::Gog, &m);
                result = Err(m);
                break;
            }
        }
    }
    let cache = lock(&gog.cache)?;
    let owned: std::collections::HashSet<u64> = cache.owned.iter().copied().collect();
    let mut kept = Cache { version: CACHE_VERSION, user_id: cache.user_id.clone(), owned: cache.owned.clone(), checked: cache.checked, products: BTreeMap::new() };
    kept.products = cache.products.iter().filter(|(id, _)| owned.contains(id)).map(|(id, p)| (*id, p.clone())).collect();
    drop(cache);
    kept.save(&gog.cache_path);
    *lock(&gog.cache)? = kept;
    result
}

// ---------------------------------------------------------------------------------------------
// Installation
// ---------------------------------------------------------------------------------------------

/// Vrai téléchargement chez GOG : lien signé par `downlink`, puis le CDN (redirections vérifiées).
struct GogSource<'a> {
    app: &'a AppHandle,
    agent: ureq::Agent,
    product: u64,
}

impl install::Source for GogSource<'_> {
    fn link(&self, part: &api::Part, first: bool) -> Result<install::Link, Failure> {
        let gog = self.app.state::<Gog>();
        let url = gog
            .with_token(self.app, &self.agent, |s| api::part_link(&self.agent, &gog.ends, s, self.product, part))
            .map_err(Failure::Network)?;
        let name = api::file_name(&url, first).ok_or_else(|| Failure::Network(tr!("nom de fichier inattendu", "unexpected file name")))?;
        Ok(install::Link { url: url.to_string(), name })
    }

    fn open(&self, link: &install::Link, start: u64) -> Result<install::Body, Failure> {
        let mut url = url::Url::parse(&link.url).map_err(|_| Failure::Network(tr!("lien illisible", "unreadable link")))?;
        // Redirections suivies à la main, chacune vérifiée (CDN de GOG seulement).
        for _ in 0..5 {
            if !api::download_allowed(&url) {
                return Err(Failure::Network(tr!("redirection hors de GOG refusée", "redirect outside GOG refused")));
            }
            let mut request = self.agent.get(url.as_str());
            if start > 0 {
                request = request.set("Range", &format!("bytes={start}-"));
            }
            let response = request.call().map_err(api::failure)?;
            let status = response.status();
            if (300..400).contains(&status) {
                let location = response.header("location").unwrap_or("").to_string();
                url = url.join(&location).map_err(|_| Failure::Network(tr!("redirection illisible", "unreadable redirect")))?;
                continue;
            }
            let served = match status {
                206 => start,
                200 => 0,
                code => return Err(Failure::Status(code)),
            };
            let total = api::total_of(response.header("content-range"), response.header("content-length"), served);
            return Ok(install::Body { reader: Box::new(response.into_reader()), total, start: served });
        }
        Err(Failure::Network(tr!("trop de redirections", "too many redirects")))
    }
}

fn emit_progress(app: &AppHandle, job: &Job, change: impl FnOnce(&mut Progress)) {
    let snapshot = {
        let mut p = job.progress.lock().unwrap_or_else(|e| e.into_inner());
        change(&mut p);
        p.clone()
    };
    let _ = app.emit(INSTALL_EVENT, snapshot);
}

/// Dossier de téléchargement de ce jeu, dans le cache des données lourdes.
fn download_dir(app: &AppHandle, product: u64) -> Option<PathBuf> {
    let state = app.state::<AppState>();
    if crate::relocate::unavailable(&state) {
        return None;
    }
    Some(crate::relocate::current_cache(&state).join(install::DOWNLOADS).join(product.to_string()))
}

/// Place libre du disque qui porte `dir` (ou son premier parent existant).
fn free_space(dir: &Path) -> Option<u64> {
    let existing = dir.ancestors().find(|d| d.is_dir())?;
    let volumes = crate::storage::volumes();
    crate::storage::volume_of(&existing.to_string_lossy(), &volumes).map(|v| v.free)
}

/// Installe un jeu possédé non installé. Ouvre d'abord le sélecteur de dossier (côté Rust,
/// dossier « Jeux » proposé) ; `false` : annulé. La suite se déroule dans un fil (`gog-install`).
#[tauri::command]
pub async fn gog_install(app: AppHandle, window: tauri::WebviewWindow, id: u64) -> Result<bool, String> {
    blocking(move || {
        let gog = app.state::<Gog>();
        if gog.state() != AccountState::Connected {
            return Err(tr!("Connecte ton compte GOG dans les Paramètres.", "Sign in to your GOG account in Settings."));
        }
        if gog.busy() {
            return Err(tr!("Une installation GOG est déjà en cours.", "A GOG install is already in progress."));
        }
        // Seulement un jeu possédé, installable et pas encore installé.
        let title = {
            let state = app.state::<AppState>();
            let installed = installed_ids(&state);
            let cache = lock(&gog.cache)?;
            let products: BTreeMap<u64, Product> = cache.products.iter().filter_map(|(i, c)| c.product.clone().map(|p| (*i, p))).collect();
            not_installed(&cache.owned, &products, &installed).into_iter().find(|g| g.id == id).map(|g| g.title)
        }
        .ok_or_else(|| tr!("Ce jeu n'est pas dans tes jeux GOG à installer.", "This game isn't in your GOG games to install."))?;
        let start = lock(&gog.last_dir)?.clone().or_else(|| crate::library::game_folders().into_iter().next());
        let mut dialog = window.dialog().file().set_title(tr!("Installer « {} » dans…", "Install \"{}\" in…", title)).set_parent(&window);
        if let Some(start) = start.filter(|d| d.is_dir()) {
            dialog = dialog.set_directory(start);
        }
        let Some(base) = dialog.blocking_pick_folder().and_then(|f| f.into_path().ok()) else { return Ok(false) };
        let target = install::target_dir(&base, &title)?;
        let downloads = download_dir(&app, id).ok_or_else(|| tr!("Dossier des données de Ludex introuvable (disque débranché ?).", "Ludex data folder not found (drive unplugged?)."))?;
        *lock(&gog.last_dir)? = Some(base);
        let job = Arc::new(Job {
            id,
            control: Arc::new(Control::default()),
            progress: Mutex::new(Progress { id, title: title.clone(), phase: Phase::Preparing, received: 0, total: 0, speed: None, error: None, game_id: None }),
            finished: AtomicBool::new(false),
        });
        *lock(&gog.job)? = Some(job.clone());
        let _ = app.emit(INSTALL_EVENT, job.progress.lock().map(|p| p.clone()).unwrap_or_else(|e| e.into_inner().clone()));
        let handle = app.clone();
        std::thread::Builder::new()
            .name("ludex-gog-install".into())
            .spawn(move || run_job(&handle, &job, &title, &target, &downloads))
            .map_err(|_| tr!("Installation impossible à démarrer.", "Couldn't start the install."))?;
        Ok(true)
    })
    .await
}

fn run_job(app: &AppHandle, job: &Job, title: &str, target: &Path, downloads: &Path) {
    let outcome = install_steps(app, job, title, target, downloads);
    crate::library::set_installing(None);
    match outcome {
        Ok(game_id) => {
            let _ = std::fs::remove_dir_all(downloads);
            log::info!("Jeu GOG installé : {title}");
            emit_progress(app, job, |p| {
                p.phase = Phase::Done;
                p.speed = None;
                p.game_id = game_id;
            });
        }
        Err(Stop::Cancelled) => {
            // Téléchargement annulé : fichiers de Ludex effacés (rien d'autre).
            let _ = std::fs::remove_dir_all(downloads);
            emit_progress(app, job, |p| {
                p.phase = Phase::Cancelled;
                p.speed = None;
            });
        }
        Err(Stop::Failed(e)) => {
            log::warn!("Installation GOG de {title} impossible : {e}");
            emit_progress(app, job, |p| {
                p.phase = Phase::Failed;
                p.speed = None;
                p.error = Some(e);
            });
        }
    }
    job.finished.store(true, Ordering::SeqCst);
}

fn install_steps(app: &AppHandle, job: &Job, title: &str, target: &Path, downloads: &Path) -> Result<Option<String>, Stop> {
    let agent = api::agent();
    let gog = app.state::<Gog>();
    let language = crate::i18n::current().code();
    let installer = api::installer(&agent, &gog.ends, job.id, language)
        .map_err(|e| Stop::Failed(e.message()))?
        .ok_or_else(|| Stop::Failed(tr!("GOG ne propose pas d'installeur Windows pour « {} ».", "GOG has no Windows installer for \"{}\".", title)))?;
    let total = installer.total();
    emit_progress(app, job, |p| p.total = total);
    // Place : parties restantes + marge sur le disque des téléchargements ; à peu près autant sur
    // le disque d'installation (taille installée inconnue avant l'installation).
    let already: u64 = std::fs::read_dir(downloads).map(|d| d.flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum()).unwrap_or(0);
    if free_space(downloads).is_some_and(|free| free < total.saturating_sub(already) + install::SPACE_MARGIN) {
        return Err(Stop::Failed(tr!("Pas assez de place pour télécharger l'installeur ({}).", "Not enough space to download the installer ({}).", crate::notify::gigabytes(total))));
    }
    if free_space(target).is_some_and(|free| free < total) {
        return Err(Stop::Failed(tr!("Pas assez de place dans le dossier d'installation.", "Not enough space in the install folder.")));
    }
    let source = GogSource { app, agent, product: job.id };
    let exe = install::download(&source, &installer.parts, downloads, &job.control, |phase, received, speed| {
        emit_progress(app, job, |p| {
            p.phase = phase;
            p.received = received;
            p.speed = speed;
        });
    })?;
    emit_progress(app, job, |p| {
        p.phase = Phase::Verifying;
        p.speed = None;
    });
    install::verify_signature(&exe).map_err(Stop::Failed)?;
    if job.control.cancelled() {
        return Err(Stop::Cancelled);
    }
    emit_progress(app, job, |p| p.phase = Phase::Installing);
    // Le dossier du jeu n'est pas pris pour un jeu « Autres » le temps de l'installation.
    crate::library::set_installing(Some(target));
    install::run_installer(&exe, &install::installer_args(target, &installer.language)).map_err(Stop::Failed)?;
    crate::library::set_installing(None);
    // Rescan : le jeu apparaît par la source GOG (clés écrites par l'installeur).
    let game_id = format!("gog:{}", job.id);
    for _ in 0..30 {
        if crate::auto_rescan(app, false) {
            break;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    let found = app.state::<AppState>().games.lock().map(|g| g.iter().any(|g| g.id == game_id)).unwrap_or(false);
    Ok(found.then_some(game_id))
}

fn with_job(app: &AppHandle, id: u64, act: impl FnOnce(&Job)) -> Result<(), String> {
    let gog = app.state::<Gog>();
    let job = gog.current_job().filter(|j| j.id == id && !j.finished.load(Ordering::SeqCst)).ok_or_else(|| tr!("Aucune installation en cours pour ce jeu.", "No install in progress for this game."))?;
    act(&job);
    Ok(())
}

#[tauri::command]
pub fn gog_install_pause(app: AppHandle, id: u64) -> Result<(), String> {
    with_job(&app, id, |job| {
        let phase = job.progress.lock().map(|p| p.phase).unwrap_or(Phase::Downloading);
        if matches!(phase, Phase::Downloading | Phase::Preparing) {
            job.control.pause();
        }
    })
}

#[tauri::command]
pub fn gog_install_resume(app: AppHandle, id: u64) -> Result<(), String> {
    with_job(&app, id, |job| {
        job.control.resume();
        emit_progress(&app, job, |p| {
            if p.phase == Phase::Paused {
                p.phase = Phase::Downloading;
            }
        });
    })
}

/// Annule le téléchargement (pas l'installeur une fois lancé : il a sa propre fenêtre).
#[tauri::command]
pub fn gog_install_cancel(app: AppHandle, id: u64) -> Result<(), String> {
    with_job(&app, id, |job| {
        let phase = job.progress.lock().map(|p| p.phase).unwrap_or(Phase::Downloading);
        if phase != Phase::Installing {
            job.control.cancel();
        }
    })
}

/// Oublie une installation finie (échec ou réussite) affichée au front.
#[tauri::command]
pub fn gog_install_dismiss(app: AppHandle, id: u64) -> Result<(), String> {
    let gog = app.state::<Gog>();
    let mut job = lock(&gog.job)?;
    if job.as_ref().is_some_and(|j| j.id == id && j.finished.load(Ordering::SeqCst)) {
        *job = None;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::io::{BufRead, BufReader, Write as _};

    fn products() -> BTreeMap<u64, Product> {
        api::parse_products(include_str!("samples/products.json")).unwrap().into_iter().map(|p| (p.id, p)).collect()
    }

    #[test]
    fn not_installed_skips_installed_packs_and_unknown() {
        let owned = api::parse_owned(include_str!("samples/owned.json")).unwrap();
        let mut products = products();
        // DLC fabriqué : jamais proposé seul.
        products.insert(42, Product { id: 42, title: "Un DLC".into(), kind: Kind::Dlc, windows: true, installable: true, image: None });
        let mut owned = owned;
        owned.push(42);
        owned.push(999); // fiche inconnue
        let installed: HashSet<String> = ["1207658930".to_string()].into();
        let list = not_installed(&owned, &products, &installed);
        let titles: Vec<&str> = list.iter().map(|g| g.title.as_str()).collect();
        assert_eq!(titles.len(), 3);
        assert_eq!(titles[0], "Fallout Classic");
        assert!(titles[1].starts_with("STAR WARS"));
        assert_eq!(titles[2], "The Witcher: Enhanced Edition");
        assert!(list.iter().all(|g| g.id != 1207664663), "lot écarté");
        assert!(list.iter().all(|g| g.id != 1207658930), "déjà installé écarté");
        assert!(list[2].image_url.as_deref().is_some_and(|u| u.starts_with("https://images.gog-statics.com/")));
    }

    #[test]
    fn cache_marks_stale_products() {
        let mut cache = Cache { version: CACHE_VERSION, owned: vec![1, 2, 3], ..Default::default() };
        cache.products.insert(1, CachedProduct { product: None, fetched: 1_000 });
        cache.products.insert(2, CachedProduct { product: None, fetched: 1_000 + PRODUCT_TTL });
        assert_eq!(cache.stale_products(1_000 + PRODUCT_TTL), vec![1, 3]);
        let dir = std::env::temp_dir().join(format!("ludex-gog-cache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(CACHE_FILE);
        cache.save(&path);
        let back = Cache::load(&path);
        assert_eq!(back.owned, vec![1, 2, 3]);
        std::fs::write(&path, br#"{"version":99,"owned":[5]}"#).unwrap();
        assert!(Cache::load(&path).owned.is_empty(), "autre version : repart de zéro");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Serveur HTTP local qui répond une fois par connexion, dans l'ordre donné, et garde les
    /// lignes de requête reçues.
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
                let mut auth = String::new();
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line.to_ascii_lowercase().starts_with("authorization:") {
                        auth = line.trim().to_string();
                    }
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                }
                seen.push(format!("{} {}", first.trim(), auth));
                let _ = write!(stream, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
            seen
        });
        (base, handle)
    }

    #[test]
    fn code_exchange_then_owned_list_with_bearer() {
        let (base, server) = server(vec![(200, include_str!("samples/token.json")), (200, include_str!("samples/owned.json"))]);
        let ends = Endpoints { auth: base.clone(), embed: base.clone(), api: base };
        let agent = api::agent();
        let session = api::token(&agent, &ends, Grant::Code("CODE-123"), 10, None).unwrap();
        assert_eq!(session.user_id, "48628349957132247");
        let owned = api::owned(&agent, &ends, &session).unwrap();
        assert_eq!(owned.len(), 5);
        let seen = server.join().unwrap();
        assert!(seen[0].starts_with("GET /token?client_id=46899977096215655&client_secret="));
        assert!(seen[0].contains("grant_type=authorization_code&code=CODE-123&redirect_uri="));
        assert!(seen[1].starts_with("GET /user/data/games"));
        assert!(seen[1].ends_with("Authorization: Bearer faux-jeton-d-acces-0123456789abcdef"));
    }

    #[test]
    fn refused_code_or_refresh_is_unauthorized_without_url() {
        let (base, server) = server(vec![(400, r#"{"error":"invalid_grant"}"#), (401, "{}")]);
        let ends = Endpoints { auth: base.clone(), embed: base.clone(), api: base };
        let agent = api::agent();
        let err = api::token(&agent, &ends, Grant::Code("SECRET-CODE"), 0, None).unwrap_err();
        assert_eq!(err, Failure::Unauthorized);
        assert!(!err.message().contains("SECRET-CODE"));
        let err = api::token(&agent, &ends, Grant::Refresh("SECRET-REFRESH"), 0, None).unwrap_err();
        assert_eq!(err, Failure::Unauthorized);
        server.join().unwrap();
    }
}
