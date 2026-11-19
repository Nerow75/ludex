//! Mesure du temps de jeu par Ludex, et état « En jeu ».
//!
//! Après un lancement depuis Ludex, un fil de surveillance relève toutes les 5 secondes les
//! processus dont l'exécutable est rangé sous le dossier du jeu. Une session dure tant qu'au
//! moins un de ces processus tourne. Le temps n'est cumulé (dans `user-data.json`) que pour les
//! jeux dont le launcher ne compte rien lui-même : Steam et GOG Galaxy gardent leur chiffre.
//!
//! Chaque session terminée (une minute au moins) va dans l'historique (`history.rs`), Steam et
//! Galaxy compris, marquée `launcher` pour ces deux-là. Les sessions en cours sont enregistrées à
//! chaque relevé : au redémarrage de Ludex, une session dont le jeu tourne encore (même processus,
//! démarré avant le dernier relevé) reprend ; sinon elle est close avec ce qui a été mesuré.
//!
//! Limites assumées : un jeu lancé hors de Ludex n'est pas mesuré ; le temps passé pendant que
//! Ludex était fermé ne compte pas (écart de plus de `MAX_STEP` entre deux relevés).

use crate::history::{Journal, OpenSession, PastSession, SharedHistory};
use crate::library::{Game, Platform};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

/// Intervalle entre deux relevés des processus.
const POLL: Duration = Duration::from_secs(5);
/// Écart maximal entre deux relevés compté comme du jeu : au-delà, le PC était en veille.
const MAX_STEP: Duration = Duration::from_secs(20);
/// Délai pour voir apparaître le jeu après le lancement : démarrage du launcher, mise à jour, connexion.
const DETECT_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// Absence tolérée avant de clore une session tant que seul un lanceur intermédiaire a tourné
/// (PlayGTAV.exe passe la main au Rockstar Launcher, qui démarre ensuite le jeu).
const GRACE_EARLY: Duration = Duration::from_secs(5 * 60);
/// Absence tolérée une fois le jeu installé : redémarrage du jeu après un réglage, changement d'exécutable.
const GRACE: Duration = Duration::from_secs(30);
/// Les sessions plus courtes ne comptent pas : lanceur intermédiaire, plantage au démarrage.
const MIN_SESSION: Duration = Duration::from_secs(60);
/// Plafond d'une session : une mesure aberrante ne peut pas fausser le cumul.
const MAX_SESSION: Duration = Duration::from_secs(24 * 3600);
/// Enregistrement pendant la partie : fermer Ludex fait perdre au plus cette durée.
const FLUSH_EVERY: u64 = 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    /// Lancé, le jeu n'est pas (ou plus) visible : launcher en cours de démarrage, lanceur intermédiaire.
    Detecting,
    /// Un processus du jeu tourne.
    Running,
    /// Session terminée, ou jeu jamais apparu.
    Ended,
}

/// Résultat d'un relevé pour une session.
#[derive(Debug, PartialEq, Eq)]
pub struct Tick {
    pub phase: Phase,
    /// Secondes de jeu à ajouter au cumul maintenant (0 si rien à enregistrer).
    pub credit_secs: u64,
}

/// Événement transmis à l'application.
#[derive(Debug)]
pub struct Update {
    pub game_id: String,
    pub phase: Phase,
    pub credit_secs: u64,
}

/// Heure actuelle en secondes Unix.
fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Processus en cours : exécutable (chemin normalisé) et démarrage (secondes Unix, 0 si inconnu).
#[derive(Clone, Debug)]
pub struct Proc {
    pub exe: String,
    pub start: u64,
}

/// Chemin comparable : séparateurs Windows, minuscules, sans préfixe `\\?\`.
fn normalize(path: &str) -> String {
    let path = path.replace('/', "\\").to_lowercase();
    path.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(path)
}

