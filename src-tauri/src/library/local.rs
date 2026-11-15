//! Jeux installés hors des launchers déjà pris en charge, trouvés dans les dossiers de jeux.
//!
//! Dossiers lus : les sous-dossiers des dossiers « Jeux » ou « Games » à la racine de chaque
//! lecteur (un dossier « Jeux Epic » à l'intérieur est lu de la même façon), plus les
//! emplacements d'installation déclarés dans la désinstallation Windows quand ils contiennent un
//! jeu reconnaissable. Chaque dossier est ensuite rangé :
//! - Battle.net : `.product.db` et `.build.info` à la racine (voir `battlenet.rs`) ;
//! - EA : `__Installer\installerdata.xml` (voir `ea.rs`) ;
//! - Autres : exécutable principal choisi par score (nom proche du dossier, profondeur, taille).

use super::{Game, Launch, Platform};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Profondeur maximale de recherche de l'exécutable sous le dossier du jeu.
const MAX_DEPTH: usize = 3;

/// Nature d'un dossier de jeu.
#[derive(Debug, PartialEq, Eq)]
pub enum Kind {
    Battlenet,
    Ea(PathBuf),
    Other,
}

/// Range un dossier selon les fichiers laissés par son launcher.
pub fn kind(dir: &Path) -> Kind {
    if dir.join(".product.db").is_file() && dir.join(".build.info").is_file() {
        return Kind::Battlenet;
    }
    if let Some(xml) = find_installer_data(dir) {
        return Kind::Ea(xml);
    }
    Kind::Other
}

/// `__Installer\installerdata.xml`, à la racine ou un niveau plus bas (`FIFA\EA SPORTS FC 24\…`).
fn find_installer_data(dir: &Path) -> Option<PathBuf> {
    let direct = dir.join(r"__Installer\installerdata.xml");
    if direct.is_file() {
        return Some(direct);
    }
    subdirs(dir).into_iter().map(|d| d.join(r"__Installer\installerdata.xml")).find(|p| p.is_file())
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let mut list: Vec<PathBuf> = fs::read_dir(dir)
        .map(|it| it.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect())
        .unwrap_or_default();
    list.sort();
    list
}

/// Un nom de dossier qui annonce une bibliothèque de jeux : « Jeux », « Games », « Jeux SSD », « My Games »…
/// Les dossiers Xbox (`XboxGames`) sont lus par la source Xbox.
fn is_library_name(name: &str) -> bool {
    let n = name.to_lowercase();
    !n.contains("xbox") && (n.split(|c: char| !c.is_alphanumeric()).any(|w| w == "jeux" || w == "jeu" || w == "games" || w == "game"))
}

/// Dossiers candidats. EA, Battle.net et « Autres » les demandent tour à tour pendant un même
/// scan : la liste est gardée quelques secondes pour ne parcourir les disques qu'une fois.
pub fn candidates() -> Vec<PathBuf> {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    static CACHE: Mutex<Option<(Instant, Vec<PathBuf>)>> = Mutex::new(None);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, list)) = cache.as_ref()
        && at.elapsed() < Duration::from_secs(10)
    {
        return list.clone();
    }
    let list = find_candidates();
    *cache = Some((Instant::now(), list.clone()));
    list
}

/// Sous-dossiers des bibliothèques de jeux, puis emplacements de la désinstallation Windows.
fn find_candidates() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for drive in b'A'..=b'Z' {
        let root = PathBuf::from(format!("{}:\\", drive as char));
        if !root.is_dir() {
            continue;
        }
        roots.extend(subdirs(&root).into_iter().filter(|d| d.file_name().and_then(|n| n.to_str()).is_some_and(is_library_name)));
    }

    let mut seen = HashSet::new();
    let mut found = Vec::new();
    let mut push = |dir: PathBuf, found: &mut Vec<PathBuf>| {
        if seen.insert(super::dir_key(&dir.to_string_lossy())) {
            found.push(dir);
        }
    };
    for root in roots {
        for dir in subdirs(&root) {
            let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // Une bibliothèque dans la bibliothèque (« Jeux Epic ») : ses sous-dossiers sont des jeux.
            if is_library_name(name) && kind(&dir) == Kind::Other && !has_exe(&dir) {
                subdirs(&dir).into_iter().for_each(|d| push(d, &mut found));
            } else {
                push(dir, &mut found);
            }
        }
    }
    for dir in uninstall_locations() {
        if kind(&dir) != Kind::Other || has_engine_marker(&dir) {
            push(dir, &mut found);
        }
    }
    found
}

