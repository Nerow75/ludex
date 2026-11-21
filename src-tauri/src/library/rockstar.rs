//! Rockstar Games Launcher : une clé par jeu sous
//! `HKLM\SOFTWARE\WOW6432Node\Rockstar Games\<titre>` avec son `InstallFolder`.
//! On lance l'exécutable « Play… » du jeu, qui passe lui-même par le launcher.

use super::{Game, Launch, Platform};
use std::fs;
use std::path::Path;

/// Clés qui ne sont pas des jeux.
const NOT_GAMES: &[&str] = &["Launcher", "Rockstar Games Launcher", "Rockstar Games Social Club", "Social Club"];

/// Jeux connus : exécutable de lancement et appid Steam (pour les images du CDN).
fn known(title: &str) -> Option<(&'static str, &'static str)> {
    Some(match title {
        "Grand Theft Auto V" => ("PlayGTAV.exe", "271590"),
        "Grand Theft Auto V Enhanced" => ("PlayGTAV.exe", "3240220"),
        "Red Dead Redemption 2" => ("PlayRDR2.exe", "1174180"),
        "Red Dead Redemption" => ("PlayRDR.exe", "2668510"),
        "L.A. Noire" => ("LANoire.exe", "110800"),
        "Max Payne 3" => ("MaxPayne3.exe", "204100"),
        "Bully Scholarship Edition" => ("Bully.exe", "12200"),
        _ => return None,
    })
}

/// Exécutable à lancer : celui de la table, sinon le premier `Play*.exe` du dossier.
fn launcher_exe(dir: &Path, title: &str) -> Option<std::path::PathBuf> {
    if let Some((exe, _)) = known(title) {
        let path = dir.join(exe);
        if path.is_file() {
            return Some(path);
        }
    }
    let mut candidates: Vec<_> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.len() > 8 && n[..4].eq_ignore_ascii_case("play") && n.to_ascii_lowercase().ends_with(".exe"))
        })
        .collect();
    candidates.sort();
    candidates.into_iter().next()
}

fn build(title: &str, dir: &Path) -> Option<Game> {
    if NOT_GAMES.iter().any(|n| n.eq_ignore_ascii_case(title)) || !dir.is_dir() {
        return None;
    }
    let exe = launcher_exe(dir, title)?;
    let id = title.to_lowercase().replace(|c: char| !c.is_ascii_alphanumeric(), "-");
    let mut game = Game::new(Platform::Rockstar, &id, title, Launch::Exec { path: exe, args: Vec::new() });
    game.install_dir = Some(dir.to_string_lossy().into_owned());
    if let Some((_, appid)) = known(title) {
        let cdn = |file: &str| format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{appid}/{file}");
        game.cover_url = Some(cdn("library_600x900_2x.jpg"));
        game.hero_url = Some(cdn("library_hero.jpg"));
        game.logo_url = Some(cdn("logo.png"));
        game.steam_appid = appid.parse().ok();
    }
    Some(game)
}

pub fn scan() -> Result<Option<Vec<Game>>, String> {
    #[cfg(windows)]
    {
        use super::reg;
        let Some(root) = reg::hklm(r"SOFTWARE\WOW6432Node\Rockstar Games") else { return Ok(None) };
        let games = reg::children(&root)
            .into_iter()
            .filter_map(|(title, key)| {
                let dir = reg::string(&key, "InstallFolder")?;
                let game = build(&title, Path::new(&dir));
                if game.is_none() && !NOT_GAMES.iter().any(|n| n.eq_ignore_ascii_case(&title)) {
                    let reason = if Path::new(&dir).is_dir() {
                        tr_str!("Exécutable de lancement (Play…) introuvable", "Launch executable (Play…) not found")
                    } else {
                        tr_str!("Dossier d'installation introuvable", "Install folder not found")
                    };
                    super::skip(&title, reason);
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
    fn builds_known_and_unknown_titles() {
        let root = std::env::temp_dir().join(format!("ludex-rockstar-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let gta = root.join("gta");
        let other = root.join("other");
        fs::create_dir_all(&gta).unwrap();
        fs::create_dir_all(&other).unwrap();
        fs::write(gta.join("PlayGTAV.exe"), b"").unwrap();
        fs::write(other.join("PlayOther.exe"), b"").unwrap();

        let game = build("Grand Theft Auto V", &gta).unwrap();
        assert_eq!(game.id, "rockstar:grand-theft-auto-v");
        assert!(game.launch_label.starts_with("PlayGTAV.exe"));
        assert!(game.cover_url.unwrap().contains("/271590/"));
        assert_eq!(game.steam_appid, Some(271590));

        let unknown = build("Some Game", &other).unwrap();
        assert!(unknown.launch_label.starts_with("PlayOther.exe") && unknown.cover_url.is_none() && unknown.steam_appid.is_none());

        assert!(build("Launcher", &gta).is_none());
        assert!(build("Grand Theft Auto V", &root.join("absent")).is_none());
        let _ = fs::remove_dir_all(&root);
    }
}
