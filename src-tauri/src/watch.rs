//! Surveillance des dossiers où les launchers déclarent leurs jeux : un jeu installé ou
//! désinstallé déclenche un rescan tout seul, une fois que les écritures se sont calmées.
//!
//! Pendant un téléchargement Steam, chaque écriture d'un manifeste est aussi relue tout de suite
//! (une fois par seconde et par jeu au plus) : la progression part au front sans rescan.
//!
//! Aucun relevé périodique : le fil dort sur le canal des événements du système (crate `notify`)
//! tant que rien ne bouge. Une erreur de surveillance n'est qu'un message dans la console.

use crate::library::{self, Game, InstallState, ManifestState, Platform};
use notify::event::{EventKind, ModifyKind};
use notify::{RecursiveMode, Watcher};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant, SystemTime};

/// Temps de calme attendu avant de rescanner : Steam écrit un manifeste plusieurs fois de suite.
pub const QUIET: Duration = Duration::from_secs(5);

/// Écart minimal entre deux relectures d'un même manifeste pendant un téléchargement.
pub const LIVE_GAP: Duration = Duration::from_secs(1);

/// Le débit est estimé sur les relectures de ces dernières secondes.
const SPEED_WINDOW: Duration = Duration::from_secs(20);

/// Durée minimale entre la première et la dernière relecture pour donner un débit.
const SPEED_MIN_SPAN: Duration = Duration::from_secs(2);

/// Ce que contient un dossier surveillé.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// `steamapps` d'une bibliothèque Steam : manifestes `appmanifest_<appid>.acf`.
    SteamApps,
    /// Manifestes Epic `*.item`.
    Epic,
    /// `ProgramData\Riot Games` : `RiotClientInstalls.json`.
    RiotRoot,
    /// `Riot Games\Metadata`, avec ses sous-dossiers : réglages `*.product_settings.yaml` d'un jeu.
    RiotMetadata,
    /// Bibliothèque de jeux (« Jeux », « Games ») : un sous-dossier par jeu.
    Library,
}

impl Source {
    fn mode(self) -> RecursiveMode {
        if self == Source::RiotMetadata { RecursiveMode::Recursive } else { RecursiveMode::NonRecursive }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Watched {
    pub dir: PathBuf,
    pub source: Source,
}

impl Watched {
    /// Le chemin d'un événement est-il dans ce dossier (enfant direct, ou plus bas si récursif) ?
    fn holds(&self, path: &Path) -> bool {
        match self.source.mode() {
            RecursiveMode::Recursive => path != self.dir && path.starts_with(&self.dir),
            RecursiveMode::NonRecursive => path.parent() == Some(self.dir.as_path()),
        }
    }
}

/// Dossiers surveillés sur ce PC, seulement ceux qui existent.
pub fn targets() -> Vec<Watched> {
    let dirs = library::watched_dirs();
    let mut list: Vec<Watched> = dirs.steamapps.into_iter().map(|dir| Watched { dir, source: Source::SteamApps }).collect();
    list.push(Watched { dir: dirs.epic, source: Source::Epic });
    list.push(Watched { dir: dirs.riot.join("Metadata"), source: Source::RiotMetadata });
    list.push(Watched { dir: dirs.riot, source: Source::RiotRoot });
    list.extend(dirs.libraries.into_iter().map(|dir| Watched { dir, source: Source::Library }));
    list.retain(|w| w.dir.is_dir());
    list
}

/// Suite à donner à un événement.
#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Ignore,
    /// Changement certain : rescan après le temps de calme.
    Rescan,
    /// Manifeste touché : relu après le temps de calme, rescan seulement si son état a changé.
    Manifest,
}

/// Filtre un événement sur un chemin d'un dossier surveillé. `is_dir` n'est appelé que pour une
/// création dans une bibliothèque (un fichier posé à la racine n'est pas un jeu).
fn verdict(source: Source, kind: &EventKind, path: &Path, is_dir: impl FnOnce() -> bool) -> Verdict {
    let changed = matches!(kind, EventKind::Create(_) | EventKind::Remove(_))
        || matches!(kind, EventKind::Modify(m) if !matches!(m, ModifyKind::Metadata(_)));
    if !changed {
        return Verdict::Ignore;
    }
    if is_manifest(source, path) {
        return Verdict::Manifest;
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
    let yes = |hit: bool, then: Verdict| if hit { then } else { Verdict::Ignore };
    match source {
        Source::SteamApps | Source::Epic => Verdict::Ignore,
        Source::RiotRoot => yes(name == "riotclientinstalls.json", Verdict::Rescan),
        Source::RiotMetadata => yes(name.ends_with(".product_settings.yaml"), Verdict::Rescan),
        // Dans une bibliothèque, seul compte un sous-dossier qui apparaît, disparaît ou change de nom.
        Source::Library => match kind {
            EventKind::Create(_) => yes(is_dir(), Verdict::Rescan),
            EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_)) => Verdict::Rescan,
            _ => Verdict::Ignore,
        },
    }
}

