mod art;
mod history;
mod images;
mod library;
mod metadata;
mod session;
mod store;
mod watch;

use library::{Game, Launch, PlaytimeSource, ScanResult};
use serde::Serialize;
use session::{Phase, Tracker};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock, TryLockError};
use std::time::{Duration, Instant};
use store::UserData;
use tauri::http::{header, Response, StatusCode};
use tauri::{Emitter, Manager, Runtime, State};
use tauri_plugin_opener::OpenerExt;

/// Derniers jeux détectés et préférences de l'utilisateur. Le front ne transmet qu'un identifiant :
/// la commande de lancement vient toujours d'ici, jamais de l'interface.
struct AppState {
    games: Mutex<Vec<Game>>,
    data: Mutex<UserData>,
    data_path: PathBuf,
    /// Sessions de jeu lancées depuis Ludex.
    tracker: Tracker,
    /// Jaquettes retrouvées sur la boutique Steam pour les jeux qui n'en ont pas.
    art: Mutex<art::ArtCache>,
    data_dir: PathBuf,
    /// Une recherche de jaquettes tourne déjà.
    art_busy: AtomicBool,
    /// Fiches (genres, studio, sortie…) lues sur la boutique Steam pour les jeux qui ont un appid.
    meta: Mutex<metadata::MetaCache>,
    /// Une lecture de fiches tourne déjà.
    meta_busy: AtomicBool,
    /// Images distantes gardées sur disque (`images.rs`).
    images: Mutex<images::ImageCache>,
    /// Un téléchargement d'images tourne déjà.
    images_busy: AtomicBool,
    /// Tenu pendant un scan : jamais deux scans à la fois.
    scan_lock: Mutex<()>,
    /// Fin du dernier scan.
    last_scan: Mutex<Option<Instant>>,
    /// Surveillance des dossiers des launchers, lancée après le premier scan.
    watch: OnceLock<watch::Watch>,
}

/// Événement `game-art` : jaquette trouvée pour un jeu après le scan, ou image passée en cache.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ArtEvent {
    id: String,
    cover_url: Option<String>,
    hero_url: Option<String>,
    logo_url: Option<String>,
}

const ART_EVENT: &str = "game-art";

impl ArtEvent {
    fn of(g: &Game) -> Self {
        ArtEvent { id: g.id.clone(), cover_url: g.cover_url.clone(), hero_url: g.hero_url.clone(), logo_url: g.logo_url.clone() }
    }
}

/// Cherche en arrière-plan les jaquettes manquantes, puis met à jour l'état et le front jeu par jeu.
fn resolve_art(app: tauri::AppHandle) {
    let Some(state) = app.try_state::<AppState>() else { return };
    let titles = match (lock(&state.art), lock(&state.games)) {
        (Ok(cache), Ok(games)) => cache.to_search(&games),
        _ => return,
    };
    if titles.is_empty() || state.art_busy.swap(true, Ordering::SeqCst) {
        return;
    }
    let dir = state.data_dir.clone();
    std::thread::spawn(move || {
        let Some(state) = app.try_state::<AppState>() else { return };
        let mut search = art::ArtCache::load(&dir);
        search.resolve(titles, |title| {
            let fresh = art::ArtCache::load(&dir);
            let mut events = Vec::new();
            if let Ok(mut games) = lock(&state.games) {
                for g in games.iter_mut().filter(|g| g.title == title) {
                    if fresh.apply(g) {
                        // Images de ce jeu déjà en cache (jaquette retrouvée lors d'un scan précédent).
                        if let Ok(images) = lock(&state.images) {
                            images.apply(g);
                        }
                        events.push(ArtEvent::of(g));
                    }
                }
            }
            if let Ok(mut cache) = lock(&state.art) {
                *cache = fresh;
            }
            for e in events {
                let _ = app.emit(ART_EVENT, e);
            }
        });
        state.art_busy.store(false, Ordering::SeqCst);
        // Les jaquettes trouvées vont en cache, et les jeux retrouvés ont maintenant un appid :
        // leur fiche peut être demandée.
        resolve_images(app.clone());
        resolve_details(app.clone());
    });
}

