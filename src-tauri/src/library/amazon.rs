//! Amazon Games : base SQLite de l'appli,
//! `%LOCALAPPDATA%\Amazon Games\Data\Games\Sql\GameInstallInfo.sqlite`, table `DbSet`
//! (`Id`, `ProductTitle`, `InstallDirectory`, `Installed`), lue en lecture seule.
//! Lancement par l'appli : `amazon-games://play/<Id>` (id vérifié avant de construire l'URI).
//! Aucun temps de jeu local : Ludex mesure les sessions.

use super::{Game, Launch, Platform};
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Dossier de l'appli Amazon Games (`%LOCALAPPDATA%\Amazon Games`).
pub(super) fn root_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_default().join("Amazon Games")
}

/// Base des jeux installés, dans le dossier de l'appli.
pub(super) fn database(root: &Path) -> PathBuf {
    root.join(r"Data\Games\Sql\GameInstallInfo.sqlite")
}

/// Une ligne de `DbSet`.
#[derive(Debug, Default)]
struct Row {
    id: String,
    title: String,
    dir: String,
    installed: bool,
}

/// Lit `DbSet` en lecture seule. Schéma différent (autre version de l'appli) : erreur, jamais de plantage.
fn read(db: &Path) -> rusqlite::Result<Vec<Row>> {
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    // L'appli peut écrire au même moment : on attend un peu plutôt que d'échouer.
    conn.busy_timeout(Duration::from_millis(500))?;
    let mut stmt = conn.prepare("SELECT Id, ProductTitle, InstallDirectory, Installed FROM DbSet")?;
    let rows = stmt.query_map([], |r| {
        Ok(Row {
            id: r.get::<_, Option<String>>(0)?.unwrap_or_default().trim().to_string(),
            title: r.get::<_, Option<String>>(1)?.unwrap_or_default().trim().to_string(),
            dir: r.get::<_, Option<String>>(2)?.unwrap_or_default().trim().to_string(),
            installed: r.get::<_, Option<i64>>(3)?.unwrap_or(0) != 0,
        })
    })?;
    rows.collect()
}

/// Identifiant utilisable dans l'URI (`amzn1.adg.product.<uuid>`) : lettres, chiffres, `.`, `-`, `_`.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

/// Jeu tiré d'une ligne, ou la raison de l'écarter (page de diagnostic).
fn build(row: &Row) -> Result<Game, &'static str> {
    if !row.installed {
        return Err(tr_str!("Non installé (encore listé par l'appli Amazon Games)", "Not installed (still listed by the Amazon Games app)"));
    }
    if !valid_id(&row.id) {
        return Err(tr_str!("Identifiant Amazon illisible", "Unreadable Amazon ID"));
    }
    let dir = row.dir.replace('/', "\\");
    let dir = dir.trim_end_matches('\\');
    if dir.is_empty() || !Path::new(dir).is_dir() {
        return Err(tr_str!("Dossier d'installation introuvable", "Install folder not found"));
    }
    let title = if row.title.is_empty() {
        Path::new(dir).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| row.id.clone())
    } else {
        row.title.clone()
    };
    let mut game = Game::new(Platform::Amazon, &row.id, &title, Launch::Uri(format!("amazon-games://play/{}", row.id)));
    game.install_dir = Some(dir.to_string());
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
    let rows = read(&db).map_err(|e| tr!("Base des jeux d'Amazon Games illisible : {e}", "Can't read the Amazon Games database: {e}"))?;
    let games = rows
        .iter()
        .filter_map(|row| match build(row) {
            Ok(game) => Some(game),
            Err(reason) => {
                let title = if row.title.is_empty() { tr!("Jeu Amazon {}", "Amazon game {}", row.id) } else { row.title.clone() };
                super::skip(&title, reason);
                None
            }
        })
        .collect();
    Ok(Some(games))
}

