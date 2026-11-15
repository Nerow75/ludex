//! Surveillance des dossiers où les launchers déclarent leurs jeux : un jeu installé ou
//! désinstallé déclenche un rescan tout seul, une fois que les écritures se sont calmées.
//!
//! Aucun relevé périodique : le fil dort sur le canal des événements du système (crate `notify`)
//! tant que rien ne bouge. Une erreur de surveillance n'est qu'un message dans la console.

use crate::library::{self, Game};
use notify::event::{EventKind, ModifyKind};
use notify::{RecursiveMode, Watcher};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant, SystemTime};

/// Temps de calme attendu avant de rescanner : Steam écrit un manifeste plusieurs fois de suite.
pub const QUIET: Duration = Duration::from_secs(5);

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
    pub fn spawn(rescan: impl FnMut() -> bool + Send + 'static) -> Watch {
        Watch::spawn_with(targets, QUIET, rescan)
    }

    fn spawn_with(
        targets: impl Fn() -> Vec<Watched> + Send + 'static,
        quiet: Duration,
        rescan: impl FnMut() -> bool + Send + 'static,
    ) -> Watch {
        let (tx, rx) = mpsc::channel();
        let events = tx.clone();
        let spawned = std::thread::Builder::new()
            .name("ludex-surveillance".into())
            .spawn(move || run(rx, events, targets, quiet, rescan));
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
    mut rescan: impl FnMut() -> bool,
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

    loop {
        // Rien en attente : le fil dort jusqu'au prochain événement.
        let msg = match pending.wait(Instant::now()) {
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
                        pending.signal(now, verdict(w.source, &event.kind, path, || path.is_dir()), path);
                    }
                }
            }
            Some(Msg::Fs(Err(e))) => eprintln!("Surveillance des dossiers de jeux : {e}"),
            Some(Msg::Rewatch) => rewatch(&mut watcher, &mut watched, &mut manifests, targets()),
            None => {
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
}

pub fn changes(before: &[Game], after: &[Game]) -> Changes {
    let ids = |games: &[Game]| games.iter().map(|g| g.id.clone()).collect::<HashSet<_>>();
    let (old, new) = (ids(before), ids(after));
    let titles = |games: &[Game], other: &HashSet<String>| {
        games.iter().filter(|g| !g.hidden && !other.contains(&g.id)).map(|g| g.title.clone()).collect()
    };
    Changes { added: titles(after, &old), removed: titles(before, &new) }
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
        let manifest = |appid: u32, flags: u32| {
            let text = format!("\"AppState\"\n{{\n\t\"appid\"\t\t\"{appid}\"\n\t\"name\"\t\t\"Jeu {appid}\"\n\t\"StateFlags\"\t\t\"{flags}\"\n}}\n");
            std::fs::write(dir.join(format!("appmanifest_{appid}.acf")), text).unwrap();
        };
        // Un jeu déjà installé avant la surveillance : ne déclenche rien.
        manifest(10, 4);

        let rescans = Arc::new(AtomicUsize::new(0));
        let counter = rescans.clone();
        let target = dir.clone();
        let quiet = Duration::from_secs(2);
        let _watch = Watch::spawn_with(
            move || vec![Watched { dir: target.clone(), source: Source::SteamApps }],
            quiet,
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
                println!("rescan demandé");
                true
            },
        );
        std::thread::sleep(Duration::from_millis(500));

        // Téléchargement : manifeste réécrit sans être installé, et mise à jour d'un jeu installé.
        for _ in 0..5 {
            manifest(20, 1026);
            manifest(10, 6);
            std::thread::sleep(Duration::from_millis(300));
        }
        std::thread::sleep(quiet + Duration::from_secs(1));
        assert_eq!(rescans.load(Ordering::SeqCst), 0, "un téléchargement ne doit pas rescanner");

        // Fin de l'installation : plusieurs écritures rapprochées, un seul rescan après le calme.
        let start = Instant::now();
        for _ in 0..6 {
            manifest(20, 4);
            std::thread::sleep(Duration::from_millis(300));
        }
        assert_eq!(rescans.load(Ordering::SeqCst), 0, "pas de rescan pendant les écritures");
        std::thread::sleep(quiet + Duration::from_secs(1));
        assert_eq!(rescans.load(Ordering::SeqCst), 1);
        println!("rescan après {:?}", start.elapsed());

        // Désinstallation : le manifeste disparaît.
        std::fs::remove_file(dir.join("appmanifest_20.acf")).unwrap();
        std::thread::sleep(quiet + Duration::from_secs(1));
        assert_eq!(rescans.load(Ordering::SeqCst), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
