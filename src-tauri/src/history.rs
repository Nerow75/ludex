//! Historique des sessions de jeu et sessions en cours, dans le dossier de données de l'app.
//!
//! - `sessions.json` : sessions terminées, une par ligne, `{ gameId, start, secs, launcher }`
//!   (`start` en secondes Unix, `secs` en secondes de jeu, `launcher` = le temps de référence est
//!   celui de Steam ou GOG Galaxy : la session n'est jamais ajoutée à leur chiffre). Réécrit en
//!   entier à la fin de chaque session (tmp + rename).
//! - `session-en-cours.json` : sessions encore ouvertes, réécrit à chaque relevé qui les change,
//!   supprimé quand plus rien ne tourne. Sert à reprendre le suivi au redémarrage de Ludex.
//!
//! Taille gardée raisonnable : au-delà de `MAX_SESSIONS`, la moitié la plus ancienne est fusionnée
//! par jeu et par jour (le total par mois et les jours joués restent exacts), puis les plus
//! anciennes sont retirées si cela ne suffit pas. Une session pèse environ 80 octets.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Nombre de sessions gardées : plusieurs années de jeu, quelques centaines de Ko.
const MAX_SESSIONS: usize = 5000;
const DAY: u64 = 86_400;

/// Session terminée.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PastSession {
    pub game_id: String,
    /// Premier relevé où le jeu tournait, en secondes Unix.
    pub start: u64,
    /// Durée de jeu mesurée, en secondes.
    pub secs: u64,
    /// Temps de référence compté par le launcher (Steam, GOG Galaxy) : jamais cumulé par Ludex.
    #[serde(default)]
    pub launcher: bool,
    /// Entrée qui regroupe plusieurs sessions d'un même jour (`compact`) : durée totale exacte,
    /// mais ni heure de début ni durée d'une session.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub merged: bool,
}

/// Session en cours, telle qu'enregistrée pour une reprise après redémarrage.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenSession {
    pub game_id: String,
    /// Dossiers surveillés, reconstruits depuis l'état au lancement (jamais venus du front).
    pub roots: Vec<String>,
    /// Faux pour Steam et GOG Galaxy : session suivie, jamais cumulée.
    pub record: bool,
    /// Premier relevé où le jeu tournait (secondes Unix).
    pub start: u64,
    /// Dernier relevé et son résultat.
    pub last_poll: u64,
    pub last_poll_seen: bool,
    /// Dernier relevé où le jeu tournait.
    pub last_seen: u64,
    /// Secondes de jeu mesurées, et secondes déjà ajoutées au cumul.
    pub played: u64,
    pub credited: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct HistoryFile {
    version: u32,
    sessions: Vec<PastSession>,
}

/// Historique en mémoire, partagé entre le fil des sessions (qui écrit) et les commandes (qui lisent).
pub type SharedHistory = Arc<Mutex<Vec<PastSession>>>;

/// Écrit dans un fichier temporaire puis le renomme : jamais de fichier à moitié écrit.
fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| tr!("Dossier de données inaccessible : {e}", "Data folder unreachable: {e}"))?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text).map_err(|e| tr!("Enregistrement impossible : {e}", "Couldn't save: {e}"))?;
    fs::rename(&tmp, path).map_err(|e| tr!("Enregistrement impossible : {e}", "Couldn't save: {e}"))
}

/// Une session par ligne : lisible et comparable à l'œil.
fn to_lines(sessions: &[PastSession]) -> String {
    let lines: Vec<String> = sessions.iter().filter_map(|s| serde_json::to_string(s).ok()).collect();
    format!("{{\"version\":1,\"sessions\":[\n{}\n]}}\n", lines.join(",\n"))
}

