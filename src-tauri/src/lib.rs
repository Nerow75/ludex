#[macro_use]
mod i18n;
mod art;
mod collections;
mod corrections;
mod covers;
mod deeplink;
mod diagnostic;
mod discord;
mod footprint;
mod history;
mod hltb;
mod images;
mod journal;
mod jumplist;
mod launch_args;
mod library;
mod metadata;
mod recap;
mod relocate;
mod requirements;
mod screenshots;
mod stats;
mod saves;
mod moods;
mod news;
mod notify;
mod quick;
mod performance;
mod session;
mod story;
mod settings;
mod shortcut;
mod showcase;
mod storage;
mod store;
mod update;
mod transfer;
mod tray;
mod watch;
mod welcome;
mod window_controls;
mod wishlist;
mod workshop;

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
use tauri_plugin_deep_link::DeepLinkExt;
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
    /// Durées de vie (HowLongToBeat) des jeux, par titre.
    hltb: Mutex<hltb::HltbCache>,
    /// Une lecture des durées tourne déjà.
    hltb_busy: AtomicBool,
    /// Images distantes gardées sur disque (`images.rs`).
    images: Mutex<images::ImageCache>,
    /// Un téléchargement d'images tourne déjà.
    images_busy: AtomicBool,
    /// Images remplacées par une jaquette perso (`covers.rs`), rendues par « Jaquette d'origine ».
    /// Verrouillé après `games` (jamais l'inverse).
    originals: Mutex<covers::Originals>,
    /// Pourcentages mondiaux des succès Steam (`showcase.rs`), lus à la demande.
    global: Mutex<showcase::GlobalCache>,
    /// Tailles des dossiers d'installation mesurées par Ludex (`storage.rs`).
    sizes: Mutex<storage::SizeCache>,
    /// Une mesure des dossiers tourne déjà.
    sizes_busy: AtomicBool,
    /// Emplacements des jeux ouverts dans la vue avancée du Stockage (`footprint.rs`), gardés après
    /// leur départ de la bibliothèque pour en montrer les restes. Mémoire seulement.
    footprints: Mutex<std::collections::HashMap<String, footprint::Targets>>,
    /// Tenu pendant un scan : jamais deux scans à la fois.
    scan_lock: Mutex<()>,
    /// Fin du dernier scan complet (pas le premier affichage d'un scan progressif).
    last_scan: Mutex<Option<Instant>>,
    /// Le prochain `scan_library` est le premier du démarrage : scan progressif.
    first_scan: AtomicBool,
    /// Surveillance des dossiers des launchers, lancée après le premier scan.
    watch: OnceLock<watch::Watch>,
    /// Dernier menu posé sur l'icône de la zone de notification : reconstruit seulement s'il change.
    tray_menu: Mutex<Vec<tray::Entry>>,
    /// Jump List de la barre des tâches (jeux récents), posée par son propre fil.
    jump_list: jumplist::JumpList,
    /// Fenêtre sur le point d'être cachée, après l'explication de la première fermeture.
    hide_pending: AtomicBool,
    /// Captures envoyées au front, retrouvées par leur empreinte pour être servies.
    shots: Mutex<screenshots::Index>,
    /// Entrées de la base des sauvegardes (Ludusavi) pour les jeux de la bibliothèque.
    saves: Mutex<saves::Known>,
    /// Une lecture de la base des sauvegardes tourne déjà.
    saves_busy: AtomicBool,
    /// Tenu pendant une copie ou une restauration : jamais deux à la fois.
    saves_lock: Mutex<()>,
    /// Configurations requises des jeux (`requirements.rs`), lues à la demande.
    requirements: Mutex<requirements::ReqCache>,
    /// Tenu pendant une requête de configuration : une seule à la fois.
    requirements_lock: Mutex<()>,
    /// Une requête de configuration tourne.
    requirements_busy: AtomicBool,
    /// Titres et vignettes des mods Steam Workshop (`workshop.rs`), lus à la demande.
    workshop: Mutex<workshop::WorkshopCache>,
    /// Tenu pendant les requêtes de l'API Workshop : une lecture à la fois.
    workshop_lock: Mutex<()>,
    /// Une lecture de l'API Workshop tourne.
    workshop_busy: AtomicBool,
    /// Archive d'import lue et vérifiée, en attente de confirmation (`transfer.rs`).
    pending_import: Mutex<Option<transfer::Export>>,
    /// Annonces officielles des jeux (`news.rs`), lues à l'ouverture d'une fiche.
    news: Mutex<news::NewsCache>,
    /// Une vérification des actualités des jeux « En cours » tourne déjà.
    news_busy: AtomicBool,
    /// Annonces à signaler quand la fenêtre principale sera de nouveau visible.
    pending_news: Mutex<Vec<news::Alert>>,
    /// Liste de souhaits (`wishlist.rs`) : prix en cache, résultats de recherche, promos en attente.
    wishlist: wishlist::State,
    /// Un déplacement des données lourdes tourne (`relocate.rs`).
    relocating: AtomicBool,
    /// Dossier des données choisi avec le sélecteur, en attente de confirmation (`relocate.rs`).
    picked_data_dir: Mutex<Option<PathBuf>>,
}

/// Événement `game-art` : jaquette trouvée pour un jeu après le scan, image passée en cache, ou
/// jaquette perso choisie ou retirée.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ArtEvent {
    id: String,
    cover_url: Option<String>,
    hero_url: Option<String>,
    logo_url: Option<String>,
    custom_art: Vec<covers::Kind>,
}

const ART_EVENT: &str = "game-art";

impl ArtEvent {
    fn of(g: &Game) -> Self {
        ArtEvent {
            id: g.id.clone(),
            cover_url: g.cover_url.clone(),
            hero_url: g.hero_url.clone(),
            logo_url: g.logo_url.clone(),
            custom_art: g.custom_art.clone(),
        }
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
        resolve_hltb(app.clone());
    });
}

/// Images distantes à télécharger. Les jeux sont copiés avant de verrouiller le cache : jamais le
/// cache des images puis les jeux (ordre inverse de `resolve_art`).
fn images_to_fetch(state: &AppState) -> Vec<images::Job> {
    let Ok(games) = lock(&state.games).map(|g| g.clone()) else { return Vec::new() };
    lock(&state.images).map(|cache| cache.to_download(&games)).unwrap_or_default()
}

/// Note la présence des jeux scannés, puis purge les images des jeux partis depuis longtemps et
/// les jaquettes perso des jeux que plus rien ne référence.
fn tidy_images(state: &AppState) {
    let games = lock(&state.games).map(|g| g.clone()).unwrap_or_default();
    let Ok(chosen) = lock(&state.data).map(|d| d.covers.keys().cloned().collect::<Vec<_>>()) else { return };
    if let Ok(mut cache) = lock(&state.images) {
        cache.touch(&games);
        cache.purge();
        covers::purge(cache.root(), chosen.iter().map(String::as_str).chain(games.iter().map(|g| g.id.as_str())));
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
            Ok(file) => {
                diagnostic::ok(diagnostic::Service::Images);
                publish_image(app, state, &job, file);
            }
            Err(e) => eprintln!("Image non enregistrée ({}) : {e}", job.url),
        },
        images::Fetch::Missing => {
            diagnostic::ok(diagnostic::Service::Images);
            if let Ok(mut cache) = lock(&state.images) {
                cache.note_missing(&job.url);
            }
        }
        images::Fetch::Failed(e) => {
            diagnostic::failed(diagnostic::Service::Images, &e);
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
    // Disque des données débranché : rien n'est téléchargé (ni créé ailleurs).
    if relocate::unavailable(&state) {
        eprintln!("Dossier des données de Ludex introuvable : images non téléchargées.");
        return;
    }
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
    // Genres corrigés copiés avant de prendre la bibliothèque (jamais `data` sous `games`) : une
    // fiche qui arrive ne doit pas effacer la correction de l'utilisateur.
    let genres = lock(&state.data).map(|d| d.genres.clone()).unwrap_or_default();
    if let Ok(mut games) = lock(&state.games) {
        for g in games.iter_mut() {
            let before = g.details.clone();
            fresh.apply(g);
            corrections::apply_genres(g, genres.get(&g.id));
            if g.details != before
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
        // Les années de sortie départagent les homonymes : les durées sont cherchées après les fiches.
        resolve_hltb(app.clone());
    });
}

/// Événement `game-hltb` : durée de vie reçue pour un jeu après le scan.
#[derive(Clone, Serialize)]
struct HltbEvent {
    id: String,
    hltb: hltb::Hltb,
}

const HLTB_EVENT: &str = "game-hltb";

/// Reporte les durées d'un cache à jour sur les jeux de l'état, garde ce cache et prévient le front.
fn publish_hltb(app: &tauri::AppHandle, state: &AppState, fresh: &hltb::HltbCache) {
    let mut events = Vec::new();
    if let Ok(mut games) = lock(&state.games) {
        for g in games.iter_mut() {
            if fresh.apply(g)
                && let Some(times) = &g.hltb
            {
                events.push(HltbEvent { id: g.id.clone(), hltb: times.clone() });
            }
        }
    }
    if let Ok(mut cache) = lock(&state.hltb) {
        *cache = fresh.clone();
    }
    for e in events {
        let _ = app.emit(HLTB_EVENT, e);
    }
}

/// Titres sans durée ou dont la durée a plus de 30 jours, avec une copie du cache à compléter.
fn hltb_to_fetch(state: &AppState) -> Option<(hltb::HltbCache, Vec<hltb::Query>)> {
    let (cache, games) = (lock(&state.hltb).ok()?, lock(&state.games).ok()?);
    let queries = cache.to_fetch(&games);
    (!queries.is_empty()).then(|| (cache.clone(), queries))
}

/// Cherche en arrière-plan les durées manquantes (HowLongToBeat), une requête à la fois, puis met
/// à jour l'état et le front titre par titre. Attend la fin des jaquettes et des fiches (titres
/// retrouvés, années de sortie) : leurs fils rappellent cette fonction en finissant.
fn resolve_hltb(app: tauri::AppHandle) {
    let Some(state) = app.try_state::<AppState>() else { return };
    if state.art_busy.load(Ordering::SeqCst) || state.meta_busy.load(Ordering::SeqCst) {
        return;
    }
    if hltb_to_fetch(&state).is_none() || state.hltb_busy.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        let Some(state) = app.try_state::<AppState>() else { return };
        for _ in 0..2 {
            let Some((mut cache, queries)) = hltb_to_fetch(&state) else { break };
            if !cache.fetch(queries, None, |fresh| publish_hltb(&app, &state, fresh)) {
                break;
            }
        }
        state.hltb_busy.store(false, Ordering::SeqCst);
    });
}

/// Événement `game-news` : annonces récentes de jeux « En cours », jamais signalées.
const NEWS_EVENT: &str = "game-news";

/// Vérification de fond des actualités des jeux « En cours », au plus une fois par jour : quelques
/// requêtes espacées, puis `game-news` pour les annonces de moins de 3 jours jamais signalées.
fn resolve_news(app: tauri::AppHandle) {
    let Some(state) = app.try_state::<AppState>() else { return };
    if !lock(&state.news).is_ok_and(|c| c.sweep_due(news::now())) || state.news_busy.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        let Some(state) = app.try_state::<AppState>() else { return };
        sweep_news(&app, &state);
        state.news_busy.store(false, Ordering::SeqCst);
    });
}

fn sweep_news(app: &tauri::AppHandle, state: &AppState) {
    let games: Vec<(String, String, u32)> = match lock(&state.games) {
        Ok(games) => games
            .iter()
            .filter(|g| g.status == Some(collections::Status::Playing) && !g.hidden)
            .filter_map(|g| g.steam_appid.map(|appid| (g.id.clone(), g.title.clone(), appid)))
            .take(news::SWEEP_MAX)
            .collect(),
        Err(_) => return,
    };
    let agent = news::agent();
    let mut asked = false;
    for (_, _, appid) in &games {
        if !lock(&state.news).is_ok_and(|c| c.stale(*appid, news::now())) {
            continue;
        }
        if asked {
            std::thread::sleep(news::SWEEP_GAP);
        }
        asked = true;
        match news::fetch(&agent, *appid) {
            Ok(items) => {
                diagnostic::ok(diagnostic::Service::News);
                if let Ok(mut cache) = lock(&state.news) {
                    cache.record(*appid, items, news::now());
                }
            }
            Err(e) => {
                // Réessayée au prochain scan : la journée n'est pas comptée comme vérifiée.
                diagnostic::failed(diagnostic::Service::News, &e);
                eprintln!("Vérification des actualités interrompue : {e}");
                if let Ok(cache) = lock(&state.news)
                    && let Err(e) = cache.save()
                {
                    eprintln!("Cache des actualités non enregistré : {e}");
                }
                return;
            }
        }
    }
    let alerts = {
        let Ok(mut cache) = lock(&state.news) else { return };
        let at = news::now();
        cache.swept(at);
        let alerts = cache.alerts(&games, at);
        if let Err(e) = cache.save() {
            eprintln!("Cache des actualités non enregistré : {e}");
        }
        alerts
    };
    if !alerts.is_empty() {
        // Fenêtre cachée : notification Windows ; sinon (ou type coupé) toast de l'appli, tout de
        // suite ou au retour sur la fenêtre.
        if notify::news_note(&alerts).is_some_and(|note| notify_user(app, note)) {
            return;
        }
        if let Ok(mut pending) = lock(&state.pending_news) {
            pending.extend(alerts);
        }
        flush_news(app);
    }
}

