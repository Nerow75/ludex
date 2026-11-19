//! Steam : bibliothèques listées dans `steamapps/libraryfolders.vdf`,
//! un `appmanifest_<appid>.acf` par jeu installé.

use super::{achievements, art_url, vdf, Game, InstallState, LocalArt, Launch, Platform};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Bits de `StateFlags` (`EAppState` du client Steam). Valve ne les documente pas : valeurs reprises
/// de SteamKit et de la crate `game-scanner`, où les bits 13 à 15 n'existent pas.
/// 1 désinstallé, 2 mise à jour requise, 4 entièrement installé, 8 chiffré, 16 verrouillé,
/// 32 fichiers manquants, 64 en cours d'exécution, 128 fichiers corrompus, 256 mise à jour en cours,
/// 512 mise à jour en pause, 1024 mise à jour lancée (créée, éventuellement en file d'attente),
/// 2048 désinstallation, 4096 sauvegarde, 65536 reconfiguration, 131072 vérification,
/// 262144 ajout de fichiers, 524288 préallocation, 1048576 téléchargement, 2097152 mise en place,
/// 4194304 validation finale, 8388608 arrêt de la mise à jour.
/// Relevé sur un vrai PC : 4 (prêt) et 6 (mise à jour requise) seulement.
mod flags {
    pub const UNINSTALLED: u32 = 1;
    pub const UPDATE_REQUIRED: u32 = 2;
    pub const FULLY_INSTALLED: u32 = 4;
    pub const FILES_MISSING: u32 = 32;
    pub const FILES_CORRUPT: u32 = 128;
    pub const UPDATE_RUNNING: u32 = 256;
    pub const UPDATE_PAUSED: u32 = 512;
    pub const UPDATE_STARTED: u32 = 1024;
    pub const UNINSTALLING: u32 = 2048;
    pub const RECONFIGURING: u32 = 65_536;
    pub const VALIDATING: u32 = 131_072;
    pub const ADDING_FILES: u32 = 262_144;
    pub const PREALLOCATING: u32 = 524_288;
    pub const DOWNLOADING: u32 = 1_048_576;
    pub const STAGING: u32 = 2_097_152;
    pub const COMMITTING: u32 = 4_194_304;

    /// Steam travaille sur les fichiers du jeu.
    pub const WORKING: u32 =
        UPDATE_RUNNING | RECONFIGURING | VALIDATING | ADDING_FILES | PREALLOCATING | DOWNLOADING | STAGING | COMMITTING;
    /// Une mise à jour existe : lancée, en file, en pause ou en cours.
    pub const JOB: u32 = UPDATE_STARTED | UPDATE_PAUSED | WORKING;
}

/// Jeu à lister : entièrement installé, ou déjà installé une fois (`buildid` connu) et en cours
/// de mise à jour, Steam pouvant retirer le bit « installé » pendant ce temps. Une première
/// installation en cours (`buildid` 0) n'est pas encore jouable : écartée.
fn listed(flags: u32, app: &vdf::Vdf) -> bool {
    if flags & (flags::UNINSTALLED | flags::UNINSTALLING) != 0 {
        return false;
    }
    flags & flags::FULLY_INSTALLED != 0 || (flags & flags::JOB != 0 && number(app, "buildid").is_some())
}

/// Progression du téléchargement en % (`BytesDownloaded` sur `BytesToDownload`), `None` si rien à télécharger.
fn download_progress(app: &vdf::Vdf) -> Option<u8> {
    let total = number(app, "BytesToDownload")?;
    let done = number(app, "BytesDownloaded").unwrap_or(0).min(total);
    u8::try_from(done * 100 / total).ok()
}

/// État lu dans `StateFlags`, du plus actif au plus passif. `TargetBuildID` et `ScheduledAutoUpdate`
/// ne sont pas lus : sur le PC relevé, toute mise à jour en attente porte déjà le bit 2.
fn install_state(flags: u32, app: &vdf::Vdf) -> InstallState {
    if flags & flags::UPDATE_PAUSED != 0 {
        InstallState::Paused { progress: download_progress(app) }
    } else if flags & flags::WORKING != 0 {
        InstallState::Updating { progress: download_progress(app) }
    } else if flags & flags::UPDATE_STARTED != 0 {
        InstallState::Queued
    } else if flags & (flags::FILES_MISSING | flags::FILES_CORRUPT) != 0 {
        InstallState::Incomplete
    } else if flags & flags::UPDATE_REQUIRED != 0 {
        InstallState::UpdateRequired
    } else {
        InstallState::Ready
    }
}

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

