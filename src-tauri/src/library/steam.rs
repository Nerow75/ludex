//! Steam : bibliothèques listées dans `steamapps/libraryfolders.vdf`,
//! un `appmanifest_<appid>.acf` par jeu installé.

use super::{vdf, Game, Launch, Platform};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Outils Steam qui ne sont pas des jeux.
const EXCLUDED_APPIDS: &[&str] = &["228980", "1070560", "1391110", "1628350", "1826330", "250820"];
const EXCLUDED_PREFIXES: &[&str] = &["Proton", "Steam Linux Runtime", "Steamworks Common"];

/// Applications installées via Steam qui ne sont pas des jeux : gardées, mais masquées par défaut.
const TOOL_APPIDS: &[&str] = &["431960"]; // Wallpaper Engine
const TOOL_SUFFIXES: &[&str] = &[" Editor", " Dedicated Server", " SDK", " Soundtrack"];

fn is_tool(appid: &str, title: &str) -> bool {
    TOOL_APPIDS.contains(&appid) || TOOL_SUFFIXES.iter().any(|s| title.ends_with(s))
}

/// Champ numérique d'un manifeste, `None` si absent ou nul.
fn number(app: &vdf::Vdf, key: &str) -> Option<u64> {
    app.str(key).and_then(|v| v.parse().ok()).filter(|&n| n > 0)
}

fn steam_root() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
        use winreg::RegKey;
        if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Software\Valve\Steam") {
            if let Ok(path) = key.get_value::<String, _>("SteamPath") {
                let p = PathBuf::from(path.replace('/', "\\"));
                if p.exists() {
                    return Some(p);
                }
            }
        }
        if let Ok(key) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\WOW6432Node\Valve\Steam") {
            if let Ok(path) = key.get_value::<String, _>("InstallPath") {
                let p = PathBuf::from(path);
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }
    let default = PathBuf::from(r"C:\Program Files (x86)\Steam");
    default.exists().then_some(default)
}

fn library_paths(root: &Path) -> Vec<PathBuf> {
    let mut paths = vec![root.to_path_buf()];
    let file = root.join("steamapps").join("libraryfolders.vdf");
    if let Ok(text) = fs::read_to_string(file) {
        let doc = vdf::parse(&text);
        if let Some(folders) = doc.get("libraryfolders") {
            for (_, folder) in folders.entries() {
                if let Some(path) = folder.str("path") {
                    let p = PathBuf::from(path);
                    if !paths.iter().any(|known| same_path(known, &p)) {
                        paths.push(p);
                    }
                }
            }
        }
    }
    paths
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase().trim_end_matches('\\') == b.to_string_lossy().to_lowercase().trim_end_matches('\\')
}

pub fn scan() -> Result<Option<Vec<Game>>, String> {
    let Some(root) = steam_root() else { return Ok(None) };
    let mut games = Vec::new();
    let mut seen = HashSet::new();

    for library in library_paths(&root) {
        let steamapps = library.join("steamapps");
        let Ok(entries) = fs::read_dir(&steamapps) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !(name.starts_with("appmanifest_") && name.ends_with(".acf")) {
                continue;
            }
            let Ok(text) = fs::read_to_string(entry.path()) else { continue };
            let doc = vdf::parse(&text);
            let Some(app) = doc.get("AppState") else { continue };
            let (Some(appid), Some(title)) = (app.str("appid"), app.str("name")) else { continue };

            // StateFlags bit 4 = complètement installé.
            let flags: u32 = app.str("StateFlags").and_then(|f| f.parse().ok()).unwrap_or(0);
            if flags & 4 == 0
                || EXCLUDED_APPIDS.contains(&appid)
                || EXCLUDED_PREFIXES.iter().any(|p| title.starts_with(p))
                || !seen.insert(appid.to_string())
            {
                continue;
            }

            let mut game = Game::new(Platform::Steam, appid, title, Launch::Uri(format!("steam://rungameid/{appid}")));
            game.install_dir = app
                .str("installdir")
                .map(|d| steamapps.join("common").join(d).to_string_lossy().into_owned());
            game.cover_url = Some(format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{appid}/library_600x900_2x.jpg"));
            game.hero_url = Some(format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{appid}/library_hero.jpg"));
            game.logo_url = Some(format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{appid}/logo.png"));
            game.last_played = number(app, "LastPlayed");
            game.size_on_disk = number(app, "SizeOnDisk");
            game.is_tool = is_tool(appid, title);
            games.push(game);
        }
    }
    Ok(Some(games))
}

#[cfg(test)]
mod tests {
    use super::is_tool;

    #[test]
    fn detects_tools() {
        assert!(is_tool("431960", "Wallpaper Engine"));
        assert!(is_tool("1624110", "Football Manager 2022 Editor"));
        assert!(!is_tool("730", "Counter-Strike 2"));
    }
}