/// Envoie les annonces en attente si la fenêtre principale est à l'écran ; sinon elles attendent
/// le retour sur la fenêtre (Ludex dans la zone de notification).
fn flush_news<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<AppState>() else { return };
    let shown = app
        .get_webview_window(MAIN_WINDOW)
        .is_some_and(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false));
    if !shown {
        return;
    }
    let alerts = match lock(&state.pending_news) {
        Ok(mut pending) if !pending.is_empty() => std::mem::take(&mut *pending),
        _ => return,
    };
    let _ = app.emit_to(MAIN_WINDOW, NEWS_EVENT, alerts);
}

/// Relit les annonces d'un jeu si elles ont plus de 6 heures. Erreur seulement si rien n'est connu.
fn refresh_news(state: &AppState, appid: u32) -> Result<(), String> {
    if !lock(&state.news)?.stale(appid, news::now()) {
        return Ok(());
    }
    match news::fetch(&news::agent(), appid) {
        Ok(items) => {
            diagnostic::ok(diagnostic::Service::News);
            let mut cache = lock(&state.news)?;
            cache.record(appid, items, news::now());
            if let Err(e) = cache.save() {
                eprintln!("Cache des actualités non enregistré : {e}");
            }
            Ok(())
        }
        Err(e) => {
            diagnostic::failed(diagnostic::Service::News, &e);
            eprintln!("Actualités de {appid} non relues : {e}");
            if lock(&state.news)?.known(appid) { Ok(()) } else { Err(tr!("Actualités indisponibles : {e}", "News unavailable: {e}")) }
        }
    }
}

/// Annonces officielles d'un jeu (Steam), la plus récente d'abord ; vide sans appid connu. Relues
/// au plus toutes les 6 heures. `seen` : l'onglet est affiché, ses annonces ne seront plus signalées.
#[tauri::command]
async fn game_news(app: AppHandle, id: String, seen: bool) -> Result<Vec<news::News>, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let Some(appid) = find_game(&state, &id)?.steam_appid else { return Ok(Vec::new()) };
        refresh_news(&state, appid)?;
        let mut cache = lock(&state.news)?;
        if seen && cache.mark_seen(appid) && let Err(e) = cache.save() {
            eprintln!("Cache des actualités non enregistré : {e}");
        }
        Ok(cache.news(appid))
    })
    .await
}

/// Ouvre une annonce sur la boutique Steam (`link` absent) ou l'un de ses liens, par son numéro :
/// l'adresse est reprise dans le cache, jamais envoyée par le front, et revérifiée (https vers un
/// hôte Steam).
#[tauri::command]
fn open_news(app: AppHandle, state: State<'_, AppState>, id: String, gid: String, link: Option<u32>) -> Result<(), String> {
    if gid.len() > 24 {
        return Err(tr!("Annonce inconnue.", "Unknown announcement."));
    }
    let appid = find_game(&state, &id)?.steam_appid.ok_or(tr_str!("Pas d'actualités pour ce jeu.", "No news for this game."))?;
    let url = lock(&state.news)?
        .target(appid, &gid, link)
        .and_then(|u| news::safe_link(&u))
        .ok_or(tr_str!("Lien introuvable : rouvre les actualités du jeu.", "Link not found: reopen the game's news."))?;
    app.opener().open_url(&url, None::<&str>).map_err(|e| tr!("Impossible d'ouvrir le lien : {e}", "Couldn't open the link: {e}"))
}

/// Lit la base des sauvegardes (téléchargée au plus une fois par semaine) et garde les entrées des
/// jeux de la bibliothèque. Recommence si la bibliothèque a changé pendant la lecture.
fn resolve_saves(app: tauri::AppHandle) {
    let Some(state) = app.try_state::<AppState>() else { return };
    let needed = match (lock(&state.saves), lock(&state.games)) {
        (Ok(known), Ok(games)) => known.needs_lookup(&games),
        _ => false,
    };
    if !needed || relocate::unavailable(&state) || state.saves_busy.swap(true, Ordering::SeqCst) {
        return;
    }
    let dir = relocate::current_cache(&state);
    std::thread::spawn(move || {
        let Some(state) = app.try_state::<AppState>() else { return };
        match saves::refresh_manifest(&dir) {
            Ok(manifest) => {
                for _ in 0..3 {
                    // Jeux copiés avant de verrouiller la base : même ordre que partout ailleurs.
                    let games = lock(&state.games).map(|g| g.clone()).unwrap_or_default();
                    let Ok(mut known) = lock(&state.saves) else { break };
                    if !known.needs_lookup(&games) {
                        break;
                    }
                    known.select(&manifest, &games);
                }
            }
            Err(e) => {
                eprintln!("{e}");
                if let Ok(mut known) = lock(&state.saves) {
                    known.status = saves::Status::Unavailable;
                }
            }
        }
        state.saves_busy.store(false, Ordering::SeqCst);
    });
}

/// Dossier des copies de sauvegardes : réglage, sinon `<dataDir>\Sauvegardes`, sinon
/// `Documents\Ludex\Sauvegardes`.
fn saves_root(state: &AppState) -> PathBuf {
    let (saves_dir, data_dir) = lock(&state.data).map(|d| (d.settings.saves_dir.clone(), d.settings.data_dir.clone())).unwrap_or_default();
    relocate::copies_root(saves_dir.as_deref(), data_dir.as_deref(), saves::default_dir)
}

/// Entrée de la base des sauvegardes d'un jeu, et état de la base.
fn saves_entry(state: &AppState, id: &str) -> Result<(saves::Status, Option<saves::Entry>), String> {
    let known = lock(&state.saves)?;
    Ok((known.status.clone(), known.entry(id).cloned()))
}

/// Emplacements des sauvegardes d'un jeu sur ce PC, reconstruits depuis la base (jamais depuis le front).
fn saves_scope(state: &AppState, game: &Game) -> Result<saves::Scope, String> {
    match saves_entry(state, &game.id)? {
        (_, Some(entry)) => Ok(saves::Scope::new(&entry, game, &saves::Env::system())),
        (saves::Status::Loading, None) => Err(tr!(
            "La base des sauvegardes est en cours de téléchargement : réessaie dans un instant.",
            "The saves database is still downloading: try again in a moment."
        )),
        (saves::Status::Unavailable, None) => Err(tr!(
            "Base des sauvegardes indisponible (hors ligne ?) : emplacements inconnus.",
            "Saves database unavailable (offline?): locations unknown."
        )),
        (saves::Status::Ready, None) => Err(tr!(
            "{} n'est pas dans la base des sauvegardes : emplacement inconnu.",
            "{} isn't in the saves database: location unknown.",
            game.title
        )),
    }
}

/// Copie les sauvegardes d'un jeu, puis ne garde que les copies les plus récentes.
fn backup_game(state: &AppState, game: &Game, reason: saves::Reason) -> Result<saves::Outcome, String> {
    let root = saves_root(state);
    let scope = saves_scope(state, game)?;
    let _busy = lock(&state.saves_lock)?;
    let outcome = saves::backup(&root, &game.id, &game.title, &scope.locate(Some(&root)), reason, &saves::Stamp::now())?;
    if !outcome.unchanged {
        saves::rotate(&root, &game.id, saves::KEEP, &[&outcome.copy.id]);
    }
    Ok(outcome)
}

/// Événement `saves-backup` : copie automatique faite (ou ratée) à la fin d'une partie.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SavesEvent {
    id: String,
    title: String,
    copy: Option<saves::CopySummary>,
    /// Rien n'avait changé depuis la copie précédente.
    unchanged: bool,
    error: Option<String>,
}

const SAVES_EVENT: &str = "saves-backup";

/// Copie automatique à la fin d'une partie d'une minute au moins, si le réglage est actif et le jeu
/// connu de la base. Appelée hors du fil des sessions.
fn auto_backup<R: Runtime>(app: &AppHandle<R>, game_id: &str) {
    let Some(state) = app.try_state::<AppState>() else { return };
    if !lock(&state.data).is_ok_and(|d| d.settings.auto_saves) {
        return;
    }
    let Ok(game) = find_game(&state, game_id) else { return };
    if !lock(&state.saves).is_ok_and(|k| k.entry(game_id).is_some()) {
        return; // jeu inconnu de la base, ou base pas encore lue
    }
    let event = match backup_game(&state, &game, saves::Reason::Auto) {
        Ok(o) => SavesEvent { id: game.id, title: game.title, copy: Some(o.copy), unchanged: o.unchanged, error: None },
        Err(e) if e == saves::NOTHING_FOUND => return,
        Err(e) => {
            eprintln!("Copie des sauvegardes de {} impossible : {e}", game.title);
            notify_user(app, notify::saves_error_note(&game.id, &game.title, &e));
            SavesEvent { id: game.id, title: game.title, copy: None, unchanged: false, error: Some(e) }
        }
    };
    let _ = app.emit(SAVES_EVENT, event);
}

/// Sauvegardes d'un jeu : emplacements trouvés sur ce PC, copies existantes et réglages.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GameSaves {
    status: saves::Status,
    /// Nom du jeu dans la base, `None` : jeu inconnu de la base.
    name: Option<String>,
    /// Le jeu garde aussi des données dans le registre, non copiées.
    registry: bool,
    locations: Vec<saves::Location>,
    copies: Vec<saves::CopySummary>,
    /// Dossier des copies (affichage seulement).
    dir: String,
    auto: bool,
}

#[tauri::command]
async fn game_saves(app: tauri::AppHandle, id: String) -> Result<GameSaves, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let game = find_game(&state, &id)?;
        let root = saves_root(&state);
        let (status, entry) = saves_entry(&state, &id)?;
        let scope = entry.as_ref().map(|e| saves::Scope::new(e, &game, &saves::Env::system()));
        let locations = scope.as_ref().map(|s| saves::measure(&s.locate(Some(&root)), Some(&root))).unwrap_or_default();
        Ok(GameSaves {
            status,
            name: entry.map(|e| e.name),
            registry: scope.is_some_and(|s| s.registry),
            locations,
            copies: saves::list(&root, &id),
            dir: root.display().to_string(),
            auto: lock(&state.data)?.settings.auto_saves,
        })
    })
    .await
}

/// Ouvre un emplacement de sauvegardes du jeu dans l'Explorateur. Le texte reçu doit être l'un des
/// emplacements que Ludex retrouve lui-même pour ce jeu (jamais un chemin libre venu du front).
#[tauri::command]
async fn open_save_location(app: tauri::AppHandle, id: String, path: String) -> Result<(), String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let game = find_game(&state, &id)?;
        let root = saves_root(&state);
        let scope = saves_scope(&state, &game)?;
        let found = scope
            .locate(Some(&root))
            .into_iter()
            .find(|p| p.to_string_lossy() == path)
            .ok_or(tr_str!("Cet emplacement n'existe plus : rouvre l'onglet Sauvegardes.", "This location no longer exists: reopen the Saves tab."))?;
        // Un dossier s'ouvre ; un fichier seul est montré sélectionné (jamais ouvert par son appli).
        let opened = if found.is_dir() {
            app.opener().open_path(found.to_string_lossy(), None::<&str>)
        } else {
            app.opener().reveal_item_in_dir(&found)
        };
        opened.map_err(|e| tr!("Impossible d'ouvrir le dossier : {e}", "Couldn't open the folder: {e}"))
    })
    .await
}

/// Montre une copie des sauvegardes dans l'Explorateur (son dossier sélectionné).
#[tauri::command]
async fn open_save_copy(app: tauri::AppHandle, id: String, backup_id: String) -> Result<(), String> {
    if !saves::valid_copy_id(&backup_id) {
        return Err(tr!("Copie inconnue.", "Unknown backup."));
    }
    blocking(move || {
        let state = app.state::<AppState>();
        find_game(&state, &id)?;
        let dir = saves::copy_dir(&saves_root(&state), &id, &backup_id).ok_or(tr_str!("Copie introuvable : elle a peut-être été supprimée.", "Backup not found: it may have been deleted."))?;
        app.opener()
            .reveal_item_in_dir(&dir)
            .map_err(|e| tr!("Impossible d'ouvrir le dossier de la copie : {e}", "Couldn't open the backup folder: {e}"))
    })
    .await
}

/// « Sauvegarder maintenant ».
#[tauri::command]
async fn backup_saves(app: tauri::AppHandle, id: String) -> Result<saves::Outcome, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let game = find_game(&state, &id)?;
        backup_game(&state, &game, saves::Reason::Manual)
    })
    .await
}

/// Restaure une copie, après une copie de sécurité de l'état actuel. Refusée si le jeu tourne.
#[tauri::command]
async fn restore_saves(app: tauri::AppHandle, id: String, backup_id: String) -> Result<saves::Restored, String> {
    if !saves::valid_copy_id(&backup_id) {
        return Err(tr!("Copie inconnue.", "Unknown backup."));
    }
    blocking(move || {
        let state = app.state::<AppState>();
        let game = find_game(&state, &id)?;
        if state.tracker.active().iter().any(|(g, _)| *g == id) || session::game_running(&game) {
            return Err(tr!("{} est ouvert : ferme le jeu avant de restaurer ses sauvegardes.", "{} is running: close the game before restoring its saves.", game.title));
        }
        let root = saves_root(&state);
        let scope = saves_scope(&state, &game)?;
        let _busy = lock(&state.saves_lock)?;
        saves::restore(&root, &game.id, &game.title, &backup_id, &scope, &saves::Stamp::now())
    })
    .await
}

