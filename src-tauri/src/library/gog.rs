//! GOG : une clé par jeu sous `HKLM\SOFTWARE\WOW6432Node\GOG.com\Games\<gameID>`
//! (`gameName`, `path`, `exe`, `launchParam`). Les DLC portent `dependsOn`.
//! Lancement par GOG Galaxy s'il est installé, sinon par l'exécutable du jeu.
//! Temps de jeu : base SQLite de Galaxy, `ProgramData\GOG.com\Galaxy\storage\galaxy-2.0.db`.

use super::{program_data, Game, Launch, Platform};
use rusqlite::{Connection, OpenFlags};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Minutes de jeu et dernière session (secondes Unix), par identifiant de jeu GOG.
type Times = HashMap<String, (u64, Option<u64>)>;

/// `AAAA-MM-JJ HH:MM:SS` (UTC, éventuellement suivi d'un `T`, de fractions ou d'un fuseau) en secondes Unix.
pub(super) fn parse_datetime(s: &str) -> Option<u64> {
    let s = s.get(..19)?;
    let n = |range: std::ops::Range<usize>| s.get(range)?.parse::<i64>().ok();
    let (y, m, d) = (n(0..4)?, n(5..7)?, n(8..10)?);
    let (hh, mm, ss) = (n(11..13)?, n(14..16)?, n(17..19)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || hh > 23 || mm > 59 || ss > 60 {
        return None;
    }
    // Jours depuis le 1er janvier 1970 (algorithme « days from civil » de H. Hinnant).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days * 86_400 + hh * 3600 + mm * 60 + ss).ok()
}

/// Lit les temps de jeu de Galaxy, en lecture seule : `GameTimes` (minutes) et, si elle existe,
/// `LastPlayedDates`. Un jeu GOG y est désigné par `gog_<gameID>` ; les autres plateformes
/// que Galaxy peut agréger sont ignorées. Plusieurs comptes : le plus grand chiffre.
fn galaxy_times(db: &Path) -> rusqlite::Result<Times> {
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    // Galaxy peut écrire au même moment : on attend un peu plutôt que d'échouer.
    conn.busy_timeout(Duration::from_millis(500))?;
    let mut times = Times::new();
    let mut stmt = conn.prepare("SELECT releaseKey, MAX(minutesInGame) FROM GameTimes GROUP BY releaseKey")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?)))?;
    for (key, minutes) in rows.flatten() {
        if let Some(id) = key.strip_prefix("gog_") {
            times.insert(id.to_string(), (minutes.unwrap_or(0).max(0) as u64, None));
        }
    }
    if let Ok(mut stmt) = conn.prepare("SELECT gameReleaseKey, MAX(lastPlayedDate) FROM LastPlayedDates GROUP BY gameReleaseKey") {
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)))?;
        for (key, date) in rows.flatten() {
            if let (Some(id), Some(ts)) = (key.strip_prefix("gog_"), date.as_deref().and_then(parse_datetime)) {
                times.entry(id.to_string()).or_insert((0, None)).1 = Some(ts);
            }
        }
    }
    Ok(times)
}

struct Entry {
    id: String,
    name: String,
    path: String,
    exe: String,
    params: String,
    depends_on: String,
}

/// Pourquoi `build` écarte une entrée du registre (page de diagnostic).
#[cfg(windows)]
fn skip_reason(e: &Entry, galaxy: bool) -> Option<&'static str> {
    if e.name.is_empty() {
        None
    } else if !e.depends_on.is_empty() {
        Some(tr_str!("Contenu additionnel (DLC)", "Additional content (DLC)"))
    } else if !Path::new(&e.path).is_dir() {
        Some(tr_str!("Dossier d'installation introuvable", "Install folder not found"))
    } else if !galaxy && !Path::new(&e.exe).is_file() {
        Some(tr_str!("Exécutable introuvable (GOG Galaxy absent)", "Executable not found (GOG Galaxy not installed)"))
    } else {
        None
    }
}

/// `times` : temps de jeu de Galaxy, `None` si la base est absente ou illisible.
fn build(e: &Entry, galaxy: Option<&Path>, times: Option<&Times>) -> Option<Game> {
    if !e.depends_on.is_empty() || e.name.is_empty() || !Path::new(&e.path).is_dir() {
        return None;
    }
    let launch = match galaxy {
        Some(client) => Launch::Exec {
            path: client.to_path_buf(),
            args: vec!["/command=runGame".into(), format!("/gameId={}", e.id), format!("/path={}", e.path)],
        },
        None => {
            let exe = PathBuf::from(&e.exe);
            if !exe.is_file() {
                return None;
            }
            Launch::Exec { path: exe, args: e.params.split_whitespace().map(String::from).collect() }
        }
    };
    let mut game = Game::new(Platform::Gog, &e.id, &e.name, launch);
    game.install_dir = Some(e.path.clone());
    // Lancé par Galaxy, le jeu est compté par Galaxy : Ludex ne le mesure pas en plus.
    // Sans Galaxy, c'est Ludex qui mesure les sessions.
    if galaxy.is_some() {
        game.launcher_tracks_playtime = true;
        if let Some(times) = times {
            let (minutes, last) = times.get(&e.id).copied().unwrap_or((0, None));
            game.set_launcher_playtime(minutes, last);
        }
    }
    Some(game)
}

