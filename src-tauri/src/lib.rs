mod art;
mod history;
mod images;
mod library;
mod metadata;
mod moods;
mod session;
mod store;
mod update;
mod tray;
mod watch;

use library::{manual, Game, Launch, Platform, PlaytimeSource, ScanResult};
use serde::Serialize;
use session::{Phase, Tracker};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock, TryLockError};
use std::time::{Duration, Instant};
use store::UserData;
use tauri::http::{header, Response, StatusCode};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tauri_plugin_autostart::AutoLaunchManager;
use tauri_plugin_dialog::DialogExt;
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
    /// Dernier menu posé sur l'icône de la zone de notification : reconstruit seulement s'il change.
    tray_menu: Mutex<Vec<tray::Entry>>,
    /// Fenêtre sur le point d'être cachée, après l'explication de la première fermeture.
    hide_pending: AtomicBool,
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

/// Temps de jeu des 14 derniers jours, tiré de l'historique (bas de la barre latérale).
#[tauri::command]
fn recent_playtime(state: State<'_, AppState>) -> history::RecentPlaytime {
    state.tracker.recent_playtime()
}

/// Dernière session de chaque jeu, la plus récente d'abord (section « Historique »).
#[tauri::command]
fn last_sessions(state: State<'_, AppState>) -> Vec<history::PastSession> {
    state.tracker.last_sessions()
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
        // D'abord les jeux ajoutés à la main et les corrections : le titre choisi sert aux jaquettes.
        data.customize(&mut result.games);
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
    refresh_tray(app, false);
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
fn update_data(state: &AppState, id: &str, change: impl FnOnce(&mut UserData, &Game)) -> Result<(), String> {
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
fn set_hidden(app: AppHandle, state: State<'_, AppState>, id: String, value: bool) -> Result<(), String> {
    update_data(&state, &id, |data, game| data.set_hidden(game, value))?;
    // Un jeu masqué quitte les récents du menu de la zone de notification.
    refresh_tray(&app, false);
    Ok(())
}

fn find_game(state: &AppState, id: &str) -> Result<Game, String> {
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

/// Jeu ajouté ou modifié, avec la bibliothèque relue : le front n'a pas à rescanner.
#[derive(Serialize)]
struct LibraryUpdate {
    id: String,
    title: String,
    scan: ScanResult,
}

/// Titre reçu du front au-delà de cette taille (octets) : refusé avant tout traitement.
const MAX_TITLE_INPUT: usize = 1024;

/// Tâche bloquante (sélecteur de fichier, scan) hors du fil de l'interface.
async fn blocking<T: Send + 'static>(task: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(task).await.map_err(|e| format!("Opération interrompue : {e}"))?
}

/// Attend la fin d'un scan en cours (automatique ou demandé) : jamais deux à la fois.
fn wait_scan(state: &AppState) -> MutexGuard<'_, ()> {
    state.scan_lock.lock().unwrap_or_else(|e| e.into_inner())
}

/// Sélecteur de fichier natif, limité aux exécutables, au-dessus de la fenêtre de Ludex. Le chemin
/// choisi reste côté Rust : le front ne le voit ni ne le transmet. `None` : annulé.
fn pick_exe(window: &tauri::WebviewWindow, title: &str, start: Option<&Path>) -> Option<PathBuf> {
    let mut dialog = window.dialog().file().set_title(title).add_filter("Exécutable", &["exe"]).set_parent(window);
    if let Some(dir) = start.filter(|d| d.is_dir()) {
        dialog = dialog.set_directory(dir);
    }
    dialog.blocking_pick_file()?.into_path().ok()
}

/// Un exécutable déjà lancé par un autre jeu de la bibliothèque ne crée pas de doublon.
fn check_unused(games: &[Game], exe: &Path, except: Option<&str>) -> Result<(), String> {
    match manual::same_exe(games, exe, except) {
        Some(g) => Err(format!("« {} » lance déjà cet exécutable : il est déjà dans la bibliothèque.", g.title)),
        None => Ok(()),
    }
}

/// Modifie et enregistre les préférences, puis rescanne (le scan applique les corrections).
fn save_and_rescan<R>(
    app: &tauri::AppHandle,
    state: &AppState,
    scanning: MutexGuard<'_, ()>,
    change: impl FnOnce(&mut UserData) -> R,
) -> Result<(R, ScanResult), String> {
    let out = {
        let mut data = lock(&state.data)?;
        let out = change(&mut data);
        data.save(&state.data_path)?;
        out
    };
    let (scan, _) = rescan(app, state, scanning)?;
    Ok((out, scan))
}

/// Réponse d'un ajout ou d'une modification : le jeu doit être dans la bibliothèque relue.
fn updated(id: String, scan: ScanResult) -> Result<LibraryUpdate, String> {
    let title = scan
        .games
        .iter()
        .find(|g| g.id == id)
        .map(|g| g.title.clone())
        .ok_or("Le jeu n'apparaît pas dans la bibliothèque : son exécutable a-t-il disparu ?")?;
    Ok(LibraryUpdate { id, title, scan })
}

/// Ajoute un jeu à la main : sélecteur de fichier, validation, titre deviné (nom du produit, sinon
/// dossier), enregistrement puis rescan. `None` : sélection annulée.
#[tauri::command]
async fn add_game(app: tauri::AppHandle, window: tauri::WebviewWindow) -> Result<Option<LibraryUpdate>, String> {
    blocking(move || {
        let Some(picked) = pick_exe(&window, "Ajouter un jeu", None) else { return Ok(None) };
        let exe = manual::check_exe(&picked)?;
        let state = app.state::<AppState>();
        let scanning = wait_scan(&state);
        {
            let games = lock(&state.games)?;
            check_unused(&games, &exe, None)?;
            if let Some(g) = manual::folder_owner(&games, &exe) {
                return Err(format!(
                    "Cet exécutable est dans le dossier de « {} », déjà dans la bibliothèque. Pour le lancer avec, ouvre sa fiche et choisis « Changer l'exécutable ».",
                    g.title
                ));
            }
        }
        let title = manual::guess_title(&exe);
        let (id, scan) = save_and_rescan(&app, &state, scanning, move |data| data.add_manual(exe, title))?;
        updated(id, scan).map(Some)
    })
    .await
}

/// Remplace l'exécutable d'un jeu « Autres » (détecté ou ajouté à la main). Le sélecteur s'ouvre
/// dans le dossier du jeu ; pour un jeu détecté, l'exécutable doit y rester (suivi des sessions).
#[tauri::command]
async fn change_executable(app: tauri::AppHandle, window: tauri::WebviewWindow, id: String) -> Result<Option<LibraryUpdate>, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let game = find_game(&state, &id)?;
        if game.platform != Platform::Other {
            return Err(format!("{} est lancé par son launcher : son exécutable ne se change pas dans Ludex.", game.title));
        }
        let start = game.install_dir.as_deref().map(Path::new);
        let Some(picked) = pick_exe(&window, &format!("Exécutable de {}", game.title), start) else { return Ok(None) };
        let exe = manual::check_exe(&picked)?;
        if !game.manual {
            let dir = game.install_dir.as_deref().ok_or("Dossier du jeu inconnu.")?;
            if !manual::is_in_dir(&exe, dir) {
                return Err(format!("Choisis un exécutable dans le dossier du jeu : {dir}"));
            }
        }
        let scanning = wait_scan(&state);
        check_unused(&lock(&state.games)?, &exe, Some(&id))?;
        let ((), scan) = save_and_rescan(&app, &state, scanning, |data| data.set_exe(&id, exe))?;
        updated(id, scan).map(Some)
    })
    .await
}