/// Choisit le dossier des copies avec le sélecteur natif. Le chemin reste côté Rust ; le front ne
/// reçoit que son texte, pour l'afficher. `None` : annulé.
#[tauri::command]
async fn choose_saves_dir(app: tauri::AppHandle, window: tauri::WebviewWindow) -> Result<Option<String>, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let current = saves_root(&state);
        let mut dialog = window.dialog().file().set_title(tr!("Dossier des copies de sauvegardes", "Folder for save backups")).set_parent(&window);
        if let Some(start) = current.ancestors().find(|d| d.is_dir()) {
            dialog = dialog.set_directory(start);
        }
        let Some(picked) = dialog.blocking_pick_folder().and_then(|f| f.into_path().ok()) else { return Ok(None) };
        let dir = saves::check_dir(&picked)?;
        let shown = dir.display().to_string();
        let mut data = lock(&state.data)?;
        data.settings.saves_dir = Some(dir);
        data.save(&state.data_path)?;
        Ok(Some(shown))
    })
    .await
}

/// Active ou coupe la copie automatique à la fin des parties.
#[tauri::command]
fn set_auto_saves(app: AppHandle, value: bool) {
    update_settings(&app, |s| s.auto_saves = value);
}

/// Jeu de la bibliothèque qui a des sauvegardes sur ce PC, ou des copies.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SavesSummary {
    id: String,
    title: String,
    locations: usize,
    size: u64,
    copies: usize,
    /// Date de la copie la plus récente (secondes Unix).
    last_copy: Option<u64>,
    registry: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SavesOverview {
    status: saves::Status,
    dir: String,
    auto: bool,
    games: Vec<SavesSummary>,
}

/// Page « Sauvegardes » : jeux avec des sauvegardes trouvées ou des copies, par titre.
#[tauri::command]
async fn saves_overview(app: tauri::AppHandle) -> Result<SavesOverview, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let root = saves_root(&state);
        let games = lock(&state.games)?.clone();
        let (status, entries): (saves::Status, Vec<Option<saves::Entry>>) = {
            let known = lock(&state.saves)?;
            (known.status.clone(), games.iter().map(|g| known.entry(&g.id).cloned()).collect())
        };
        let mut copies = saves::list_all(&root);
        let env = saves::Env::system();
        let mut out = Vec::new();
        for (game, entry) in games.into_iter().zip(entries) {
            let scope = entry.map(|e| saves::Scope::new(&e, &game, &env));
            let locations = scope.as_ref().map(|s| saves::measure(&s.locate(Some(&root)), Some(&root))).unwrap_or_default();
            let game_copies = copies.remove(&game.id).unwrap_or_default();
            if locations.is_empty() && game_copies.is_empty() {
                continue;
            }
            out.push(SavesSummary {
                id: game.id,
                title: game.title,
                locations: locations.len(),
                size: locations.iter().map(|l| l.size).sum(),
                copies: game_copies.len(),
                last_copy: game_copies.first().map(|c| c.created),
                registry: scope.is_some_and(|s| s.registry),
            });
        }
        out.sort_by_key(|s| s.title.to_lowercase());
        Ok(SavesOverview { status, dir: root.display().to_string(), auto: lock(&state.data)?.settings.auto_saves, games: out })
    })
    .await
}

/// Événement `game-size` : taille d'un jeu mesurée par Ludex (dossier d'installation).
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SizeEvent {
    id: String,
    size_on_disk: u64,
}

const SIZE_EVENT: &str = "game-size";

/// Dossiers à mesurer. Les jeux sont copiés avant de verrouiller le cache des tailles : jamais les
/// deux verrous ensemble.
fn sizes_to_measure(state: &AppState) -> Vec<storage::Job> {
    let Ok(games) = lock(&state.games).map(|g| g.clone()) else { return Vec::new() };
    lock(&state.sizes).map(|cache| cache.to_measure(&games, storage::now(), storage::modified)).unwrap_or_default()
}

/// Mesure un dossier, garde la mesure, la pose sur les jeux de l'état et prévient le front.
fn measure_size(app: &tauri::AppHandle, state: &AppState, job: &storage::Job) {
    let bytes = match storage::dir_size(&job.dir) {
        Ok(bytes) => bytes,
        Err(e) => return eprintln!("Taille de {} non mesurée : {e}", job.dir.display()),
    };
    if let Ok(mut cache) = lock(&state.sizes) {
        cache.record(job, bytes, storage::now());
        if let Err(e) = cache.save() {
            eprintln!("Tailles des jeux non enregistrées : {e}");
        }
    }
    let changed = lock(&state.games).map(|mut games| storage::publish(job, bytes, &mut games)).unwrap_or_default();
    for (id, size_on_disk) in changed {
        let _ = app.emit(SIZE_EVENT, SizeEvent { id, size_on_disk });
    }
}

/// Mesure en arrière-plan les dossiers des jeux dont le launcher ne donne pas la taille, un à la
/// fois, sans bloquer le scan. Recommence si des jeux sont apparus entre-temps.
fn resolve_sizes(app: tauri::AppHandle) {
    let Some(state) = app.try_state::<AppState>() else { return };
    if state.sizes_busy.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        let Some(state) = app.try_state::<AppState>() else { return };
        let games = lock(&state.games).map(|g| g.clone()).unwrap_or_default();
        if let Ok(mut cache) = lock(&state.sizes) {
            cache.forget_old(&games, storage::now());
        }
        for _ in 0..3 {
            let jobs = sizes_to_measure(&state);
            if jobs.is_empty() {
                break;
            }
            jobs.iter().for_each(|job| measure_size(&app, &state, job));
        }
        state.sizes_busy.store(false, Ordering::SeqCst);
    });
}

/// Section « Stockage » : disques qui portent des jeux, place des jeux par launcher, jeux à
/// désinstaller. Lecture des disques hors du fil de l'interface.
#[tauri::command]
async fn storage(app: tauri::AppHandle) -> Result<storage::Storage, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let games = lock(&state.games)?.clone();
        let measuring = state.sizes_busy.load(Ordering::SeqCst);
        Ok(storage::report(&games, &storage::volumes(), storage::now(), measuring))
    })
    .await
}

/// Ouvre la désinstallation du jeu chez son launcher (Steam, qui demande confirmation). Le lien
/// est reconstruit depuis le jeu de l'état ; Ludex ne supprime jamais de fichier lui-même.
#[tauri::command]
fn uninstall_game(app: tauri::AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    let game = find_game(&state, &id)?;
    let uri = storage::uninstall_uri(&game)
        .ok_or_else(|| tr!("Ludex ne sait pas ouvrir la désinstallation de {} : ouvre son dossier.", "Ludex can't open the uninstaller for {}: open its folder.", game.title))?;
    app.opener().open_url(&uri, None::<&str>).map_err(|e| tr!("Le launcher n'a pas répondu : {e}", "The launcher didn't respond: {e}"))
}

/// Emplacements d'un jeu de la bibliothèque pour la vue avancée : sauvegardes de la base (si elle
/// est prête), entrées de désinstallation et clés du registre relues maintenant.
fn footprint_targets(state: &AppState, game: &Game) -> footprint::Targets {
    let saves = match saves_scope(state, game) {
        Ok(scope) => footprint::Saves { paths: scope.locate(Some(&saves_root(state))), registry: scope.registry, note: None },
        Err(e) => footprint::Saves { note: Some(e), ..footprint::Saves::default() },
    };
    let entries = footprint::uninstall_entries(&footprint::System);
    footprint::targets(game, &footprint::System, &entries, saves, library::steam_account_dir().as_deref())
}

/// Emplacements gardés d'un jeu, relevés par `game_footprint` (jamais reçus du front).
fn kept_targets(state: &AppState, id: &str) -> Result<footprint::Targets, String> {
    lock(&state.footprints)?
        .get(id)
        .cloned()
        .ok_or_else(|| tr!("Emplacements inconnus : rouvre la vue avancée de ce jeu.", "Locations unknown: reopen the advanced view for this game."))
}

/// Vue avancée du Stockage : ce que le jeu laisse sur le PC (lecture seule). Jeu encore dans la
/// bibliothèque : emplacements relevés de nouveau et gardés. Jeu parti (désinstallé) : emplacements
/// gardés relus, ce qui existe encore est un reste. `rescan` : rescanne d'abord (après un
/// désinstalleur), la bibliothèque du front suit par `library-changed`.
#[tauri::command]
async fn game_footprint(app: tauri::AppHandle, id: String, rescan: bool) -> Result<footprint::Footprint, String> {
    blocking(move || {
        if rescan {
            auto_rescan(&app, false);
        }
        let state = app.state::<AppState>();
        let game = lock(&state.games)?.iter().find(|g| g.id == id).cloned();
        let targets = match &game {
            Some(g) => {
                let t = footprint_targets(&state, g);
                lock(&state.footprints)?.insert(id.clone(), t.clone());
                t
            }
            None => kept_targets(&state, &id).map_err(|_| not_in_library())?,
        };
        Ok(footprint::check(&targets, &footprint::System, game.as_ref(), footprint::size_of, |p: &Path| p.is_file()))
    })
    .await
}

/// Ouvre un dossier de la vue avancée (un fichier seul est montré sélectionné). `item` désigne un
/// élément de la liste relevée par Ludex (`install`, `save-0`…), jamais un chemin.
#[tauri::command]
async fn open_footprint_item(app: tauri::AppHandle, id: String, item: String) -> Result<(), String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let targets = kept_targets(&state, &id)?;
        let folder = targets.folders.iter().find(|f| f.item == item).ok_or(tr_str!("Emplacement inconnu : rouvre la vue avancée.", "Unknown location: reopen the advanced view."))?;
        let opened = if folder.path.is_dir() {
            app.opener().open_path(folder.path.to_string_lossy(), None::<&str>)
        } else if folder.path.is_file() {
            app.opener().reveal_item_in_dir(&folder.path)
        } else {
            return Err(tr!("{} n'existe plus.", "{} no longer exists.", folder.path.display()));
        };
        opened.map_err(|e| tr!("Impossible d'ouvrir le dossier : {e}", "Couldn't open the folder: {e}"))
    })
    .await
}

/// Lance le désinstalleur officiel d'une entrée de la vue avancée : `UninstallString` relue dans
/// le registre maintenant, découpée côté Rust, lancée sans interpréteur de commandes. Refusé si le
/// jeu tourne. Ludex ne supprime rien lui-même : le désinstalleur demande sa propre confirmation.
#[tauri::command]
async fn run_uninstaller(app: tauri::AppHandle, id: String, item: String) -> Result<(), String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let targets = kept_targets(&state, &id)?;
        let game = find_game(&state, &id)?;
        if state.tracker.active().iter().any(|(g, _)| *g == id) || session::game_running(&game) {
            return Err(tr!("{} est ouvert : ferme le jeu avant de le désinstaller.", "{} is running: close the game before uninstalling it.", game.title));
        }
        let index: usize = item.strip_prefix("uninstall-").and_then(|n| n.parse().ok()).ok_or(tr_str!("Entrée inconnue : rouvre la vue avancée.", "Unknown entry: reopen the advanced view."))?;
        let (hive, path) = targets.uninstall.get(index).ok_or(tr_str!("Entrée inconnue : rouvre la vue avancée.", "Unknown entry: reopen the advanced view."))?;
        let entry = footprint::read_entry(&footprint::System, *hive, path).ok_or(tr_str!(
            "Cette entrée a disparu du registre : le jeu semble déjà désinstallé.",
            "This entry is gone from the registry: the game seems to be uninstalled already."
        ))?;
        let text = entry.uninstall_string.ok_or(tr_str!("Cette entrée n'a pas de commande de désinstallation.", "This entry has no uninstall command."))?;
        let cmd = footprint::parse_command(&text, &footprint::system_dir(), |p| p.is_file())?;
        if cmd.exe.file_name().is_some_and(|n| n.eq_ignore_ascii_case("steam.exe")) {
            return Err(tr!("Désinstallation gérée par Steam : utilise « Désinstaller avec Steam ».", "Steam handles this uninstall: use \"Uninstall with Steam\"."));
        }
        footprint::launch(&cmd)
    })
    .await
}

/// Paramètres > Applications de Windows (paquets du Microsoft Store, qui n'ont pas de désinstalleur).
#[tauri::command]
fn open_apps_settings(app: tauri::AppHandle) -> Result<(), String> {
    app.opener()
        .open_url("ms-settings:appsfeatures", None::<&str>)
        .map_err(|e| tr!("Impossible d'ouvrir les Paramètres : {e}", "Couldn't open Windows Settings: {e}"))
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
    // Partie d'une minute au moins terminée : copie des sauvegardes, hors du fil des sessions.
    if update.finished.is_some() {
        let (app, id) = (app.clone(), update.game_id.clone());
        std::thread::spawn(move || auto_backup(&app, &id));
    }
    // Statut Discord : message au fil `ludex-discord`, jamais d'attente ici.
    if let Some(presence) = app.try_state::<discord::Presence>() {
        presence.session(&update.game_id, update.phase, update.start);
    }
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

/// Performances des dernières parties d'un jeu lancées depuis Ludex (processeur, mémoire, carte
/// graphique), la plus récente d'abord. Lues en mémoire, rien n'est relevé ici.
#[tauri::command]
fn game_performance(state: State<'_, AppState>, id: String) -> Vec<performance::SessionPerf> {
    state.tracker.performance(&id)
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

/// Date d'installation de chaque jeu (création de son dossier, secondes Unix), pour « Ta pile » de
/// « Pour toi ». Un jeu sans dossier ou dont la date est illisible n'y est pas.
#[tauri::command]
async fn install_dates(app: AppHandle) -> Result<std::collections::HashMap<String, i64>, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let dirs: Vec<(String, String)> =
            lock(&state.games)?.iter().filter_map(|g| Some((g.id.clone(), g.install_dir.clone()?))).collect();
        Ok(dirs.into_iter().filter_map(|(id, dir)| Some((id, library::install_date(&dir)?))).collect())
    })
    .await
}