/// Manifeste d'un jeu : `appmanifest_<appid>.acf` de Steam ou `.item` d'Epic.
fn is_manifest(source: Source, path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
    match source {
        Source::SteamApps => name.starts_with("appmanifest_") && name.ends_with(".acf"),
        Source::Epic => name.ends_with(".item"),
        _ => false,
    }
}

/// État d'un manifeste : installé ou non. Fichier absent = non installé, `None` = illisible.
fn manifest_state(path: &Path) -> Option<bool> {
    if !path.exists() {
        return Some(false);
    }
    let text = std::fs::read_to_string(path).ok()?;
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("item")) {
        library::item_installed(&text)
    } else {
        library::manifest_installed(&text)
    }
}

/// État connu des manifestes, pour ne rescanner que quand un jeu passe d'installé à absent ou
/// l'inverse (un téléchargement ou une mise à jour réécrit le manifeste sans rien changer).
#[derive(Default)]
struct Manifests {
    known: HashMap<PathBuf, bool>,
}

impl Manifests {
    /// Retient l'état des manifestes d'un dossier qui vient d'être surveillé.
    fn remember(&mut self, watched: &Watched) {
        if !matches!(watched.source, Source::SteamApps | Source::Epic) {
            return;
        }
        let Ok(entries) = std::fs::read_dir(&watched.dir) else { return };
        for path in entries.flatten().map(|e| e.path()) {
            if is_manifest(watched.source, &path)
                && let Some(state) = manifest_state(&path)
            {
                self.known.entry(path).or_insert(state);
            }
        }
    }

    /// Relit un manifeste : `Some(true)` si son état a changé, `None` s'il est illisible.
    fn update(&mut self, path: &Path, state: Option<bool>) -> Option<bool> {
        let state = state?;
        let before = self.known.insert(path.to_path_buf(), state).unwrap_or(false);
        Some(before != state)
    }
}

/// Changements en attente du temps de calme.
struct Pending {
    quiet: Duration,
    /// Dernier changement reçu.
    last: Option<Instant>,
    /// Rescan certain (dossier de jeu, Riot, scan précédent refusé).
    force: bool,
    /// Manifestes à relire à l'échéance.
    manifests: HashSet<PathBuf>,
}

impl Pending {
    fn new(quiet: Duration) -> Self {
        Pending { quiet, last: None, force: false, manifests: HashSet::new() }
    }

    /// Un changement repousse l'échéance : on attend que tout se calme.
    fn signal(&mut self, now: Instant, verdict: Verdict, path: &Path) {
        match verdict {
            Verdict::Ignore => return,
            Verdict::Rescan => self.force = true,
            Verdict::Manifest => {
                self.manifests.insert(path.to_path_buf());
            }
        }
        self.last = Some(now);
    }

    /// Temps à attendre avant l'échéance, `None` si rien n'est en attente.
    fn wait(&self, now: Instant) -> Option<Duration> {
        self.last.map(|last| self.quiet.saturating_sub(now.saturating_duration_since(last)))
    }

    /// À l'échéance : rescan certain et manifestes à relire, puis plus rien en attente.
    fn take_due(&mut self, now: Instant) -> Option<(bool, HashSet<PathBuf>)> {
        if self.wait(now)? > Duration::ZERO {
            return None;
        }
        self.last = None;
        Some((std::mem::take(&mut self.force), std::mem::take(&mut self.manifests)))
    }
}

/// Débit d'un téléchargement, tiré des octets lus dans les dernières relectures du manifeste.
#[derive(Default)]
struct Speed {
    samples: VecDeque<(Instant, u64)>,
    /// Octets à télécharger annoncés : un autre chiffre = autre téléchargement, on repart de zéro.
    total: Option<u64>,
}

impl Speed {
    /// Ajoute une relecture : débit (octets par seconde) et temps restant (secondes), si connus.
    fn record(&mut self, now: Instant, m: &ManifestState) -> (Option<u64>, Option<u64>) {
        let downloading = matches!(m.install_state, InstallState::Updating { .. });
        let restarted = self.samples.back().is_some_and(|&(_, bytes)| bytes > m.downloaded);
        if !downloading || restarted || m.to_download != self.total {
            self.samples.clear();
            self.total = m.to_download;
        }
        if !downloading {
            return (None, None);
        }
        self.samples.push_back((now, m.downloaded));
        // Les relectures trop anciennes sortent, en gardant toujours les deux dernières.
        while self.samples.len() > 2 && self.samples.front().is_some_and(|&(t, _)| now.saturating_duration_since(t) > SPEED_WINDOW) {
            self.samples.pop_front();
        }
        let (Some(&(t0, b0)), Some(&(t1, b1))) = (self.samples.front(), self.samples.back()) else { return (None, None) };
        let span = t1.saturating_duration_since(t0);
        if span < SPEED_MIN_SPAN || b1 <= b0 {
            return (None, None);
        }
        let speed = ((b1 - b0) as f64 / span.as_secs_f64()) as u64;
        if speed == 0 {
            return (None, None);
        }
        let remaining = self.total.map(|total| total.saturating_sub(b1).div_ceil(speed));
        (Some(speed), remaining)
    }
}