/// Renomme un jeu (le titre sert aussi à chercher sa jaquette). Titre vide : titre d'origine.
#[tauri::command]
async fn rename_game(app: tauri::AppHandle, id: String, title: String) -> Result<LibraryUpdate, String> {
    if title.len() > MAX_TITLE_INPUT {
        return Err(format!("Titre trop long : {} caractères au plus.", manual::MAX_TITLE));
    }
    blocking(move || {
        let state = app.state::<AppState>();
        find_game(&state, &id)?;
        let title = manual::clean_title(&title);
        let scanning = wait_scan(&state);
        let ((), scan) = save_and_rescan(&app, &state, scanning, |data| data.rename(&id, title))?;
        updated(id, scan)
    })
    .await
}

/// Retire un jeu ajouté à la main. Les jeux détectés ne se retirent pas : ils se masquent.
#[tauri::command]
async fn remove_game(app: tauri::AppHandle, id: String) -> Result<ScanResult, String> {
    if !manual::is_manual(&id) {
        return Err("Seuls les jeux ajoutés à la main se retirent. Les autres se masquent.".into());
    }
    blocking(move || {
        let state = app.state::<AppState>();
        let scanning = wait_scan(&state);
        let (removed, scan) = save_and_rescan(&app, &state, scanning, |data| data.remove_manual(&id))?;
        if removed { Ok(scan) } else { Err("Ce jeu n'est plus dans la bibliothèque. Relance un scan.".into()) }
    })
    .await
}