/// Récap d'une semaine, d'un mois ou d'une année (`back` périodes en arrière, 0 par défaut).
/// Valeurs fermées seulement : tout est calculé depuis l'historique et l'état.
#[tauri::command]
fn recap(state: State<'_, AppState>, period: recap::Period, back: Option<u32>) -> recap::Recap {
    let unlocks = state.games.lock().map(|games| unlock_times(&games)).unwrap_or_default();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    recap::compute(&state.tracker.sessions(), &unlocks, now, period, back.unwrap_or(0), &recap::LocalZone)
}

/// Section « Statistiques » : 12 derniers mois (temps mesuré par Ludex, sessions, jours joués,
/// succès Steam) et séries de jours de tout l'historique. Sans argument.
#[tauri::command]
fn stats(state: State<'_, AppState>) -> stats::Stats {
    let unlocks = state.games.lock().map(|games| unlock_times(&games)).unwrap_or_default();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    stats::compute(&state.tracker.sessions(), &unlocks, now, &recap::LocalZone)
}

/// Dates de déblocage des succès de chaque jeu qui en a (récap, journal et statistiques).
fn unlock_times(games: &[Game]) -> Vec<(String, Vec<u64>)> {
    games
        .iter()
        .filter_map(|g| Some((g.id.clone(), g.achievements.as_ref()?.unlock_times.clone())))
        .filter(|(_, times)| !times.is_empty())
        .collect()
}

/// Section « Journal » : une page de jours actifs (sessions, succès, captures), du plus récent au
/// plus ancien. `before` : date `AAAA-MM-JJ` (exclue) renvoyée par la page précédente ; `game` : un
/// seul jeu de la bibliothèque. Succès : détail lu seulement pour les jeux de la page, pourcentages
/// mondiaux du cache seulement (aucune requête).
#[tauri::command]
async fn journal(app: AppHandle, before: Option<String>, game: Option<String>) -> Result<journal::Page, String> {
    let before = match before.as_deref() {
        Some(text) => Some(journal::parse_day(text).ok_or(tr_str!("Date invalide.", "Invalid date."))?),
        None => None,
    };
    blocking(move || {
        let state = app.state::<AppState>();
        if let Some(id) = &game {
            find_game(&state, id)?;
        }
        let (games, roots) = screenshot_context(&state)?;
        let shots = screenshots::collect(&roots, &games, game.as_deref());
        let unlocks = unlock_times(&games);
        let sessions = state.tracker.sessions();
        let stats = library::SteamStats::locate();
        let page = {
            let cache = lock(&state.global)?;
            let src = journal::Sources { sessions: &sessions, unlocks: &unlocks, shots: &shots };
            let title = |id: &str| games.iter().find(|g| g.id == id).map(|g| g.title.clone());
            journal::compute(&src, before, game.as_deref(), &recap::LocalZone, title, |id| {
                let g = games.iter().find(|g| g.id == id && g.platform == Platform::Steam)?;
                let mut list = stats.as_ref()?.details(&g.platform_id)?;
                if let Ok(appid) = g.platform_id.parse::<u32>() {
                    cache.apply(appid, &mut list);
                }
                Some(list)
            })
        };
        // Seules les captures envoyées pourront être servies par le protocole `ludex`.
        let sent: Vec<screenshots::Shot> = page.days.iter().flat_map(|d| d.games.iter().flat_map(|g| g.shots.iter().cloned())).collect();
        lock(&state.shots)?.remember(&roots, &sent, false);
        Ok(page)
    })
    .await
}

fn lock<T>(m: &Mutex<T>) -> Result<MutexGuard<'_, T>, String> {
    m.lock().map_err(|_| tr!("État interne indisponible", "Internal state unavailable"))
}

/// Scan complet : préférences, jaquettes et fiches appliquées, état mis à jour, puis recherche des
/// images et fiches manquantes. Le verrou `scanning` (pris par l'appelant) empêche deux scans à la
/// fois. Renvoie aussi les jeux apparus et disparus depuis le scan précédent.
fn rescan(app: &tauri::AppHandle, state: &AppState, scanning: MutexGuard<'_, ()>) -> Result<(ScanResult, watch::Changes), String> {
    // Launchers désactivés dans les Paramètres : lus, mais leurs jeux écartés.
    let disabled = lock(&state.data)?.settings.disabled_sources.clone();
    let mut result = library::scan_except(&disabled);
    let changes = store(state, &mut result)?;
    *lock(&state.last_scan)? = Some(Instant::now());
    drop(scanning);
    after_scan(app, state);
    Ok((result, changes))
}

/// Premier scan du démarrage, en deux temps : les sources rapides sont envoyées par `shown` dès
/// qu'elles sont lues (la fenêtre affiche déjà ces jeux), puis les sources lentes (EA, Battle.net,
/// « Autres », qui parcourent les disques) complètent la bibliothèque, annoncée par
/// `library-changed` (`complement`, sans toast). Le verrou reste tenu jusqu'au bout : aucun autre
/// scan entre les deux. Tout ce qui suit un scan (recherches en ligne, tailles, sauvegardes,
/// surveillance, menu, Jump List, date du dernier scan) attend la bibliothèque complète.
fn progressive_scan(app: &tauri::AppHandle, state: &AppState, scanning: MutexGuard<'_, ()>, shown: impl FnOnce(ScanResult)) -> Result<(), String> {
    let disabled = lock(&state.data)?.settings.disabled_sources.clone();
    let (mut first, pending) = library::scan_fast(&disabled);
    // Dans l'état aussi : un jeu déjà affiché peut être lancé ou ouvert pendant la suite.
    store(state, &mut first)?;
    shown(first);
    let mut result = library::finish(pending);
    let changes = store(state, &mut result)?;
    *lock(&state.last_scan)? = Some(Instant::now());
    drop(scanning);
    // Avant les recherches en ligne : leurs événements (`game-art`…) visent des jeux que le front
    // doit déjà avoir.
    let _ = app.emit(LIBRARY_EVENT, LibraryEvent { added: changes.added, removed: changes.removed, scan: result, complement: true });
    after_scan(app, state);
    Ok(())
}

/// Applique à un scan brut les jeux ajoutés à la main, les préférences, jaquettes, fiches, durées,
/// images en cache, tailles et images perso, puis remplace la bibliothèque de l'état. Renvoie les
/// jeux apparus et disparus.
fn store(state: &AppState, result: &mut ScanResult) -> Result<watch::Changes, String> {
    let mut originals = covers::Originals::new();
    {
        let data = lock(&state.data)?;
        let art = lock(&state.art)?;
        let meta = lock(&state.meta)?;
        let durations = lock(&state.hltb)?;
        let images = lock(&state.images)?;
        let sizes = lock(&state.sizes)?;
        // D'abord les jeux ajoutés à la main et les corrections : le titre choisi sert aux jaquettes.
        data.customize(&mut result.games);
        result.games.iter_mut().for_each(|g| {
            data.apply(g);
            art.apply(g);
            meta.apply(g);
            // Les genres choisis par l'utilisateur, par-dessus ceux de la fiche.
            data.correct_genres(g);
            // Après la fiche : son année de sortie fait partie de la clé.
            durations.apply(g);
            // Puis les images distantes (launcher ou `art.rs`) déjà sur disque.
            images.apply(g);
            // Taille mesurée par Ludex pour les jeux dont le launcher ne la donne pas.
            sizes.apply(g);
            // En dernier : les images choisies par l'utilisateur priment sur tout.
            covers::apply(g, data.covers.get(&g.id), images.root(), &mut originals);
        });
    }
    let mut games = lock(&state.games)?;
    let changes = watch::changes(&games, &result.games);
    *games = result.games.clone();
    *lock(&state.originals)? = originals;
    Ok(changes)
}

/// Après un scan complet (verrou rendu) : surveillance, recherches en ligne, mesures, menu.
fn after_scan(app: &tauri::AppHandle, state: &AppState) {
    // Une bibliothèque Steam ou un dossier « Jeux » a pu apparaître.
    if let Some(w) = state.watch.get() {
        w.rewatch();
    }
    resolve_art(app.clone());
    resolve_images(app.clone());
    resolve_details(app.clone());
    resolve_hltb(app.clone());
    resolve_saves(app.clone());
    resolve_sizes(app.clone());
    resolve_news(app.clone());
    wishlist::resolve(app.clone());
    refresh_tray(app, false);
    check_disks(app);
}

#[tauri::command]
async fn scan_library(app: tauri::AppHandle) -> Result<ScanResult, String> {
    let handle = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        if !state.first_scan.swap(false, Ordering::SeqCst) {
            // Un rescan automatique en cours : on attend sa fin plutôt que de scanner en même temps.
            let scanning = wait_scan(&state);
            return rescan(&handle, &state, scanning).map(|(result, _)| result);
        }
        // Premier scan : la suite (sources lentes) continue dans son propre fil, qui garde le verrou.
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = handle.clone();
        std::thread::spawn(move || {
            let state = worker.state::<AppState>();
            let scanning = wait_scan(&state);
            let first = tx.clone();
            if let Err(e) = progressive_scan(&worker, &state, scanning, move |scan| {
                let _ = first.send(Ok(scan));
            }) {
                eprintln!("Premier scan impossible : {e}");
                let _ = tx.send(Err(e)); // sans effet si les premiers jeux sont déjà partis
            }
        });
        rx.recv().map_err(|_| tr!("Le scan s'est arrêté.", "The scan stopped."))?
    })
    .await
    .map_err(|e| tr!("Le scan a échoué : {e}", "The scan failed: {e}"))??;
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
    /// Fin du premier scan (sources lentes) : jeux ajoutés sans toast « Nouveau jeu ».
    complement: bool,
}

const LIBRARY_EVENT: &str = "library-changed";

/// Retour sur la fenêtre après ce temps sans scan : rescan, pour les sources lues dans le registre
/// (GOG, Ubisoft, Rockstar, Xbox) que la surveillance des dossiers ne voit pas.
const STALE_ON_FOCUS: Duration = Duration::from_secs(5 * 60);

/// Surveille les dossiers des launchers, après le premier scan seulement : la bibliothèque
/// entière ne doit jamais être annoncée comme nouvelle.
fn start_watch(app: &tauri::AppHandle) {
    let (handle, live) = (app.clone(), app.clone());
    app.state::<AppState>()
        .watch
        .get_or_init(|| watch::Watch::spawn(move || auto_rescan(&handle, false), move |p| install_progress(&live, &p)));
}

/// Événement `game-install` : progression d'un téléchargement ou d'une mise à jour Steam, relue
/// dans le manifeste du jeu sans rescan.
const INSTALL_EVENT: &str = "game-install";

/// Pose la progression relue sur le jeu de l'état, puis prévient le front si elle a changé.
fn install_progress(app: &tauri::AppHandle, progress: &watch::Progress) {
    let Some(state) = app.try_state::<AppState>() else { return };
    let (event, done) = match lock(&state.games) {
        Ok(mut games) => {
            let before = games
                .iter()
                .find(|g| g.platform == library::Platform::Steam && g.platform_id == progress.appid)
                .map(|g| (g.install_state, g.hidden));
            let event = watch::apply(&mut games, progress);
            let done = match (&event, before) {
                (Some(e), Some((state, false))) if notify::download_done(state, e.install_state) => {
                    games.iter().find(|g| g.id == e.id).map(|g| notify::download_note(&g.id, &g.title, true))
                }
                _ => None,
            };
            (event, done)
        }
        Err(_) => (None, None),
    };
    if let Some(event) = event {
        let _ = app.emit(INSTALL_EVENT, event);
    }
    if let Some(note) = done {
        notify_user(app, note);
        check_disks(app);
    }
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
            let installed: Vec<(String, String)> = scan
                .games
                .iter()
                .filter(|g| g.platform == library::Platform::Steam && changes.added_ids.contains(&g.id))
                .map(|g| (g.id.clone(), g.title.clone()))
                .collect();
            let _ = app.emit(LIBRARY_EVENT, LibraryEvent { added: changes.added, removed: changes.removed, scan, complement: false });
            match installed.as_slice() {
                [] => {}
                [(id, title)] => {
                    notify_user(app, notify::download_note(id, title, false));
                }
                many => {
                    notify_user(app, notify::downloads_note(many.len()));
                }
            }
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
        .ok_or_else(not_in_library)
}

/// Jeu absent de l'état (désinstallé, ou id d'un ancien scan).
fn not_in_library() -> String {
    tr!("Ce jeu n'est plus dans la bibliothèque. Relance un scan.", "This game is no longer in the library. Run a new scan.")
}

/// Demande les pourcentages mondiaux manquants ou vieux de plus de 7 jours, sans tenir le verrou
/// pendant les requêtes. Une erreur réseau laisse simplement les pourcentages inconnus.
fn refresh_global(state: &AppState, appids: &[u32]) {
    let Ok(todo) = lock(&state.global).map(|c| c.to_fetch(appids)) else { return };
    if todo.is_empty() {
        return;
    }
    let fetched = showcase::fetch(&todo);
    if fetched.is_empty() {
        return;
    }
    if let Ok(mut cache) = lock(&state.global) {
        cache.record(fetched);
        if let Err(e) = cache.save() {
            eprintln!("Pourcentages des succès non enregistrés : {e}");
        }
    }
}