/// Images distantes à télécharger. Les jeux sont copiés avant de verrouiller le cache : jamais le
/// cache des images puis les jeux (ordre inverse de `resolve_art`).
fn images_to_fetch(state: &AppState) -> Vec<images::Job> {
    let Ok(games) = lock(&state.games).map(|g| g.clone()) else { return Vec::new() };
    lock(&state.images).map(|cache| cache.to_download(&games)).unwrap_or_default()
}

/// Note la présence des jeux scannés, puis purge les images des jeux partis depuis longtemps.
fn tidy_images(state: &AppState) {
    let games = lock(&state.games).map(|g| g.clone()).unwrap_or_default();
    if let Ok(mut cache) = lock(&state.images) {
        cache.touch(&games);
        cache.purge();
        if let Err(e) = cache.save() {
            eprintln!("Index des images non enregistré : {e}");
        }
    }
}

/// Image téléchargée : notée dans l'index, posée sur le jeu, puis signalée au front.
fn publish_image(app: &tauri::AppHandle, state: &AppState, job: &images::Job, file: PathBuf) {
    if let Ok(mut cache) = lock(&state.images) {
        cache.record(job);
        if let Err(e) = cache.save() {
            eprintln!("Index des images non enregistré : {e}");
        }
    }
    let event = lock(&state.games).ok().and_then(|mut games| {
        let g = games.iter_mut().find(|g| g.id == job.game_id)?;
        images::use_file(g, job, file).then(|| ArtEvent::of(g))
    });
    if let Some(e) = event {
        let _ = app.emit(ART_EVENT, e);
    }
}

/// Télécharge et range une image. Renvoie faux sur une erreur réseau.
fn download_image(app: &tauri::AppHandle, state: &AppState, agent: &ureq::Agent, root: &std::path::Path, job: images::Job) -> bool {
    match images::fetch(agent, &job.url) {
        images::Fetch::Image(bytes, ext) => match images::write(root, &job, &bytes, ext) {
            Ok(file) => publish_image(app, state, &job, file),
            Err(e) => eprintln!("Image non enregistrée ({}) : {e}", job.url),
        },
        images::Fetch::Missing => {
            if let Ok(mut cache) = lock(&state.images) {
                cache.note_missing(&job.url);
            }
        }
        images::Fetch::Failed(e) => {
            eprintln!("Image non téléchargée ({}) : {e}", job.url);
            return false;
        }
    }
    true
}

/// Télécharge un lot d'images, quelques-unes à la fois. Renvoie faux si le réseau a lâché
/// (plusieurs échecs de suite) : le reste attend le prochain scan.
fn download_images(app: &tauri::AppHandle, state: &AppState, jobs: Vec<images::Job>) -> bool {
    let Ok(root) = lock(&state.images).map(|c| c.root().to_path_buf()) else { return false };
    let queue = Mutex::new(jobs.into_iter());
    let failures = AtomicUsize::new(0);
    let agent = images::agent();
    std::thread::scope(|s| {
        for _ in 0..images::WORKERS {
            s.spawn(|| {
                while failures.load(Ordering::SeqCst) < images::MAX_FAILURES {
                    let Some(job) = queue.lock().ok().and_then(|mut q| q.next()) else { break };
                    if download_image(app, state, &agent, &root, job) {
                        failures.store(0, Ordering::SeqCst);
                    } else {
                        failures.fetch_add(1, Ordering::SeqCst);
                    }
                }
            });
        }
    });
    if let Ok(cache) = lock(&state.images) {
        let _ = cache.save();
    }
    failures.load(Ordering::SeqCst) < images::MAX_FAILURES
}

