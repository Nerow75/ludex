mod art;
mod library;
mod session;
mod store;

use library::{Game, Launch, PlaytimeSource, ScanResult};
use serde::Serialize;
use session::{Phase, Tracker};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
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
}

/// Événement `game-art` : jaquette trouvée pour un jeu après le scan.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ArtEvent {
    id: String,
    cover_url: Option<String>,
    hero_url: Option<String>,
    logo_url: Option<String>,
}

const ART_EVENT: &str = "game-art";

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
                        events.push(ArtEvent { id: g.id.clone(), cover_url: g.cover_url.clone(), hero_url: g.hero_url.clone(), logo_url: g.logo_url.clone() });
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

fn lock<T>(m: &Mutex<T>) -> Result<MutexGuard<'_, T>, String> {
    m.lock().map_err(|_| "État interne indisponible".to_string())
}

#[tauri::command]
async fn scan_library(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<ScanResult, String> {
    let mut result = tauri::async_runtime::spawn_blocking(library::scan)
        .await
        .map_err(|e| format!("Le scan a échoué : {e}"))?;
    {
        let data = lock(&state.data)?;
        let art = lock(&state.art)?;
        result.games.iter_mut().for_each(|g| {
            data.apply(g);
            art.apply(g);
        });
    }
    *lock(&state.games)? = result.games.clone();
    resolve_art(app);
    Ok(result)
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

/// Sert une image locale d'un jeu connu. L'URL n'est jamais lue comme un chemin :
/// `/<plateforme>/<id plateforme>/<type>` désigne un jeu de l'état, qui fournit le fichier.
fn serve_art<R: Runtime>(app: &tauri::AppHandle<R>, uri_path: &str) -> Response<Vec<u8>> {
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
    let (bytes, mime) = if kind == "icon" {
        // Icône extraite de l'exécutable du jeu (chemin tiré de l'état, jamais de l'URL).
        let Some(icon) = library::exe_icon(&file) else { return not_found() };
        icon
    } else {
        let Ok(bytes) = std::fs::read(&file) else { return not_found() };
        let mime = match file.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
            Some("png") => "image/png",
            Some("webp") => "image/webp",
            _ => "image/jpeg",
        };
        (bytes, mime)
    };
    Response::builder()
        .header(header::CONTENT_TYPE, mime)
        // Lecture des pixels (couleur du jeu) depuis un canvas.
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::CACHE_CONTROL, "max-age=86400")
        .body(bytes)
        .unwrap_or_else(|_| not_found())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .register_asynchronous_uri_scheme_protocol("ludex", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let path = request.uri().path().to_string();
            tauri::async_runtime::spawn_blocking(move || responder.respond(serve_art(&app, &path)));
        })
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let data_path = store::file_in(dir.clone());
            let handle = app.handle().clone();
            app.manage(AppState {
                games: Mutex::default(),
                data: Mutex::new(UserData::load(&data_path)),
                data_path,
                tracker: Tracker::spawn(move |update| on_session_update(&handle, update)),
                art: Mutex::new(art::ArtCache::load(&dir)),
                data_dir: dir,
                art_busy: AtomicBool::new(false),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            scan_library,
            launch_game,
            open_install_dir,
            set_favorite,
            set_hidden,
            game_sessions
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