/// Liste complète des succès d'un jeu Steam, lue à la demande (jamais pendant le scan), avec les
/// pourcentages mondiaux : non débloqués les plus faciles d'abord, puis débloqués. Vide si le jeu
/// n'a pas de succès connus.
#[tauri::command]
async fn game_achievements(app: AppHandle, id: String) -> Result<Vec<library::Achievement>, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let game = find_game(&state, &id)?;
        if game.platform != Platform::Steam || game.achievements.is_none() {
            return Ok(Vec::new());
        }
        let Some(mut list) = library::SteamStats::locate().and_then(|s| s.details(&game.platform_id)) else { return Ok(Vec::new()) };
        // Une seule requête, demandée en ouvrant la liste : utile aussi sans succès débloqué
        // (les plus faciles en tête).
        if let Ok(appid) = game.platform_id.parse::<u32>() {
            refresh_global(&state, &[appid]);
            lock(&state.global)?.apply(appid, &mut list);
        }
        showcase::sort_for_display(&mut list);
        Ok(list)
    })
    .await
}

/// Configuration requise d'un jeu qui a un appid Steam (onglet Aperçu de la fiche), comparée au
/// matériel de ce PC. Demandée à la boutique au plus une fois par 30 jours, une requête à la fois ;
/// hors ligne, la configuration déjà connue sert. `None` : rien de connu.
#[tauri::command]
async fn game_requirements(app: AppHandle, id: String) -> Result<Option<requirements::Requirements>, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let game = find_game(&state, &id)?;
        let Some(appid) = game.steam_appid else { return Ok(None) };
        if lock(&state.requirements)?.stale(appid) {
            // Une requête à la fois : la demande suivante attend, puis relit le cache.
            let _gate = state.requirements_lock.lock().unwrap_or_else(|e| e.into_inner());
            if lock(&state.requirements)?.stale(appid) {
                state.requirements_busy.store(true, Ordering::SeqCst);
                let answer = requirements::request(appid);
                state.requirements_busy.store(false, Ordering::SeqCst);
                match answer {
                    Ok(html) => {
                        diagnostic::ok(diagnostic::Service::Requirements);
                        let mut cache = lock(&state.requirements)?;
                        cache.record(appid, html);
                        if let Err(e) = cache.save() {
                            eprintln!("Cache des configurations non enregistré : {e}");
                        }
                    }
                    Err(e) => {
                        diagnostic::failed(diagnostic::Service::Requirements, &e);
                        eprintln!("Configuration requise non lue : {e}");
                    }
                }
            }
        }
        let hw = requirements::hardware();
        let disk = requirements::disk_of(&game);
        Ok(lock(&state.requirements)?.requirements(appid, hw, disk))
    })
    .await
}

/// Mods Steam Workshop installés d'un jeu Steam (onglet « Mods » de la fiche) : manifeste et
/// dossiers lus sur le disque, titres et vignettes du cache seulement (aucune requête).
/// `None` : pas un jeu Steam, ou aucun manifeste Workshop.
#[tauri::command]
async fn game_mods(app: AppHandle, id: String) -> Result<Option<workshop::Mods>, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let game = find_game(&state, &id)?;
        let Some((content, installed)) = workshop::installed(&game) else { return Ok(None) };
        let cache = lock(&state.workshop)?;
        Ok(Some(workshop::list(&content, &installed, &cache)))
    })
    .await
}

/// Comme `game_mods`, après avoir demandé à l'API publique de Steam les titres et vignettes
/// inconnus ou vieux de plus de 30 jours (lots de 50, une lecture à la fois, arrêt à la première
/// erreur réseau : ce qui est connu reste affiché).
#[tauri::command]
async fn refresh_mods(app: AppHandle, id: String) -> Result<Option<workshop::Mods>, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let game = find_game(&state, &id)?;
        let Some((content, installed)) = workshop::installed(&game) else { return Ok(None) };
        let ids: Vec<u64> = installed.iter().map(|m| m.id).collect();
        if !lock(&state.workshop)?.to_fetch(&ids).is_empty() {
            // Une lecture à la fois : la suivante attend, puis relit le cache.
            let _gate = state.workshop_lock.lock().unwrap_or_else(|e| e.into_inner());
            let todo = lock(&state.workshop)?.to_fetch(&ids);
            if !todo.is_empty() {
                state.workshop_busy.store(true, Ordering::SeqCst);
                let mut changed = false;
                for (i, chunk) in todo.chunks(workshop::BATCH).enumerate() {
                    if i > 0 {
                        std::thread::sleep(std::time::Duration::from_millis(300));
                    }
                    match workshop::request(chunk) {
                        Ok(answers) => {
                            diagnostic::ok(diagnostic::Service::Workshop);
                            let mut cache = lock(&state.workshop)?;
                            for (mod_id, info) in answers {
                                cache.record(mod_id, info);
                            }
                            changed = true;
                        }
                        Err(e) => {
                            diagnostic::failed(diagnostic::Service::Workshop, &e);
                            eprintln!("Mods Workshop non lus : {e}");
                            break;
                        }
                    }
                }
                state.workshop_busy.store(false, Ordering::SeqCst);
                if changed && let Err(e) = lock(&state.workshop)?.save() {
                    eprintln!("Cache des mods Workshop non enregistré : {e}");
                }
            }
        }
        let cache = lock(&state.workshop)?;
        Ok(Some(workshop::list(&content, &installed, &cache)))
    })
    .await
}

/// Ouvre le dossier des mods Workshop du jeu (`steamapps/workshop/content/<appid>`), reconstruit
/// depuis le jeu de l'état.
#[tauri::command]
fn open_mods_dir(app: tauri::AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    let game = find_game(&state, &id)?;
    let (content, _) = workshop::installed(&game).ok_or(tr_str!("Aucun mod Workshop pour ce jeu.", "No Workshop mods for this game."))?;
    if !content.is_dir() {
        return Err(tr!("Le dossier des mods n'existe plus.", "The mods folder no longer exists."));
    }
    app.opener()
        .open_path(content.to_string_lossy(), None::<&str>)
        .map_err(|e| tr!("Impossible d'ouvrir {} : {e}", "Couldn't open {}: {e}", content.display()))
}

/// Ouvre la page d'un mod dans Steam (`steam://url/CommunityFilePage/<id>`). L'id doit être un
/// nombre et un mod installé de ce jeu : aucune URL ne vient du front.
#[tauri::command]
fn open_mod_page(app: tauri::AppHandle, state: State<'_, AppState>, id: String, mod_id: String) -> Result<(), String> {
    let game = find_game(&state, &id)?;
    let wanted = workshop::parse_id(&mod_id).ok_or(tr_str!("Mod inconnu.", "Unknown mod."))?;
    let (_, installed) = workshop::installed(&game).ok_or(tr_str!("Aucun mod Workshop pour ce jeu.", "No Workshop mods for this game."))?;
    if !installed.iter().any(|m| m.id == wanted) {
        return Err(tr!("Ce mod n'est plus installé.", "This mod is no longer installed."));
    }
    app.opener()
        .open_url(format!("steam://url/CommunityFilePage/{wanted}"), None::<&str>)
        .map_err(|e| tr!("Steam n'a pas répondu : {e}", "Steam didn't respond: {e}"))
}

/// Section « Vitrine » : total des succès, jeux à 100 %, dix plus rares, derniers déblocages.
/// Jeux Steam non masqués ; détail et pourcentages lus seulement pour ceux qui ont débloqué
/// au moins un succès.
#[tauri::command]
async fn showcase(app: AppHandle) -> Result<showcase::Showcase, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let games: Vec<Game> = lock(&state.games)?
            .iter()
            .filter(|g| g.platform == Platform::Steam && !g.hidden && g.achievements.is_some())
            .cloned()
            .collect();
        let unlocked: Vec<&Game> = games.iter().filter(|g| g.achievements.as_ref().is_some_and(|a| a.unlocked > 0)).collect();
        let appids: Vec<u32> = unlocked.iter().filter_map(|g| g.platform_id.parse().ok()).collect();
        refresh_global(&state, &appids);
        let Some(stats) = library::SteamStats::locate() else { return Ok(showcase::build(&games, &[])) };
        let cache = lock(&state.global)?;
        let details: Vec<(&Game, Vec<library::Achievement>)> = unlocked
            .iter()
            .filter_map(|&g| {
                let mut list = stats.details(&g.platform_id)?;
                cache.apply(g.platform_id.parse().ok()?, &mut list);
                Some((g, list))
            })
            .collect();
        Ok(showcase::build(&games, &details))
    })
    .await
}

#[tauri::command]
fn open_install_dir(app: tauri::AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    let dir = find_game(&state, &id)?
        .install_dir
        .ok_or(tr_str!("Dossier d'installation inconnu pour ce jeu.", "Install folder unknown for this game."))?;
    app.opener()
        .open_path(&dir, None::<&str>)
        .map_err(|e| tr!("Impossible d'ouvrir {dir} : {e}", "Couldn't open {dir}: {e}"))
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
    tauri::async_runtime::spawn_blocking(task).await.map_err(|e| tr!("Opération interrompue : {e}", "Operation interrupted: {e}"))?
}

/// Attend la fin d'un scan en cours (automatique ou demandé) : jamais deux à la fois.
fn wait_scan(state: &AppState) -> MutexGuard<'_, ()> {
    state.scan_lock.lock().unwrap_or_else(|e| e.into_inner())
}

/// Sélecteur de fichier natif, limité aux exécutables, au-dessus de la fenêtre de Ludex. Le chemin
/// choisi reste côté Rust : le front ne le voit ni ne le transmet. `None` : annulé.
fn pick_exe(window: &tauri::WebviewWindow, title: &str, start: Option<&Path>) -> Option<PathBuf> {
    let mut dialog = window.dialog().file().set_title(title).add_filter(tr!("Exécutable", "Executable"), &["exe"]).set_parent(window);
    if let Some(dir) = start.filter(|d| d.is_dir()) {
        dialog = dialog.set_directory(dir);
    }
    dialog.blocking_pick_file()?.into_path().ok()
}