/// Range en arrière-plan les images distantes des jeux sur disque, puis met à jour l'état et le
/// front image par image. Recommence si des jaquettes sont apparues entre-temps (`art.rs`).
fn resolve_images(app: tauri::AppHandle) {
    let Some(state) = app.try_state::<AppState>() else { return };
    if state.images_busy.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        let Some(state) = app.try_state::<AppState>() else { return };
        tidy_images(&state);
        for _ in 0..3 {
            let jobs = images_to_fetch(&state);
            if jobs.is_empty() || !download_images(&app, &state, jobs) {
                break;
            }
        }
        state.images_busy.store(false, Ordering::SeqCst);
    });
}

/// Événement `game-details` : fiche reçue pour un jeu après le scan.
#[derive(Clone, Serialize)]
struct DetailsEvent {
    id: String,
    details: metadata::Details,
}

const DETAILS_EVENT: &str = "game-details";

/// Reporte les fiches d'un cache à jour sur les jeux de l'état, garde ce cache et prévient le front.
fn publish_details(app: &tauri::AppHandle, state: &AppState, fresh: &metadata::MetaCache) {
    let mut events = Vec::new();
    if let Ok(mut games) = lock(&state.games) {
        for g in games.iter_mut() {
            if fresh.apply(g)
                && let Some(details) = &g.details
            {
                events.push(DetailsEvent { id: g.id.clone(), details: details.clone() });
            }
        }
    }
    if let Ok(mut cache) = lock(&state.meta) {
        *cache = fresh.clone();
    }
    for e in events {
        let _ = app.emit(DETAILS_EVENT, e);
    }
}

/// Appids sans fiche ou dont la fiche a plus de 30 jours, avec une copie du cache à compléter.
fn details_to_fetch(state: &AppState) -> Option<(metadata::MetaCache, Vec<u32>)> {
    let (cache, games) = (lock(&state.meta).ok()?, lock(&state.games).ok()?);
    let appids = cache.to_fetch(&games);
    (!appids.is_empty()).then(|| (cache.clone(), appids))
}

/// Demande en arrière-plan les fiches manquantes, puis met à jour l'état et le front lot par lot.
/// Recommence si des appids sont apparus entre-temps (jaquettes trouvées pendant la lecture).
fn resolve_details(app: tauri::AppHandle) {
    let Some(state) = app.try_state::<AppState>() else { return };
    if details_to_fetch(&state).is_none() || state.meta_busy.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        let Some(state) = app.try_state::<AppState>() else { return };
        for _ in 0..3 {
            let Some((mut cache, appids)) = details_to_fetch(&state) else { break };
            if !cache.fetch(appids, |fresh| publish_details(&app, &state, fresh)) {
                break;
            }
        }
        state.meta_busy.store(false, Ordering::SeqCst);
    });
}

/// Événement `game-session` : phase d'une session et, quand Ludex vient d'ajouter du temps,
/// le nouveau total du jeu.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionEvent {
    id: String,
    phase: Phase,
    playtime_minutes: Option<u64>,
    playtime_source: Option<PlaytimeSource>,
}

const SESSION_EVENT: &str = "game-session";

/// Enregistre le temps mesuré (s'il y en a) et prévient le front.
fn on_session_update<R: Runtime>(app: &tauri::AppHandle<R>, update: session::Update) {
    let mut event = SessionEvent { id: update.game_id, phase: update.phase, playtime_minutes: None, playtime_source: None };
    if update.credit_secs > 0
        && let Some(state) = app.try_state::<AppState>()
        && let Ok(mut data) = lock(&state.data)
    {
        data.add_playtime(&event.id, update.credit_secs);
        if let Err(e) = data.save(&state.data_path) {
            eprintln!("Temps de jeu non enregistré : {e}");
        }
        if let Ok(mut games) = lock(&state.games)
            && let Some(g) = games.iter_mut().find(|g| g.id == event.id)
        {
            data.apply(g);
            event.playtime_minutes = g.playtime_minutes;
            event.playtime_source = g.playtime_source;
        }
    }
    let _ = app.emit(SESSION_EVENT, event);
}