/// Garde l'historique sous `max` sessions : fusion par jeu et par jour (UTC) de la moitié la plus
/// ancienne, puis retrait des plus anciennes si besoin. Les sessions récentes restent intactes.
fn compact(sessions: &mut Vec<PastSession>, max: usize) {
    if sessions.len() <= max {
        return;
    }
    sessions.sort_by_key(|s| s.start);
    let old: Vec<PastSession> = sessions.drain(..sessions.len() / 2).collect();
    let mut merged: Vec<PastSession> = Vec::new();
    let mut index: HashMap<(String, u64, bool), usize> = HashMap::new();
    for s in old {
        let key = (s.game_id.clone(), s.start / DAY, s.launcher);
        match index.get(&key) {
            Some(&i) => {
                merged[i].secs = merged[i].secs.saturating_add(s.secs);
                merged[i].merged = true;
            }
            None => {
                index.insert(key, merged.len());
                merged.push(s);
            }
        }
    }
    merged.append(sessions);
    let excess = merged.len().saturating_sub(max);
    merged.drain(..excess);
    *sessions = merged;
}

/// Garde l'historique sous `MAX_SESSIONS` sessions (import de données).
pub fn compact_to_max(sessions: &mut Vec<PastSession>) {
    compact(sessions, MAX_SESSIONS);
}

/// Réécrit `sessions.json` du dossier de données (import de données ; appelé par le fil des
/// sessions ou sous le verrou de l'historique partagé, jamais en même temps qu'un `record`).
pub fn write_history(dir: &Path, sessions: &[PastSession]) -> Result<(), String> {
    write_atomic(&dir.join("sessions.json"), &to_lines(sessions))
}

/// Fichiers d'historique et de sessions en cours. Seul le fil des sessions écrit.
pub struct Journal {
    history_path: PathBuf,
    open_path: PathBuf,
    history: SharedHistory,
    /// Dernier contenu écrit dans `session-en-cours.json`, pour ne réécrire que les changements.
    open_written: Option<String>,
}

impl Journal {
    pub fn open(dir: &Path) -> Self {
        let history_path = dir.join("sessions.json");
        let sessions = fs::read_to_string(&history_path)
            .ok()
            .and_then(|text| serde_json::from_str::<HistoryFile>(&text).ok())
            .map(|f| f.sessions)
            .unwrap_or_default();
        Journal {
            history_path,
            open_path: dir.join("session-en-cours.json"),
            history: Arc::new(Mutex::new(sessions)),
            open_written: None,
        }
    }

    pub fn shared(&self) -> SharedHistory {
        self.history.clone()
    }