fn has_exe(dir: &Path) -> bool {
    fs::read_dir(dir).map(|it| it.flatten().any(|e| is_exe(&e.path()))).unwrap_or(false)
}

fn is_exe(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("exe")) && path.is_file()
}

/// Fichiers qui trahissent un moteur de jeu (Unity, Unreal, Steamworks, Godot).
fn has_engine_marker(dir: &Path) -> bool {
    const MARKERS: &[&str] = &["UnityPlayer.dll", "GameAssembly.dll", "steam_api64.dll", "steam_api.dll"];
    MARKERS.iter().any(|m| dir.join(m).is_file())
        || dir.join(r"Engine\Binaries").is_dir()
        || subdirs(dir).iter().any(|d| MARKERS.iter().any(|m| d.join(m).is_file()) || d.join(r"Engine\Binaries").is_dir())
}

/// Emplacements d'installation déclarés à Windows (programmes installés), hors dossiers trop larges.
fn uninstall_locations() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        use super::reg;
        let mut dirs = Vec::new();
        let paths = [
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        ];
        let keys = paths.iter().filter_map(|p| reg::hklm(p)).chain(reg::hkcu(paths[0]));
        for root in keys {
            for (_, key) in reg::children(&root) {
                let Some(dir) = reg::string(&key, "InstallLocation") else { continue };
                let dir = PathBuf::from(dir.trim_matches('"').replace('/', "\\"));
                if dir.components().count() >= 3 && dir.is_dir() {
                    dirs.push(dir);
                }
            }
        }
        dirs
    }
    #[cfg(not(windows))]
    Vec::new()
}

/// Morceaux de nom d'exécutable qui ne lancent pas le jeu : installeurs, utilitaires, anti-triche…
const NOT_GAME_EXE: &[&str] = &[
    "unins", "setup", "install", "redist", "dotnet", "directx", "dxsetup", "crash", "report", "update", "patch", "helper",
    "service", "easyanticheat", "anticheat", "touchup", "cleanup", "activation", "editor", "server", "uplay", "ffmpeg",
    "qtwebengine", "prereq", "benchmark", "config", "settings", "cef", "notification", "overlay", "gdf", "dowser", "launcherpatcher",
    "vc_", "vcredist", "7z", "java", "python", "subprocess", "uploader", "diagnostic", "repair", "verify", "register",
    "toggler", "unlocker", "language", "changer", "sfv", "dx9",
];

/// Le nom d'exécutable peut-il être celui du jeu ?
pub fn is_game_exe_name(name: &str) -> bool {
    let name = name.to_lowercase();
    name.ends_with(".exe") && !NOT_GAME_EXE.iter().any(|n| name.contains(n))
}

/// Launchers installés comme des programmes : jamais présentés comme des jeux.
const LAUNCHER_APPS: &[&str] = &[
    "ubisoftconnect", "ubisoftgamelauncher", "steam", "epicgameslauncher", "eaapp", "eadesktop", "origin", "battlenet",
    "goggalaxy", "rockstargameslauncher", "riotclient", "battleyelauncher", "paradoxlauncher", "curseforge", "overwolf",
    "ankamalauncher", "hoyoplay", "unityhub",
];
/// Dossiers où l'exécutable du jeu ne se trouve jamais.
const NOT_GAME_DIR: &[&str] = &[
    "_redist", "redist", "_commonredist", "commonredist", "__installer", "installer_resources", "directx", "battleye",
    "easyanticheat", "eac", "prereq", "prerequisites", "support", "support64", "tools", "thirdparty", "dotnet", "vcredist",
];

