//! itch.io : base SQLite de l'appli itch (`%APPDATA%\itch\db\butler.db`, tenue par `butler`),
//! lue en lecture seule. Une ligne de `caves` par jeu installé (`id`, `game_id`, `verdict`,
//! `install_location_id`, `install_folder_name`, `seconds_run`, `last_touched_at`,
//! `installed_size`), titre et genre dans `games` (`title`, `classification`), dossier racine
//! dans `install_locations` (`path`). Colonnes = noms des champs Go de butler en snake_case (ORM hades).
//!
//! `verdict` est le JSON du configurateur de butler : `basePath` (dossier du jeu) et `candidates`
//! (`path` relatif, `flavor` : `windows`, `html`, `jar`…, `arch`). Lancement par l'exécutable
//! Windows retenu par butler (`Launch::Exec`, mesuré par Ludex) ; sans exécutable Windows (jeu
//! HTML, Java, LÖVE…), par l'appli itch : `itch://install?game_id=<n>&launch` (l'appli lance le
//! jeu installé après confirmation et compte elle-même le temps, `seconds_run`).

use super::{Game, Launch, Platform};
use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

/// Dossier de l'appli itch (`%APPDATA%\itch`).
pub(super) fn root_dir() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default().join("itch")
}

/// Base de butler, dans le dossier de l'appli.
pub(super) fn database(root: &Path) -> PathBuf {
    root.join(r"db\butler.db")
}

/// Une ligne de `caves` avec son jeu et son emplacement.
#[derive(Debug, Default)]
struct Cave {
    game_id: i64,
    title: String,
    classification: String,
    verdict: String,
    location: String,
    folder: String,
    seconds_run: i64,
    last_touched: String,
    installed_size: i64,
}

#[derive(Deserialize, Default)]
struct Verdict {
    #[serde(rename = "basePath", default)]
    base_path: String,
    #[serde(default)]
    candidates: Vec<Candidate>,
}

#[derive(Deserialize)]
struct Candidate {
    #[serde(default)]
    path: String,
    #[serde(default)]
    flavor: String,
    #[serde(default)]
    arch: String,
}

/// Lit les jeux installés en lecture seule. Schéma différent (autre version de butler) : erreur.
fn read(db: &Path) -> rusqlite::Result<Vec<Cave>> {
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    // butler peut écrire au même moment : on attend un peu plutôt que d'échouer.
    conn.busy_timeout(Duration::from_millis(500))?;
    let mut stmt = conn.prepare(
        "SELECT c.game_id, g.title, g.classification, c.verdict, l.path, c.install_folder_name,
                c.seconds_run, c.last_touched_at, c.installed_size
         FROM caves c
         LEFT JOIN games g ON g.id = c.game_id
         LEFT JOIN install_locations l ON l.id = c.install_location_id",
    )?;
    let rows = stmt.query_map([], |r| {
        let text = |i: usize| -> rusqlite::Result<String> { Ok(r.get::<_, Option<String>>(i)?.unwrap_or_default().trim().to_string()) };
        Ok(Cave {
            game_id: r.get::<_, Option<i64>>(0)?.unwrap_or(0),
            title: text(1)?,
            classification: text(2)?,
            verdict: text(3)?,
            location: text(4)?,
            folder: text(5)?,
            seconds_run: r.get::<_, Option<i64>>(6)?.unwrap_or(0),
            last_touched: text(7)?,
            installed_size: r.get::<_, Option<i64>>(8)?.unwrap_or(0),
        })
    })?;
    rows.collect()
}

/// Chemin relatif sans `..`, sans lecteur ni racine.
fn safe_relative(path: &str) -> Option<&Path> {
    let p = Path::new(path);
    (!path.is_empty() && p.components().all(|c| matches!(c, Component::Normal(_)))).then_some(p)
}