/// Dossier racine terminé par `\`. Refuse une racine trop large (lecteur entier, `Program Files`…)
/// qui ferait passer n'importe quel programme pour le jeu.
fn root_key(dir: &str) -> Option<String> {
    let dir = normalize(dir);
    let dir = dir.trim_end_matches('\\');
    let parts: Vec<&str> = dir.split('\\').filter(|p| !p.is_empty()).collect();
    const TOO_WIDE: &[&str] = &["windows", "program files", "program files (x86)", "programdata", "users", "windowsapps"];
    match parts.as_slice() {
        [] | [_] => None,
        [_, folder] if TOO_WIDE.contains(folder) => None,
        _ => Some(format!("{dir}\\")),
    }
}

/// Dossiers où tournent les processus d'un jeu, reconstruits depuis l'état : dossier
/// d'installation, plus pour Xbox le dossier de données du paquet (`%LOCALAPPDATA%\Packages\<famille>`),
/// d'où certains paquets démarrent leurs exécutables (Minecraft Launcher et son Java).
pub fn roots_for(game: &Game) -> Vec<String> {
    let mut roots: Vec<String> = game.install_dir.as_deref().and_then(root_key).into_iter().collect();
    if game.platform == Platform::Xbox
        && let Some(local) = std::env::var_os("LOCALAPPDATA")
    {
        let packages = std::path::Path::new(&local).join("Packages").join(&game.platform_id);
        roots.extend(root_key(&packages.to_string_lossy()));
    }
    roots
}

pub struct Session {
    pub game_id: String,
    roots: Vec<String>,
    /// Cumuler le temps ? Faux quand le launcher le compte déjà (Steam, Galaxy).
    record: bool,
    started: Instant,
    /// Correspondance entre l'horloge monotone et l'heure Unix, pour l'historique.
    epoch: (Instant, u64),
    last_poll: Option<(Instant, bool)>,
    last_seen: Option<Instant>,
    /// Premier relevé où le jeu tournait (secondes Unix) : début de la session dans l'historique.
    first_seen: Option<u64>,
    /// Session reprise : au premier relevé, seuls comptent les processus démarrés avant cette heure
    /// (secondes Unix), pour ne pas confondre la partie d'avant avec un jeu relancé hors de Ludex.
    resume_before: Option<u64>,
    played: Duration,
    /// Secondes déjà ajoutées au cumul.
    credited: u64,
    /// Dernière phase transmise, pour ne signaler que les changements.
    reported: Option<Phase>,
}

/// Clôt une session qui ne peut pas reprendre (PC redémarré depuis) : ce qui a été mesuré
/// jusqu'au dernier relevé va dans l'historique, et dans le cumul ce qui n'y était pas encore.
pub fn close_stale(open: &OpenSession) -> (Option<PastSession>, u64) {
    let played = open.played.min(MAX_SESSION.as_secs());
    if played < MIN_SESSION.as_secs() {
        return (None, 0);
    }
    let past = PastSession { game_id: open.game_id.clone(), start: open.start, secs: played, launcher: !open.record };
    let credit = if open.record { played.saturating_sub(open.credited) } else { 0 };
    (Some(past), credit)
}

impl Session {
    pub fn new(game_id: &str, roots: Vec<String>, record: bool, now: Instant) -> Self {
        Session {
            game_id: game_id.to_string(),
            roots,
            record,
            started: now,
            epoch: (now, unix_now()),
            last_poll: None,
            last_seen: None,
            first_seen: None,
            resume_before: None,
            played: Duration::ZERO,
            credited: 0,
            reported: None,
        }
    }

    /// Reprend une session enregistrée avant l'arrêt de Ludex. `None` si ses relevés précèdent ce
    /// que l'horloge monotone peut représenter (PC redémarré depuis) : la session est alors close.
    pub fn resume(open: &OpenSession, now: Instant, now_unix: u64) -> Option<Session> {
        let at = |unix: u64| now.checked_sub(Duration::from_secs(now_unix.saturating_sub(unix)));
        let last_seen = at(open.last_seen)?;
        let played = Duration::from_secs(open.played).min(MAX_SESSION);
        Some(Session {
            game_id: open.game_id.clone(),
            roots: open.roots.clone(),
            record: open.record,
            started: at(open.start).unwrap_or(last_seen),
            epoch: (now, now_unix),
            last_poll: Some((at(open.last_poll)?, open.last_poll_seen)),
            last_seen: Some(last_seen),
            first_seen: Some(open.start),
            resume_before: Some(open.last_poll),
            played,
            credited: open.credited.min(played.as_secs()),
            reported: None,
        })
    }