/// Dossier `userdata/<compte>` du compte retenu pour le temps de jeu (captures d'écran : `760`).
pub fn account_dir() -> Option<PathBuf> {
    let localconfig = localconfig_path(&steam_root()?)?;
    localconfig.parent()?.parent().map(Path::to_path_buf)
}

/// Numéro de compte (steamid3) du dossier `userdata/<compte>/config/localconfig.vdf`.
fn account_of(localconfig: &Path) -> Option<u64> {
    localconfig.parent()?.parent()?.file_name()?.to_str()?.parse().ok()
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

/// Temps de jeu du compte Steam de ce PC, `None` si le fichier est illisible.
fn playtimes(localconfig: &Path) -> Option<HashMap<String, AppTimes>> {
    let text = fs::read_to_string(localconfig).ok()?;
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

/// `StateFlags` bit 4 = complètement installé (un jeu en téléchargement a déjà son manifeste).
/// Même règle que le scan : un jeu en cours de mise à jour reste listé.
fn is_installed(app: &vdf::Vdf) -> bool {
    let flags: u32 = app.str("StateFlags").and_then(|f| f.parse().ok()).unwrap_or(0);
    listed(flags, app)
}

/// Le manifeste `appmanifest_*.acf` décrit-il un jeu installé ? `None` : illisible (en cours d'écriture).
pub fn manifest_installed(text: &str) -> Option<bool> {
    let doc = vdf::parse(text);
    let app = doc.get("AppState")?;
    app.str("StateFlags")?;
    Some(is_installed(app))
}

/// Dossiers `steamapps` de toutes les bibliothèques Steam (là où Steam écrit ses manifestes).
pub fn steamapps_dirs() -> Vec<PathBuf> {
    let Some(root) = steam_root() else { return Vec::new() };
    library_paths(&root).into_iter().map(|l| l.join("steamapps")).filter(|d| d.is_dir()).collect()
}

pub fn scan() -> Result<Option<Vec<Game>>, String> {
    let Some(root) = steam_root() else { return Ok(None) };
    let mut games = Vec::new();
    let mut seen = HashSet::new();
    let art_cache = root.join("appcache").join("librarycache");
    let localconfig = localconfig_path(&root);
    let times = localconfig.as_deref().and_then(playtimes);
    // Même compte pour les succès : ses fichiers sont nommés par son steamid3.
    let account = localconfig.as_deref().and_then(account_of);
    let stats_dir = root.join("appcache").join("stats");

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

            let flags: u32 = app.str("StateFlags").and_then(|f| f.parse().ok()).unwrap_or(0);
            if !listed(flags, app)
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
            game.install_state = install_state(flags, app);
            // Steam compte toutes les sessions, même lancées hors de Ludex : son chiffre fait foi.
            // Une application absente de localconfig.vdf n'a jamais été lancée.
            game.launcher_tracks_playtime = true;
            if let Some(times) = &times {
                let t = times.get(appid).copied().unwrap_or(AppTimes { minutes: 0, last_played: None });
                game.set_launcher_playtime(t.minutes, t.last_played);
            }
            game.achievements = account.and_then(|a| achievements::read(&stats_dir, a, appid));
            game.is_tool = is_tool(appid, title);
            game.steam_appid = appid.parse().ok();
            games.push(game);
        }
    }
    Ok(Some(games))
}

#[cfg(test)]
mod tests {
    use super::{
        account_ids, account_of, install_state, is_tool, listed, local_art, localconfig_path, parse_playtimes, playtimes, AppTimes,
    };
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
        let path = localconfig_path(&root).unwrap();
        assert!(path.starts_with(root.join("userdata").join("52870646")));
        assert_eq!(account_of(&path), Some(52_870_646));
        assert_eq!(playtimes(&path).unwrap().get("10").map(|t| t.minutes), Some(15));

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