/// Lance le jeu et renvoie l'heure du lancement (secondes Unix).
#[tauri::command]
fn launch_game(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<u64, String> {
    launch(&app, &state, &id)
}

/// Lance un jeu connu, démarre son suivi et note la date : commun au front et à la zone de
/// notification. Renvoie l'heure du lancement (secondes Unix).
fn launch<R: Runtime>(app: &AppHandle<R>, state: &AppState, id: &str) -> Result<u64, String> {
    let game = find_game(state, id)?;

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

    let now = {
        let mut data = lock(&state.data)?;
        let now = data.record_launch(id);
        // Un historique non enregistré ne doit pas faire croire à un échec du lancement.
        let _ = data.save(&state.data_path);
        if let Ok(mut games) = lock(&state.games)
            && let Some(g) = games.iter_mut().find(|g| g.id == id)
        {
            data.apply(g);
        }
        now
    };
    // Le jeu passe en tête des récents du menu.
    refresh_tray(app, false);
    Ok(now)
}

/// Événement `tray-launch` : jeu lancé depuis la zone de notification, pour mettre à jour la date
/// du dernier lancement côté front, ou afficher l'erreur.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrayLaunchEvent {
    id: String,
    title: String,
    last_played: Option<u64>,
    error: Option<String>,
}

const TRAY_LAUNCH_EVENT: &str = "tray-launch";
/// Événement `tray-hint` : première fermeture, la fenêtre va se cacher dans la zone de notification.
const TRAY_HINT_EVENT: &str = "tray-hint";
/// Temps laissé au toast d'explication avant de cacher la fenêtre.
const HINT_DELAY: Duration = Duration::from_millis(3200);

/// Affiche la fenêtre principale et la met au premier plan, restaurée si elle était réduite.
fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(state) = app.try_state::<AppState>() {
        state.hide_pending.store(false, Ordering::SeqCst);
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn autostart_enabled<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.try_state::<AutoLaunchManager>().is_some_and(|m| m.is_enabled().unwrap_or(false))
}

/// Reconstruit le menu de la zone de notification s'il a changé (`force` : toujours, après un
/// clic sur une case à cocher que Windows a déjà basculée). Le menu est construit sur le fil
/// principal sans l'attendre : appelée depuis le fil d'un scan, une construction directe attendrait
/// le fil principal et pourrait bloquer le scan. Aucun verrou n'est tenu pendant la construction.
fn refresh_tray<R: Runtime>(app: &AppHandle<R>, force: bool) {
    let (Some(state), Some(_)) = (app.try_state::<AppState>(), app.tray_by_id(tray::TRAY_ID)) else { return };
    let keep = lock(&state.data).map(|d| d.settings.keep_in_tray).unwrap_or(true);
    let autostart = autostart_enabled(app);
    let Ok(entries) = lock(&state.games).map(|games| tray::entries(&tray::recent(&games, tray::RECENT_COUNT), autostart, keep)) else {
        return;
    };
    match lock(&state.tray_menu) {
        Ok(mut last) if force || *last != entries => *last = entries.clone(),
        _ => return,
    }
    let handle = app.clone();
    let queued = app.run_on_main_thread(move || {
        let Some(icon) = handle.tray_by_id(tray::TRAY_ID) else { return };
        match tray::build(&handle, &entries) {
            Ok(menu) => {
                if let Err(e) = icon.set_menu(Some(menu)) {
                    eprintln!("Menu de la zone de notification non mis à jour : {e}");
                }
            }
            Err(e) => eprintln!("Menu de la zone de notification non construit : {e}"),
        }
    });
    if let Err(e) = queued {
        eprintln!("Menu de la zone de notification non mis à jour : {e}");
    }
}

/// Modifie les réglages de l'application, puis les enregistre.
fn update_settings<R: Runtime>(app: &AppHandle<R>, change: impl FnOnce(&mut store::Settings)) {
    let Some(state) = app.try_state::<AppState>() else { return };
    if let Ok(mut data) = lock(&state.data) {
        change(&mut data.settings);
        if let Err(e) = data.save(&state.data_path) {
            eprintln!("Réglages non enregistrés : {e}");
        }
    }
}

/// Active ou coupe le lancement avec Windows. L'entrée du registre fait foi : le réglage
/// enregistré reprend son état réel après la modification.
fn toggle_autostart<R: Runtime>(app: &AppHandle<R>) {
    let Some(manager) = app.try_state::<AutoLaunchManager>() else { return };
    let result = if autostart_enabled(app) { manager.disable() } else { manager.enable() };
    if let Err(e) = result {
        eprintln!("Lancement avec Windows non modifié : {e}");
    }
    let enabled = autostart_enabled(app);
    update_settings(app, |s| s.launch_at_startup = enabled);
}

