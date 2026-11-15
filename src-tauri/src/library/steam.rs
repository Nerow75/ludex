//! Steam : bibliothèques listées dans `steamapps/libraryfolders.vdf`,
//! un `appmanifest_<appid>.acf` par jeu installé.

use super::{art_url, vdf, Game, LocalArt, Launch, Platform};
use std::collections::{HashMap, HashSet};
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
    LocalArt { cover: pick(COVER_FILES), hero: pick(HERO_FILES), logo: pick(LOGO_FILES), icon: None }
}

fn steam_root() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
        use winreg::RegKey;
        if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Software\Valve\Steam")
            && let Ok(path) = key.get_value::<String, _>("SteamPath")
        {
            let p = PathBuf::from(path.replace('/', "\\"));
            if p.exists() {
                return Some(p);
            }
        }
        if let Ok(key) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\WOW6432Node\Valve\Steam")
            && let Ok(path) = key.get_value::<String, _>("InstallPath")
        {
            let p = PathBuf::from(path);
            if p.exists() {
                return Some(p);
            }
        }
    }
    let default = PathBuf::from(r"C:\Program Files (x86)\Steam");
    default.exists().then_some(default)
}

/// Écart entre un SteamID64 et le numéro de compte (steamid3) qui nomme le dossier `userdata`.
const STEAMID64_BASE: u64 = 76_561_197_960_265_728;

/// Comptes de `config/loginusers.vdf`, du plus probable au moins probable : `MostRecent` = 1
/// d'abord, puis le `timestamp` de dernière connexion le plus récent.
fn account_ids(loginusers: &vdf::Vdf) -> Vec<u64> {
    let Some(users) = loginusers.get("users") else { return Vec::new() };
    let mut accounts: Vec<(bool, u64, u64)> = users
        .entries()
        .iter()
        .filter_map(|(id64, user)| {
            let id3 = id64.parse::<u64>().ok()?.checked_sub(STEAMID64_BASE)?;
            let most_recent = user.str("MostRecent") == Some("1");
            let timestamp = user.str("Timestamp").and_then(|t| t.parse().ok()).unwrap_or(0);
            Some((most_recent, timestamp, id3))
        })
        .collect();
    accounts.sort_by(|a, b| b.cmp(a));
    accounts.into_iter().map(|(_, _, id3)| id3).collect()
}

/// `localconfig.vdf` du compte utilisé sur ce PC. Repli si `loginusers.vdf` ne désigne aucun
/// dossier existant : le `localconfig.vdf` modifié le plus récemment dans `userdata`.
fn localconfig_path(root: &Path) -> Option<PathBuf> {
    let userdata = root.join("userdata");
    let file = |id: &str| userdata.join(id).join("config").join("localconfig.vdf");
    if let Ok(text) = fs::read_to_string(root.join("config").join("loginusers.vdf"))
        && let Some(path) = account_ids(&vdf::parse(&text)).into_iter().map(|id| file(&id.to_string())).find(|p| p.is_file())
    {
        return Some(path);
    }
    fs::read_dir(&userdata)
        .ok()?
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| name != "0" && name.bytes().all(|b| b.is_ascii_digit()))
        .map(|name| file(&name))
        .filter_map(|p| Some((fs::metadata(&p).ok()?.modified().ok()?, p)))
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, p)| p)
}

/// Temps de jeu d'une application dans `localconfig.vdf`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppTimes {
    pub minutes: u64,
    pub last_played: Option<u64>,
}

/// `UserLocalConfigStore > Software > Valve > Steam > apps > <appid>` : `Playtime` en minutes,
/// `LastPlayed` en secondes Unix. Steam y garde toutes les applications du compte, installées ou non.
fn parse_playtimes(doc: &vdf::Vdf) -> HashMap<String, AppTimes> {
    let apps = ["UserLocalConfigStore", "Software", "Valve", "Steam", "apps"]
        .iter()
        .try_fold(doc, |node, key| node.get(key));
    let Some(apps) = apps else { return HashMap::new() };
    apps.entries()
        .iter()
        .filter(|(appid, _)| appid.bytes().all(|b| b.is_ascii_digit()))
        .map(|(appid, app)| {
            let minutes = app.str("Playtime").and_then(|v| v.parse().ok()).unwrap_or(0);
            (appid.clone(), AppTimes { minutes, last_played: number(app, "LastPlayed") })
        })
        .collect()
}