/// Relectures immédiates des manifestes Steam : une par seconde et par manifeste au plus. Une
/// écriture qui arrive trop tôt est relue à la fin de l'écart, pour ne jamais manquer la dernière.
struct Live {
    gap: Duration,
    entries: HashMap<PathBuf, LiveEntry>,
}

#[derive(Default)]
struct LiveEntry {
    last: Option<Instant>,
    /// Une écriture attend sa relecture.
    due: bool,
    speed: Speed,
}

impl Live {
    fn new(gap: Duration) -> Self {
        Live { gap, entries: HashMap::new() }
    }

    /// Écriture vue : `true` s'il faut relire maintenant, sinon la relecture est programmée.
    fn signal(&mut self, now: Instant, path: &Path) -> bool {
        let entry = self.entries.entry(path.to_path_buf()).or_default();
        if entry.last.is_some_and(|last| now.saturating_duration_since(last) < self.gap) {
            entry.due = true;
            return false;
        }
        entry.last = Some(now);
        entry.due = false;
        true
    }

    /// Temps avant la prochaine relecture programmée, `None` s'il n'y en a pas.
    fn wait(&self, now: Instant) -> Option<Duration> {
        self.entries
            .values()
            .filter(|e| e.due)
            .filter_map(|e| e.last)
            .map(|last| (last + self.gap).saturating_duration_since(now))
            .min()
    }

    /// Manifestes à relire maintenant (écart écoulé).
    fn take_due(&mut self, now: Instant) -> Vec<PathBuf> {
        let gap = self.gap;
        let mut due = Vec::new();
        for (path, entry) in &mut self.entries {
            if entry.due && entry.last.is_none_or(|last| now.saturating_duration_since(last) >= gap) {
                entry.due = false;
                entry.last = Some(now);
                due.push(path.clone());
            }
        }
        due
    }

    fn speed(&mut self, path: &Path) -> &mut Speed {
        &mut self.entries.entry(path.to_path_buf()).or_default().speed
    }

    fn forget(&mut self, path: &Path) {
        self.entries.remove(path);
    }
}

/// Progression d'un jeu Steam relue dans son manifeste, sans rescan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Progress {
    pub appid: String,
    pub state: InstallState,
    pub size_on_disk: Option<u64>,
    /// Débit estimé, en octets par seconde.
    pub speed: Option<u64>,
    /// Temps restant estimé, en secondes.
    pub remaining: Option<u64>,
}

/// Progression à envoyer pour un manifeste relu. Rien si le jeu n'est pas listé ou si son état
/// installé change par rapport à ce que la surveillance connaît : c'est alors le rescan qui
/// s'en charge, après le temps de calme.
fn live_progress(m: &ManifestState, known: Option<bool>, speed: &mut Speed, now: Instant) -> Option<Progress> {
    if !m.installed || known != Some(true) {
        return None;
    }
    let (speed, remaining) = speed.record(now, m);
    Some(Progress { appid: m.appid.clone(), state: m.install_state, size_on_disk: m.size_on_disk, speed, remaining })
}

/// Relit un manifeste Steam tout de suite et passe sa progression à `progress`.
fn live_read(path: &Path, manifests: &Manifests, live: &mut Live, now: Instant, progress: &mut impl FnMut(Progress)) {
    let Ok(text) = std::fs::read_to_string(path) else {
        if !path.exists() {
            live.forget(path);
        }
        return;
    };
    // En cours d'écriture : l'écriture suivante sera relue.
    let Some(m) = library::read_steam_manifest(&text) else { return };
    if let Some(p) = live_progress(&m, manifests.known.get(path).copied(), live.speed(path), now) {
        progress(p);
    }
}

/// Événement `game-install` : état d'installation d'un jeu changé pendant un téléchargement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallEvent {
    pub id: String,
    pub install_state: InstallState,
    pub size_on_disk: Option<u64>,
    pub speed: Option<u64>,
    pub remaining: Option<u64>,
}

/// Pose une progression sur le jeu de l'état. Un événement seulement si l'état, le % ou la
/// taille change (le débit seul ne suffit pas).
pub fn apply(games: &mut [Game], p: &Progress) -> Option<InstallEvent> {
    let game = games.iter_mut().find(|g| g.platform == Platform::Steam && g.platform_id == p.appid)?;
    let size = p.size_on_disk.filter(|_| !game.size_measured).or(game.size_on_disk);
    if game.install_state == p.state && game.size_on_disk == size {
        return None;
    }
    game.install_state = p.state;
    game.size_on_disk = size;
    Some(InstallEvent { id: game.id.clone(), install_state: p.state, size_on_disk: size, speed: p.speed, remaining: p.remaining })
}