/// Un exécutable déjà lancé par un autre jeu de la bibliothèque ne crée pas de doublon.
fn check_unused(games: &[Game], exe: &Path, except: Option<&str>) -> Result<(), String> {
    match manual::same_exe(games, exe, except) {
        Some(g) => Err(tr!(
            "« {} » lance déjà cet exécutable : il est déjà dans la bibliothèque.",
            "\"{}\" already launches this executable: it's already in the library.",
            g.title
        )),
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
        .ok_or(tr_str!(
            "Le jeu n'apparaît pas dans la bibliothèque : son exécutable a-t-il disparu ?",
            "The game doesn't show up in the library: is its executable gone?"
        ))?;
    Ok(LibraryUpdate { id, title, scan })
}

/// Ajoute un jeu à la main : sélecteur de fichier, validation, titre deviné (nom du produit, sinon
/// dossier), enregistrement puis rescan. `None` : sélection annulée.
#[tauri::command]
async fn add_game(app: tauri::AppHandle, window: tauri::WebviewWindow) -> Result<Option<LibraryUpdate>, String> {
    blocking(move || {
        let Some(picked) = pick_exe(&window, tr_str!("Ajouter un jeu", "Add a game"), None) else { return Ok(None) };
        let exe = manual::check_exe(&picked)?;
        let state = app.state::<AppState>();
        let scanning = wait_scan(&state);
        {
            let games = lock(&state.games)?;
            check_unused(&games, &exe, None)?;
            if let Some(g) = manual::folder_owner(&games, &exe) {
                return Err(tr!(
                    "Cet exécutable est dans le dossier de « {} », déjà dans la bibliothèque. Pour le lancer avec, ouvre sa fiche et choisis « Changer l'exécutable ».",
                    "This executable is in the folder of \"{}\", already in the library. To launch the game with it, open its details and choose \"Change executable\".",
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
            return Err(tr!(
                "{} est lancé par son launcher : son exécutable ne se change pas dans Ludex.",
                "{} is launched by its launcher: its executable can't be changed in Ludex.",
                game.title
            ));
        }
        let start = game.install_dir.as_deref().map(Path::new);
        let Some(picked) = pick_exe(&window, &tr!("Exécutable de {}", "Executable for {}", game.title), start) else { return Ok(None) };
        let exe = manual::check_exe(&picked)?;
        if !game.manual {
            let dir = game.install_dir.as_deref().ok_or(tr_str!("Dossier du jeu inconnu.", "Game folder unknown."))?;
            if !manual::is_in_dir(&exe, dir) {
                return Err(tr!("Choisis un exécutable dans le dossier du jeu : {dir}", "Choose an executable in the game's folder: {dir}"));
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
        return Err(tr!("Titre trop long : {} caractères au plus.", "Title too long: {} characters at most.", manual::MAX_TITLE));
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

/// Statut de progression d'un jeu (valeur fermée), `None` : aucun.
#[tauri::command]
fn set_status(state: State<'_, AppState>, id: String, status: Option<collections::Status>) -> Result<(), String> {
    update_data(&state, &id, |data, game| data.set_status(&game.id, status))
}

/// Note perso d'un jeu, nettoyée ici ; renvoie la note enregistrée (`None` : effacée).
#[tauri::command]
fn set_note(state: State<'_, AppState>, id: String, note: String) -> Result<Option<String>, String> {
    collections::check_input(&note)?;
    let note = collections::clean_note(&note);
    let saved = note.clone();
    update_data(&state, &id, move |data, game| data.set_note(&game.id, note))?;
    Ok(saved)
}

/// Alias de recherche d'un jeu, séparés par des virgules, nettoyés ici
/// (`corrections::clean_aliases`) ; renvoie la liste enregistrée (vide : alias effacés).
#[tauri::command]
fn set_aliases(state: State<'_, AppState>, id: String, text: String) -> Result<Vec<String>, String> {
    collections::check_input(&text)?;
    let aliases = corrections::clean_aliases(&text);
    let saved = aliases.clone();
    update_data(&state, &id, move |data, game| data.set_aliases(&game.id, aliases))?;
    Ok(saved)
}

/// Genres d'un jeu après une correction : fiche à jour et provenance des genres.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GenresUpdate {
    details: Option<metadata::Details>,
    custom_genres: bool,
}

/// Genres choisis à la place de ceux de Steam : seulement ceux de la liste de Ludex, trois au
/// plus (`corrections::clean_genres`). Liste vide : genres de Steam. Renvoie la fiche à jour.
#[tauri::command]
fn set_genres(state: State<'_, AppState>, id: String, genres: Vec<String>) -> Result<GenresUpdate, String> {
    if genres.iter().any(|g| g.len() > 64) {
        return Err(tr!("Genre inconnu.", "Unknown genre."));
    }
    let genres = corrections::clean_genres(&genres)?;
    let mut fresh = find_game(&state, &id)?;
    let mut data = lock(&state.data)?;
    data.set_genres(&fresh.id, genres);
    data.save(&state.data_path)?;
    // Fiche refaite depuis le cache (genres de Steam), puis la correction s'il y en a une. Le cache
    // est relâché avant de prendre la bibliothèque (`publish_details` les prend dans l'autre sens).
    fresh.details = None;
    lock(&state.meta)?.apply(&mut fresh);
    data.correct_genres(&mut fresh);
    if let Some(g) = lock(&state.games)?.iter_mut().find(|g| g.id == id) {
        g.details = fresh.details.clone();
        g.custom_genres = fresh.custom_genres;
    }
    Ok(GenresUpdate { details: fresh.details, custom_genres: fresh.custom_genres })
}

/// Genres que l'on peut choisir dans la fiche, dans l'ordre de Ludex.
#[tauri::command]
fn genre_choices() -> Vec<&'static str> {
    metadata::genre_names().collect()
}

/// Options de lancement d'un jeu, validées et normalisées ici (`launch_args::clean`) ; renvoie
/// le texte enregistré (`None` : effacé). Refusé si le launcher du jeu ne transmet pas d'options.
#[tauri::command]
fn set_launch_args(state: State<'_, AppState>, id: String, text: String) -> Result<Option<String>, String> {
    collections::check_input(&text)?;
    let game = find_game(&state, &id)?;
    if let Some(why) = launch_args::refusal(&game) {
        return Err(why.to_string());
    }
    let args = launch_args::clean(&text)?;
    let saved = args.clone();
    update_data(&state, &id, move |data, game| data.set_launch_args(&game.id, args))?;
    Ok(saved)
}

/// Collections de l'utilisateur et, à part, celles du client Steam (lecture seule).
#[derive(Serialize)]
struct CollectionsInfo {
    mine: Vec<collections::Collection>,
    steam: Vec<collections::Collection>,
}

#[tauri::command]
async fn collections(app: tauri::AppHandle) -> Result<CollectionsInfo, String> {
    blocking(move || {
        let mine = lock(&app.state::<AppState>().data)?.collections.clone();
        Ok(CollectionsInfo { mine, steam: collections::steam_collections() })
    })
    .await
}

/// Modifie les collections de l'utilisateur, enregistre, puis renvoie le résultat et la liste à jour.
fn update_collections<R>(
    state: &AppState,
    change: impl FnOnce(&mut UserData) -> Result<R, String>,
) -> Result<(R, Vec<collections::Collection>), String> {
    let mut data = lock(&state.data)?;
    // Changement refusé ou non enregistré : les collections en mémoire reviennent en arrière.
    let before = data.collections.clone();
    match change(&mut data).and_then(|out| data.save(&state.data_path).map(|()| out)) {
        Ok(out) => Ok((out, data.collections.clone())),
        Err(e) => {
            data.collections = before;
            Err(e)
        }
    }
}

/// Collection créée : son id (généré ici) et la liste à jour.
#[derive(Serialize)]
struct CreatedCollection {
    id: String,
    mine: Vec<collections::Collection>,
}

/// Crée une collection ; `game` : jeu à y mettre tout de suite (création depuis une fiche).
#[tauri::command]
fn create_collection(state: State<'_, AppState>, name: String, game: Option<String>) -> Result<CreatedCollection, String> {
    collections::check_input(&name)?;
    if let Some(id) = &game {
        find_game(&state, id)?;
    }
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    let (id, mine) = update_collections(&state, |data| {
        let id = data.create_collection(&name, seed)?;
        if let Some(game) = &game {
            data.set_in_collection(&id, game, true)?;
        }
        Ok(id)
    })?;
    Ok(CreatedCollection { id, mine })
}

#[tauri::command]
fn rename_collection(state: State<'_, AppState>, id: String, name: String) -> Result<Vec<collections::Collection>, String> {
    collections::check_input(&name)?;
    update_collections(&state, |data| data.rename_collection(&id, &name)).map(|((), mine)| mine)
}

#[tauri::command]
fn delete_collection(state: State<'_, AppState>, id: String) -> Result<Vec<collections::Collection>, String> {
    update_collections(&state, |data| data.delete_collection(&id)).map(|(_, mine)| mine)
}

/// Ajoute un jeu de la bibliothèque à une collection, ou l'en retire.
#[tauri::command]
fn set_in_collection(state: State<'_, AppState>, collection: String, id: String, value: bool) -> Result<Vec<collections::Collection>, String> {
    if value {
        find_game(&state, &id)?;
    }
    update_collections(&state, |data| data.set_in_collection(&collection, &id, value)).map(|((), mine)| mine)
}

/// Retire un jeu ajouté à la main. Les jeux détectés ne se retirent pas : ils se masquent.
#[tauri::command]
async fn remove_game(app: tauri::AppHandle, id: String) -> Result<ScanResult, String> {
    if !manual::is_manual(&id) {
        return Err(tr!(
            "Seuls les jeux ajoutés à la main se retirent. Les autres se masquent.",
            "Only games added by hand can be removed. Other games can be hidden."
        ));
    }
    blocking(move || {
        let state = app.state::<AppState>();
        let scanning = wait_scan(&state);
        let (removed, scan) = save_and_rescan(&app, &state, scanning, |data| data.remove_manual(&id))?;
        if removed { Ok(scan) } else { Err(not_in_library()) }
    })
    .await
}

/// Sélecteur d'image natif (PNG, JPEG, WebP), au-dessus de la fenêtre de Ludex. Le chemin choisi
/// reste côté Rust. `None` : annulé.
fn pick_image(window: &tauri::WebviewWindow, title: &str) -> Option<PathBuf> {
    let dialog = window.dialog().file().set_title(title).add_filter("Image (PNG, JPEG, WebP)", &covers::EXTENSIONS).set_parent(window);
    dialog.blocking_pick_file()?.into_path().ok()
}

/// Choisit une jaquette ou une image de fond perso pour un jeu : sélecteur natif, vérification,
/// copie dans le dossier de données, puis l'image passe devant toutes les autres (événement
/// `game-art`). La recherche en ligne de la jaquette s'arrête pour ce jeu. `None` : annulé.
#[tauri::command]
async fn choose_cover(app: tauri::AppHandle, window: tauri::WebviewWindow, id: String, kind: covers::Kind) -> Result<Option<ArtEvent>, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let game = find_game(&state, &id)?;
        let Some(picked) = pick_image(&window, &tr!("{} de {}", "{} for {}", kind.title(), game.title)) else { return Ok(None) };
        let (bytes, ext) = covers::check_image(&picked)?;
        // Un scan en cours remplacerait l'état sans ce choix : on attend sa fin.
        let _scanning = wait_scan(&state);
        let root = lock(&state.images)?.root().to_path_buf();
        let file = covers::save(&root, &id, kind, &bytes, ext)?;
        let choice = covers::Picked { ext: ext.to_string(), at: covers::stamp() };
        {
            let mut data = lock(&state.data)?;
            let old = data.set_cover(&id, kind, Some(choice.clone()));
            if let Err(e) = data.save(&state.data_path) {
                data.set_cover(&id, kind, old);
                return Err(e);
            }
        }
        let event = {
            let mut games = lock(&state.games)?;
            let mut originals = lock(&state.originals)?;
            let g = games.iter_mut().find(|g| g.id == id).ok_or_else(not_in_library)?;
            covers::set(g, kind, file, &choice, &mut originals);
            ArtEvent::of(g)
        };
        let _ = app.emit(ART_EVENT, event.clone());
        Ok(Some(event))
    })
    .await
}

/// Retire la jaquette ou l'image de fond perso d'un jeu : l'image d'origine revient sans rescan
/// (launcher, cache, CDN, sinon jaquette générée et recherche en ligne relancée).
#[tauri::command]
async fn reset_cover(app: tauri::AppHandle, id: String, kind: covers::Kind) -> Result<ArtEvent, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        find_game(&state, &id)?;
        let _scanning = wait_scan(&state);
        {
            let mut data = lock(&state.data)?;
            let old = data.set_cover(&id, kind, None);
            if let Err(e) = data.save(&state.data_path) {
                data.set_cover(&id, kind, old);
                return Err(e);
            }
        }
        let event = {
            // Même ordre que `resolve_art` (jaquettes, jeux) puis jeux, images : jamais l'inverse.
            let art = lock(&state.art)?;
            let mut games = lock(&state.games)?;
            let images = lock(&state.images)?;
            let mut originals = lock(&state.originals)?;
            covers::remove(images.root(), &id, kind);
            let g = games.iter_mut().find(|g| g.id == id).ok_or_else(not_in_library)?;
            covers::restore(g, kind, &mut originals);
            // Jaquette retrouvée en ligne pendant que l'image perso la cachait, image déjà en cache.
            art.apply(g);
            images.apply(g);
            ArtEvent::of(g)
        };
        let _ = app.emit(ART_EVENT, event.clone());
        // Jaquette encore inconnue ou image distante pas en cache : recherche et téléchargement.
        resolve_art(app.clone());
        resolve_images(app.clone());
        Ok(event)
    })
    .await
}

/// Lance le jeu et renvoie l'heure du lancement (secondes Unix). Depuis la recherche rapide : la
/// fenêtre se cache et la fenêtre principale reçoit la date du lancement (`tray-launch`) ; une
/// erreur reste affichée dans la recherche rapide.
#[tauri::command]
fn launch_game(app: AppHandle, window: tauri::Window, state: State<'_, AppState>, id: String) -> Result<u64, String> {
    let launched = launch(&app, &state, &id)?;
    if window.label() == quick::LABEL {
        quick::hide(&app);
        let title = find_game(&state, &id).map(|g| g.title).unwrap_or_default();
        let event = TrayLaunchEvent { id, title, last_played: Some(launched), error: None };
        let _ = app.emit_to(MAIN_WINDOW, TRAY_LAUNCH_EVENT, event);
    }
    Ok(launched)
}

/// Fenêtre principale (label par défaut de Tauri).
const MAIN_WINDOW: &str = "main";

/// Jeux proposés par la recherche rapide : la bibliothèque du dernier scan, sans les jeux masqués.
/// Aucun scan ici : la fenêtre principale (même cachée) scanne au démarrage.
#[tauri::command]
fn quick_games(state: State<'_, AppState>) -> Result<Vec<Game>, String> {
    Ok(lock(&state.games)?.iter().filter(|g| !g.hidden).cloned().collect())
}

/// Cache la recherche rapide (Échap). Sans argument : le front ne peut cacher qu'elle.
#[tauri::command]
fn hide_quick(app: AppHandle) {
    quick::hide(&app);
}

/// Lance un jeu connu, démarre son suivi et note la date : commun au front et à la zone de
/// notification. Renvoie l'heure du lancement (secondes Unix).
fn launch<R: Runtime>(app: &AppHandle<R>, state: &AppState, id: &str) -> Result<u64, String> {
    let game = find_game(state, id)?;
    // Lancement reconstruit ici depuis l'état, options de l'utilisateur comprises.
    let launch = launch_args::launch_for(&game)?;

    match &launch {
        Launch::Uri(uri) => app
            .opener()
            .open_url(uri, None::<&str>)
            .map_err(|e| tr!("Le launcher n'a pas répondu : {e}", "The launcher didn't respond: {e}")),
        Launch::Exec { path, args } => std::process::Command::new(path)
            .args(args)
            // Beaucoup de jeux cherchent leurs fichiers dans le dossier courant.
            .current_dir(path.parent().unwrap_or(path))
            .spawn()
            .map(|_| ())
            .map_err(|e| tr!("Impossible de démarrer {} : {e}", "Couldn't start {}: {e}", path.display())),
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
    if let Some(w) = app.get_webview_window(MAIN_WINDOW) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// État de la fenêtre principale, pour décider d'une notification Windows.
fn presence<R: Runtime>(app: &AppHandle<R>) -> notify::Presence {
    let Some(w) = app.get_webview_window(MAIN_WINDOW) else { return notify::Presence::Hidden };
    if !w.is_visible().unwrap_or(false) || w.is_minimized().unwrap_or(false) {
        notify::Presence::Hidden
    } else if w.is_focused().unwrap_or(false) {
        notify::Presence::Focused
    } else {
        notify::Presence::Visible
    }
}

/// Notification Windows, si son type est activé, si la fenêtre ne la rend pas inutile et si le
/// limiteur l'accepte. `false` : rien n'est parti (l'appelant garde alors son toast de l'appli).
pub(crate) fn notify_user<R: Runtime>(app: &AppHandle<R>, note: notify::Note) -> bool {
    let Some(state) = app.try_state::<AppState>() else { return false };
    if !lock(&state.data).is_ok_and(|d| d.settings.notifications.get(note.kind)) || !notify::wanted(note.kind, presence(app)) {
        return false;
    }
    app.try_state::<notify::Notifier>().is_some_and(|n| n.show(note))
}

/// Clic sur une notification : fenêtre affichée, puis le front ouvre la fiche ou la section.
fn notification_clicked<R: Runtime>(app: &AppHandle<R>, target: notify::Target) {
    show_main(app);
    let _ = app.emit_to(MAIN_WINDOW, notify::OPEN_EVENT, target);
}

/// Disques presque pleins (moins de 10 % libres) qui portent des jeux : une notification par
/// disque et par jour.
fn check_disks<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<AppState>() else { return };
    if !lock(&state.data).is_ok_and(|d| d.settings.notifications.low_disk) {
        return;
    }
    let Ok(games) = lock(&state.games).map(|g| g.clone()) else { return };
    for note in notify::low_disk_notes(&storage::disks(&games, &storage::volumes()), &notify::today()) {
        notify_user(app, note);
    }
}

/// Essai à l'écran (`notify-preview`, `LUDEX_NOTIFY_PREVIEW=1`) : deux vraies notifications,
/// 15 s après le premier scan, avec les mêmes règles que les vraies (fenêtre sans le focus).
#[cfg(feature = "notify-preview")]
fn notify_preview(app: &tauri::AppHandle) {
    if std::env::var("LUDEX_NOTIFY_PREVIEW").as_deref() != Ok("1") {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let Some(state) = app.try_state::<AppState>() else { return };
        while lock(&state.last_scan).is_ok_and(|t| t.is_none()) {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        std::thread::sleep(std::time::Duration::from_secs(15));
        let game = lock(&state.games).ok().and_then(|g| g.iter().find(|g| g.platform == library::Platform::Steam && !g.hidden).cloned());
        if let Some(g) = game {
            println!("Essai : notification de {} : {}", g.title, notify_user(&app, notify::download_note(&g.id, &g.title, true)));
        }
        let note = notify::Note {
            kind: notify::Kind::LowDisk,
            key: format!("essai:{}", notify::now()),
            title: tr!("Disque E: presque plein (essai)", "Drive E: almost full (test)"),
            body: tr!("Notification d'essai de Ludex : un clic ouvre le Stockage.", "Ludex test notification: click to open Storage."),
            target: notify::Target::Storage,
        };
        println!("Essai : notification du disque : {}", notify_user(&app, note));
    });
}

fn autostart_enabled<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.try_state::<AutoLaunchManager>().is_some_and(|m| m.is_enabled().unwrap_or(false))
}

/// Reconstruit le menu de la zone de notification s'il a changé (`force` : toujours, après un
/// clic sur une case à cocher que Windows a déjà basculée). Le menu est construit sur le fil
/// principal sans l'attendre : appelée depuis le fil d'un scan, une construction directe attendrait
/// le fil principal et pourrait bloquer le scan. Aucun verrou n'est tenu pendant la construction.
fn refresh_tray<R: Runtime>(app: &AppHandle<R>, force: bool) {
    // Statut Discord : titres relus (jeu renommé, partie reprise avant le premier scan).
    if let Some(presence) = app.try_state::<discord::Presence>() {
        presence.refresh();
    }
    let (Some(state), Some(_)) = (app.try_state::<AppState>(), app.tray_by_id(tray::TRAY_ID)) else { return };
    let keep = lock(&state.data).map(|d| d.settings.keep_in_tray).unwrap_or(true);
    let autostart = autostart_enabled(app);
    let Ok(entries) = lock(&state.games).map(|games| {
        let recent = tray::recent(&games, tray::RECENT_COUNT);
        // Mêmes jeux dans la Jump List (son fil ne la repose que si elle change).
        state.jump_list.set(&recent);
        tray::entries(&recent, autostart, keep)
    }) else {
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
/// Relit les caches propres à une langue (fiches Steam, configurations requises) après un
/// changement de langue : le rescan qui suit pose les fiches connues dans la nouvelle langue et
/// relance la lecture des autres.
fn reload_language_caches(state: &AppState) {
    if let Ok(mut meta) = lock(&state.meta) {
        *meta = metadata::MetaCache::load(&state.data_dir);
    }
    if let Ok(mut req) = lock(&state.requirements) {
        *req = requirements::ReqCache::load(&state.data_dir);
    }
}

fn update_settings<R: Runtime>(app: &AppHandle<R>, change: impl FnOnce(&mut store::Settings)) {
    let Some(state) = app.try_state::<AppState>() else { return };
    if let Ok(mut data) = lock(&state.data) {
        change(&mut data.settings);
        if let Err(e) = data.save(&state.data_path) {
            eprintln!("Réglages non enregistrés : {e}");
        }
    }
}

/// Active ou coupe le lancement avec Windows (menu de la zone de notification, Paramètres).
/// L'entrée du registre fait foi : le réglage enregistré reprend son état réel après la modification.
fn set_autostart<R: Runtime>(app: &AppHandle<R>, value: bool) -> Result<(), String> {
    let manager = app.try_state::<AutoLaunchManager>().ok_or(tr_str!("Lancement avec Windows indisponible.", "Start with Windows unavailable."))?;
    let result = if value { manager.enable() } else { manager.disable() };
    let enabled = autostart_enabled(app);
    update_settings(app, |s| s.launch_at_startup = enabled);
    result.map_err(|e| tr!("Lancement avec Windows non modifié : {e}", "Start with Windows not changed: {e}"))
}

fn on_tray_menu<R: Runtime>(app: &AppHandle<R>, id: &str) {
    match tray::action(id) {
        Some(tray::Action::Launch(game)) => {
            let Some(state) = app.try_state::<AppState>() else { return };
            let title = find_game(&state, &game).map_or_else(|_| this_game(), |g| g.title);
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
            if let Err(e) = set_autostart(app, !autostart_enabled(app)) {
                eprintln!("{e}");
            }
            refresh_tray(app, true);
            // La page Paramètres, si elle est ouverte, suit.
            let _ = settings::publish(app);
        }
        Some(tray::Action::ToggleKeepInTray) => {
            update_settings(app, |s| s.keep_in_tray = !s.keep_in_tray);
            refresh_tray(app, true);
            let _ = settings::publish(app);
        }
        Some(tray::Action::Quit) => app.exit(0),
        None => {}
    }
}

/// Titre d'un jeu inconnu dans l'événement `tray-launch`.
fn this_game() -> String {
    tr!("ce jeu", "this game")
}

/// Délai laissé au premier scan quand Ludex est démarré par un lien.
const FIRST_SCAN_WAIT: Duration = Duration::from_secs(120);

/// Attend la fin du premier scan (bibliothèque vide avant). `false` : toujours rien après le délai.
fn wait_first_scan(state: &AppState) -> bool {
    let start = Instant::now();
    while start.elapsed() < FIRST_SCAN_WAIT {
        if lock(&state.last_scan).is_ok_and(|last| last.is_some()) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    false
}

/// Liens `ludex://` reçus (au démarrage ou relayés par l'instance unique), traités hors du fil
/// appelant : le premier scan peut se faire attendre.
fn open_links<R: Runtime>(app: &AppHandle<R>, urls: Vec<String>) {
    let app = app.clone();
    std::thread::spawn(move || {
        for url in urls {
            open_link(&app, &url);
        }
    });
}

/// Lance le jeu d'un lien `ludex://launch/<id>`, par le même chemin que la zone de notification
/// (`launch`, puis `tray-launch` pour le front). Lien refusé ou jeu inconnu : message console, et
/// toast seulement si la fenêtre est déjà visible (un lien peut venir de n'importe où). Échec du
/// lancement d'un jeu connu : la fenêtre s'affiche avec l'erreur, comme depuis la zone de notification.
fn open_link<R: Runtime>(app: &AppHandle<R>, link: &str) {
    let Some(state) = app.try_state::<AppState>() else { return };
    let refuse = |id: String, error: String| {
        eprintln!("Lien {link} refusé : {error}");
        if app.get_webview_window(MAIN_WINDOW).is_some_and(|w| w.is_visible().unwrap_or(false)) {
            let event = TrayLaunchEvent { id, title: this_game(), last_played: None, error: Some(error) };
            let _ = app.emit_to(MAIN_WINDOW, TRAY_LAUNCH_EVENT, event);
        }
    };
    let id = match deeplink::parse(link) {
        Ok(id) => id,
        Err(e) => return refuse(String::new(), e.to_string()),
    };
    if !wait_first_scan(&state) {
        return refuse(id, tr!("Bibliothèque pas encore lue.", "Library not read yet."));
    }
    let Ok(game) = find_game(&state, &id) else {
        return refuse(
            id,
            tr!(
                "Ce jeu n'est pas dans la bibliothèque (désinstallé, masqué par un launcher désactivé ?).",
                "This game isn't in the library (uninstalled, or hidden by a disabled launcher?)."
            ),
        );
    };
    let event = match launch(app, &state, &id) {
        Ok(ts) => TrayLaunchEvent { id, title: game.title, last_played: Some(ts), error: None },
        Err(e) => {
            show_main(app);
            TrayLaunchEvent { id, title: game.title, last_played: None, error: Some(e) }
        }
    };
    let _ = app.emit_to(MAIN_WINDOW, TRAY_LAUNCH_EVENT, event);
}

/// Crée sur le Bureau un raccourci `.url` qui lance le jeu par `ludex://launch/<id>`. Le chemin et
/// le contenu sont construits ici depuis le jeu de l'état ; renvoie le nom du fichier créé.
#[tauri::command]
async fn create_shortcut(app: AppHandle, id: String) -> Result<String, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let game = find_game(&state, &id)?;
        let desktop = dirs::desktop_dir().ok_or(tr_str!("Bureau introuvable.", "Desktop not found."))?;
        let ludex = std::env::current_exe().ok();
        let icon = shortcut::icon_for(&game, ludex.as_deref());
        let text = shortcut::contents(&deeplink::launch_url(&game.id), icon.as_deref());
        shortcut::write(&desktop, &shortcut::file_stem(&game.title), &text)
    })
    .await
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
    serve_file(&file, if_none_match, |f| read_art(f, kind))
}

fn not_found() -> Response<Vec<u8>> {
    Response::builder().status(StatusCode::NOT_FOUND).body(Vec::new()).unwrap_or_default()
}

/// Réponse pour un fichier local, revalidée par `ETag` (304 sans lecture du fichier).
fn serve_file(file: &Path, if_none_match: Option<&str>, read: impl FnOnce(&Path) -> Option<(Vec<u8>, &'static str)>) -> Response<Vec<u8>> {
    let tag = etag(file);
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
    let Some((bytes, mime)) = read(file) else { return not_found() };
    builder.header(header::CONTENT_TYPE, mime).body(bytes).unwrap_or_else(|_| not_found())
}

/// Sert une capture : `/capture/<empreinte>/<thumb|full>`. L'empreinte doit venir d'un relevé
/// (`game_screenshots`, `all_screenshots`) et le fichier être encore dans un dossier de captures
/// connu (`screenshots::Index::file`). Jamais de chemin dans l'URL.
fn serve_capture<R: Runtime>(app: &tauri::AppHandle<R>, rest: &str, if_none_match: Option<&str>, range: Option<&str>) -> Response<Vec<u8>> {
    let mut parts = rest.split('/');
    let (Some(key), Some(kind), None) = (parts.next(), parts.next(), parts.next()) else { return not_found() };
    if !screenshots::valid_key(key) || !matches!(kind, "thumb" | "full") {
        return not_found();
    }
    let Some(state) = app.try_state::<AppState>() else { return not_found() };
    let Some(file) = lock(&state.shots).ok().and_then(|index| index.file(key, kind == "thumb")) else { return not_found() };
    if file.extension().is_some_and(|e| e.eq_ignore_ascii_case("mp4")) {
        return serve_video(&file, range);
    }
    serve_file(&file, if_none_match, |f| {
        let bytes = std::fs::read(f).ok()?;
        let mime = screenshots::mime(f, &bytes);
        Some((bytes, mime))
    })
}

/// Vidéo servie par morceaux (`Range`) : la lecture et le déplacement dans la vidéo ne chargent
/// jamais le fichier entier en mémoire.
fn serve_video(file: &Path, range: Option<&str>) -> Response<Vec<u8>> {
    let Ok(len) = std::fs::metadata(file).map(|m| m.len()) else { return not_found() };
    let Some((start, end)) = screenshots::byte_range(range, len) else {
        return Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(header::CONTENT_RANGE, format!("bytes */{len}"))
            .body(Vec::new())
            .unwrap_or_else(|_| not_found());
    };
    let Some(bytes) = screenshots::read_range(file, start, end) else { return not_found() };
    let partial = range.is_some() || end + 1 < len;
    let mut builder = Response::builder()
        .status(if partial { StatusCode::PARTIAL_CONTENT } else { StatusCode::OK })
        .header(header::CONTENT_TYPE, "video/mp4")
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CACHE_CONTROL, "no-cache");
    if partial {
        builder = builder.header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"));
    }
    builder.body(bytes).unwrap_or_else(|_| not_found())
}

/// Jeux de l'état et dossiers de captures, pour un relevé hors du verrou des jeux.
fn screenshot_context(state: &AppState) -> Result<(Vec<Game>, screenshots::Roots), String> {
    Ok((lock(&state.games)?.clone(), screenshots::Roots::this_pc()))
}

/// Captures d'un jeu, la plus récente d'abord.
#[tauri::command]
async fn game_screenshots(app: tauri::AppHandle, id: String) -> Result<Vec<screenshots::Shot>, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        find_game(&state, &id)?;
        let (games, roots) = screenshot_context(&state)?;
        let shots = screenshots::collect(&roots, &games, Some(&id));
        lock(&state.shots)?.remember(&roots, &shots, false);
        Ok(shots)
    })
    .await
}