/// Sessions en cours, pour retrouver l'état « En jeu » après un rechargement de l'interface.
#[tauri::command]
fn game_sessions(state: State<'_, AppState>) -> Vec<SessionEvent> {
    state
        .tracker
        .active()
        .into_iter()
        .map(|(id, phase)| SessionEvent { id, phase, playtime_minutes: None, playtime_source: None })
        .collect()
}

/// Sessions terminées d'un jeu (historique), la plus récente d'abord.
#[tauri::command]
fn game_history(state: State<'_, AppState>, id: String) -> Vec<history::PastSession> {
    state.tracker.history(&id)
}

fn lock<T>(m: &Mutex<T>) -> Result<MutexGuard<'_, T>, String> {
    m.lock().map_err(|_| "État interne indisponible".to_string())
}

/// Scan complet : préférences, jaquettes et fiches appliquées, état mis à jour, puis recherche des
/// images et fiches manquantes. Le verrou `scanning` (pris par l'appelant) empêche deux scans à la
/// fois. Renvoie aussi les jeux apparus et disparus depuis le scan précédent.
fn rescan(app: &tauri::AppHandle, state: &AppState, scanning: MutexGuard<'_, ()>) -> Result<(ScanResult, watch::Changes), String> {
    let mut result = library::scan();
    {
        let data = lock(&state.data)?;
        let art = lock(&state.art)?;
        let meta = lock(&state.meta)?;
        let images = lock(&state.images)?;
        result.games.iter_mut().for_each(|g| {
            data.apply(g);
            art.apply(g);
            meta.apply(g);
            // En dernier : les images distantes (launcher ou `art.rs`) déjà sur disque.
            images.apply(g);
        });
    }
    let changes = {
        let mut games = lock(&state.games)?;
        let changes = watch::changes(&games, &result.games);
        *games = result.games.clone();
        changes
    };
    *lock(&state.last_scan)? = Some(Instant::now());
    drop(scanning);
    // Une bibliothèque Steam ou un dossier « Jeux » a pu apparaître.
    if let Some(w) = state.watch.get() {
        w.rewatch();
    }
    resolve_art(app.clone());
    resolve_images(app.clone());
    resolve_details(app.clone());
    Ok((result, changes))
}

#[tauri::command]
async fn scan_library(app: tauri::AppHandle) -> Result<ScanResult, String> {
    let handle = app.clone();
    let (result, _) = tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        // Un rescan automatique en cours : on attend sa fin plutôt que de scanner en même temps.
        let scanning = state.scan_lock.lock().unwrap_or_else(|e| e.into_inner());
        rescan(&handle, &state, scanning)
    })
    .await
    .map_err(|e| format!("Le scan a échoué : {e}"))??;
    start_watch(&app);
    Ok(result)
}

/// Événement `library-changed` : bibliothèque relue sans demande du front (jeu installé ou
/// désinstallé, retour sur la fenêtre), avec les titres des jeux apparus et disparus.
#[derive(Clone, Serialize)]
struct LibraryEvent {
    added: Vec<String>,
    removed: Vec<String>,
    scan: ScanResult,
}

const LIBRARY_EVENT: &str = "library-changed";

/// Retour sur la fenêtre après ce temps sans scan : rescan, pour les sources lues dans le registre
/// (GOG, Ubisoft, Rockstar, Xbox) que la surveillance des dossiers ne voit pas.
const STALE_ON_FOCUS: Duration = Duration::from_secs(5 * 60);

/// Surveille les dossiers des launchers, après le premier scan seulement : la bibliothèque
/// entière ne doit jamais être annoncée comme nouvelle.
fn start_watch(app: &tauri::AppHandle) {
    let handle = app.clone();
    app.state::<AppState>().watch.get_or_init(|| watch::Watch::spawn(move || auto_rescan(&handle, false)));
}

fn is_stale(state: &AppState) -> bool {
    lock(&state.last_scan).is_ok_and(|last| last.is_some_and(|t| t.elapsed() >= STALE_ON_FOCUS))
}