pub fn scan() -> Result<Option<Vec<Game>>, String> {
    scan_in(&root_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ludex-amazon-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Fausse base au format de l'appli (colonnes en plus comprises).
    fn fake_db(root: &Path, rows: &[(&str, &str, &str, i64)]) {
        let db = database(root);
        std::fs::create_dir_all(db.parent().unwrap()).unwrap();
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE DbSet (Id TEXT PRIMARY KEY, InstallDirectory TEXT, Installed INTEGER, ProductAsin TEXT,
                                 ProductTitle TEXT, ProductSku TEXT, InstallDate TEXT);",
        )
        .unwrap();
        for (id, title, dir, installed) in rows {
            conn.execute(
                "INSERT INTO DbSet (Id, InstallDirectory, Installed, ProductAsin, ProductTitle) VALUES (?1, ?2, ?3, 'B0', ?4)",
                rusqlite::params![id, dir, installed, title],
            )
            .unwrap();
        }
    }

    #[test]
    fn reads_installed_games() {
        let root = temp("base");
        let games_dir = root.join("Jeux");
        let quake = games_dir.join("Quake");
        std::fs::create_dir_all(&quake).unwrap();
        let fallout = games_dir.join("Fallout 3");
        std::fs::create_dir_all(&fallout).unwrap();
        let id = "amzn1.adg.product.9c0a5e2f-4b1d-4d7e-a0c8-1f2e3d4c5b6a";
        let raw_quake = format!("{}/", quake.to_string_lossy().replace('\\', "/"));
        fake_db(
            &root,
            &[
                (id, "Quake", &raw_quake, 1),
                ("amzn1.adg.product.sans-titre", "", &fallout.to_string_lossy(), 1),
                ("amzn1.adg.product.desinstalle", "Ancien jeu", &quake.to_string_lossy(), 0),
                ("amzn1.adg.product.disparu", "Disparu", &games_dir.join("Absent").to_string_lossy(), 1),
                ("x/../../calc", "Piège", &quake.to_string_lossy(), 1),
            ],
        );

        let games = scan_in(&root).unwrap().unwrap();
        assert_eq!(games.len(), 2);
        let q = &games[0];
        assert_eq!(q.id, format!("amazon:{id}"));
        assert_eq!(q.title, "Quake");
        assert_eq!(q.launch_label, format!("amazon-games://play/{id}"));
        assert_eq!(q.install_dir.as_deref(), Some(quake.to_string_lossy().as_ref()));
        // Personne d'autre ne compte : Ludex mesurera.
        assert!(!q.launcher_tracks_playtime && q.playtime_minutes.is_none());
        // Titre vide : nom du dossier.
        assert_eq!(games[1].title, "Fallout 3");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn notes_why_games_are_skipped() {
        let dir = temp("raisons");
        let row = |id: &str, d: &Path, installed: bool| Row { id: id.into(), title: "Jeu".into(), dir: d.to_string_lossy().into_owned(), installed };
        assert_eq!(build(&row("amzn1.a", &dir, false)).unwrap_err(), "Non installé (encore listé par l'appli Amazon Games)");
        assert_eq!(build(&row("a b", &dir, true)).unwrap_err(), "Identifiant Amazon illisible");
        assert_eq!(build(&row("a?x=1", &dir, true)).unwrap_err(), "Identifiant Amazon illisible");
        assert_eq!(build(&row("", &dir, true)).unwrap_err(), "Identifiant Amazon illisible");
        assert_eq!(build(&row(&"a".repeat(129), &dir, true)).unwrap_err(), "Identifiant Amazon illisible");
        assert_eq!(build(&row("amzn1.a", &dir.join("non"), true)).unwrap_err(), "Dossier d'installation introuvable");
        assert!(build(&row("amzn1.a", &dir, true)).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn app_without_database_or_with_another_schema() {
        let root = temp("schema");
        // Appli absente.
        assert!(scan_in(&root.join("absente")).unwrap().is_none());
        // Appli installée, aucune base encore : installée sans jeu.
        assert_eq!(scan_in(&root).unwrap().unwrap().len(), 0);
        // Base d'une autre version : erreur lisible.
        let db = database(&root);
        std::fs::create_dir_all(db.parent().unwrap()).unwrap();
        Connection::open(&db).unwrap().execute_batch("CREATE TABLE Autre (x INTEGER);").unwrap();
        assert!(scan_in(&root).unwrap_err().starts_with("Base des jeux d'Amazon Games illisible"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