    /// Heure Unix d'un instant de l'horloge monotone.
    fn unix(&self, at: Instant) -> u64 {
        let (origin, unix) = self.epoch;
        match at.checked_duration_since(origin) {
            Some(d) => unix + d.as_secs(),
            None => unix.saturating_sub(origin.duration_since(at).as_secs()),
        }
    }

    /// L'exécutable (chemin normalisé) appartient-il au jeu ?
    fn owns(&self, exe: &str) -> bool {
        self.roots.iter().any(|root| exe.starts_with(root.as_str()))
    }

    /// Un processus du jeu tourne-t-il ? Après une reprise, le premier relevé écarte les processus
    /// démarrés après le dernier relevé enregistré : ce n'est plus la même partie.
    pub fn sees(&mut self, procs: &[Proc]) -> bool {
        let before = self.resume_before.take();
        procs
            .iter()
            .any(|p| self.owns(&p.exe) && before.is_none_or(|t| p.start == 0 || p.start <= t))
    }

    /// État à enregistrer pour une reprise. Rien tant que le jeu n'a pas été vu.
    pub fn snapshot(&self) -> Option<OpenSession> {
        let (poll_at, poll_seen) = self.last_poll?;
        Some(OpenSession {
            game_id: self.game_id.clone(),
            roots: self.roots.clone(),
            record: self.record,
            start: self.first_seen?,
            last_poll: self.unix(poll_at),
            last_poll_seen: poll_seen,
            last_seen: self.unix(self.last_seen?),
            played: self.played.as_secs(),
            credited: self.credited,
        })
    }

    /// Entrée d'historique d'une session terminée, si elle a duré assez longtemps.
    pub fn past(&self) -> Option<PastSession> {
        (self.played >= MIN_SESSION).then(|| PastSession {
            game_id: self.game_id.clone(),
            start: self.first_seen.unwrap_or_else(|| self.unix(self.started)),
            secs: self.played.as_secs(),
            launcher: !self.record,
        })
    }

    /// Avance la session d'un relevé : `seen` indique si un processus du jeu tourne à `now`.
    pub fn tick(&mut self, now: Instant, seen: bool) -> Tick {
        let previous = self.last_poll.replace((now, seen));
        if seen {
            // Seul l'intervalle entre deux relevés positifs compte : un écart trop long
            // (veille, hibernation) n'est pas du jeu.
            if let Some((at, true)) = previous {
                let step = now.saturating_duration_since(at);
                if step <= MAX_STEP {
                    self.played = (self.played + step).min(MAX_SESSION);
                }
            }
            self.last_seen = Some(now);
            if self.first_seen.is_none() {
                self.first_seen = Some(self.unix(now));
            }
        }
        let installed = self.played >= MIN_SESSION;
        let phase = match self.last_seen {
            _ if seen => Phase::Running,
            None if now.saturating_duration_since(self.started) > DETECT_TIMEOUT => Phase::Ended,
            None => Phase::Detecting,
            Some(last) => {
                let grace = if installed { GRACE } else { GRACE_EARLY };
                if now.saturating_duration_since(last) > grace {
                    Phase::Ended
                } else if installed {
                    Phase::Running
                } else {
                    Phase::Detecting
                }
            }
        };

        let played = self.played.as_secs();
        let pending = played - self.credited;
        let flush = phase == Phase::Ended || pending >= FLUSH_EVERY;
        let credit_secs = if self.record && installed && flush && pending > 0 {
            self.credited = played;
            pending
        } else {
            0
        };
        Tick { phase, credit_secs }
    }
}

/// Processus en cours. `System` est gardé d'un relevé à l'autre : le chemin d'un processus
/// n'est lu qu'une fois (son heure de démarrage est lue à sa découverte).
fn running_processes(system: &mut Option<System>) -> Vec<Proc> {
    if system.is_none() {
        *system = System::new().ok();
    }
    let Some(sys) = system else { return Vec::new() };
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
    sys.processes()
        .values()
        .filter_map(|p| p.exe().map(|exe| Proc { exe: normalize(&exe.to_string_lossy()), start: p.start_time() }))
        .collect()
}