/// Rescan sans demande du front, puis `library-changed`. `false` : un scan tourne déjà.
fn auto_rescan(app: &tauri::AppHandle, only_if_stale: bool) -> bool {
    let Some(state) = app.try_state::<AppState>() else { return true };
    let scanning = match state.scan_lock.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::WouldBlock) => return false,
        Err(TryLockError::Poisoned(e)) => e.into_inner(),
    };
    if only_if_stale && !is_stale(&state) {
        return true;
    }
    match rescan(app, &state, scanning) {
        Ok((scan, changes)) => {
            let _ = app.emit(LIBRARY_EVENT, LibraryEvent { added: changes.added, removed: changes.removed, scan });
        }
        Err(e) => eprintln!("Rescan automatique impossible : {e}"),
    }
    true
}

/// Modifie les préférences d'un jeu connu, puis les enregistre.
fn update_data(state: &State<'_, AppState>, id: &str, change: impl FnOnce(&mut UserData, &Game)) -> Result<(), String> {
    let game = find_game(state, id)?;
    let mut data = lock(&state.data)?;
    change(&mut data, &game);
    data.save(&state.data_path)?;
    if let Some(g) = lock(&state.games)?.iter_mut().find(|g| g.id == id) {
        data.apply(g);
    }
    Ok(())
}

#[tauri::command]
fn set_favorite(state: State<'_, AppState>, id: String, value: bool) -> Result<(), String> {
    update_data(&state, &id, |data, game| data.set_favorite(&game.id, value))
}

#[tauri::command]
fn set_hidden(state: State<'_, AppState>, id: String, value: bool) -> Result<(), String> {
    update_data(&state, &id, |data, game| data.set_hidden(game, value))
}

fn find_game(state: &State<'_, AppState>, id: &str) -> Result<Game, String> {
    lock(&state.games)?
        .iter()
        .find(|g| g.id == id)
        .cloned()
        .ok_or_else(|| "Ce jeu n'est plus dans la bibliothèque. Relance un scan.".to_string())
}

#[tauri::command]
fn open_install_dir(app: tauri::AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    let dir = find_game(&state, &id)?
        .install_dir
        .ok_or("Dossier d'installation inconnu pour ce jeu.")?;
    app.opener()
        .open_path(&dir, None::<&str>)
        .map_err(|e| format!("Impossible d'ouvrir {dir} : {e}"))
}

/// Lance le jeu et renvoie l'heure du lancement (secondes Unix).
#[tauri::command]
fn launch_game(app: tauri::AppHandle, state: State<'_, AppState>, id: String) -> Result<u64, String> {
    let game = find_game(&state, &id)?;

    match &game.launch {
        Launch::Uri(uri) => app
            .opener()
            .open_url(uri, None::<&str>)
            .map_err(|e| format!("Le launcher n'a pas répondu : {e}")),
        Launch::Exec { path, args } => std::process::Command::new(path)
            .args(args)
            // Beaucoup de jeux cherchent leurs fichiers dans le dossier courant.
            .current_dir(path.parent().unwrap_or(path))
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Impossible de démarrer {} : {e}", path.display())),
    }?;

    // Surveille le jeu pour l'état « En jeu » et, si son launcher ne le fait pas, son temps de jeu.
    state.tracker.start(&game);

    let mut data = lock(&state.data)?;
    let now = data.record_launch(&id);
    // Un historique non enregistré ne doit pas faire croire à un échec du lancement.
    let _ = data.save(&state.data_path);
    Ok(now)
}

/// Version d'un fichier pour l'en-tête `ETag` : taille et date de modification.
fn etag(file: &std::path::Path) -> Option<String> {
    let meta = std::fs::metadata(file).ok()?;
    let modified = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis();
    Some(format!("\"{:x}-{modified:x}\"", meta.len()))
}

/// Contenu et type d'une image : icône extraite de l'exécutable, sinon fichier dont le type est
/// lu dans sa signature (l'extension en dernier recours).
fn read_art(file: &std::path::Path, kind: &str) -> Option<(Vec<u8>, &'static str)> {
    if kind == "icon" {
        return library::exe_icon(file);
    }
    let bytes = std::fs::read(file).ok()?;
    let ext = file.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    let mime = images::mime(images::sniff(&bytes).unwrap_or(ext.as_str()));
    Some((bytes, mime))
}