pub fn scan() -> Result<Option<Vec<Game>>, String> {
    #[cfg(windows)]
    {
        use super::reg;
        let galaxy = reg::hklm(r"SOFTWARE\WOW6432Node\GOG.com\GalaxyClient\paths")
            .and_then(|k| reg::string(&k, "client"))
            .map(|dir| PathBuf::from(dir).join("GalaxyClient.exe"))
            .filter(|p| p.is_file());
        let Some(root) = reg::hklm(r"SOFTWARE\WOW6432Node\GOG.com\Games") else {
            return Ok(galaxy.map(|_| Vec::new()));
        };
        let times = galaxy.as_ref().and_then(|_| galaxy_times(&program_data().join(r"GOG.com\Galaxy\storage\galaxy-2.0.db")).ok());
        let games = reg::children(&root)
            .into_iter()
            .filter_map(|(id, key)| {
                let s = |name: &str| reg::string(&key, name).unwrap_or_default();
                let entry = Entry {
                    id: if s("gameID").is_empty() { id } else { s("gameID") },
                    name: s("gameName"),
                    path: s("path"),
                    exe: s("exe"),
                    params: s("launchParam"),
                    depends_on: s("dependsOn"),
                };
                let game = build(&entry, galaxy.as_deref(), times.as_ref());
                if game.is_none()
                    && let Some(reason) = skip_reason(&entry, galaxy.is_some())
                {
                    super::skip(&entry.name, reason);
                }
                game
            })
            .collect();
        Ok(Some(games))
    }
    #[cfg(not(windows))]
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_games_and_skips_dlc() {
        let dir = std::env::temp_dir().join(format!("ludex-gog-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("witcher3.exe");
        std::fs::write(&exe, b"").unwrap();
        let entry = Entry {
            id: "1207664663".into(),
            name: "The Witcher 3".into(),
            path: dir.to_string_lossy().into_owned(),
            exe: exe.to_string_lossy().into_owned(),
            params: "-skipintro".into(),
            depends_on: String::new(),
        };

        let direct = build(&entry, None, None).unwrap();
        assert_eq!(direct.id, "gog:1207664663");
        assert_eq!(direct.launch_label, "witcher3.exe -skipintro");
        // Sans Galaxy, personne d'autre ne compte : Ludex mesurera.
        assert!(!direct.launcher_tracks_playtime && direct.playtime_minutes.is_none());

        let client = Path::new(r"C:\GOG Galaxy\GalaxyClient.exe");
        let galaxy = build(&entry, Some(client), None).unwrap();
        assert!(galaxy.launch_label.starts_with("GalaxyClient.exe /command=runGame /gameId=1207664663"));
        assert!(galaxy.launcher_tracks_playtime && galaxy.playtime_minutes.is_none());

        let times = Times::from([("1207664663".to_string(), (754, Some(1_700_000_000)))]);
        let played = build(&entry, Some(client), Some(&times)).unwrap();
        assert_eq!(played.playtime_minutes, Some(754));
        assert_eq!(played.playtime_source, Some(crate::library::PlaytimeSource::Launcher));
        assert_eq!(played.last_played, Some(1_700_000_000));
        // Base lisible mais jeu absent : jamais lancé par Galaxy.
        assert_eq!(build(&entry, Some(client), Some(&Times::new())).unwrap().playtime_minutes, Some(0));

        let dlc = Entry { depends_on: "1207664663".into(), ..entry };
        assert!(build(&dlc, None, None).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_galaxy_dates() {
        assert_eq!(parse_datetime("1970-01-01 00:00:00"), Some(0));
        assert_eq!(parse_datetime("2021-04-05 19:15:32"), Some(1_617_650_132));
        assert_eq!(parse_datetime("2024-02-29T12:00:00.000Z"), Some(1_709_208_000));
        assert_eq!(parse_datetime("2021-13-05 19:15:32"), None);
        assert_eq!(parse_datetime("hier"), None);
    }

    #[test]
    fn reads_galaxy_database() {
        let db = std::env::temp_dir().join(format!("ludex-galaxy-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&db);
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(
                "CREATE TABLE GameTimes (userId INTEGER, releaseKey TEXT, minutesInGame INTEGER);
                 CREATE TABLE LastPlayedDates (userId INTEGER, gameReleaseKey TEXT, lastPlayedDate TEXT);
                 INSERT INTO GameTimes VALUES (1, 'gog_1207664663', 754), (2, 'gog_1207664663', 120),
                                              (1, 'steam_730', 9000), (1, 'gog_1495134320', NULL);
                 INSERT INTO LastPlayedDates VALUES (1, 'gog_1207664663', '2021-04-05 19:15:32'),
                                                    (1, 'gog_42', '2020-01-01 00:00:00');",
            )
            .unwrap();
        }
        let times = galaxy_times(&db).unwrap();
        assert_eq!(times.get("1207664663"), Some(&(754, Some(1_617_650_132))));
        assert_eq!(times.get("1495134320"), Some(&(0, None)));
        assert_eq!(times.get("42"), Some(&(0, Some(1_577_836_800))));
        assert!(!times.contains_key("730") && times.len() == 3);

        // Schéma inattendu (autre version de Galaxy) : erreur, jamais de plantage.
        let empty = db.with_extension("vide.db");
        let _ = std::fs::remove_file(&empty);
        Connection::open(&empty).unwrap().execute_batch("CREATE TABLE Autre (x INTEGER);").unwrap();
        assert!(galaxy_times(&empty).is_err());
        assert!(galaxy_times(&db.with_extension("absente.db")).is_err());
        let _ = std::fs::remove_file(&db);
        let _ = std::fs::remove_file(&empty);
    }
}