enum Msg {
    /// Jeu lancé depuis Ludex.
    Start(Session),
    /// Reprendre les sessions ouvertes lors de l'exécution précédente de Ludex.
    Resume,
}

/// Reprend les sessions enregistrées, ou clôt celles qui ne peuvent pas reprendre (PC redémarré).
fn resume_open(journal: &Journal, sessions: &mut Vec<Session>, finished: &mut Vec<PastSession>, updates: &mut Vec<Update>) {
    let (now, now_unix) = (Instant::now(), unix_now());
    let boot = System::boot_time().unwrap_or(0);
    for open in journal.load_open() {
        if sessions.iter().any(|s| s.game_id == open.game_id) {
            continue;
        }
        match Session::resume(&open, now, now_unix).filter(|_| open.last_poll >= boot) {
            Some(s) => sessions.push(s),
            None => {
                let (past, credit_secs) = close_stale(&open);
                finished.extend(past);
                updates.push(Update { game_id: open.game_id, phase: Phase::Ended, credit_secs });
            }
        }
    }
}

/// Fil de surveillance des sessions. Il dort tant qu'aucun jeu n'a été lancé.
pub struct Tracker {
    tx: mpsc::Sender<Msg>,
    phases: Arc<Mutex<HashMap<String, Phase>>>,
    history: SharedHistory,
}