fn compact(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase()
}

/// Score d'un exécutable : nom proche du dossier, peu profond, gros.
fn score(exe: &Path, depth: usize, folder: &str) -> i64 {
    let stem = compact(exe.file_stem().and_then(|s| s.to_str()).unwrap_or(""));
    let folder = compact(folder);
    let similar = stem.len() >= 3 && (folder.contains(&stem) || stem.contains(&folder));
    let size_mb = exe.metadata().map(|m| m.len() / 1_000_000).unwrap_or(0).min(400) as i64;
    let mut s = size_mb / 10 - 10 * depth as i64;
    if similar {
        s += 100;
    }
    if stem.ends_with("be") && stem.len() > 4 {
        s -= 5; // variante BattlEye : on préfère l'exécutable normal à nom égal
    }
    s
}

/// Exécutable principal d'un dossier de jeu.
pub fn main_exe(dir: &Path) -> Option<PathBuf> {
    let folder = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let mut best: Option<(i64, PathBuf)> = None;
    let mut stack = vec![(dir.to_path_buf(), 0usize)];
    while let Some((current, depth)) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if path.is_dir() {
                if depth < MAX_DEPTH && !NOT_GAME_DIR.contains(&name.as_str()) && !name.starts_with('.') {
                    stack.push((path, depth + 1));
                }
            } else if is_game_exe_name(&name) {
                let s = score(&path, depth, folder);
                if best.as_ref().is_none_or(|(b, p)| s > *b || (s == *b && path < *p)) {
                    best = Some((s, path));
                }
            }
        }
    }
    best.map(|(_, p)| p)
}

/// « CrusaderKing » → « Crusader King », « Bus_Simulator-21 » → « Bus Simulator 21 », « PokeMMO » reste entier.
pub fn pretty_folder(name: &str) -> String {
    let chars: Vec<char> = name.chars().map(|c| if c == '_' || c == '-' || c == '.' { ' ' } else { c }).collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if i > 0 {
            let p = chars[i - 1];
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            let word = p.is_lowercase() && c.is_uppercase() && next_lower;
            let number = p.is_alphabetic() && c.is_ascii_digit();
            if (word || number) && !out.ends_with(' ') {
                out.push(' ');
            }
        }
        if !(c == ' ' && out.ends_with(' ')) {
            out.push(c);
        }
    }
    out.trim().to_string()
}

/// Nom du produit dans les informations de version de l'exécutable, s'il est parlant.
pub fn product_name(exe: &Path) -> Option<String> {
    #[cfg(windows)]
    {
        use pelite::{FileMap, PeFile};
        let map = FileMap::open(exe).ok()?;
        let file = PeFile::from_bytes(&map).ok()?;
        let info = file.resources().ok()?.version_info().ok()?;
        let lang = *info.translation().first()?;
        const GENERIC: &[&str] = &["unrealengine", "unrealgame", "bootstrappackagedgame", "unityplayer", "unity", "game", "launcher", "application"];
        let stem = compact(exe.file_stem()?.to_str()?);
        ["ProductName", "FileDescription"]
            .into_iter()
            .filter_map(|key| info.value(lang, key))
            .map(|v| v.trim().trim_end_matches('\0').trim().to_string())
            .find(|v| {
                let c = compact(v.strip_suffix(".exe").unwrap_or(v));
                // Un nom égal à celui du fichier (« USF7 ») n'apprend rien : le dossier dira mieux.
                !v.is_empty() && v.len() <= 60 && !GENERIC.contains(&c.as_str()) && !c.contains("battleye") && c != stem
            })
    }
    #[cfg(not(windows))]
    {
        let _ = exe;
        None
    }
}

/// Identifiant stable tiré du chemin : `e-jeux-frostpunk`.
pub fn path_id(dir: &Path) -> String {
    let key = super::dir_key(&dir.to_string_lossy());
    let mut id = String::new();
    for c in key.chars() {
        if c.is_ascii_alphanumeric() {
            id.push(c);
        } else if !id.ends_with('-') {
            id.push('-');
        }
    }
    id.trim_matches('-').to_string()
}