/// Sert une image locale d'un jeu connu (cache du launcher ou cache de Ludex). L'URL n'est jamais
/// lue comme un chemin : `/<plateforme>/<id plateforme>/<type>` désigne un jeu de l'état, qui
/// fournit le fichier. `no-cache` + `ETag` : la WebView revalide (réponse 304 sans lecture du
/// fichier) et voit tout de suite une image remplacée sous la même URL.
fn serve_art<R: Runtime>(app: &tauri::AppHandle<R>, uri_path: &str, if_none_match: Option<&str>) -> Response<Vec<u8>> {
    let not_found = || Response::builder().status(StatusCode::NOT_FOUND).body(Vec::new()).unwrap();
    let mut parts = uri_path.trim_start_matches('/').splitn(3, '/');
    let (Some(platform), Some(platform_id), Some(kind)) = (parts.next(), parts.next(), parts.next()) else {
        return not_found();
    };
    let id = format!("{platform}:{platform_id}");
    let Some(state) = app.try_state::<AppState>() else { return not_found() };
    let file = match lock(&state.games) {
        Ok(games) => games.iter().find(|g| g.id == id).and_then(|g| g.local_art.get(kind).cloned()),
        Err(_) => None,
    };
    let Some(file) = file else { return not_found() };
    let tag = etag(&file);
    let builder = Response::builder()
        // Lecture des pixels (couleur du jeu) depuis un canvas.
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::CACHE_CONTROL, "no-cache");
    let builder = match &tag {
        Some(tag) => builder.header(header::ETAG, tag),
        None => builder,
    };
    if tag.is_some() && tag.as_deref() == if_none_match {
        return builder.status(StatusCode::NOT_MODIFIED).body(Vec::new()).unwrap_or_else(|_| not_found());
    }
    let Some((bytes, mime)) = read_art(&file, kind) else { return not_found() };
    builder.header(header::CONTENT_TYPE, mime).body(bytes).unwrap_or_else(|_| not_found())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .register_asynchronous_uri_scheme_protocol("ludex", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let path = request.uri().path().to_string();
            let if_none_match = request.headers().get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()).map(String::from);
            tauri::async_runtime::spawn_blocking(move || responder.respond(serve_art(&app, &path, if_none_match.as_deref())));
        })
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let data_path = store::file_in(dir.clone());
            let handle = app.handle().clone();
            app.manage(AppState {
                games: Mutex::default(),
                data: Mutex::new(UserData::load(&data_path)),
                data_path,
                tracker: Tracker::spawn(history::Journal::open(&dir), move |update| on_session_update(&handle, update)),
                art: Mutex::new(art::ArtCache::load(&dir)),
                meta: Mutex::new(metadata::MetaCache::load(&dir)),
                images: Mutex::new(images::ImageCache::load(dir.join("images"))),
                data_dir: dir,
                art_busy: AtomicBool::new(false),
                meta_busy: AtomicBool::new(false),
                images_busy: AtomicBool::new(false),
                scan_lock: Mutex::new(()),
                last_scan: Mutex::new(None),
                watch: OnceLock::new(),
            });
            // Une fois l'état en place : le temps mesuré avant l'arrêt précédent peut y être enregistré.
            app.state::<AppState>().tracker.resume();
            Ok(())
        })
        .on_window_event(|window, event| {
            // Retour sur Ludex après un long moment : relit les launchers sans surveillance.
            if let tauri::WindowEvent::Focused(true) = event
                && window.app_handle().try_state::<AppState>().is_some_and(|s| is_stale(&s))
            {
                let app = window.app_handle().clone();
                std::thread::spawn(move || auto_rescan(&app, true));
            }
        })
        .invoke_handler(tauri::generate_handler![
            scan_library,
            launch_game,
            open_install_dir,
            set_favorite,
            set_hidden,
            game_sessions,
            game_history
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
