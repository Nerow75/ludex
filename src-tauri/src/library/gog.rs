//! GOG : une clé par jeu sous `HKLM\SOFTWARE\WOW6432Node\GOG.com\Games\<gameID>`
//! (`gameName`, `path`, `exe`, `launchParam`). Les DLC portent `dependsOn`.
//! Lancement par GOG Galaxy s'il est installé, sinon par l'exécutable du jeu.

use super::{Game, Launch, Platform};
use std::path::{Path, PathBuf};

struct Entry {
    id: String,
    name: String,
    path: String,
    exe: String,
    params: String,
    depends_on: String,
}

fn build(e: &Entry, galaxy: Option<&Path>) -> Option<Game> {
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
                build(&entry, galaxy.as_deref())
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

        let direct = build(&entry, None).unwrap();
        assert_eq!(direct.id, "gog:1207664663");
        assert_eq!(direct.launch_label, "witcher3.exe -skipintro");

        let galaxy = build(&entry, Some(Path::new(r"C:\GOG Galaxy\GalaxyClient.exe"))).unwrap();
        assert!(galaxy.launch_label.starts_with("GalaxyClient.exe /command=runGame /gameId=1207664663"));

        let dlc = Entry { depends_on: "1207664663".into(), ..entry };
        assert!(build(&dlc, None).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