    /// Sessions restées ouvertes à la fermeture (ou au plantage) précédente de Ludex.
    pub fn load_open(&self) -> Vec<OpenSession> {
        fs::read_to_string(&self.open_path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Enregistre les sessions en cours ; supprime le fichier quand il n'y en a plus.
    pub fn save_open(&mut self, open: &[OpenSession]) {
        let text = if open.is_empty() { String::new() } else { serde_json::to_string_pretty(open).unwrap_or_default() };
        if self.open_written.as_deref() == Some(text.as_str()) {
            return;
        }
        let result = if text.is_empty() {
            match fs::remove_file(&self.open_path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(tr!("Suppression impossible : {e}", "Couldn't delete: {e}")),
                _ => Ok(()),
            }
        } else {
            write_atomic(&self.open_path, &text)
        };
        match result {
            Ok(()) => self.open_written = Some(text),
            Err(e) => eprintln!("Session en cours non enregistrée : {e}"),
        }
    }

    /// Ajoute des sessions terminées. Une session déjà présente (même jeu, même début) n'est pas
    /// ajoutée deux fois : cas d'un arrêt entre l'écriture de l'historique et celle des sessions en cours.
    pub fn record(&self, finished: Vec<PastSession>) {
        if finished.is_empty() {
            return;
        }
        let Ok(mut sessions) = self.history.lock() else { return };
        let mut added = false;
        for s in finished {
            if !sessions.iter().any(|known| known.game_id == s.game_id && known.start == s.start) {
                sessions.push(s);
                added = true;
            }
        }
        if !added {
            return;
        }
        compact(&mut sessions, MAX_SESSIONS);
        if let Err(e) = write_atomic(&self.history_path, &to_lines(&sessions)) {
            eprintln!("Historique non enregistré : {e}");
        }
    }
}

/// Sessions d'un jeu, la plus récente d'abord.
pub fn sessions_of(history: &SharedHistory, game_id: &str) -> Vec<PastSession> {
    let mut found: Vec<PastSession> = history
        .lock()
        .map(|all| all.iter().filter(|s| s.game_id == game_id).cloned().collect())
        .unwrap_or_default();
    found.sort_by_key(|s| std::cmp::Reverse(s.start));
    found
}

/// Période du temps de jeu récent affiché en bas de la barre latérale.
pub const RECENT_DAYS: u64 = 14;

/// Temps de jeu des derniers jours, tiré de l'historique de Ludex.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentPlaytime {
    /// Période couverte, en jours.
    pub days: u64,
    /// Secondes de jeu dans la période.
    pub secs: u64,
    /// Sessions et jeux différents comptés.
    pub sessions: usize,
    pub games: usize,
}

/// Temps de jeu des `days` derniers jours avant `now` (secondes Unix). Chaque session compte une
/// fois, celles de Steam ou GOG Galaxy (`launcher`) comprises : c'est la durée observée par Ludex,
/// jamais le cumul du launcher. Une session commencée avant la période n'y compte que pour sa
/// partie récente (durée de jeu supposée continue depuis son début). Sessions en cours non comptées.
pub fn recent_playtime(sessions: &[PastSession], now: u64, days: u64) -> RecentPlaytime {
    let since = now.saturating_sub(days * DAY);
    let mut seen: std::collections::HashSet<(&str, u64)> = std::collections::HashSet::new();
    let mut games: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut total = RecentPlaytime { days, ..Default::default() };
    for s in sessions {
        let counted = s.secs.saturating_sub(since.saturating_sub(s.start));
        // Même jeu, même début : une seule session (le journal évite déjà ce doublon).
        if counted == 0 || s.start > now || !seen.insert((s.game_id.as_str(), s.start)) {
            continue;
        }
        total.secs = total.secs.saturating_add(counted);
        total.sessions += 1;
        games.insert(s.game_id.as_str());
    }
    total.games = games.len();
    total
}

/// Dernière session de chaque jeu de l'historique, la plus récente d'abord.
pub fn last_sessions(sessions: &[PastSession]) -> Vec<PastSession> {
    let mut last: HashMap<&str, &PastSession> = HashMap::new();
    for s in sessions {
        let slot = last.entry(s.game_id.as_str()).or_insert(s);
        if s.start > slot.start {
            *slot = s;
        }
    }
    let mut found: Vec<PastSession> = last.into_values().cloned().collect();
    found.sort_by(|a, b| b.start.cmp(&a.start).then_with(|| a.game_id.cmp(&b.game_id)));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn past(id: &str, start: u64, secs: u64) -> PastSession {
        PastSession { game_id: id.into(), start, secs, launcher: false, merged: false }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ludex-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn records_reloads_and_ignores_duplicates() {
        let dir = temp_dir("history");
        let journal = Journal::open(&dir);
        journal.record(vec![past("epic:Sugar", 1000, 4800)]);
        journal.record(vec![past("epic:Sugar", 1000, 4800), PastSession { launcher: true, ..past("steam:730", 9000, 600) }]);

        let reloaded = Journal::open(&dir);
        let all = reloaded.shared().lock().unwrap().clone();
        assert_eq!(all.len(), 2);
        assert!(all[1].launcher);
        // Une session par ligne dans le fichier.
        let text = fs::read_to_string(dir.join("sessions.json")).unwrap();
        assert_eq!(text.lines().filter(|l| l.contains("gameId")).count(), 2);

        journal.record(vec![past("epic:Sugar", 50_000, 120)]);
        let sugar = sessions_of(&journal.shared(), "epic:Sugar");
        assert_eq!(sugar.iter().map(|s| s.start).collect::<Vec<_>>(), vec![50_000, 1000]);
        assert!(sessions_of(&journal.shared(), "steam:440").is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn open_sessions_are_saved_then_removed() {
        let dir = temp_dir("open");
        let mut journal = Journal::open(&dir);
        assert!(journal.load_open().is_empty());
        let open = OpenSession {
            game_id: "epic:Sugar".into(),
            roots: vec![r"d:\games\rocket league\".into()],
            record: true,
            start: 1000,
            last_poll: 1600,
            last_poll_seen: true,
            last_seen: 1600,
            played: 600,
            credited: 600,
        };
        journal.save_open(std::slice::from_ref(&open));
        assert_eq!(Journal::open(&dir).load_open(), vec![open]);
        journal.save_open(&[]);
        assert!(!dir.join("session-en-cours.json").exists());
        journal.save_open(&[]); // déjà absent : pas d'erreur
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn old_sessions_are_merged_by_day_then_dropped() {
        // 10 sessions de 10 min le même jour pour un jeu, puis 10 jours avec une session chacun.
        let mut sessions: Vec<PastSession> = (0..10).map(|i| past("epic:Sugar", i * 600, 600)).collect();
        sessions.extend((1..=10).map(|d| past("epic:Sugar", d * DAY, 60)));
        compact(&mut sessions, 15);
        // La moitié ancienne (les 10 du premier jour) devient une seule entrée de 100 min.
        assert_eq!(sessions.len(), 11);
        assert_eq!(sessions[0], PastSession { merged: true, ..past("epic:Sugar", 0, 6000) });
        assert!(!sessions[1].merged);
        assert_eq!(sessions.iter().map(|s| s.secs).sum::<u64>(), 6000 + 600);

        // Sans fusion possible (un jeu par jour), les plus anciennes sont retirées.
        let mut daily: Vec<PastSession> = (0..20).map(|d| past("epic:Sugar", d * DAY, 60)).collect();
        compact(&mut daily, 15);
        assert_eq!(daily.len(), 15);
        assert_eq!(daily[0].start, 5 * DAY);
    }

    #[test]
    fn recent_playtime_counts_each_session_once() {
        let now = 100 * DAY;
        let sessions = vec![
            past("epic:Sugar", now - DAY, 3600),
            // Steam : durée observée par Ludex, comptée une fois comme les autres.
            PastSession { launcher: true, ..past("steam:730", now - 2 * DAY, 1800) },
            // Doublon (même jeu, même début) ignoré.
            PastSession { launcher: true, ..past("steam:730", now - 2 * DAY, 1800) },
            // Commencée 10 min avant la période : seules les 20 min récentes comptent.
            past("epic:Sugar", now - 14 * DAY - 600, 1800),
            // Trop ancienne.
            past("steam:440", now - 20 * DAY, 7200),
        ];
        let recent = recent_playtime(&sessions, now, RECENT_DAYS);
        assert_eq!(recent, RecentPlaytime { days: 14, secs: 3600 + 1800 + 1200, sessions: 3, games: 2 });
        assert_eq!(recent_playtime(&[], now, RECENT_DAYS), RecentPlaytime { days: 14, ..Default::default() });
    }

    #[test]
    fn last_session_of_each_game_most_recent_first() {
        let sessions = vec![past("epic:Sugar", 1000, 60), past("steam:730", 5000, 60), past("epic:Sugar", 9000, 120), past("steam:440", 3000, 60)];
        let last = last_sessions(&sessions);
        assert_eq!(last, vec![past("epic:Sugar", 9000, 120), past("steam:730", 5000, 60), past("steam:440", 3000, 60)]);
    }

    #[test]
    fn launcher_sessions_are_not_merged_with_ludex_ones() {
        let mut sessions = vec![past("steam:730", 0, 60), PastSession { launcher: true, ..past("steam:730", 100, 60) }];
        sessions.extend((1..=4).map(|d| past("steam:730", d * DAY, 60)));
        compact(&mut sessions, 5);
        assert_eq!(sessions.iter().filter(|s| s.launcher).count(), 1);
    }
}