/// Titre sans information de version : le nom du dossier, ou celui de l'exécutable s'il est plus
/// complet (« EFT » → « Escape From Tarkov », « Schedule » → « Schedule I »).
fn fallback_title(folder: &str, exe: &Path) -> String {
    let stem = exe.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let stem = stem.strip_suffix("_BE").unwrap_or(stem);
    if compact(stem).len() > compact(folder).len() { pretty_folder(stem) } else { pretty_folder(folder) }
}

/// Jeu « Autres » depuis un dossier : exécutable principal, titre du produit sinon du dossier.
pub fn build(dir: &Path) -> Option<Game> {
    let exe = main_exe(dir)?;
    let folder = dir.file_name().and_then(|n| n.to_str()).unwrap_or("Jeu");
    let title = product_name(&exe).unwrap_or_else(|| fallback_title(folder, &exe));
    let names = [compact(&title), compact(exe.file_stem().and_then(|s| s.to_str()).unwrap_or(""))];
    if names.iter().any(|n| LAUNCHER_APPS.contains(&n.as_str())) {
        return None; // un launcher installé dans le dossier des jeux n'est pas un jeu
    }
    let mut game = Game::new(Platform::Other, &path_id(dir), &title, Launch::Exec { path: exe, args: Vec::new() });
    game.install_dir = Some(dir.to_string_lossy().into_owned());
    Some(game)
}

/// Jeux de lanceurs tiers et de mods connus, à emplacement fixe : FiveM, RedM, RAGE MP, Roblox,
/// Ankama, Pokeisland.
fn known_launchers() -> Vec<Game> {
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let mut games = Vec::new();
    let mut add = |id: &str, title: &str, dir: PathBuf, exe: PathBuf| {
        if exe.is_file() {
            let mut g = Game::new(Platform::Other, id, title, Launch::Exec { path: exe, args: Vec::new() });
            g.install_dir = Some(dir.to_string_lossy().into_owned());
            games.push(g);
        }
    };
    if let Some(local) = &local {
        for (id, title) in [("fivem", "FiveM"), ("redm", "RedM")] {
            let dir = local.join(title);
            add(id, title, dir.clone(), dir.join(format!("{title}.exe")));
        }
        // Roblox : la version la plus récente de l'application.
        let versions = local.join(r"Roblox\Versions");
        let player = subdirs(&versions)
            .into_iter()
            .map(|d| d.join("RobloxPlayerBeta.exe"))
            .filter(|p| p.is_file())
            .max_by_key(|p| p.metadata().and_then(|m| m.modified()).ok());
        if let Some(player) = player {
            add("roblox", "Roblox", local.join("Roblox"), player);
        }
        // Jeux Ankama (Dofus, Wakfu…) : lancés par l'Ankama Launcher, qui gère la connexion.
        let ankama = local.join(r"Programs\Ankama Launcher\Ankama Launcher.exe");
        if ankama.is_file() {
            for dir in subdirs(&local.join("Ankama")) {
                let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
                if has_exe(&dir) {
                    add(&format!("ankama-{}", compact(&name)), &ankama_title(&name), dir, ankama.clone());
                }
            }
        }
        let pokeisland = local.join(r"Programs\Pokeisland-Launcher");
        add("pokeisland", "Pokeisland", pokeisland.clone(), pokeisland.join("Pokeisland Launcher.exe"));
    }
    #[cfg(windows)]
    if let Some(dir) = super::reg::hkcu(r"Software\RAGE-MP").and_then(|k| super::reg::string(&k, "rage_path")) {
        let dir = PathBuf::from(dir);
        add("ragemp", "RAGE Multiplayer", dir.clone(), dir.join("ragemp_v.exe"));
    }
    games
}

/// Dossier Ankama (`Dofus-dofus3`, `Wakfu-main`) → titre du jeu.
fn ankama_title(dir: &str) -> String {
    let (game, release) = dir.split_once('-').unwrap_or((dir, ""));
    match release.to_lowercase().as_str() {
        "dofus3" => "Dofus 3".to_string(),
        "retro" => "Dofus Retro".to_string(),
        "beta" => format!("{game} (bêta)"),
        _ => pretty_folder(game),
    }
}