    /// Coût des succès dans le scan de ce PC : `cargo test achievements_this_pc -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn achievements_this_pc() {
        let start = std::time::Instant::now();
        let games = super::scan().unwrap().unwrap_or_default();
        println!("Scan Steam complet : {:?} ({} jeux)", start.elapsed(), games.len());
        let root = super::steam_root().unwrap();
        let account = localconfig_path(&root).as_deref().and_then(account_of).unwrap();
        let stats = root.join("appcache").join("stats");
        let start = std::time::Instant::now();
        let found = games.iter().filter_map(|g| crate::library::achievements::read(&stats, account, &g.platform_id)).count();
        println!("Dont succès : {:?} ({found} jeux avec succès, compte {account})", start.elapsed());
    }

    /// Manifeste réduit aux champs lus pour l'état d'installation.
    fn manifest(flags: u32, buildid: u64, to_download: u64, downloaded: u64) -> (u32, vdf::Vdf) {
        let text = format!(
            r#""AppState" {{ "appid" "730" "StateFlags" "{flags}" "buildid" "{buildid}" "TargetBuildID" "26393695"
                "BytesToDownload" "{to_download}" "BytesDownloaded" "{downloaded}" "UpdateResult" "0" }}"#
        );
        (flags, vdf::parse(&text).get("AppState").unwrap().clone())
    }

    #[test]
    fn reads_install_state_from_flags() {
        use crate::library::InstallState::*;
        let state = |(flags, app): (u32, vdf::Vdf)| install_state(flags, &app);
        // Valeurs relevées sur un vrai PC : 4 prêt, 6 mise à jour requise (taille connue, rien téléchargé).
        assert_eq!(state(manifest(4, 26334290, 0, 0)), Ready);
        assert_eq!(state(manifest(6, 26334290, 96837264, 0)), UpdateRequired);
        // Mise à jour lancée en file (1024), téléchargement (1048576 + 256), pause (512).
        assert_eq!(state(manifest(1030, 26334290, 96837264, 0)), Queued);
        assert_eq!(state(manifest(4 | 2 | 1024 | 256 | 1_048_576, 26334290, 200, 84)), Updating { progress: Some(42) });
        assert_eq!(state(manifest(2 | 1024 | 2_097_152, 26334290, 0, 0)), Updating { progress: None });
        assert_eq!(state(manifest(4 | 2 | 1024 | 512, 26334290, 1000, 999)), Paused { progress: Some(99) });
        // Plus téléchargé qu'annoncé : 100 % au plus.
        assert_eq!(state(manifest(2 | 256, 1, 10, 50)), Updating { progress: Some(100) });
        assert_eq!(state(manifest(4 | 32, 26334290, 0, 0)), Incomplete);
        assert_eq!(state(manifest(4 | 128, 26334290, 0, 0)), Incomplete);
    }

    #[test]
    fn lists_installed_or_updating_games_only() {
        let listed = |(flags, app): (u32, vdf::Vdf)| listed(flags, &app);
        assert!(listed(manifest(4, 26334290, 0, 0)));
        assert!(listed(manifest(6, 26334290, 96837264, 0)));
        // Mise à jour sans le bit « installé » : jeu déjà installé une fois, gardé.
        assert!(listed(manifest(2 | 1024 | 1_048_576, 26334290, 100, 10)));
        // Première installation en cours (buildid 0) : pas encore jouable.
        assert!(!listed(manifest(1026, 0, 100, 10)));
        assert!(!listed(manifest(2, 26334290, 0, 0)));
        assert!(!listed(manifest(4 | 2048, 26334290, 0, 0)));
        assert!(!listed(manifest(1, 26334290, 0, 0)));
    }

    #[test]
    fn reads_installed_state_of_a_manifest() {
        let manifest = |flags: &str| format!(r#""AppState" {{ "appid" "10" "name" "Hades" "StateFlags" "{flags}" }}"#);
        assert_eq!(super::manifest_installed(&manifest("4")), Some(true));
        assert_eq!(super::manifest_installed(&manifest("6")), Some(true)); // mise à jour en attente
        assert_eq!(super::manifest_installed(&manifest("1026")), Some(false)); // téléchargement
        assert_eq!(super::manifest_installed(r#""AppState" { "appid" "10" "#), None); // en cours d'écriture
        assert_eq!(super::manifest_installed(""), None);
    }

    #[test]
    fn detects_tools() {
        assert!(is_tool("431960", "Wallpaper Engine"));
        assert!(is_tool("1624110", "Football Manager 2022 Editor"));
        assert!(!is_tool("730", "Counter-Strike 2"));
    }
}
