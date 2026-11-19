//! Ubisoft Connect : une clé par jeu sous
//! `HKLM\SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs\<id>` avec son `InstallDir`.
//! Le titre vient de la désinstallation Windows (« Uplay Install <id> »).

use super::{Game, Launch, Platform};
use std::path::Path;

fn build(id: &str, dir: &str, display_name: Option<String>) -> Option<Game> {
    let dir = dir.replace('/', "\\");
    let dir = dir.trim_end_matches('\\');
    if dir.is_empty() || !Path::new(dir).is_dir() {
        return None; // Clé laissée par un jeu désinstallé.
    }
    let title = display_name.unwrap_or_else(|| {
        Path::new(dir).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| id.to_string())
    });
    let mut game = Game::new(Platform::Ubisoft, id, &title, Launch::Uri(format!("uplay://launch/{id}/0")));
    game.install_dir = Some(dir.to_string());
    Some(game)
}

pub fn scan() -> Result<Option<Vec<Game>>, String> {
    #[cfg(windows)]
    {
        use super::reg;
        if reg::hklm(r"SOFTWARE\WOW6432Node\Ubisoft\Launcher").is_none() {
            return Ok(None);
        }
        let Some(installs) = reg::hklm(r"SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs") else {
            return Ok(Some(Vec::new()));
        };
        let uninstall = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";
        let games = reg::children(&installs)
            .into_iter()
            .filter_map(|(id, key)| {
                let dir = reg::string(&key, "InstallDir")?;
                let name = reg::hklm(&format!(r"{uninstall}\Uplay Install {id}")).and_then(|k| reg::string(&k, "DisplayName"));
                let title = name.clone().unwrap_or_else(|| format!("Jeu Ubisoft {id}"));
                let game = build(&id, &dir, name);
                if game.is_none() {
                    super::skip(&title, "Dossier d'installation introuvable (jeu désinstallé)");
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
    fn builds_from_install_key() {
        let dir = std::env::temp_dir().join(format!("ludex-ubi-{}", std::process::id())).join("Far Cry 6");
        std::fs::create_dir_all(&dir).unwrap();
        let raw = format!("{}/", dir.to_string_lossy().replace('\\', "/"));

        let game = build("5266", &raw, Some("Far Cry® 6".into())).unwrap();
        assert_eq!(game.id, "ubisoft:5266");
        assert_eq!(game.title, "Far Cry® 6");
        assert_eq!(game.launch_label, "uplay://launch/5266/0");
        assert!(!game.install_dir.unwrap().ends_with('\\'));

        assert_eq!(build("5266", &raw, None).unwrap().title, "Far Cry 6");
        assert!(build("856", "", None).is_none());
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }
}