pub fn scan() -> Result<Option<Vec<Game>>, String> {
    let mut games = known_launchers();
    games.extend(candidates().iter().filter(|d| kind(d) == Kind::Other).filter_map(|d| build(d)));
    // Le même jeu copié sur deux disques : le premier suffit.
    let mut titles = HashSet::new();
    games.retain(|g| titles.insert(compact(&g.title)));
    Ok(Some(games))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(files: &[&str]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("ludex-local-{}-{}", std::process::id(), files.len()));
        let _ = fs::remove_dir_all(&root);
        for f in files {
            let path = root.join(f);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"").unwrap();
        }
        root
    }

    #[test]
    fn picks_the_game_executable() {
        let root = tree(&[
            r"Cyberpunk 2077\setup_redlauncher.exe",
            r"Cyberpunk 2077\REDprelauncher.exe",
            r"Cyberpunk 2077\unins000.exe",
            r"Cyberpunk 2077\bin\x64\Cyberpunk2077.exe",
            r"Cyberpunk 2077\_Redist\vcredist_x64.exe",
            r"Cyberpunk 2077\BattlEye\BEService_x64.exe",
        ]);
        let exe = main_exe(&root.join("Cyberpunk 2077")).unwrap();
        assert!(exe.ends_with(r"bin\x64\Cyberpunk2077.exe"), "{exe:?}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn prefers_the_normal_exe_over_battleye() {
        let root = tree(&[r"EFT\EscapeFromTarkov.exe", r"EFT\EscapeFromTarkov_BE.exe", r"EFT\UnityCrashHandler64.exe"]);
        assert!(main_exe(&root.join("EFT")).unwrap().ends_with("EscapeFromTarkov.exe"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn classifies_folders() {
        let root = tree(&[
            r"StarCraft II\.product.db",
            r"StarCraft II\.build.info",
            r"FIFA\EA SPORTS FC 24\__Installer\installerdata.xml",
            r"Frostpunk\Frostpunk.exe",
        ]);
        assert_eq!(kind(&root.join("StarCraft II")), Kind::Battlenet);
        assert!(matches!(kind(&root.join("FIFA")), Kind::Ea(_)));
        assert_eq!(kind(&root.join("Frostpunk")), Kind::Other);
        let game = build(&root.join("Frostpunk")).unwrap();
        assert_eq!(game.platform, Platform::Other);
        assert_eq!(game.title, "Frostpunk");
        assert!(game.id.starts_with("other:") && game.id.ends_with("frostpunk"));
        assert!(build(&root.join("StarCraft II")).is_none()); // aucun exécutable
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn names_and_libraries() {
        assert_eq!(pretty_folder("CrusaderKing"), "Crusader King");
        assert_eq!(pretty_folder("Bus_Simulator-21"), "Bus Simulator 21");
        assert_eq!(pretty_folder("FootballManager2024"), "Football Manager 2024");
        assert_eq!(pretty_folder("PokeMMO"), "PokeMMO");
        assert_eq!(fallback_title("EFT", Path::new(r"C:\EFT\EscapeFromTarkov_BE.exe")), "Escape From Tarkov");
        assert_eq!(fallback_title("Frostpunk", Path::new(r"C:\F\Frostpunk.exe")), "Frostpunk");
        assert!(is_library_name("Jeux SSD") && is_library_name("Games") && is_library_name("Jeux Epic"));
        assert!(!is_library_name("XboxGames") && !is_library_name("Gameplay") && !is_library_name("Logiciel SSD"));
        assert_eq!(ankama_title("Dofus-dofus3"), "Dofus 3");
        assert_eq!(ankama_title("Wakfu-main"), "Wakfu");
        assert_eq!(path_id(Path::new(r"E:\Jeux\Need for Speed(TM) Rivals\")), "e-jeux-need-for-speed-tm-rivals");
    }
}
