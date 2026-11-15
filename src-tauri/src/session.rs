//! Mesure du temps de jeu par Ludex, et état « En jeu ».
//!
//! Après un lancement depuis Ludex, un fil de surveillance relève toutes les 5 secondes les
//! processus dont l'exécutable est rangé sous le dossier du jeu. Une session dure tant qu'au
//! moins un de ces processus tourne. Le temps n'est cumulé (dans `user-data.json`) que pour les
//! jeux dont le launcher ne compte rien lui-même : Steam et GOG Galaxy gardent leur chiffre.
//!
//! Limites assumées : un jeu lancé hors de Ludex n'est pas mesuré ; si Ludex est fermé pendant
//! la partie, le temps déjà mesuré est gardé (enregistré chaque minute) mais la suite ne l'est pas.

use crate::library::{Game, Platform};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
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
    last_poll: Option<(Instant, bool)>,
    last_seen: Option<Instant>,
    played: Duration,
    /// Secondes déjà ajoutées au cumul.
    credited: u64,
    /// Dernière phase transmise, pour ne signaler que les changements.
    reported: Option<Phase>,
}

impl Session {
    pub fn new(game_id: &str, roots: Vec<String>, record: bool, now: Instant) -> Self {
        Session {
            game_id: game_id.to_string(),
            roots,
            record,
            started: now,
            last_poll: None,
            last_seen: None,
            played: Duration::ZERO,
            credited: 0,
            reported: None,
        }
    }

    /// L'exécutable (chemin normalisé) appartient-il au jeu ?
    fn owns(&self, exe: &str) -> bool {
        self.roots.iter().any(|root| exe.starts_with(root.as_str()))
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

/// Exécutables en cours (chemins normalisés). `System` est gardé d'un relevé à l'autre :
/// le chemin d'un processus n'est lu qu'une fois.
fn running_executables(system: &mut Option<System>) -> Vec<String> {
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
        .filter_map(|p| p.exe())
        .map(|exe| normalize(&exe.to_string_lossy()))
        .collect()
}

/// Fil de surveillance des sessions. Il dort tant qu'aucun jeu n'a été lancé.
pub struct Tracker {
    tx: mpsc::Sender<Session>,
    phases: Arc<Mutex<HashMap<String, Phase>>>,
}

impl Tracker {
    pub fn spawn(on_update: impl Fn(Update) + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel::<Session>();
        let phases: Arc<Mutex<HashMap<String, Phase>>> = Arc::default();
        let shared = phases.clone();
        let spawned = std::thread::Builder::new().name("ludex-sessions".into()).spawn(move || {
            let mut sessions: Vec<Session> = Vec::new();
            let mut system: Option<System> = None;
            loop {
                let incoming = if sessions.is_empty() {
                    system = None; // libère la liste des processus entre deux parties
                    match rx.recv() {
                        Ok(s) => Some(s),
                        Err(_) => return,
                    }
                } else {
                    match rx.recv_timeout(POLL) {
                        Ok(s) => Some(s),
                        Err(RecvTimeoutError::Timeout) => None,
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                };
                if let Some(s) = incoming
                    && !sessions.iter().any(|known| known.game_id == s.game_id)
                {
                    sessions.push(s);
                }

                let exes = running_executables(&mut system);
                let now = Instant::now();
                sessions.retain_mut(|s| {
                    let seen = exes.iter().any(|exe| s.owns(exe));
                    let tick = s.tick(now, seen);
                    if tick.credit_secs > 0 || s.reported != Some(tick.phase) {
                        s.reported = Some(tick.phase);
                        if let Ok(mut map) = shared.lock() {
                            if tick.phase == Phase::Ended {
                                map.remove(&s.game_id);
                            } else {
                                map.insert(s.game_id.clone(), tick.phase);
                            }
                        }
                        on_update(Update { game_id: s.game_id.clone(), phase: tick.phase, credit_secs: tick.credit_secs });
                    }
                    tick.phase != Phase::Ended
                });
            }
        });
        if let Err(e) = spawned {
            eprintln!("Surveillance des sessions indisponible : {e}");
        }
        Tracker { tx, phases }
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
        if self.tx.send(session).is_err()
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

    /// Relevé réel des processus de ce PC : `cargo test processes_this_pc -- --ignored --nocapture`.
    /// Ne lance rien : indique seulement quels jeux de la bibliothèque tournent déjà.
    #[test]
    #[ignore]
    fn processes_this_pc() {
        let mut system = None;
        let started = Instant::now();
        let exes = running_executables(&mut system);
        let first = started.elapsed();
        let again = Instant::now();
        running_executables(&mut system);
        println!("{} exécutables lus ({first:?}, relevé suivant {:?})", exes.len(), again.elapsed());
        assert!(exes.iter().any(|e| e.ends_with("explorer.exe")));
        for game in crate::library::scan().games {
            let s = Session::new(&game.id, roots_for(&game), false, Instant::now());
            let running: Vec<&String> = exes.iter().filter(|e| s.owns(e)).collect();
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