enum Msg {
    Fs(notify::Result<notify::Event>),
    /// La liste des dossiers a pu changer (nouvelle bibliothèque Steam, premier jeu Epic…).
    Rewatch,
}

/// Fil de surveillance. `rescan` renvoie `false` quand un scan tourne déjà : il est retenté plus tard.
pub struct Watch {
    tx: Sender<Msg>,
}

impl Watch {
    /// `progress` reçoit chaque relecture d'un manifeste Steam pendant un téléchargement.
    pub fn spawn(rescan: impl FnMut() -> bool + Send + 'static, progress: impl FnMut(Progress) + Send + 'static) -> Watch {
        Watch::spawn_with(targets, QUIET, LIVE_GAP, rescan, progress)
    }

    fn spawn_with(
        targets: impl Fn() -> Vec<Watched> + Send + 'static,
        quiet: Duration,
        gap: Duration,
        rescan: impl FnMut() -> bool + Send + 'static,
        progress: impl FnMut(Progress) + Send + 'static,
    ) -> Watch {
        let (tx, rx) = mpsc::channel();
        let events = tx.clone();
        let spawned = std::thread::Builder::new()
            .name("ludex-surveillance".into())
            .spawn(move || run(rx, events, targets, quiet, gap, rescan, progress));
        if let Err(e) = spawned {
            eprintln!("Surveillance des dossiers de jeux impossible : {e}");
        }
        Watch { tx }
    }

    /// Relit la liste des dossiers à surveiller (après un scan).
    pub fn rewatch(&self) {
        let _ = self.tx.send(Msg::Rewatch);
    }
}

fn run(
    rx: Receiver<Msg>,
    events: Sender<Msg>,
    targets: impl Fn() -> Vec<Watched>,
    quiet: Duration,
    gap: Duration,
    mut rescan: impl FnMut() -> bool,
    mut progress: impl FnMut(Progress),
) {
    let watcher = notify::recommended_watcher(move |res| {
        let _ = events.send(Msg::Fs(res));
    });
    let mut watcher = match watcher {
        Ok(w) => w,
        Err(e) => return eprintln!("Surveillance des dossiers de jeux impossible : {e}"),
    };
    let mut watched = Vec::new();
    let mut manifests = Manifests::default();
    rewatch(&mut watcher, &mut watched, &mut manifests, targets());
    let mut pending = Pending::new(quiet);
    let mut live = Live::new(gap);

    loop {
        // Rien en attente : le fil dort jusqu'au prochain événement.
        let now = Instant::now();
        let wait = match (pending.wait(now), live.wait(now)) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let msg = match wait {
            None => match rx.recv() {
                Ok(msg) => Some(msg),
                // Canal fermé : l'application s'arrête.
                Err(_) => return,
            },
            Some(wait) => match rx.recv_timeout(wait) {
                Ok(msg) => Some(msg),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            },
        };
        let now = Instant::now();
        match msg {
            Some(Msg::Fs(Ok(event))) => {
                // Événements perdus (tampon du système plein) : on ne sait plus rien, on rescanne.
                if event.need_rescan() {
                    pending.signal(now, Verdict::Rescan, Path::new(""));
                }
                for path in &event.paths {
                    for w in watched.iter().filter(|w| w.holds(path)) {
                        let verdict = verdict(w.source, &event.kind, path, || path.is_dir());
                        // Manifeste Steam : relu tout de suite pour la progression, en plus du calme.
                        if verdict == Verdict::Manifest && w.source == Source::SteamApps && live.signal(now, path) {
                            live_read(path, &manifests, &mut live, now, &mut progress);
                        }
                        pending.signal(now, verdict, path);
                    }
                }
            }
            Some(Msg::Fs(Err(e))) => eprintln!("Surveillance des dossiers de jeux : {e}"),
            Some(Msg::Rewatch) => rewatch(&mut watcher, &mut watched, &mut manifests, targets()),
            None => {}
        }
        for path in live.take_due(now) {
            live_read(&path, &manifests, &mut live, now, &mut progress);
        }
        let Some((force, paths)) = pending.take_due(now) else { continue };
        let mut changed = force;
        for path in paths {
            match manifests.update(&path, manifest_state(&path)) {
                Some(c) => changed |= c,
                // En cours d'écriture : relu au prochain calme, s'il vient d'être écrit
                // (un manifeste abîmé depuis longtemps n'est pas relu sans fin).
                None if recently_modified(&path) => pending.signal(now, Verdict::Manifest, &path),
                None => {}
            }
        }
        if changed && !rescan() {
            // Un scan tourne déjà : on retente après un nouveau temps de calme.
            pending.signal(now, Verdict::Rescan, Path::new(""));
        }
    }
}

fn recently_modified(path: &Path) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .is_ok_and(|t| SystemTime::now().duration_since(t).is_ok_and(|age| age < Duration::from_secs(60)))
}