fn on_tray_menu<R: Runtime>(app: &AppHandle<R>, id: &str) {
    match tray::action(id) {
        Some(tray::Action::Launch(game)) => {
            let Some(state) = app.try_state::<AppState>() else { return };
            let title = find_game(&state, &game).map_or_else(|_| "ce jeu".to_string(), |g| g.title);
            let event = match launch(app, &state, &game) {
                Ok(ts) => TrayLaunchEvent { id: game, title, last_played: Some(ts), error: None },
                Err(e) => {
                    // L'erreur s'affiche dans la fenêtre : elle doit être visible.
                    show_main(app);
                    TrayLaunchEvent { id: game, title, last_played: None, error: Some(e) }
                }
            };
            let _ = app.emit(TRAY_LAUNCH_EVENT, event);
        }
        Some(tray::Action::Open) => show_main(app),
        Some(tray::Action::ToggleAutostart) => {
            toggle_autostart(app);
            refresh_tray(app, true);
        }
        Some(tray::Action::ToggleKeepInTray) => {
            update_settings(app, |s| s.keep_in_tray = !s.keep_in_tray);
            refresh_tray(app, true);
        }
        Some(tray::Action::Quit) => app.exit(0),
        None => {}
    }
}

/// Pose l'icône dans la zone de notification. Clic gauche : la fenêtre ; clic droit : le menu.
fn create_tray(app: &tauri::App) -> tauri::Result<()> {
    let mut builder = TrayIconBuilder::with_id(tray::TRAY_ID)
        .tooltip("Ludex")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_tray_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|icon, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(icon.app_handle());
            }
        });
    if let Some(image) = app.default_window_icon() {
        builder = builder.icon(image.clone());
    }
    builder.build(app)?;
    refresh_tray(app.handle(), true);
    Ok(())
}

/// Fermeture de la fenêtre avec « garder dans la zone de notification » actif : la fenêtre est
/// cachée, Ludex continue (suivi des parties compris). La première fois, le front explique ce qui
/// se passe avant que la fenêtre ne disparaisse.
fn hide_to_tray<R: Runtime>(window: &tauri::Window<R>, state: &AppState) {
    let first = lock(&state.data).is_ok_and(|mut data| {
        let first = !data.settings.tray_hint_shown;
        if first {
            data.settings.tray_hint_shown = true;
            if let Err(e) = data.save(&state.data_path) {
                eprintln!("Réglages non enregistrés : {e}");
            }
        }
        first
    });
    if !first || window.emit(TRAY_HINT_EVENT, ()).is_err() {
        let _ = window.hide();
        return;
    }
    state.hide_pending.store(true, Ordering::SeqCst);
    let window = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(HINT_DELAY);
        // Fenêtre ré-affichée entre-temps (icône, nouvelle instance) : elle reste visible.
        if window.app_handle().try_state::<AppState>().is_some_and(|s| s.hide_pending.swap(false, Ordering::SeqCst)) {
            let _ = window.hide();
        }
    });
}

fn keep_in_tray(state: &AppState) -> bool {
    lock(&state.data).map(|d| d.settings.keep_in_tray).unwrap_or(true)
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
        // En premier : relancer Ludex alors qu'il tourne ramène la fenêtre existante.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        // Utilisé depuis Rust seulement (menu de la zone de notification) : aucune permission au front.
        .plugin(tauri_plugin_autostart::Builder::new().arg(tray::MINIMIZED_ARG).build())
        .plugin(tauri_plugin_opener::init())
        // Aucune permission du plugin côté front : tout passe par `update.rs`.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(update::PendingUpdate::default())
        // Sélecteur de fichier, utilisé seulement depuis Rust : aucune permission `dialog` côté front.
        .plugin(tauri_plugin_dialog::init())
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
                tray_menu: Mutex::default(),
                hide_pending: AtomicBool::new(false),
            });
            // Une fois l'état en place : le temps mesuré avant l'arrêt précédent peut y être enregistré.
            app.state::<AppState>().tracker.resume();
            update::spawn_check(app.handle().clone());
            // Le réglage enregistré reprend l'état réel du registre (entrée retirée par Windows…).
            let enabled = autostart_enabled(app.handle());
            update_settings(app.handle(), |s| s.launch_at_startup = enabled);
            create_tray(app)?;
            // Fenêtre créée cachée : lancé avec Windows, Ludex reste dans la zone de notification.
            if !std::env::args().any(|a| a == tray::MINIMIZED_ARG) {
                show_main(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // Retour sur Ludex après un long moment : relit les launchers sans surveillance.
            tauri::WindowEvent::Focused(true) if window.app_handle().try_state::<AppState>().is_some_and(|s| is_stale(&s)) => {
                let app = window.app_handle().clone();
                std::thread::spawn(move || auto_rescan(&app, true));
            }
            tauri::WindowEvent::CloseRequested { api, .. } => {
                if let Some(state) = window.app_handle().try_state::<AppState>()
                    && keep_in_tray(&state)
                {
                    api.prevent_close();
                    hide_to_tray(window, &state);
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            scan_library,
            launch_game,
            open_install_dir,
            set_favorite,
            set_hidden,
            game_sessions,
            game_history,
            recent_playtime,
            last_sessions,
            update::available_update,
            update::install_update,
            add_game,
            change_executable,
            rename_game,
            remove_game
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