/// Dossier du jeu : `basePath` du verdict, sinon emplacement + nom du dossier.
fn install_dir(cave: &Cave, verdict: &Verdict) -> Option<PathBuf> {
    let dir = if verdict.base_path.is_empty() {
        if cave.location.is_empty() {
            return None;
        }
        // Nom de dossier seul : jamais un chemin qui sortirait de l'emplacement.
        let folder = safe_relative(&cave.folder).filter(|f| f.components().count() == 1)?;
        Path::new(&cave.location).join(folder)
    } else {
        PathBuf::from(verdict.base_path.replace('/', "\\").trim_end_matches('\\'))
    };
    (dir.is_absolute() && dir.is_dir()).then_some(dir)
}

/// Exécutable Windows retenu par butler (64 bits d'abord), présent dans le dossier du jeu.
fn windows_exe(dir: &Path, verdict: &Verdict) -> Option<PathBuf> {
    let mut list: Vec<&Candidate> = verdict.candidates.iter().filter(|c| c.flavor == "windows").collect();
    list.sort_by_key(|c| c.arch != "amd64");
    list.into_iter().find_map(|c| {
        let rel = safe_relative(&c.path)?;
        let is_exe = rel.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"));
        let exe = dir.join(rel);
        (is_exe && exe.is_file()).then_some(exe)
    })
}

/// Contenus d'itch.io qui ne se jouent pas.
const NOT_GAMES: &[&str] = &["soundtrack", "assets", "comic", "book", "physical_game"];

/// Jeu tiré d'une ligne, ou la raison de l'écarter (page de diagnostic).
fn build(cave: &Cave) -> Result<Game, &'static str> {
    if cave.game_id <= 0 {
        return Err("Identifiant itch.io illisible");
    }
    if NOT_GAMES.contains(&cave.classification.as_str()) {
        return Err("Pas un jeu (bande-son, ressources, livre…)");
    }
    let verdict: Verdict = serde_json::from_str(&cave.verdict).unwrap_or_default();
    let dir = install_dir(cave, &verdict).ok_or("Dossier d'installation introuvable")?;
    let title = if cave.title.is_empty() {
        dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| format!("Jeu itch.io {}", cave.game_id))
    } else {
        cave.title.clone()
    };
    let id = cave.game_id.to_string();
    let mut game = match windows_exe(&dir, &verdict) {
        // Lancé directement : l'appli itch ne compte rien, Ludex mesure.
        Some(exe) => Game::new(Platform::Itch, &id, &title, Launch::Exec { path: exe, args: Vec::new() }),
        None => {
            let mut game = Game::new(Platform::Itch, &id, &title, Launch::Uri(format!("itch://install?game_id={id}&launch")));
            // Lancé par l'appli, qui compte le temps elle-même : jamais mesuré en plus.
            game.launcher_tracks_playtime = true;
            let minutes = u64::try_from(cave.seconds_run.max(0) / 60).unwrap_or(0);
            game.set_launcher_playtime(minutes, super::gog::parse_datetime(&cave.last_touched));
            game
        }
    };
    game.install_dir = Some(dir.to_string_lossy().into_owned());
    game.size_on_disk = u64::try_from(cave.installed_size).ok().filter(|&s| s > 0);
    game.is_tool = cave.classification == "tool";
    Ok(game)
}

/// Lecture depuis le dossier de l'appli : `None` = appli absente, vide = installée sans jeu.
fn scan_in(root: &Path) -> Result<Option<Vec<Game>>, String> {
    if !root.is_dir() {
        return Ok(None);
    }
    let db = database(root);
    if !db.is_file() {
        return Ok(Some(Vec::new()));
    }
    let caves = read(&db).map_err(|e| format!("Base de l'appli itch illisible : {e}"))?;
    let mut games: Vec<Game> = Vec::new();
    for cave in &caves {
        let title = || if cave.title.is_empty() { format!("Jeu itch.io {}", cave.game_id) } else { cave.title.clone() };
        match build(cave) {
            Ok(game) if games.iter().any(|g| g.id == game.id) => super::skip(&title(), "Même jeu installé deux fois (autre version)"),
            Ok(game) => games.push(game),
            Err(reason) => super::skip(&title(), reason),
        }
    }
    Ok(Some(games))
}