/// Met la surveillance à jour : cesse de suivre les dossiers disparus, suit les nouveaux.
fn rewatch(watcher: &mut impl Watcher, watched: &mut Vec<Watched>, manifests: &mut Manifests, wanted: Vec<Watched>) {
    for old in watched.iter().filter(|w| !wanted.contains(w)) {
        let _ = watcher.unwatch(&old.dir);
    }
    let mut now: Vec<Watched> = Vec::new();
    for w in wanted {
        if now.iter().any(|n| n.dir == w.dir) {
            continue;
        }
        if !watched.contains(&w) {
            if let Err(e) = watcher.watch(&w.dir, w.source.mode()) {
                eprintln!("Dossier non surveillé {} : {e}", w.dir.display());
                continue;
            }
            manifests.remember(&w);
        }
        now.push(w);
    }
    *watched = now;
}

/// Jeux apparus et disparus entre deux scans (titres), hors jeux masqués et outils.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Changes {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    /// Ids des jeux apparus (notification d'un jeu Steam installé).
    #[serde(skip)]
    pub added_ids: Vec<String>,
}

pub fn changes(before: &[Game], after: &[Game]) -> Changes {
    let ids = |games: &[Game]| games.iter().map(|g| g.id.clone()).collect::<HashSet<_>>();
    let (old, new) = (ids(before), ids(after));
    let titles = |games: &[Game], other: &HashSet<String>| {
        games.iter().filter(|g| !g.hidden && !other.contains(&g.id)).map(|g| g.title.clone()).collect()
    };
    let added_ids = after.iter().filter(|g| !g.hidden && !old.contains(&g.id)).map(|g| g.id.clone()).collect();
    Changes { added: titles(after, &old), removed: titles(before, &new), added_ids }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{Launch, Platform};
    use notify::event::{CreateKind, DataChange, MetadataKind, RemoveKind, RenameMode};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const CREATE: EventKind = EventKind::Create(CreateKind::Any);
    const WRITE: EventKind = EventKind::Modify(ModifyKind::Any);
    const REMOVE: EventKind = EventKind::Remove(RemoveKind::Any);

    #[test]
    fn filters_events() {
        let steam = Path::new(r"D:\SteamLibrary\steamapps\appmanifest_1145360.acf");
        assert_eq!(verdict(Source::SteamApps, &WRITE, steam, || false), Verdict::Manifest);
        assert_eq!(verdict(Source::SteamApps, &REMOVE, steam, || false), Verdict::Manifest);
        assert_eq!(verdict(Source::SteamApps, &EventKind::Access(notify::event::AccessKind::Any), steam, || false), Verdict::Ignore);
        let attrs = EventKind::Modify(ModifyKind::Metadata(MetadataKind::Any));
        assert_eq!(verdict(Source::SteamApps, &attrs, steam, || false), Verdict::Ignore);
        // Le dossier de téléchargement et libraryfolders.vdf bougent sans cesse : ignorés.
        assert_eq!(verdict(Source::SteamApps, &WRITE, Path::new(r"D:\s\steamapps\downloading"), || true), Verdict::Ignore);
        assert_eq!(verdict(Source::SteamApps, &WRITE, Path::new(r"D:\s\steamapps\libraryfolders.vdf"), || false), Verdict::Ignore);

        let item = Path::new(r"C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests\ABC.item");
        assert_eq!(verdict(Source::Epic, &CREATE, item, || false), Verdict::Manifest);
        assert_eq!(verdict(Source::Epic, &CREATE, &item.with_extension("tmp"), || false), Verdict::Ignore);

        let riot = Path::new(r"C:\ProgramData\Riot Games\RiotClientInstalls.json");
        assert_eq!(verdict(Source::RiotRoot, &EventKind::Modify(ModifyKind::Data(DataChange::Any)), riot, || false), Verdict::Rescan);
        let yaml = Path::new(r"C:\ProgramData\Riot Games\Metadata\valorant.live\valorant.live.product_settings.yaml");
        assert_eq!(verdict(Source::RiotMetadata, &REMOVE, yaml, || false), Verdict::Rescan);
        assert_eq!(verdict(Source::RiotMetadata, &CREATE, &yaml.with_file_name("x.log"), || false), Verdict::Ignore);

        // Bibliothèque : un dossier créé, supprimé ou renommé ; pas un fichier posé à la racine.
        let game = Path::new(r"E:\Jeux\Hades");
        assert_eq!(verdict(Source::Library, &CREATE, game, || true), Verdict::Rescan);
        assert_eq!(verdict(Source::Library, &CREATE, &game.with_file_name("notes.txt"), || false), Verdict::Ignore);
        assert_eq!(verdict(Source::Library, &REMOVE, game, || unreachable!()), Verdict::Rescan);
        let rename = EventKind::Modify(ModifyKind::Name(RenameMode::To));
        assert_eq!(verdict(Source::Library, &rename, game, || false), Verdict::Rescan);
        // Un jeu qui écrit dans son dossier change sa date : pas une installation.
        assert_eq!(verdict(Source::Library, &WRITE, game, || true), Verdict::Ignore);
    }

    #[test]
    fn matches_paths_to_watched_dirs() {
        let library = Watched { dir: PathBuf::from(r"E:\Jeux"), source: Source::Library };
        assert!(library.holds(Path::new(r"E:\Jeux\Hades")));
        assert!(!library.holds(Path::new(r"E:\Jeux\Hades\Hades.exe")));
        assert!(!library.holds(Path::new(r"E:\Jeux")));
        let riot = Watched { dir: PathBuf::from(r"C:\ProgramData\Riot Games\Metadata"), source: Source::RiotMetadata };
        assert!(riot.holds(Path::new(r"C:\ProgramData\Riot Games\Metadata\valorant.live\a.yaml")));
        assert!(!riot.holds(Path::new(r"C:\ProgramData\Riot Games\Metadata")));
    }

    #[test]
    fn waits_for_quiet_before_rescanning() {
        let quiet = Duration::from_secs(5);
        let t0 = Instant::now();
        let at = |secs: u64| t0 + Duration::from_secs(secs);
        let manifest = Path::new("appmanifest_10.acf");
        let mut pending = Pending::new(quiet);
        assert_eq!(pending.wait(t0), None); // rien en attente : le fil dort

        pending.signal(at(0), Verdict::Manifest, manifest);
        pending.signal(at(3), Verdict::Manifest, manifest);
        pending.signal(at(3), Verdict::Ignore, Path::new("downloading"));
        assert_eq!(pending.wait(at(4)), Some(Duration::from_secs(4)));
        assert!(pending.take_due(at(7)).is_none()); // 4 s de calme seulement
        let (force, paths) = pending.take_due(at(8)).unwrap();
        assert!(!force);
        assert_eq!(paths.len(), 1); // trois écritures, une seule relecture
        assert_eq!(pending.wait(at(9)), None);
        assert!(pending.take_due(at(20)).is_none());

        pending.signal(at(20), Verdict::Rescan, Path::new(r"E:\Jeux\Hades"));
        let (force, paths) = pending.take_due(at(25)).unwrap();
        assert!(force && paths.is_empty());
    }

    #[test]
    fn rescans_only_when_a_manifest_changes_state() {
        let mut manifests = Manifests::default();
        let path = Path::new("appmanifest_10.acf");
        // Téléchargement : le manifeste apparaît, non installé, puis est réécrit.
        assert_eq!(manifests.update(path, Some(false)), Some(false));
        assert_eq!(manifests.update(path, Some(false)), Some(false));
        assert_eq!(manifests.update(path, None), None); // en cours d'écriture
        // Installation terminée, puis mise à jour (toujours installé), puis désinstallation.
        assert_eq!(manifests.update(path, Some(true)), Some(true));
        assert_eq!(manifests.update(path, Some(true)), Some(false));
        assert_eq!(manifests.update(path, Some(false)), Some(true));
    }

    #[test]
    fn rereads_a_manifest_at_most_once_per_second() {
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let (a, b) = (Path::new("appmanifest_10.acf"), Path::new("appmanifest_20.acf"));
        let mut live = Live::new(LIVE_GAP);
        assert_eq!(live.wait(t0), None);
        // Première écriture : relue tout de suite. Les suivantes dans la seconde : une relecture programmée.
        assert!(live.signal(at(0), a));
        assert!(!live.signal(at(200), a));
        assert!(!live.signal(at(600), a));
        // Un autre jeu a son propre écart.
        assert!(live.signal(at(300), b));
        assert_eq!(live.wait(at(600)), Some(Duration::from_millis(400)));
        assert!(live.take_due(at(900)).is_empty());
        assert_eq!(live.take_due(at(1000)), vec![a.to_path_buf()]);
        assert_eq!(live.wait(at(1000)), None);
        // La relecture programmée compte comme la dernière : écriture suivante attendue jusqu'à 2 s.
        assert!(!live.signal(at(1500), a));
        assert_eq!(live.take_due(at(2000)), vec![a.to_path_buf()]);
        assert!(live.signal(at(3500), a));
    }

    fn manifest_at(state: InstallState, downloaded: u64, to_download: Option<u64>) -> ManifestState {
        ManifestState { appid: "730".into(), installed: true, install_state: state, size_on_disk: Some(1000), downloaded, to_download }
    }

    #[test]
    fn estimates_speed_and_remaining_time() {
        let t0 = Instant::now();
        let at = |secs: u64| t0 + Duration::from_secs(secs);
        let updating = InstallState::Updating { progress: Some(10) };
        let mut speed = Speed::default();
        // Une seule relecture, puis moins de 2 s : pas encore de débit.
        assert_eq!(speed.record(at(0), &manifest_at(updating, 0, Some(1_000_000))), (None, None));
        assert_eq!(speed.record(at(1), &manifest_at(updating, 50_000, Some(1_000_000))), (None, None));
        // 200 000 octets en 4 s : 50 000 o/s, 800 000 restants = 16 s.
        assert_eq!(speed.record(at(4), &manifest_at(updating, 200_000, Some(1_000_000))), (Some(50_000), Some(16)));
        // Seules les relectures des 20 dernières secondes comptent : 220 000 octets en 11 s.
        for (secs, bytes) in [(30, 300_000), (40, 500_000), (50, 700_000)] {
            speed.record(at(secs), &manifest_at(updating, bytes, Some(1_000_000)));
        }
        assert_eq!(speed.record(at(51), &manifest_at(updating, 720_000, Some(1_000_000))), (Some(20_000), Some(14)));
        // Téléchargement relancé (octets en baisse) ou fini : on repart de zéro.
        assert_eq!(speed.record(at(52), &manifest_at(updating, 10, Some(1_000_000))), (None, None));
        assert_eq!(speed.record(at(60), &manifest_at(InstallState::Ready, 0, None)), (None, None));
        assert!(speed.samples.is_empty());
        // Rien ne bouge (téléchargement bloqué) : débit inconnu plutôt que nul.
        assert_eq!(speed.record(at(70), &manifest_at(updating, 5, Some(10))), (None, None));
        assert_eq!(speed.record(at(80), &manifest_at(updating, 5, Some(10))), (None, None));
    }

    #[test]
    fn sends_progress_only_for_listed_games_that_stay_listed() {
        let now = Instant::now();
        let updating = manifest_at(InstallState::Updating { progress: Some(42) }, 42, Some(100));
        let p = live_progress(&updating, Some(true), &mut Speed::default(), now).unwrap();
        assert_eq!((p.appid.as_str(), p.state, p.size_on_disk), ("730", InstallState::Updating { progress: Some(42) }, Some(1000)));
        // Jeu qui vient d'être installé (connu absent) : c'est le rescan qui l'ajoute.
        assert_eq!(live_progress(&updating, Some(false), &mut Speed::default(), now), None);
        assert_eq!(live_progress(&updating, None, &mut Speed::default(), now), None);
        // Première installation (pas listée) ou désinstallation : rescan.
        let first = ManifestState { installed: false, ..updating };
        assert_eq!(live_progress(&first, Some(true), &mut Speed::default(), now), None);
    }

    #[test]
    fn applies_progress_and_reports_changes_only() {
        let mut games = vec![game("730", "Counter-Strike 2", false), game("10", "Portal", false)];
        games[0].install_state = InstallState::UpdateRequired;
        games[0].size_on_disk = Some(1000);
        let progress = |state: InstallState, size: Option<u64>| Progress { appid: "730".into(), state, size_on_disk: size, speed: Some(5), remaining: Some(9) };

        let event = apply(&mut games, &progress(InstallState::Queued, Some(1000))).unwrap();
        assert_eq!(event.id, "steam:730");
        assert_eq!(serde_json::to_value(&event).unwrap(), serde_json::json!({
            "id": "steam:730", "installState": { "kind": "queued" }, "sizeOnDisk": 1000, "speed": 5, "remaining": 9
        }));
        // Même état, même taille : rien (le débit seul ne compte pas).
        assert_eq!(apply(&mut games, &Progress { speed: Some(99), ..progress(InstallState::Queued, Some(1000)) }), None);
        let downloading = InstallState::Updating { progress: Some(10) };
        assert!(apply(&mut games, &progress(downloading, Some(1000))).is_some());
        assert_eq!(apply(&mut games, &progress(downloading, None)), None); // taille absente : gardée
        assert!(apply(&mut games, &progress(InstallState::Updating { progress: Some(11) }, Some(1000))).is_some());
        // Fin de la mise à jour : prêt, nouvelle taille.
        let done = apply(&mut games, &progress(InstallState::Ready, Some(1500))).unwrap();
        assert_eq!((done.install_state, done.size_on_disk), (InstallState::Ready, Some(1500)));
        assert_eq!((games[0].install_state, games[0].size_on_disk), (InstallState::Ready, Some(1500)));
        // Jeu absent de l'état, ou jeu d'une autre plateforme avec le même numéro : rien.
        assert_eq!(apply(&mut games, &Progress { appid: "99".into(), ..progress(InstallState::Queued, None) }), None);
        let mut other = vec![Game::new(Platform::Gog, "730", "Autre", Launch::Uri(String::new()))];
        assert_eq!(apply(&mut other, &progress(InstallState::Queued, None)), None);
    }

    fn game(id: &str, title: &str, hidden: bool) -> Game {
        let mut g = Game::new(Platform::Steam, id, title, Launch::Uri(String::new()));
        g.hidden = hidden;
        g
    }

    #[test]
    fn computes_added_and_removed_games() {
        let before = [game("10", "Portal", false), game("20", "Hades", false), game("30", "Wallpaper Engine", true)];
        let after = [game("10", "Portal", false), game("40", "Celeste", false), game("50", "Dedicated Server", true)];
        let c = changes(&before, &after);
        assert_eq!(c.added, vec!["Celeste".to_string()]);
        assert_eq!(c.removed, vec!["Hades".to_string()]);
        assert_eq!(changes(&after, &after), Changes::default());
    }

    /// Dossiers surveillés sur ce PC : `cargo test watched_this_pc -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn watched_this_pc() {
        for w in targets() {
            println!("{:?}  {}", w.source, w.dir.display());
        }
    }

    /// Surveillance réelle d'un faux `steamapps` :
    /// `cargo test watches_a_steam_library -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn watches_a_steam_library() {
        let dir = std::env::temp_dir().join(format!("ludex-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // Manifeste réduit : buildid connu (jeu déjà installé une fois) et octets du téléchargement.
        let manifest = |appid: u32, flags: u32, downloaded: u64| {
            let buildid = if appid == 10 { 42 } else { 0 };
            let text = format!(
                "\"AppState\"\n{{\n\t\"appid\"\t\t\"{appid}\"\n\t\"name\"\t\t\"Jeu {appid}\"\n\t\"StateFlags\"\t\t\"{flags}\"\n\
                 \t\"buildid\"\t\t\"{buildid}\"\n\t\"SizeOnDisk\"\t\t\"5000\"\n\t\"BytesToDownload\"\t\t\"1000\"\n\
                 \t\"BytesDownloaded\"\t\t\"{downloaded}\"\n}}\n"
            );
            std::fs::write(dir.join(format!("appmanifest_{appid}.acf")), text).unwrap();
        };
        // Un jeu déjà installé avant la surveillance : ne déclenche rien.
        manifest(10, 4, 0);

        let rescans = Arc::new(AtomicUsize::new(0));
        let counter = rescans.clone();
        let seen: Arc<std::sync::Mutex<Vec<(Instant, Progress)>>> = Arc::default();
        let sink = seen.clone();
        let target = dir.clone();
        let quiet = Duration::from_secs(2);
        let _watch = Watch::spawn_with(
            move || vec![Watched { dir: target.clone(), source: Source::SteamApps }],
            quiet,
            LIVE_GAP,
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
                println!("rescan demandé");
                true
            },
            move |p| {
                println!("progression : {:?}", p);
                sink.lock().unwrap().push((Instant::now(), p));
            },
        );
        std::thread::sleep(Duration::from_millis(500));

        // Première installation (jamais listée) et mise à jour d'un jeu installé, écrites toutes les
        // 300 ms pendant 3 s.
        let downloading = 2 | 4 | 1024 | 256 | 1_048_576;
        for step in 1..=10 {
            manifest(20, 1026, step * 100);
            manifest(10, downloading, step * 100);
            std::thread::sleep(Duration::from_millis(300));
        }
        std::thread::sleep(quiet + Duration::from_secs(2));
        assert_eq!(rescans.load(Ordering::SeqCst), 0, "un téléchargement ne doit pas rescanner");
        {
            let seen = seen.lock().unwrap();
            assert!(seen.iter().all(|(_, p)| p.appid == "10"), "une première installation n'envoie rien");
            // Une relecture par seconde au plus, et la dernière écriture toujours relue.
            assert!((3..=5).contains(&seen.len()), "{} relectures", seen.len());
            assert!(seen.windows(2).all(|w| w[1].0 - w[0].0 >= LIVE_GAP - Duration::from_millis(50)));
            assert_eq!(seen.last().unwrap().1.state, InstallState::Updating { progress: Some(100) });
            assert!(seen.iter().any(|(_, p)| p.speed.is_some()), "débit estimé");
        }

        // Fin de la mise à jour : « prêt » envoyé sans rescan.
        manifest(10, 4, 0);
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(seen.lock().unwrap().last().unwrap().1.state, InstallState::Ready);
        std::thread::sleep(quiet + Duration::from_secs(2));
        assert_eq!(rescans.load(Ordering::SeqCst), 0);

        // Fin de l'installation : plusieurs écritures rapprochées, un seul rescan après le calme.
        let start = Instant::now();
        for _ in 0..6 {
            manifest(20, 4, 0);
            std::thread::sleep(Duration::from_millis(300));
        }
        assert_eq!(rescans.load(Ordering::SeqCst), 0, "pas de rescan pendant les écritures");
        std::thread::sleep(quiet + Duration::from_secs(2));
        assert_eq!(rescans.load(Ordering::SeqCst), 1);
        println!("rescan après {:?}", start.elapsed());

        // Désinstallation : le manifeste disparaît.
        std::fs::remove_file(dir.join("appmanifest_20.acf")).unwrap();
        std::thread::sleep(quiet + Duration::from_secs(2));
        assert_eq!(rescans.load(Ordering::SeqCst), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