/// Toutes les captures, groupées par jeu (« Autres captures » en dernier).
#[tauri::command]
async fn all_screenshots(app: tauri::AppHandle) -> Result<Vec<screenshots::Group>, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let (games, roots) = screenshot_context(&state)?;
        let shots = screenshots::collect(&roots, &games, None);
        lock(&state.shots)?.remember(&roots, &shots, true);
        Ok(screenshots::group(shots, &games))
    })
    .await
}

/// Montre une capture dans l'Explorateur. `id` : jeu de la capture, `None` pour « Autres captures ».
/// Le chemin est retrouvé par un nouveau relevé côté Rust, jamais reçu du front.
#[tauri::command]
async fn show_screenshot(app: tauri::AppHandle, id: Option<String>, key: String) -> Result<(), String> {
    if !screenshots::valid_key(&key) {
        return Err(tr!("Capture inconnue.", "Unknown screenshot."));
    }
    blocking(move || {
        let state = app.state::<AppState>();
        if let Some(id) = &id {
            find_game(&state, id)?;
        }
        let (games, roots) = screenshot_context(&state)?;
        let path = screenshots::collect(&roots, &games, id.as_deref())
            .into_iter()
            .find(|s| s.key == key && s.game_id == id)
            .map(|s| s.path)
            .filter(|p| screenshots::allowed(p, &roots))
            .ok_or(tr_str!("Cette capture n'est plus dans son dossier.", "This screenshot is no longer in its folder."))?;
        app.opener()
            .reveal_item_in_dir(&path)
            .map_err(|e| tr!("Impossible d'ouvrir le dossier de la capture : {e}", "Couldn't open the screenshot folder: {e}"))
    })
    .await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Avant toute fenêtre : même identifiant que les raccourcis de l'installeur (Jump List).
    jumplist::set_process_app_id();
    tauri::Builder::default()
        // En premier : relancer Ludex alors qu'il tourne ramène la fenêtre existante. Avec la fonction
        // `deep-link`, un lien `ludex://` passé à la seconde instance est d'abord relayé au plugin
        // deep-link (événement écouté dans `setup`) : la fenêtre reste alors comme elle est.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if !deeplink::is_link_launch(&args) {
                show_main(app);
            }
        }))
        // Lien `ludex://launch/<id>` (`deeplink.rs`), utilisé depuis Rust seulement : aucune
        // permission `deep-link` côté front.
        .plugin(tauri_plugin_deep_link::init())
        // Utilisé depuis Rust seulement (menu de la zone de notification) : aucune permission au front.
        .plugin(tauri_plugin_autostart::Builder::new().arg(tray::MINIMIZED_ARG).build())
        .plugin(tauri_plugin_opener::init())
        // Aucune permission du plugin côté front : tout passe par `update.rs`.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(update::PendingUpdate::default())
        // Sélecteur de fichier, utilisé seulement depuis Rust : aucune permission `dialog` côté front.
        .plugin(tauri_plugin_dialog::init())
        // Raccourci de la recherche rapide, enregistré depuis Rust (`quick.rs`) : aucune permission
        // `global-shortcut` côté front.
        .plugin(tauri_plugin_global_shortcut::Builder::new().with_handler(quick::on_shortcut).build())
        .register_asynchronous_uri_scheme_protocol("ludex", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let path = request.uri().path().to_string();
            let text = |name| request.headers().get(name).and_then(|v| v.to_str().ok()).map(String::from);
            let if_none_match = text(header::IF_NONE_MATCH);
            let range = text(header::RANGE);
            tauri::async_runtime::spawn_blocking(move || {
                let response = match path.strip_prefix("/capture/") {
                    Some(rest) => serve_capture(&app, rest, if_none_match.as_deref(), range.as_deref()),
                    None => serve_art(&app, &path, if_none_match.as_deref()),
                };
                responder.respond(response)
            });
        })
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let data_path = store::file_in(dir.clone());
            let handle = app.handle().clone();
            let data = UserData::load(&data_path);
            // Langue choisie (ou celle de Windows) avant tout texte montré.
            i18n::set(i18n::resolve(data.settings.language));
            // Cache des images dans le dossier choisi pour les données lourdes, s'il y en a un.
            let cache = relocate::cache_root(&dir, data.settings.data_dir.as_deref());
            app.manage(AppState {
                games: Mutex::default(),
                data: Mutex::new(data),
                data_path,
                tracker: Tracker::spawn(
                    history::Journal::open(&dir),
                    std::sync::Arc::new(performance::Store::open(&dir)),
                    move |update| on_session_update(&handle, update),
                ),
                art: Mutex::new(art::ArtCache::load(&dir)),
                meta: Mutex::new(metadata::MetaCache::load(&dir)),
                hltb: Mutex::new(hltb::HltbCache::load(&dir)),
                images: Mutex::new(images::ImageCache::load(cache.join("images"))),
                global: Mutex::new(showcase::GlobalCache::load(&dir)),
                requirements: Mutex::new(requirements::ReqCache::load(&dir)),
                requirements_lock: Mutex::new(()),
                requirements_busy: AtomicBool::new(false),
                news: Mutex::new(news::NewsCache::load(&dir)),
                wishlist: wishlist::State::load(&dir),
                workshop: Mutex::new(workshop::WorkshopCache::load(&dir)),
                workshop_lock: Mutex::new(()),
                workshop_busy: AtomicBool::new(false),
                sizes: Mutex::new(storage::SizeCache::load(&dir)),
                sizes_busy: AtomicBool::new(false),
                footprints: Mutex::default(),
                data_dir: dir,
                art_busy: AtomicBool::new(false),
                meta_busy: AtomicBool::new(false),
                hltb_busy: AtomicBool::new(false),
                images_busy: AtomicBool::new(false),
                originals: Mutex::default(),
                scan_lock: Mutex::new(()),
                last_scan: Mutex::new(None),
                first_scan: AtomicBool::new(true),
                watch: OnceLock::new(),
                tray_menu: Mutex::default(),
                jump_list: jumplist::JumpList::spawn(std::env::current_exe().ok()),
                hide_pending: AtomicBool::new(false),
                shots: Mutex::default(),
                saves: Mutex::default(),
                saves_busy: AtomicBool::new(false),
                saves_lock: Mutex::new(()),
                pending_import: Mutex::new(None),
                news_busy: AtomicBool::new(false),
                pending_news: Mutex::default(),
                relocating: AtomicBool::new(false),
                picked_data_dir: Mutex::default(),
            });
            // Statut Discord : avant la reprise des sessions, qui peut déjà annoncer une partie.
            let (games, prefs) = (app.handle().clone(), app.handle().clone());
            app.manage(discord::Presence::spawn(
                move |id| {
                    let state = games.try_state::<AppState>()?;
                    let games = lock(&state.games).ok()?;
                    games.iter().find(|g| g.id == id).map(discord::Shown::of)
                },
                move || {
                    prefs
                        .try_state::<AppState>()
                        .and_then(|s| lock(&s.data).ok().map(|d| discord::Prefs { enabled: d.settings.discord_presence, hide_name: d.settings.discord_hide_name }))
                        .unwrap_or_default()
                },
            ));
            let clicks = app.handle().clone();
            let notes_dir = app.state::<AppState>().data_dir.clone();
            app.manage(notify::Notifier::spawn(&notes_dir, move |target| notification_clicked(&clicks, target)));
            #[cfg(feature = "notify-preview")]
            notify_preview(app.handle());
            // Avant toute écriture dans le dossier de données : premier lancement ou habitué ?
            let welcome = welcome::Welcome::detect(&app.state::<AppState>());
            app.manage(welcome);
            // Une fois l'état en place : le temps mesuré avant l'arrêt précédent peut y être enregistré.
            app.state::<AppState>().tracker.resume();
            update::spawn_check(app.handle().clone());
            // Le réglage enregistré reprend l'état réel du registre (entrée retirée par Windows…).
            let enabled = autostart_enabled(app.handle());
            update_settings(app.handle(), |s| s.launch_at_startup = enabled);
            create_tray(app)?;
            // Thème choisi dans les Paramètres (la fenêtre est créée sombre par `tauri.conf.json`).
            let theme = lock(&app.state::<AppState>().data).map(|d| d.settings.theme).unwrap_or_default();
            settings::apply_theme(app.handle(), theme);
            let shortcut = lock(&app.state::<AppState>().data).map(|d| d.settings.quick_shortcut.clone()).unwrap_or_default();
            quick::setup(app.handle(), &shortcut);
            // Liens `ludex://` : schéma enregistré par l'installeur ; en debug, vers cet exécutable.
            #[cfg(debug_assertions)]
            if let Err(e) = app.deep_link().register_all() {
                eprintln!("Schéma ludex:// non enregistré : {e}");
            }
            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| open_links(&handle, event.urls().iter().map(|u| u.to_string()).collect()));
            // Ludex démarré par un lien : le jeu est lancé après le premier scan.
            if let Ok(Some(urls)) = app.deep_link().get_current() {
                open_links(app.handle(), urls.iter().map(|u| u.to_string()).collect());
            }
            // Fenêtre créée cachée : lancé avec Windows, Ludex reste dans la zone de notification.
            if !std::env::args().any(|a| a == tray::MINIMIZED_ARG) {
                show_main(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // Recherche rapide : cachée dès qu'on clique ailleurs, jamais fermée (Alt+F4 la cache).
            tauri::WindowEvent::Focused(false) if window.label() == quick::LABEL => {
                let _ = window.hide();
            }
            tauri::WindowEvent::CloseRequested { api, .. } if window.label() == quick::LABEL => {
                api.prevent_close();
                let _ = window.hide();
            }
            // Fenêtre principale fermée pour de bon (« garder dans la zone de notification » coupé) :
            // la recherche rapide, cachée, ne doit pas garder Ludex ouvert.
            tauri::WindowEvent::Destroyed if window.label() == MAIN_WINDOW => window.app_handle().exit(0),
            // Retour sur Ludex : annonces en attente, et après un long moment, relecture des
            // launchers sans surveillance.
            tauri::WindowEvent::Focused(true) => {
                if window.label() == MAIN_WINDOW {
                    flush_news(window.app_handle());
                    wishlist::flush(window.app_handle());
                }
                if window.app_handle().try_state::<AppState>().is_some_and(|s| is_stale(&s)) {
                    let app = window.app_handle().clone();
                    std::thread::spawn(move || auto_rescan(&app, true));
                }
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
            game_performance,
            recent_playtime,
            last_sessions,
            install_dates,
            recap,
            stats,
            story::recap_rarest,
            story::save_story_image,
            journal,
            game_achievements,
            game_requirements,
            game_news,
            open_news,
            game_mods,
            refresh_mods,
            open_mods_dir,
            open_mod_page,
            wishlist::wishlist,
            wishlist::wishlist_search,
            wishlist::wishlist_add,
            wishlist::wishlist_remove,
            wishlist::open_store_page,
            showcase,
            storage,
            uninstall_game,
            game_footprint,
            open_footprint_item,
            run_uninstaller,
            open_apps_settings,
            update::available_update,
            update::install_update,
            add_game,
            change_executable,
            rename_game,
            remove_game,
            choose_cover,
            reset_cover,
            game_screenshots,
            all_screenshots,
            show_screenshot,
            game_saves,
            backup_saves,
            open_save_location,
            open_save_copy,
            restore_saves,
            choose_saves_dir,
            set_auto_saves,
            saves_overview,
            set_status,
            set_note,
            set_launch_args,
            set_aliases,
            set_genres,
            genre_choices,
            collections,
            create_collection,
            rename_collection,
            delete_collection,
            set_in_collection,
            quick_games,
            hide_quick,
            welcome::welcome,
            welcome::welcome_done,
            window_controls::window_minimize,
            window_controls::window_toggle_maximize,
            window_controls::window_close,
            welcome::welcome_again,
            relocate::data_location,
            relocate::choose_data_dir,
            relocate::move_data,
            update::check_update,
            settings::settings,
            settings::set_theme,
            settings::set_discord_presence,
            settings::set_discord_hide_name,
            settings::set_accent,
            settings::set_language,
            settings::set_notification,
            settings::set_launch_screen,
            settings::set_quick_shortcut,
            settings::pause_quick_shortcut,
            settings::set_launch_at_startup,
            settings::set_keep_in_tray,
            settings::set_source_enabled,
            settings::show_all_hidden,
            diagnostic::diagnostic,
            window_controls::window_enter_couch,
            window_controls::window_leave_couch,
            window_controls::window_couch_fullscreen,
            create_shortcut,
            transfer::export_data,
            transfer::import_data,
            transfer::cancel_import,
            transfer::apply_import
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app, event| {
            // Ludex se ferme : activité Discord effacée (Discord l'efface aussi en voyant le
            // processus disparaître, mais plus tard).
            if let tauri::RunEvent::Exit = event
                && let Some(presence) = app.try_state::<discord::Presence>()
            {
                presence.shutdown(std::time::Duration::from_millis(1500));
            }
        });
}