pub fn scan() -> Result<Option<Vec<Game>>, String> {
    scan_in(&root_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ludex-itch-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn json_path(p: &Path) -> String {
        p.to_string_lossy().replace('\\', "\\\\")
    }

    /// Fausse base au format de butler (colonnes en plus comprises, dates RFC 3339 en texte).
    fn fake_db(root: &Path) -> Connection {
        let db = database(root);
        std::fs::create_dir_all(db.parent().unwrap()).unwrap();
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE games (id INTEGER PRIMARY KEY, url TEXT, title TEXT, short_text TEXT, type TEXT,
                                 classification TEXT, cover_url TEXT, still_cover_url TEXT);
             CREATE TABLE install_locations (id TEXT PRIMARY KEY, path TEXT);
             CREATE TABLE caves (id TEXT PRIMARY KEY, game_id INTEGER, external_game_id INTEGER, upload_id INTEGER,
                                 build_id INTEGER, morphing INTEGER, pinned INTEGER, installed_at TEXT,
                                 last_touched_at TEXT, seconds_run INTEGER, local_seconds_run INTEGER,
                                 verdict TEXT, installed_size INTEGER, install_location_id TEXT,
                                 install_folder_name TEXT, custom_install_folder TEXT);",
        )
        .unwrap();
        conn
    }

    #[allow(clippy::too_many_arguments)]
    fn add(conn: &Connection, cave: &str, game: i64, title: &str, class: &str, verdict: &str, loc: &str, folder: &str, secs: i64) {
        conn.execute("INSERT OR IGNORE INTO games (id, title, classification, type) VALUES (?1, ?2, ?3, 'default')", rusqlite::params![game, title, class])
            .unwrap();
        conn.execute(
            "INSERT INTO caves (id, game_id, verdict, install_location_id, install_folder_name, seconds_run,
                                last_touched_at, installed_size)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, '2024-02-29T12:00:00.123456789Z', 1048576)",
            rusqlite::params![cave, game, verdict, loc, folder, secs],
        )
        .unwrap();
    }

    #[test]
    fn reads_installed_games() {
        let root = temp("base");
        let apps = root.join("apps");
        let celeste = apps.join("celeste-classic");
        std::fs::create_dir_all(celeste.join("bin")).unwrap();
        std::fs::write(celeste.join(r"bin\celeste.exe"), b"").unwrap();
        std::fs::write(celeste.join(r"bin\celeste32.exe"), b"").unwrap();
        let html = apps.join("jeu-html");
        std::fs::create_dir_all(&html).unwrap();
        let outil = apps.join("outil");
        std::fs::create_dir_all(&outil).unwrap();
        std::fs::write(outil.join("outil.exe"), b"").unwrap();

        let conn = fake_db(&root);
        conn.execute("INSERT INTO install_locations VALUES ('loc-1', ?1)", [apps.to_string_lossy()]).unwrap();
        let verdict = format!(
            r#"{{"basePath":"{}","totalSize":2,"candidates":[
                {{"path":"bin/celeste32.exe","depth":2,"flavor":"windows","arch":"386","size":1}},
                {{"path":"bin/celeste.exe","depth":2,"flavor":"windows","arch":"amd64","size":1}}]}}"#,
            json_path(&celeste)
        );
        add(&conn, "cave-a", 3141, "Celeste Classic", "game", &verdict, "loc-1", "celeste-classic", 7200);
        // Jeu HTML, verdict sans basePath : emplacement + dossier, lancé par l'appli.
        let html_verdict = r#"{"basePath":"","candidates":[{"path":"index.html","depth":1,"flavor":"html"}]}"#;
        add(&conn, "cave-b", 2718, "Jeu HTML", "game", html_verdict, "loc-1", "jeu-html", 5400);
        let tool_verdict = format!(r#"{{"basePath":"{}","candidates":[{{"path":"outil.exe","flavor":"windows"}}]}}"#, json_path(&outil));
        add(&conn, "cave-c", 1618, "Éditeur de niveaux", "tool", &tool_verdict, "loc-1", "outil", 0);
        add(&conn, "cave-d", 1414, "Bande-son", "soundtrack", &tool_verdict, "loc-1", "outil", 0);
        add(&conn, "cave-e", 3141, "Celeste Classic", "game", &verdict, "loc-1", "celeste-classic", 0);
        add(&conn, "cave-f", 1732, "Disparu", "game", "{}", "loc-1", "absent", 0);
        drop(conn);

        let games = scan_in(&root).unwrap().unwrap();
        let ids: Vec<&str> = games.iter().map(|g| g.id.as_str()).collect();
        assert_eq!(ids, ["itch:3141", "itch:2718", "itch:1618"]);

        let c = &games[0];
        assert_eq!(c.title, "Celeste Classic");
        // Exécutable 64 bits d'abord, lancé directement, mesuré par Ludex.
        assert_eq!(c.launch_label, "celeste.exe");
        assert!(matches!(&c.launch, Launch::Exec { path, .. } if path == &celeste.join(r"bin\celeste.exe")));
        assert!(!c.launcher_tracks_playtime && c.playtime_minutes.is_none());
        assert_eq!(c.install_dir.as_deref(), Some(celeste.to_string_lossy().as_ref()));
        assert_eq!(c.size_on_disk, Some(1_048_576));
        assert!(!c.is_tool);

        let h = &games[1];
        assert_eq!(h.launch_label, "itch://install?game_id=2718&launch");
        assert!(h.launcher_tracks_playtime);
        assert_eq!(h.playtime_minutes, Some(90));
        assert_eq!(h.last_played, Some(1_709_208_000));
        assert_eq!(h.install_dir.as_deref(), Some(html.to_string_lossy().as_ref()));

        assert!(games[2].is_tool);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn refuses_paths_that_leave_the_game() {
        let root = temp("chemins");
        let dir = root.join("jeu");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(root.join("dehors.exe"), b"").unwrap();
        let verdict = |path: &str| Verdict {
            base_path: dir.to_string_lossy().into_owned(),
            candidates: vec![Candidate { path: path.into(), flavor: "windows".into(), arch: String::new() }],
        };
        assert!(windows_exe(&dir, &verdict("../dehors.exe")).is_none());
        assert!(windows_exe(&dir, &verdict(&root.join("dehors.exe").to_string_lossy())).is_none());
        assert!(windows_exe(&dir, &verdict("C:dehors.exe")).is_none());
        std::fs::write(dir.join("jeu.bat"), b"").unwrap();
        assert!(windows_exe(&dir, &verdict("jeu.bat")).is_none(), "pas un .exe");

        // Sans basePath, le nom du dossier ne peut pas remonter.
        let cave = |folder: &str| Cave { game_id: 1, location: root.to_string_lossy().into_owned(), folder: folder.into(), ..Cave::default() };
        let empty = Verdict::default();
        assert_eq!(install_dir(&cave("jeu"), &empty), Some(dir.clone()));
        assert!(install_dir(&cave(".."), &empty).is_none());
        assert!(install_dir(&cave(r"jeu\..\.."), &empty).is_none());
        assert!(install_dir(&cave(""), &empty).is_none());
        // Verdict illisible et aucun emplacement : écarté avec sa raison.
        let lost = Cave { game_id: 1, verdict: "pas du json".into(), ..Cave::default() };
        assert_eq!(build(&lost).unwrap_err(), "Dossier d'installation introuvable");
        assert_eq!(build(&Cave::default()).unwrap_err(), "Identifiant itch.io illisible");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn app_without_database_or_with_another_schema() {
        let root = temp("schema");
        assert!(scan_in(&root.join("absente")).unwrap().is_none());
        assert_eq!(scan_in(&root).unwrap().unwrap().len(), 0);
        let db = database(&root);
        std::fs::create_dir_all(db.parent().unwrap()).unwrap();
        Connection::open(&db).unwrap().execute_batch("CREATE TABLE caves (id TEXT);").unwrap();
        assert!(scan_in(&root).unwrap_err().starts_with("Base de l'appli itch illisible"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
