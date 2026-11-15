//! Steam : bibliothèques listées dans `steamapps/libraryfolders.vdf`,
//! un `appmanifest_<appid>.acf` par jeu installé.

use super::{art_url, vdf, Game, LocalArt, Launch, Platform};
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

/// Noms de fichiers du cache d'images de Steam, du meilleur au moins bon.
const COVER_FILES: &[&str] = &["library_600x900_2x.jpg", "library_600x900.jpg", "library_capsule.jpg"];
const HERO_FILES: &[&str] = &["library_hero.jpg"];
const LOGO_FILES: &[&str] = &["logo.png"];

/// Images d'un jeu dans `appcache/librarycache/<appid>/`. Steam les range soit à plat,
/// soit (jeux récents) dans des sous-dossiers nommés par un hash : on regarde les deux.
fn local_art(cache: &Path, appid: &str) -> LocalArt {
    let dir = cache.join(appid);
    let mut files: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Ok(sub) = fs::read_dir(&path) {
                    files.extend(sub.flatten().map(|e| e.path()).filter(|p| p.is_file()));
                }
            } else {
                files.push(path);
            }
        }
    }
    let pick = |names: &[&str]| -> Option<PathBuf> {
        names.iter().find_map(|name| {
            // Plusieurs versions possibles d'une même image : la plus récente gagne.
            files
                .iter()
                .filter(|p| p.file_name().is_some_and(|n| n.eq_ignore_ascii_case(name)))
                .filter(|p| fs::metadata(p).is_ok_and(|m| m.len() > 0))
                .max_by_key(|p| fs::metadata(p).and_then(|m| m.modified()).ok())
                .cloned()
        })
    };
    LocalArt { cover: pick(COVER_FILES), hero: pick(HERO_FILES), logo: pick(LOGO_FILES) }
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
    let art_cache = root.join("appcache").join("librarycache");

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
            // Cache local de Steam d'abord (hors ligne, et seule source pour les jeux récents
            // dont les images ont une URL hachée sur le CDN), sinon l'URL classique du CDN.
            let art = local_art(&art_cache, appid);
            let cdn = |file: &str| format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{appid}/{file}");
            game.cover_url = Some(if art.cover.is_some() { art_url(&game, "cover") } else { cdn("library_600x900_2x.jpg") });
            game.hero_url = Some(if art.hero.is_some() { art_url(&game, "hero") } else { cdn("library_hero.jpg") });
            game.logo_url = Some(if art.logo.is_some() { art_url(&game, "logo") } else { cdn("logo.png") });
            game.local_art = art;
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
    use super::{is_tool, local_art};
    use std::fs;

    #[test]
    fn finds_local_art_flat_and_hashed() {
        let cache = std::env::temp_dir().join(format!("ludex-art-{}", std::process::id()));
        let _ = fs::remove_dir_all(&cache);
        // Ancien rangement : fichiers à plat.
        fs::create_dir_all(cache.join("10")).unwrap();
        fs::write(cache.join("10").join("library_600x900.jpg"), b"x").unwrap();
        fs::write(cache.join("10").join("logo.png"), b"x").unwrap();
        // Rangement récent : sous-dossiers nommés par un hash, jaquette appelée library_capsule.
        fs::create_dir_all(cache.join("20").join("abc")).unwrap();
        fs::create_dir_all(cache.join("20").join("def")).unwrap();
        fs::write(cache.join("20").join("abc").join("library_capsule.jpg"), b"x").unwrap();
        fs::write(cache.join("20").join("def").join("library_hero.jpg"), b"x").unwrap();
        fs::write(cache.join("20").join("def").join("logo.png"), b"").unwrap(); // vide : ignoré

        let flat = local_art(&cache, "10");
        assert!(flat.cover.as_ref().unwrap().ends_with("10/library_600x900.jpg"));
        assert!(flat.logo.is_some() && flat.hero.is_none());

        let hashed = local_art(&cache, "20");
        assert!(hashed.cover.as_ref().unwrap().ends_with("abc/library_capsule.jpg"));
        assert!(hashed.hero.as_ref().unwrap().ends_with("def/library_hero.jpg"));
        assert!(hashed.logo.is_none());

        assert!(local_art(&cache, "30").cover.is_none());
        let _ = fs::remove_dir_all(&cache);
    }

    #[test]
    fn detects_tools() {
        assert!(is_tool("431960", "Wallpaper Engine"));
        assert!(is_tool("1624110", "Football Manager 2022 Editor"));
        assert!(!is_tool("730", "Counter-Strike 2"));
    }
}