/// Temps de jeu du compte Steam de ce PC, `None` si le fichier est introuvable ou illisible.
fn playtimes(root: &Path) -> Option<HashMap<String, AppTimes>> {
    let text = fs::read_to_string(localconfig_path(root)?).ok()?;
    Some(parse_playtimes(&vdf::parse(&text)))
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
    let times = playtimes(&root);

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
            // Steam compte toutes les sessions, même lancées hors de Ludex : son chiffre fait foi.
            // Une application absente de localconfig.vdf n'a jamais été lancée.
            game.launcher_tracks_playtime = true;
            if let Some(times) = &times {
                let t = times.get(appid).copied().unwrap_or(AppTimes { minutes: 0, last_played: None });
                game.set_launcher_playtime(t.minutes, t.last_played);
            }
            game.is_tool = is_tool(appid, title);
            game.steam_appid = appid.parse().ok();
            games.push(game);
        }
    }
    Ok(Some(games))
}

#[cfg(test)]
mod tests {
    use super::{account_ids, is_tool, local_art, localconfig_path, parse_playtimes, playtimes, AppTimes};
    use crate::library::vdf;
    use std::fs;

    #[test]
    fn orders_accounts_most_recent_then_timestamp() {
        // Steam récent : plus de MostRecent, seulement le timestamp de connexion.
        let recent = vdf::parse(
            r#""users" {
                "76561198013136374" { "AccountName" "a" "timestamp" "1794750893" }
                "76561199053483185" { "AccountName" "b" "timestamp" "1649598892" }
                "pas-un-id" { "timestamp" "1999999999" }
            }"#,
        );
        assert_eq!(account_ids(&recent), vec![52_870_646, 1_093_217_457]);

        // Ancien format : MostRecent l'emporte sur un timestamp plus récent.
        let old = vdf::parse(
            r#""users" {
                "76561198013136374" { "MostRecent" "0" "Timestamp" "1794750893" }
                "76561199053483185" { "MostRecent" "1" "Timestamp" "1649598892" }
            }"#,
        );
        assert_eq!(account_ids(&old), vec![1_093_217_457, 52_870_646]);
        assert!(account_ids(&vdf::parse("")).is_empty());
    }

    #[test]
    fn reads_playtime_from_localconfig() {
        let doc = vdf::parse(
            r#""UserLocalConfigStore"
            {
                "apps" { "730" { "Playtime" "999999" } }
                "Software" { "Valve" { "steam" { "apps" {
                    "730" { "LastPlayed" "1765316763" "Playtime" "6230" "PlaytimeDisconnected" "26" }
                    "440" { "LastPlayed" "1464464274" }
                    "Playtime" "pas une appli"
                } } } }
            }"#,
        );
        let times = parse_playtimes(&doc);
        assert_eq!(times.get("730"), Some(&AppTimes { minutes: 6230, last_played: Some(1_765_316_763) }));
        assert_eq!(times.get("440"), Some(&AppTimes { minutes: 0, last_played: Some(1_464_464_274) }));
        assert_eq!(times.len(), 2);
        assert!(parse_playtimes(&vdf::parse(r#""UserLocalConfigStore" { }"#)).is_empty());
    }

    #[test]
    fn picks_account_folder_with_fallback() {
        let root = std::env::temp_dir().join(format!("ludex-steam-users-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let config = |id: &str| root.join("userdata").join(id).join("config");
        for id in ["0", "52870646", "1093217457"] {
            fs::create_dir_all(config(id)).unwrap();
        }
        let local = r#""UserLocalConfigStore" { "Software" { "Valve" { "Steam" { "apps" { "10" { "Playtime" "90" } } } } } }"#;
        fs::write(config("1093217457").join("localconfig.vdf"), local).unwrap();
        fs::write(config("52870646").join("localconfig.vdf"), local.replace("90", "15")).unwrap();

        // Sans loginusers.vdf : le localconfig.vdf le plus récent.
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        fs::File::options().write(true).open(config("52870646").join("localconfig.vdf")).unwrap().set_modified(old).unwrap();
        assert!(localconfig_path(&root).unwrap().starts_with(root.join("userdata").join("1093217457")));

        // loginusers.vdf désigne le compte, même si un autre dossier est plus récent.
        fs::create_dir_all(root.join("config")).unwrap();
        fs::write(root.join("config").join("loginusers.vdf"), r#""users" { "76561198013136374" { "MostRecent" "1" } }"#).unwrap();
        assert!(localconfig_path(&root).unwrap().starts_with(root.join("userdata").join("52870646")));
        assert_eq!(playtimes(&root).unwrap().get("10").map(|t| t.minutes), Some(15));

        // Compte désigné sans dossier : repli sur les dossiers existants.
        fs::write(root.join("config").join("loginusers.vdf"), r#""users" { "76561197960265729" { "MostRecent" "1" } }"#).unwrap();
        assert!(localconfig_path(&root).is_some());
        let _ = fs::remove_dir_all(&root);
    }

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