impl Tracker {
    pub fn spawn(mut journal: Journal, on_update: impl Fn(Update) + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel::<Msg>();
        let phases: Arc<Mutex<HashMap<String, Phase>>> = Arc::default();
        let shared = phases.clone();
        let history = journal.shared();
        let spawned = std::thread::Builder::new().name("ludex-sessions".into()).spawn(move || {
            let mut sessions: Vec<Session> = Vec::new();
            let mut system: Option<System> = None;
            loop {
                let incoming = if sessions.is_empty() {
                    system = None; // libère la liste des processus entre deux parties
                    match rx.recv() {
                        Ok(m) => Some(m),
                        Err(_) => return,
                    }
                } else {
                    match rx.recv_timeout(POLL) {
                        Ok(m) => Some(m),
                        Err(RecvTimeoutError::Timeout) => None,
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                };
                let mut finished: Vec<PastSession> = Vec::new();
                let mut updates: Vec<Update> = Vec::new();
                match incoming {
                    Some(Msg::Start(s)) if !sessions.iter().any(|known| known.game_id == s.game_id) => sessions.push(s),
                    Some(Msg::Resume) => resume_open(&journal, &mut sessions, &mut finished, &mut updates),
                    _ => {}
                }

                let procs = if sessions.is_empty() { Vec::new() } else { running_processes(&mut system) };
                let now = Instant::now();
                sessions.retain_mut(|s| {
                    let seen = s.sees(&procs);
                    let tick = s.tick(now, seen);
                    if tick.phase == Phase::Ended {
                        finished.extend(s.past());
                    }
                    if tick.credit_secs > 0 || s.reported != Some(tick.phase) {
                        s.reported = Some(tick.phase);
                        if let Ok(mut map) = shared.lock() {
                            if tick.phase == Phase::Ended {
                                map.remove(&s.game_id);
                            } else {
                                map.insert(s.game_id.clone(), tick.phase);
                            }
                        }
                        updates.push(Update { game_id: s.game_id.clone(), phase: tick.phase, credit_secs: tick.credit_secs });
                    }
                    tick.phase != Phase::Ended
                });

                // Historique, puis sessions en cours, puis cumul (via `on_update`) : un arrêt entre
                // deux écritures perd au plus un relevé, sans jamais compter deux fois.
                journal.record(finished);
                let open: Vec<OpenSession> = sessions.iter().filter_map(Session::snapshot).collect();
                journal.save_open(&open);
                updates.into_iter().for_each(&on_update);
            }
        });
        if let Err(e) = spawned {
            eprintln!("Surveillance des sessions indisponible : {e}");
        }
        Tracker { tx, phases, history }
    }

    /// Reprend les sessions restées ouvertes à l'arrêt précédent de Ludex. À appeler une fois,
    /// quand l'état de l'app est en place (le cumul de temps y est enregistré).
    pub fn resume(&self) {
        let _ = self.tx.send(Msg::Resume);
    }

    /// Sessions terminées d'un jeu, la plus récente d'abord.
    pub fn history(&self, game_id: &str) -> Vec<PastSession> {
        crate::history::sessions_of(&self.history, game_id)
    }

    /// Temps de jeu des derniers jours (sessions terminées).
    pub fn recent_playtime(&self) -> crate::history::RecentPlaytime {
        let sessions = self.history.lock().map(|all| all.clone()).unwrap_or_default();
        crate::history::recent_playtime(&sessions, unix_now(), crate::history::RECENT_DAYS)
    }

    /// Dernière session de chaque jeu, la plus récente d'abord.
    pub fn last_sessions(&self) -> Vec<PastSession> {
        self.history.lock().map(|all| crate::history::last_sessions(&all)).unwrap_or_default()
    }

    /// Commence à suivre un jeu qui vient d'être lancé. Sans dossier connu, ou déjà suivi : rien.
    pub fn start(&self, game: &Game) {
        let roots = roots_for(game);
        if roots.is_empty() {
            return;
        }
        let Ok(mut map) = self.phases.lock() else { return };
        if map.contains_key(&game.id) {
            return; // relancé pendant la partie : la session en cours continue
        }
        map.insert(game.id.clone(), Phase::Detecting);
        drop(map);
        let session = Session::new(&game.id, roots, !game.launcher_tracks_playtime, Instant::now());
        if self.tx.send(Msg::Start(session)).is_err()
            && let Ok(mut map) = self.phases.lock()
        {
            map.remove(&game.id);
        }
    }

    /// Sessions en cours : identifiant du jeu et phase.
    pub fn active(&self) -> Vec<(String, Phase)> {
        self.phases
            .lock()
            .map(|map| map.iter().map(|(id, phase)| (id.clone(), *phase)).collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Launch;

    /// Rejoue une suite de relevés espacés de `POLL` : `true` = un processus du jeu tourne.
    fn replay(session: &mut Session, t0: Instant, from: u64, seen: &[bool]) -> Vec<Tick> {
        seen.iter()
            .enumerate()
            .map(|(i, &s)| session.tick(t0 + POLL * (from as u32 + i as u32), s))
            .collect()
    }

    fn session(record: bool, t0: Instant) -> Session {
        Session::new("epic:Sugar", vec![r"d:\games\rocket league\".into()], record, t0)
    }

    #[test]
    fn matches_processes_under_game_folder() {
        let s = session(true, Instant::now());
        assert!(s.owns(&normalize(r"D:\Games\Rocket League\Binaries\Win64\RocketLeague.exe")));
        assert!(s.owns(&normalize(r"\\?\D:/Games/Rocket League/RL.exe")));
        assert!(!s.owns(&normalize(r"D:\Games\Rocket League 2\RL.exe")));
        assert!(!s.owns(&normalize(r"C:\Program Files\Epic Games\Launcher\EpicGamesLauncher.exe")));
    }

    #[test]
    fn refuses_too_wide_roots() {
        assert_eq!(root_key(r"D:\Games\Rocket League\").as_deref(), Some(r"d:\games\rocket league\"));
        assert_eq!(root_key("C:/XboxGames/Halo/Content").as_deref(), Some(r"c:\xboxgames\halo\content\"));
        assert_eq!(root_key(r"C:\"), None);
        assert_eq!(root_key(r"C:\Program Files"), None);
        assert_eq!(root_key(r"C:\Windows\"), None);
        assert_eq!(root_key(""), None);
    }

    #[test]
    fn builds_roots_from_game() {
        let mut g = Game::new(Platform::Xbox, "Microsoft.4297127D64EC6_8wekyb3d8bbwe", "Minecraft", Launch::Uri(String::new()));
        assert!(roots_for(&g).iter().all(|r| r.contains("packages")));
        g.install_dir = Some(r"C:\Program Files\WindowsApps\Microsoft.4297127D64EC6_2.6.2.0_x64__8wekyb3d8bbwe".into());
        assert!(roots_for(&g)[0].starts_with(r"c:\program files\windowsapps\microsoft.4297127d64ec6"));

        let mut epic = Game::new(Platform::Epic, "Sugar", "Rocket League", Launch::Uri(String::new()));
        assert!(roots_for(&epic).is_empty());
        epic.install_dir = Some(r"D:\Games\rocketleague".into());
        assert_eq!(roots_for(&epic), vec![r"d:\games\rocketleague\".to_string()]);
    }

    #[test]
    fn measures_a_session_and_flushes_every_minute() {
        let t0 = Instant::now();
        let mut s = session(true, t0);
        // 2 relevés sans le jeu (launcher qui démarre), puis 30 relevés avec (150 s), puis fermeture.
        let mut seen = vec![false, false];
        seen.extend([true; 30]);
        seen.extend([false; 8]);
        let ticks = replay(&mut s, t0, 0, &seen);

        assert_eq!(ticks[0].phase, Phase::Detecting);
        assert_eq!(ticks[2].phase, Phase::Running);
        let credits: Vec<u64> = ticks.iter().map(|t| t.credit_secs).filter(|&c| c > 0).collect();
        // 29 intervalles de 5 s entre 30 relevés positifs = 145 s : 60 à la première minute,
        // 60 à la deuxième, le reste à la fin de la session.
        assert_eq!(credits, vec![60, 60, 25]);
        assert_eq!(credits.iter().sum::<u64>(), 145);
        // La session se termine après GRACE (30 s) d'absence.
        let end = ticks.iter().position(|t| t.phase == Phase::Ended).unwrap();
        assert_eq!(end, 2 + 30 + 6);
    }

    #[test]
    fn launcher_tracked_games_are_followed_but_not_recorded() {
        let t0 = Instant::now();
        let mut s = session(false, t0);
        let ticks = replay(&mut s, t0, 0, &[true; 40]);
        assert!(ticks.iter().all(|t| t.credit_secs == 0 && t.phase == Phase::Running));
    }

    #[test]
    fn short_sessions_are_ignored() {
        let t0 = Instant::now();
        let mut s = session(true, t0);
        // Lanceur visible 20 s, puis plus rien : fin après GRACE_EARLY, aucun temps cumulé.
        let mut seen = vec![true; 5];
        seen.extend(vec![false; (GRACE_EARLY.as_secs() / POLL.as_secs() + 1) as usize]);
        let ticks = replay(&mut s, t0, 0, &seen);
        assert_eq!(ticks.last().unwrap().phase, Phase::Ended);
        assert!(ticks.iter().all(|t| t.credit_secs == 0));
        assert_eq!(ticks[6].phase, Phase::Detecting);
    }

    #[test]
    fn waits_for_the_real_game_after_a_bootstrapper() {
        let t0 = Instant::now();
        let mut s = session(true, t0);
        // PlayGTAV.exe 10 s, Rockstar Launcher 2 min (hors du dossier), puis le jeu 5 min.
        let mut seen = vec![true; 2];
        seen.extend([false; 24]);
        seen.extend([true; 61]);
        seen.extend([false; 7]);
        let ticks = replay(&mut s, t0, 0, &seen);
        let total: u64 = ticks.iter().map(|t| t.credit_secs).sum();
        assert_eq!(total, 5 + 300);
        assert_eq!(ticks.last().unwrap().phase, Phase::Ended);
    }

    #[test]
    fn gives_up_when_the_game_never_appears() {
        let t0 = Instant::now();
        let mut s = session(true, t0);
        assert_eq!(s.tick(t0 + DETECT_TIMEOUT, false).phase, Phase::Detecting);
        assert_eq!(s.tick(t0 + DETECT_TIMEOUT + POLL, false), Tick { phase: Phase::Ended, credit_secs: 0 });
    }

    /// Le jeu en cours, démarré à `start` (secondes Unix).
    fn game_running(start: u64) -> Vec<Proc> {
        vec![Proc { exe: normalize(r"D:\Games\Rocket League\RL.exe"), start }]
    }

    #[test]
    fn finished_sessions_go_to_history_launcher_ones_too() {
        let t0 = Instant::now();
        let mut seen = vec![false, false];
        seen.extend([true; 30]);
        seen.extend([false; 7]);

        let mut ludex = session(true, t0);
        replay(&mut ludex, t0, 0, &seen);
        let past = ludex.past().unwrap();
        assert_eq!(past.secs, 145);
        assert_eq!(past.start, ludex.epoch.1 + 10); // premier relevé positif, 10 s après le lancement
        assert!(!past.launcher);

        // Steam ou Galaxy : jamais cumulé, mais gardé dans l'historique avec sa marque.
        let mut steam = session(false, t0);
        let ticks = replay(&mut steam, t0, 0, &seen);
        assert!(ticks.iter().all(|t| t.credit_secs == 0));
        assert_eq!(steam.past().map(|p| (p.secs, p.launcher)), Some((145, true)));

        // Moins d'une minute : rien dans l'historique, rien à reprendre avant que le jeu soit vu.
        let mut short = session(true, t0);
        assert!(short.snapshot().is_none());
        replay(&mut short, t0, 0, &[true; 5]);
        assert!(short.past().is_none());
        assert!(short.snapshot().is_some());
    }

    /// Partie de 90 s (60 déjà cumulées), puis Ludex fermé : état enregistré au dernier relevé.
    fn interrupted(t0: Instant) -> OpenSession {
        let mut s = session(true, t0);
        let credits: u64 = replay(&mut s, t0, 0, &[true; 19]).iter().map(|t| t.credit_secs).sum();
        assert_eq!(credits, 60);
        let open = s.snapshot().unwrap();
        assert_eq!((open.played, open.credited, open.last_poll - open.start), (90, 60, 90));
        open
    }

    #[test]
    fn resumes_after_restart_when_the_game_still_runs() {
        let t0 = Instant::now();
        let open = interrupted(t0);
        // Ludex rouvert 10 min plus tard, le jeu (démarré avant le dernier relevé) tourne toujours.
        let closed = Duration::from_secs(600);
        let t1 = t0 + POLL * 18 + closed;
        let mut s = Session::resume(&open, t1, open.last_poll + closed.as_secs()).unwrap();
        assert!(s.sees(&game_running(open.start - 30)));
        let first = s.tick(t1, true);
        assert_eq!(first, Tick { phase: Phase::Running, credit_secs: 0 }); // les 10 min ne comptent pas
        assert_eq!(s.played, Duration::from_secs(90));

        // 60 s de plus, puis le jeu se ferme : le cumul reprend là où il s'était arrêté.
        let mut seen = vec![true; 12];
        seen.extend([false; 7]);
        let ticks: Vec<Tick> = seen.iter().enumerate().map(|(i, &v)| s.tick(t1 + POLL * (i as u32 + 1), v)).collect();
        assert_eq!(ticks.iter().map(|t| t.credit_secs).sum::<u64>(), 150 - 60);
        assert_eq!(ticks.last().unwrap().phase, Phase::Ended);
        let past = s.past().unwrap();
        assert_eq!((past.start, past.secs), (open.start, 150));
    }

    #[test]
    fn a_quick_restart_keeps_counting() {
        let t0 = Instant::now();
        let open = interrupted(t0);
        // Rouvert 15 s après le dernier relevé : l'écart reste sous MAX_STEP, il compte.
        let t1 = t0 + POLL * 18 + Duration::from_secs(15);
        let mut s = Session::resume(&open, t1, open.last_poll + 15).unwrap();
        let seen = s.sees(&game_running(0)); // heure de démarrage illisible : acceptée
        assert!(seen);
        assert_eq!(s.tick(t1, seen).phase, Phase::Running);
        assert_eq!(s.played, Duration::from_secs(105));
    }

    #[test]
    fn closes_when_the_game_ended_while_ludex_was_closed() {
        let t0 = Instant::now();
        let open = interrupted(t0);
        let t1 = t0 + POLL * 18 + Duration::from_secs(3600);
        let now_unix = open.last_poll + 3600;

        // Plus aucun processus du jeu : session close avec ce qui avait été mesuré.
        let mut s = Session::resume(&open, t1, now_unix).unwrap();
        assert!(!s.sees(&[]));
        assert_eq!(s.tick(t1, false), Tick { phase: Phase::Ended, credit_secs: 30 });
        assert_eq!(s.past().map(|p| p.secs), Some(90));

        // Jeu relancé hors de Ludex entre-temps : ce n'est pas la même partie.
        let mut relaunched = Session::resume(&open, t1, now_unix).unwrap();
        let procs = game_running(open.last_poll + 600);
        assert!(!relaunched.sees(&procs));
        assert_eq!(relaunched.tick(t1, false).phase, Phase::Ended);
        // Le filtre ne vaut que pour le premier relevé.
        assert!(relaunched.sees(&procs));
    }

    #[test]
    fn sleep_after_a_restart_is_not_counted() {
        let t0 = Instant::now();
        let open = interrupted(t0);
        let t1 = t0 + POLL * 18 + Duration::from_secs(60);
        let mut s = Session::resume(&open, t1, open.last_poll + 60).unwrap();
        replay(&mut s, t1, 0, &[true; 7]); // 30 s
        assert_eq!(s.played, Duration::from_secs(120));
        // Veille de 2 h, jeu toujours ouvert au réveil : l'écart n'est pas compté.
        let wake = t1 + POLL * 6 + Duration::from_secs(7200);
        s.tick(wake, true);
        assert_eq!(s.played, Duration::from_secs(120));
        let snapshot = s.snapshot().unwrap();
        assert_eq!(snapshot.last_poll, open.last_poll + 60 + 30 + 7200);
        assert_eq!(snapshot.start, open.start);
    }

    #[test]
    fn stale_sessions_are_closed_with_what_was_measured() {
        let open = interrupted(Instant::now());
        let (past, credit) = close_stale(&open);
        assert_eq!((past.map(|p| p.secs), credit), (Some(90), 30));

        let steam = OpenSession { record: false, ..open.clone() };
        let (past, credit) = close_stale(&steam);
        assert_eq!((past.map(|p| p.launcher), credit), (Some(true), 0));

        let short = OpenSession { played: 40, credited: 0, ..open };
        assert_eq!(close_stale(&short), (None, 0));
    }

    /// Relevé réel des processus de ce PC : `cargo test processes_this_pc -- --ignored --nocapture`.
    /// Ne lance rien : indique seulement quels jeux de la bibliothèque tournent déjà.
    #[test]
    #[ignore]
    fn processes_this_pc() {
        let mut system = None;
        let started = Instant::now();
        let procs = running_processes(&mut system);
        let first = started.elapsed();
        let again = Instant::now();
        running_processes(&mut system);
        println!("{} exécutables lus ({first:?}, relevé suivant {:?})", procs.len(), again.elapsed());
        assert!(procs.iter().any(|p| p.exe.ends_with("explorer.exe") && p.start > 0));
        for game in crate::library::scan().games {
            let s = Session::new(&game.id, roots_for(&game), false, Instant::now());
            let running: Vec<&String> = procs.iter().map(|p| &p.exe).filter(|e| s.owns(e)).collect();
            if !running.is_empty() {
                println!("  En jeu : {} ({:?})", game.title, running);
            }
        }
    }

    #[test]
    fn sleep_and_absurd_lengths_are_not_counted() {
        let t0 = Instant::now();
        let mut s = session(true, t0);
        let ticks = replay(&mut s, t0, 0, &[true; 13]); // 60 s
        assert_eq!(ticks.iter().map(|t| t.credit_secs).sum::<u64>(), 60);
        // Veille de 8 h avec le jeu ouvert : l'écart n'est pas compté.
        let after_sleep = t0 + POLL * 12 + Duration::from_secs(8 * 3600);
        assert_eq!(s.tick(after_sleep, true).credit_secs, 0);
        assert_eq!(s.played, Duration::from_secs(60));

        // Plafond : jamais plus de MAX_SESSION par session.
        let mut long = session(true, t0);
        let mut t = t0;
        let mut total = 0;
        for _ in 0..(MAX_SESSION.as_secs() / POLL.as_secs() + 100) {
            total += long.tick(t, true).credit_secs;
            t += POLL;
        }
        total += long.tick(t + Duration::from_secs(3600), false).credit_secs;
        assert_eq!(total, MAX_SESSION.as_secs());
    }
}
