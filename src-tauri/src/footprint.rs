//! Vue avancée du Stockage, avant de désinstaller : ce qu'un jeu laisse sur le PC. Lecture seule.
//!
//! - Dossiers : dossier d'installation, et pour Steam le cache des shaders, les mods du Workshop,
//!   un téléchargement interrompu et les données du compte (`userdata/<compte>/<appid>`), puis les
//!   sauvegardes trouvées par `saves.rs`.
//! - Entrées de désinstallation de Windows (`HKLM`/`HKCU\…\Uninstall`) qui correspondent au jeu :
//!   nom de clé connu (`Steam App <appid>`, `Uplay Install <id>`, `<id>_is1` de GOG), désinstalleur
//!   d'Amazon Games pour cet id (`Amazon Game Remover.exe … -p <id>`), même `InstallLocation` que
//!   le jeu, ou désinstalleur rangé dans le dossier du jeu.
//! - Autres clés du registre que lisent les sources du scan (Steam, GOG, Ubisoft, Rockstar, Xbox),
//!   plus celles d'EA qui pointent vers le dossier du jeu.
//!
//! Les emplacements d'un jeu sont gardés en mémoire (`Targets`, `lib.rs`) tant que Ludex tourne :
//! après une désinstallation, le jeu quitte la bibliothèque mais ses emplacements se relisent pour
//! montrer les restes (dossier ou clé encore là). Le front n'envoie qu'un id de jeu et la clé d'un
//! élément de la liste (`install`, `save-0`, `uninstall-0`…), jamais un chemin.
//!
//! Ludex ne supprime aucun fichier ni aucune clé. Il ouvre un dossier, la désinstallation de Steam,
//! ou le désinstalleur officiel du jeu : `UninstallString` relu dans le registre au moment du clic,
//! découpé côté Rust (`parse_command`) puis lancé sans interpréteur de commandes.

use crate::library::{Game, Platform};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Branche du registre.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Hive {
    #[serde(rename = "HKLM")]
    Hklm,
    #[serde(rename = "HKCU")]
    Hkcu,
}

impl Hive {
    fn prefix(self) -> &'static str {
        match self {
            Hive::Hklm => "HKLM",
            Hive::Hkcu => "HKCU",
        }
    }
}

/// Lecture du registre (le vrai en lecture seule, ou un faux pour les tests).
pub trait Registry {
    /// Noms des sous-clés.
    fn children(&self, hive: Hive, path: &str) -> Vec<String>;
    fn exists(&self, hive: Hive, path: &str) -> bool;
    /// Valeur texte non vide, espaces retirés.
    fn string(&self, hive: Hive, path: &str, name: &str) -> Option<String>;
    fn dword(&self, hive: Hive, path: &str, name: &str) -> Option<u32>;
}

/// Registre de Windows, ouvert en lecture seule (`KEY_READ`).
pub struct System;

#[cfg(windows)]
impl System {
    fn open(hive: Hive, path: &str) -> Option<winreg::RegKey> {
        use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
        let root = match hive {
            Hive::Hklm => winreg::RegKey::predef(HKEY_LOCAL_MACHINE),
            Hive::Hkcu => winreg::RegKey::predef(HKEY_CURRENT_USER),
        };
        root.open_subkey_with_flags(path, KEY_READ).ok()
    }
}

#[cfg(windows)]
impl Registry for System {
    fn children(&self, hive: Hive, path: &str) -> Vec<String> {
        Self::open(hive, path).map(|k| k.enum_keys().flatten().collect()).unwrap_or_default()
    }
    fn exists(&self, hive: Hive, path: &str) -> bool {
        Self::open(hive, path).is_some()
    }
    fn string(&self, hive: Hive, path: &str, name: &str) -> Option<String> {
        let v: String = Self::open(hive, path)?.get_value(name).ok()?;
        Some(v.trim().to_string()).filter(|v| !v.is_empty())
    }
    fn dword(&self, hive: Hive, path: &str, name: &str) -> Option<u32> {
        Self::open(hive, path)?.get_value(name).ok()
    }
}

#[cfg(not(windows))]
impl Registry for System {
    fn children(&self, _: Hive, _: &str) -> Vec<String> {
        Vec::new()
    }
    fn exists(&self, _: Hive, _: &str) -> bool {
        false
    }
    fn string(&self, _: Hive, _: &str, _: &str) -> Option<String> {
        None
    }
    fn dword(&self, _: Hive, _: &str, _: &str) -> Option<u32> {
        None
    }
}

/// Chemin comparable : séparateurs Windows, minuscules, sans guillemets, préfixe `\\?\` ni `\` final.
fn norm(path: &str) -> String {
    let p = path.trim().trim_matches('"').replace('/', "\\").to_lowercase();
    let p = p.strip_prefix(r"\\?\").unwrap_or(&p);
    p.trim_end_matches('\\').to_string()
}

/// `path` est `parent` ou se trouve dessous (chemins déjà normalisés).
fn is_under(path: &str, parent: &str) -> bool {
    path == parent || path.strip_prefix(parent).is_some_and(|rest| rest.starts_with('\\'))
}

/// Appid d'un jeu Steam (chiffres seulement).
fn steam_appid(game: &Game) -> Option<&str> {
    let id = game.platform_id.as_str();
    (game.platform == Platform::Steam && !id.is_empty() && id.len() <= 10 && id.bytes().all(|b| b.is_ascii_digit())).then_some(id)
}

/// Id d'un jeu sans caractère qui sortirait d'un nom de clé (`\`, guillemets, contrôles).
fn safe_id(id: &str) -> Option<&str> {
    (!id.is_empty() && id.len() <= 128 && id.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))).then_some(id)
}

// ---------------------------------------------------------------------------------------------
// Entrées de désinstallation de Windows
// ---------------------------------------------------------------------------------------------

/// Listes des programmes installés : machine (64 et 32 bits), puis utilisateur.
pub const UNINSTALL_ROOTS: [(Hive, &str); 3] = [
    (Hive::Hklm, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
    (Hive::Hklm, r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"),
    (Hive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Uninstall"),
];

/// Une entrée de « Applications installées » de Windows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UninstallEntry {
    pub hive: Hive,
    /// Clé complète sous la branche.
    pub path: String,
    /// Nom de la clé (dernier élément de `path`).
    pub name: String,
    pub display_name: Option<String>,
    pub publisher: Option<String>,
    pub version: Option<String>,
    pub install_location: Option<String>,
    pub uninstall_string: Option<String>,
    /// `InstallDate`, `AAAAMMJJ`.
    pub install_date: Option<String>,
    /// `EstimatedSize`, en Kio.
    pub estimated_kb: Option<u32>,
}

/// Relit une entrée, `None` si la clé n'existe pas (ou plus).
pub fn read_entry(reg: &impl Registry, hive: Hive, path: &str) -> Option<UninstallEntry> {
    if !reg.exists(hive, path) {
        return None;
    }
    let s = |name: &str| reg.string(hive, path, name);
    Some(UninstallEntry {
        hive,
        path: path.to_string(),
        name: path.rsplit('\\').next().unwrap_or(path).to_string(),
        display_name: s("DisplayName"),
        publisher: s("Publisher"),
        version: s("DisplayVersion"),
        install_location: s("InstallLocation"),
        uninstall_string: s("UninstallString"),
        install_date: s("InstallDate"),
        estimated_kb: reg.dword(hive, path, "EstimatedSize"),
    })
}

/// Toutes les entrées des trois listes.
pub fn uninstall_entries(reg: &impl Registry) -> Vec<UninstallEntry> {
    UNINSTALL_ROOTS
        .iter()
        .flat_map(|&(hive, root)| reg.children(hive, root).into_iter().map(move |name| (hive, format!(r"{root}\{name}"))))
        .filter_map(|(hive, path)| read_entry(reg, hive, &path))
        .collect()
}

/// Exécutable d'une ligne de commande, sans vérifier qu'il existe : texte entre guillemets, sinon
/// jusqu'au premier `.exe` suivi d'un espace ou de la fin.
fn command_exe(text: &str) -> Option<String> {
    let text = text.trim();
    if let Some(rest) = text.strip_prefix('"') {
        return rest.split('"').next().map(str::to_string);
    }
    let lower = text.to_lowercase();
    lower
        .match_indices(".exe")
        .map(|(i, _)| i + 4)
        .find(|&end| lower[end..].chars().next().is_none_or(char::is_whitespace))
        .map(|end| text[..end].to_string())
}

/// Entrées de désinstallation de ce jeu. Un dossier de jeu trop large (racine d'un lecteur…) ne
/// sert jamais à comparer : n'importe quel programme y correspondrait.
pub fn matching(game: &Game, entries: &[UninstallEntry]) -> Vec<UninstallEntry> {
    let dir = game.install_dir.as_deref().filter(|d| crate::session::root_key(d).is_some()).map(norm);
    let id = game.platform_id.as_str();
    let named = match game.platform {
        Platform::Steam => steam_appid(game).map(|a| format!("Steam App {a}")),
        Platform::Ubisoft => safe_id(id).map(|i| format!("Uplay Install {i}")),
        Platform::Gog => safe_id(id).map(|i| format!("{i}_is1")),
        _ => None,
    };
    // Amazon Games : une entrée par jeu, désinstalleur commun rangé chez l'appli, id après `-p`.
    let amazon = if game.platform == Platform::Amazon { safe_id(id) } else { None };
    let mut out: Vec<UninstallEntry> = Vec::new();
    for e in entries {
        let by_name = named.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(&e.name))
            || amazon.is_some_and(|i| e.uninstall_string.as_deref().is_some_and(|u| is_amazon_remover_of(u, i)));
        let by_dir = dir.as_deref().is_some_and(|d| {
            e.install_location.as_deref().is_some_and(|l| norm(l) == d)
                || e.uninstall_string.as_deref().and_then(command_exe).is_some_and(|exe| is_under(&norm(&exe), d))
        });
        if (by_name || by_dir) && !out.iter().any(|o| o.hive == e.hive && o.path.eq_ignore_ascii_case(&e.path)) {
            out.push(e.clone());
        }
    }
    out
}

/// Commande du désinstalleur d'Amazon Games pour ce jeu : `"…\Amazon Game Remover.exe" -m Game -p <id>`.
fn is_amazon_remover_of(command: &str, id: &str) -> bool {
    let remover = command_exe(command)
        .is_some_and(|exe| Path::new(&exe).file_name().is_some_and(|n| n.eq_ignore_ascii_case("Amazon Game Remover.exe")));
    let mut words = command.split_whitespace();
    remover && words.any(|w| w == "-p") && words.next().is_some_and(|w| w.trim_matches('"') == id)
}

// ---------------------------------------------------------------------------------------------
// Désinstalleur officiel
// ---------------------------------------------------------------------------------------------

/// Commande prête à lancer : exécutable vérifié, arguments passés tels quels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub exe: PathBuf,
    pub args: String,
}

/// Programmes qui exécutent du texte (scripts, commandes) : jamais lancés par Ludex.
const INTERPRETERS: &[&str] = &[
    "cmd.exe", "powershell.exe", "pwsh.exe", "wscript.exe", "cscript.exe", "mshta.exe", "rundll32.exe", "regsvr32.exe",
    "explorer.exe", "conhost.exe", "wsl.exe", "bash.exe", "msbuild.exe", "installutil.exe",
];

/// Chemin absolu sur un lecteur local (`C:\…`).
fn on_local_drive(p: &str) -> bool {
    let b = p.as_bytes();
    b.len() > 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\'
}

/// Découpe une `UninstallString` : exécutable (entre guillemets, `MsiExec.exe` sans dossier, ou
/// chemin sans guillemets jusqu'au premier `.exe` qui existe) et arguments. Refuse un chemin
/// relatif ou réseau, un fichier absent, autre chose qu'un `.exe` et les interpréteurs.
/// `system` : dossier `System32` ; `is_file` : test d'existence (faux dossiers dans les tests).
pub fn parse_command(text: &str, system: &Path, is_file: impl Fn(&Path) -> bool) -> Result<Command, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err(tr!("Aucune commande de désinstallation.", "No uninstall command."));
    }
    if text.chars().any(char::is_control) {
        return Err(tr!("Commande de désinstallation illisible.", "Unreadable uninstall command."));
    }
    let (exe, args): (String, &str) = if let Some(rest) = text.strip_prefix('"') {
        let end = rest.find('"').ok_or_else(|| tr!("Commande de désinstallation illisible.", "Unreadable uninstall command."))?;
        (rest[..end].to_string(), &rest[end + 1..])
    } else {
        let first = text.split_whitespace().next().unwrap_or("");
        if first.eq_ignore_ascii_case("msiexec") || first.eq_ignore_ascii_case("msiexec.exe") {
            (system.join("msiexec.exe").to_string_lossy().into_owned(), &text[first.len()..])
        } else {
            let lower = text.to_lowercase();
            let ends: Vec<usize> = lower
                .match_indices(".exe")
                .map(|(i, _)| i + 4)
                .filter(|&end| lower[end..].chars().next().is_none_or(char::is_whitespace))
                .collect();
            let end = ends
                .iter()
                .copied()
                .find(|&end| is_file(Path::new(&text[..end])))
                .or_else(|| ends.first().copied())
                .ok_or_else(|| tr!("Désinstalleur introuvable : la commande ne lance aucun programme.", "Uninstaller not found: the command doesn't run any program."))?;
            (text[..end].to_string(), &text[end..])
        }
    };
    let exe = exe.trim().replace('/', "\\");
    if !on_local_drive(&exe) {
        return Err(tr!("Désinstalleur refusé : {exe} n'est pas un chemin complet sur un disque de ce PC.", "Uninstaller refused: {exe} isn't a full path on a drive of this PC."));
    }
    let name = Path::new(&exe).file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    if !name.ends_with(".exe") {
        return Err(tr!("Désinstalleur refusé : {exe} n'est pas un programme.", "Uninstaller refused: {exe} isn't a program."));
    }
    if INTERPRETERS.contains(&name.as_str()) {
        return Err(tr!(
            "Ce désinstalleur passe par {name} : lance-le depuis Paramètres > Applications de Windows.",
            "This uninstaller goes through {name}: run it from Windows Settings > Apps."
        ));
    }
    if !is_file(Path::new(&exe)) {
        return Err(tr!("Désinstalleur introuvable : {exe}", "Uninstaller not found: {exe}"));
    }
    Ok(Command { exe: PathBuf::from(exe), args: args.trim().to_string() })
}

/// Le désinstalleur est celui de Steam (`steam.exe steam://uninstall/<appid>`).
fn is_steam_exe(cmd: &Command) -> bool {
    cmd.exe.file_name().is_some_and(|n| n.eq_ignore_ascii_case("steam.exe"))
}

/// Dossier `System32` de Windows.
pub fn system_dir() -> PathBuf {
    std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows")).join("System32")
}

/// Lance le désinstalleur comme le fait Windows (`ShellExecuteExW`, sans interpréteur de commandes) :
/// Windows demande lui-même l'autorisation administrateur si le programme en a besoin.
#[cfg(windows)]
pub fn launch(cmd: &Command) -> Result<(), String> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW};
    let file = HSTRING::from(cmd.exe.as_os_str());
    let params = HSTRING::from(cmd.args.as_str());
    let dir = HSTRING::from(cmd.exe.parent().unwrap_or(Path::new("")).as_os_str());
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: if cmd.args.is_empty() { PCWSTR::null() } else { PCWSTR(params.as_ptr()) },
        lpDirectory: PCWSTR(dir.as_ptr()),
        nShow: 1, // SW_SHOWNORMAL
        ..Default::default()
    };
    // SAFETY : `info` et les chaînes qu'il désigne vivent jusqu'au retour de l'appel.
    unsafe { ShellExecuteExW(&mut info) }.map_err(|e| {
        if e.code() == windows::core::HRESULT::from_win32(1223) {
            tr!("Désinstallation annulée : Windows n'a pas reçu l'autorisation.", "Uninstall canceled: Windows didn't get permission.")
        } else {
            tr!("Impossible de lancer le désinstalleur : {}", "Couldn't start the uninstaller: {}", e.message())
        }
    })
}

#[cfg(not(windows))]
pub fn launch(_: &Command) -> Result<(), String> {
    Err(tr!("Désinstalleur disponible sous Windows seulement.", "Uninstaller only available on Windows."))
}

// ---------------------------------------------------------------------------------------------
// Emplacements d'un jeu
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Install,
    Cache,
    Mods,
    Saves,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderTarget {
    /// Clé de l'élément, renvoyée par le front pour l'ouvrir.
    pub item: String,
    pub label: String,
    pub kind: Kind,
    pub path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyTarget {
    pub label: String,
    pub hive: Hive,
    pub path: String,
}

/// Sauvegardes trouvées par `saves.rs`, ou pourquoi elles sont inconnues.
#[derive(Clone, Debug, Default)]
pub struct Saves {
    pub paths: Vec<PathBuf>,
    /// La base signale aussi des données dans le registre (non localisées par Ludex).
    pub registry: bool,
    pub note: Option<String>,
}

/// Emplacements d'un jeu, relevés quand il était dans la bibliothèque, puis relus pour les restes.
#[derive(Clone, Debug)]
pub struct Targets {
    pub id: String,
    pub title: String,
    pub platform: Platform,
    pub folders: Vec<FolderTarget>,
    pub keys: Vec<KeyTarget>,
    /// Entrées de désinstallation : (branche, clé), relues à chaque fois.
    pub uninstall: Vec<(Hive, String)>,
    pub saves_registry: bool,
    pub saves_note: Option<String>,
    /// Désinstallation par `steam://uninstall/<appid>`.
    pub steam: bool,
}

/// Dossiers propres à Steam, à côté du jeu : `<bibliothèque>/steamapps/common/<jeu>`. Seulement
/// ceux qui existent. `account` : dossier `userdata/<compte>` du compte retenu.
fn steam_folders(game: &Game, account: Option<&Path>) -> Vec<FolderTarget> {
    let Some(appid) = steam_appid(game) else { return Vec::new() };
    let mut out = Vec::new();
    let named = |p: &Path, name: &str| p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name));
    let steamapps = game
        .install_dir
        .as_deref()
        .map(Path::new)
        .and_then(Path::parent)
        .filter(|common| named(common, "common"))
        .and_then(Path::parent)
        .filter(|s| named(s, "steamapps"));
    if let Some(steamapps) = steamapps {
        for (item, label, kind, sub) in [
            ("shaders", tr_str!("Cache des shaders", "Shader cache"), Kind::Cache, steamapps.join("shadercache").join(appid)),
            ("workshop", tr_str!("Mods du Workshop", "Workshop mods"), Kind::Mods, steamapps.join(r"workshop\content").join(appid)),
            ("download", tr_str!("Téléchargement de Steam en cours ou interrompu", "Steam download in progress or interrupted"), Kind::Cache, steamapps.join("downloading").join(appid)),
        ] {
            if sub.is_dir() {
                out.push(FolderTarget { item: item.into(), label: label.into(), kind, path: sub });
            }
        }
    }
    if let Some(dir) = account.map(|a| a.join(appid)).filter(|d| d.is_dir()) {
        out.push(FolderTarget { item: "cloud".into(), label: tr!("Données du compte Steam (Cloud, réglages)", "Steam account data (Cloud, settings)"), kind: Kind::Saves, path: dir });
    }
    out
}

/// Clés du registre que lisent les sources du scan pour ce jeu (et celles d'EA qui pointent vers
/// son dossier). Seulement celles qui existent.
pub fn source_keys(game: &Game, reg: &impl Registry) -> Vec<KeyTarget> {
    let key = |label: &str, hive: Hive, path: String| KeyTarget { label: label.to_string(), hive, path };
    let id = game.platform_id.as_str();
    let dir = game.install_dir.as_deref().map(norm);
    let same_dir = |hive: Hive, path: &str, value: &str| dir.as_deref().is_some_and(|d| reg.string(hive, path, value).is_some_and(|v| norm(&v) == d));
    let mut out: Vec<KeyTarget> = Vec::new();
    match game.platform {
        Platform::Steam => {
            if let Some(appid) = steam_appid(game) {
                out.push(key(tr_str!("État du jeu chez Steam", "Game state in Steam"), Hive::Hkcu, format!(r"Software\Valve\Steam\Apps\{appid}")));
            }
        }
        Platform::Gog => {
            if let Some(id) = safe_id(id) {
                out.push(key(tr_str!("Installation GOG", "GOG installation"), Hive::Hklm, format!(r"SOFTWARE\WOW6432Node\GOG.com\Games\{id}")));
            }
        }
        Platform::Ubisoft => {
            if let Some(id) = safe_id(id) {
                out.push(key(tr_str!("Installation Ubisoft Connect", "Ubisoft Connect installation"), Hive::Hklm, format!(r"SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs\{id}")));
            }
        }
        Platform::Rockstar => {
            let root = r"SOFTWARE\WOW6432Node\Rockstar Games";
            for name in reg.children(Hive::Hklm, root) {
                let path = format!(r"{root}\{name}");
                if same_dir(Hive::Hklm, &path, "InstallFolder") {
                    out.push(key(tr_str!("Installation Rockstar", "Rockstar installation"), Hive::Hklm, path));
                }
            }
        }
        Platform::Xbox => {
            let root = r"Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages";
            for name in reg.children(Hive::Hkcu, root) {
                // `Nom_Version_Archi_Ressource_Éditeur` : famille = `Nom_Éditeur`.
                let parts: Vec<&str> = name.split('_').collect();
                if parts.len() == 5 && format!("{}_{}", parts[0], parts[4]).eq_ignore_ascii_case(id) {
                    out.push(key(tr_str!("Paquet Windows", "Windows package"), Hive::Hkcu, format!(r"{root}\{name}")));
                }
            }
        }
        Platform::Ea => {
            for root in [r"SOFTWARE\WOW6432Node\EA Games", r"SOFTWARE\WOW6432Node\EA Sports", r"SOFTWARE\WOW6432Node\Electronic Arts"] {
                for name in reg.children(Hive::Hklm, root) {
                    let path = format!(r"{root}\{name}");
                    if same_dir(Hive::Hklm, &path, "Install Dir") || same_dir(Hive::Hklm, &path, "InstallDir") {
                        out.push(key(tr_str!("Installation EA", "EA installation"), Hive::Hklm, path));
                    }
                }
            }
        }
        _ => {}
    }
    out.retain(|k| reg.exists(k.hive, &k.path));
    out
}

/// Relève les emplacements d'un jeu de la bibliothèque. `entries` : toutes les entrées de
/// désinstallation (`uninstall_entries`), `account` : `userdata/<compte>` de Steam.
pub fn targets(game: &Game, reg: &impl Registry, entries: &[UninstallEntry], saves: Saves, account: Option<&Path>) -> Targets {
    let mut folders: Vec<FolderTarget> = Vec::new();
    if let Some(dir) = game.install_dir.as_deref() {
        folders.push(FolderTarget { item: "install".into(), label: tr!("Dossier d'installation", "Install folder"), kind: Kind::Install, path: PathBuf::from(dir) });
    }
    folders.extend(steam_folders(game, account));
    let mut n = 0;
    for path in saves.paths {
        let k = norm(&path.to_string_lossy());
        if folders.iter().any(|f| norm(&f.path.to_string_lossy()) == k) {
            continue;
        }
        folders.push(FolderTarget { item: format!("save-{n}"), label: tr!("Sauvegardes", "Saves"), kind: Kind::Saves, path });
        n += 1;
    }
    Targets {
        id: game.id.clone(),
        title: game.title.clone(),
        platform: game.platform,
        folders,
        keys: source_keys(game, reg),
        uninstall: matching(game, entries).into_iter().map(|e| (e.hive, e.path)).collect(),
        saves_registry: saves.registry,
        saves_note: saves.note,
        steam: crate::storage::uninstall_uri(game).is_some(),
    }
}

// ---------------------------------------------------------------------------------------------
// Relevé envoyé au front
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub item: String,
    pub label: String,
    pub kind: Kind,
    pub path: String,
    pub exists: bool,
    /// Fichier seul (une sauvegarde), pas un dossier.
    pub file: bool,
    /// Octets, `None` : inconnu (dossier absent, ou installation pas encore mesurée).
    pub bytes: Option<u64>,
    /// Clé du dossier de la liste qui le contient (le plus proche), `None` : à part. Sa taille est
    /// déjà comptée avec ce dossier (sauvegardes sous le dossier du jeu ou sous `userdata`).
    pub inside: Option<String>,
}

/// Ce que fait le bouton de l'entrée.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    /// Désinstalleur officiel lancé par Ludex.
    Run,
    /// Passe par Steam (`steam://uninstall`).
    Steam,
    /// Rien : voir `refusal`.
    None,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Uninstall {
    pub item: String,
    /// `HKLM\…\Uninstall\<nom>`.
    pub key: String,
    pub exists: bool,
    pub name: Option<String>,
    pub publisher: Option<String>,
    pub version: Option<String>,
    /// Taille déclarée à Windows, en octets.
    pub declared_size: Option<u64>,
    /// `AAAA-MM-JJ`.
    pub install_date: Option<String>,
    pub location: Option<String>,
    pub command: Option<String>,
    pub action: Action,
    pub refusal: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Key {
    pub label: String,
    pub key: String,
    pub exists: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Footprint {
    pub id: String,
    pub title: String,
    pub platform: Platform,
    /// Le jeu est encore dans la bibliothèque. Sinon, tout ce qui existe encore est un reste.
    pub installed: bool,
    pub folders: Vec<Folder>,
    pub uninstall: Vec<Uninstall>,
    pub keys: Vec<Key>,
    pub saves_registry: bool,
    pub saves_note: Option<String>,
    /// `steam://uninstall` possible.
    pub steam: bool,
    /// Paquet du Microsoft Store : se désinstalle depuis les Paramètres de Windows.
    pub store_app: bool,
    /// Éléments encore présents alors que le jeu a quitté la bibliothèque.
    pub leftovers: usize,
}

/// `AAAAMMJJ` → `AAAA-MM-JJ`.
fn install_date(raw: Option<&str>) -> Option<String> {
    let d = raw?.trim();
    (d.len() == 8 && d.bytes().all(|b| b.is_ascii_digit())).then(|| format!("{}-{}-{}", &d[..4], &d[4..6], &d[6..]))
}

/// Bouton d'une entrée de désinstallation.
fn action(entry: &UninstallEntry, steam: bool, is_file: &impl Fn(&Path) -> bool) -> (Action, Option<String>) {
    let Some(text) = entry.uninstall_string.as_deref() else {
        return (Action::None, Some(tr!("Cette entrée n'a pas de commande de désinstallation.", "This entry has no uninstall command.")));
    };
    match parse_command(text, &system_dir(), is_file) {
        Ok(cmd) if is_steam_exe(&cmd) => {
            if steam {
                (Action::Steam, None)
            } else {
                (Action::None, Some(tr!("Désinstallation gérée par Steam.", "Uninstall handled by Steam.")))
            }
        }
        Ok(_) => (Action::Run, None),
        Err(e) => (Action::None, Some(e)),
    }
}

/// Relit les emplacements : présence, tailles, entrées de désinstallation. `game` : le jeu s'il est
/// encore dans la bibliothèque (sa taille d'installation sert alors, sans remesurer). `size` :
/// taille d'un dossier ou d'un fichier présent.
pub fn check(t: &Targets, reg: &impl Registry, game: Option<&Game>, size: impl Fn(&Path) -> Option<u64>, is_file: impl Fn(&Path) -> bool) -> Footprint {
    let installed = game.is_some();
    let paths: Vec<(&str, String)> = t.folders.iter().map(|f| (f.item.as_str(), norm(&f.path.to_string_lossy()))).collect();
    let folders: Vec<Folder> = t
        .folders
        .iter()
        .map(|f| {
            let exists = f.path.exists();
            let me = norm(&f.path.to_string_lossy());
            let inside = paths
                .iter()
                .filter(|(item, p)| *item != f.item && *p != me && is_under(&me, p))
                .max_by_key(|(_, p)| p.len())
                .map(|(item, _)| item.to_string());
            let bytes = match (f.kind, game) {
                // Taille connue (launcher ou mesure de Ludex) : jamais remesuré ici (des minutes
                // pour un gros jeu). Inconnue tant que la mesure de fond n'est pas finie.
                (Kind::Install, Some(g)) => g.size_on_disk,
                _ if exists => size(&f.path),
                _ => None,
            };
            Folder {
                item: f.item.clone(),
                label: f.label.clone(),
                kind: f.kind,
                path: f.path.to_string_lossy().into_owned(),
                exists,
                file: f.path.is_file(),
                bytes,
                inside,
            }
        })
        .collect();
    let uninstall: Vec<Uninstall> = t
        .uninstall
        .iter()
        .enumerate()
        .map(|(i, (hive, path))| {
            let key = format!(r"{}\{path}", hive.prefix());
            let item = format!("uninstall-{i}");
            match read_entry(reg, *hive, path) {
                Some(e) => {
                    let (action, refusal) = if installed { action(&e, t.steam, &is_file) } else { (Action::None, None) };
                    Uninstall {
                        item,
                        key,
                        exists: true,
                        name: e.display_name.clone(),
                        publisher: e.publisher.clone(),
                        version: e.version.clone(),
                        declared_size: e.estimated_kb.map(|kb| u64::from(kb) * 1024),
                        install_date: install_date(e.install_date.as_deref()),
                        location: e.install_location.clone(),
                        command: e.uninstall_string.clone(),
                        action,
                        refusal,
                    }
                }
                None => Uninstall {
                    item,
                    key,
                    exists: false,
                    name: None,
                    publisher: None,
                    version: None,
                    declared_size: None,
                    install_date: None,
                    location: None,
                    command: None,
                    action: Action::None,
                    refusal: None,
                },
            }
        })
        .collect();
    let keys: Vec<Key> = t
        .keys
        .iter()
        .map(|k| Key { label: k.label.clone(), key: format!(r"{}\{}", k.hive.prefix(), k.path), exists: reg.exists(k.hive, &k.path) })
        .collect();
    let leftovers = if installed {
        0
    } else {
        // Un élément dans un dossier de la liste encore là est déjà compté avec lui.
        let present = |item: &str| folders.iter().any(|f| f.item == item && f.exists);
        folders.iter().filter(|f| f.exists && !f.inside.as_deref().is_some_and(present)).count()
            + uninstall.iter().filter(|u| u.exists).count()
            + keys.iter().filter(|k| k.exists).count()
    };
    Footprint {
        id: t.id.clone(),
        title: t.title.clone(),
        platform: t.platform,
        installed,
        folders,
        uninstall,
        keys,
        saves_registry: t.saves_registry,
        saves_note: t.saves_note.clone(),
        steam: t.steam && installed,
        store_app: t.platform == Platform::Xbox && installed,
        leftovers,
    }
}

/// Taille d'un fichier, ou des fichiers d'un dossier (liens et jonctions non suivis).
pub fn size_of(path: &Path) -> Option<u64> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if meta.is_file() {
        Some(meta.len())
    } else if meta.is_dir() {
        crate::storage::dir_size(path).ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Launch;
    use std::collections::BTreeMap;
    use std::fs;

    #[derive(Clone, Debug)]
    enum Value {
        Text(&'static str),
        Dword(u32),
    }

    /// Faux registre : clés (branche, chemin en minuscules) et leurs valeurs.
    #[derive(Default)]
    struct Fake {
        keys: BTreeMap<(Hive, String), (String, BTreeMap<String, Value>)>,
    }

    impl Fake {
        fn add(&mut self, hive: Hive, path: &str, values: &[(&str, Value)]) {
            let values = values.iter().map(|(n, v)| (n.to_lowercase(), v.clone())).collect();
            self.keys.insert((hive, path.to_lowercase()), (path.to_string(), values));
        }
        fn remove(&mut self, hive: Hive, path: &str) {
            self.keys.remove(&(hive, path.to_lowercase()));
        }
        fn get(&self, hive: Hive, path: &str, name: &str) -> Option<&Value> {
            self.keys.get(&(hive, path.to_lowercase()))?.1.get(&name.to_lowercase())
        }
    }

    impl Registry for Fake {
        fn children(&self, hive: Hive, path: &str) -> Vec<String> {
            let prefix = format!(r"{}\", path.to_lowercase());
            let mut out: Vec<String> = self
                .keys
                .iter()
                .filter(|((h, k), _)| *h == hive && k.starts_with(&prefix))
                .filter_map(|(_, (orig, _))| orig[prefix.len()..].split('\\').next().map(str::to_string))
                .collect();
            out.dedup();
            out
        }
        fn exists(&self, hive: Hive, path: &str) -> bool {
            self.keys.contains_key(&(hive, path.to_lowercase()))
        }
        fn string(&self, hive: Hive, path: &str, name: &str) -> Option<String> {
            match self.get(hive, path, name)? {
                Value::Text(t) => Some(t.trim().to_string()).filter(|t| !t.is_empty()),
                Value::Dword(_) => None,
            }
        }
        fn dword(&self, hive: Hive, path: &str, name: &str) -> Option<u32> {
            match self.get(hive, path, name)? {
                Value::Dword(d) => Some(*d),
                Value::Text(_) => None,
            }
        }
    }

    const HKLM_UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
    const WOW_UNINSTALL: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";
    const HKCU_UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";

    fn game(platform: Platform, id: &str, dir: &str) -> Game {
        let mut g = Game::new(platform, id, id, Launch::Uri(String::new()));
        g.install_dir = Some(dir.to_string());
        g
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ludex-footprint-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn finds_the_uninstall_entries_of_a_game() {
        let mut reg = Fake::default();
        use Value::Text;
        reg.add(Hive::Hklm, &format!(r"{WOW_UNINSTALL}\Steam App 570"), &[("DisplayName", Text("Dota 2")), ("UninstallString", Text(r#""C:\Program Files (x86)\Steam\steam.exe" steam://uninstall/570"#))]);
        reg.add(Hive::Hklm, &format!(r"{WOW_UNINSTALL}\1207658924_is1"), &[("DisplayName", Text("Witcher"))]);
        reg.add(Hive::Hklm, &format!(r"{HKLM_UNINSTALL}\{{GUID-DOFUS}}"), &[("DisplayName", Text("Dofus")), ("InstallLocation", Text(r#""E:\Jeux\Dofus\""#))]);
        reg.add(Hive::Hkcu, &format!(r"{HKCU_UNINSTALL}\Hades"), &[("DisplayName", Text("Hades")), ("UninstallString", Text(r"E:\Jeux\Hades Game\unins000.exe /SILENT"))]);
        reg.add(Hive::Hklm, &format!(r"{HKLM_UNINSTALL}\Racine"), &[("InstallLocation", Text(r"E:\"))]);
        reg.add(Hive::Hklm, &format!(r"{HKLM_UNINSTALL}\Dofus Retro"), &[("InstallLocation", Text(r"E:\Jeux\Dofus Retro"))]);
        let remover = r#""C:\Users\Moi\AppData\Local\Amazon Games\App\Amazon Game Remover.exe" -m Game -p amzn1.adg.product.abc"#;
        reg.add(Hive::Hkcu, &format!(r"{HKCU_UNINSTALL}\AmazonGames/Quake"), &[("DisplayName", Text("Quake")), ("UninstallString", Text(remover))]);
        let entries = uninstall_entries(&reg);
        assert_eq!(entries.len(), 7);
        let names = |g: &Game| matching(g, &entries).into_iter().map(|e| e.name).collect::<Vec<_>>();

        assert_eq!(names(&game(Platform::Steam, "570", r"E:\Steam\steamapps\common\dota 2 beta")), ["Steam App 570"]);
        assert_eq!(names(&game(Platform::Gog, "1207658924", r"D:\GOG\Witcher")), ["1207658924_is1"]);
        // Même dossier (guillemets, `\` final et casse ignorés), « Dofus Retro » n'est pas « Dofus ».
        assert_eq!(names(&game(Platform::Other, "dofus", r"e:\jeux\dofus")), ["{GUID-DOFUS}"]);
        // Désinstalleur rangé dans le dossier du jeu, sans guillemets malgré l'espace.
        assert_eq!(names(&game(Platform::Other, "hades", r"E:\Jeux\Hades Game")), ["Hades"]);
        // Dossier trop large : jamais comparé (l'entrée « Racine » irait à tous les jeux de E:).
        assert!(names(&game(Platform::Other, "x", r"E:\")).is_empty());
        // Amazon Games : par l'id passé au désinstalleur commun, jamais celui d'un autre jeu.
        assert_eq!(names(&game(Platform::Amazon, "amzn1.adg.product.abc", r"D:\Amazon\Quake")), ["AmazonGames/Quake"]);
        assert!(names(&game(Platform::Amazon, "amzn1.adg.product.ab", r"D:\Amazon\Autre")).is_empty());
        assert!(names(&game(Platform::Other, "amzn1.adg.product.abc", r"D:\Amazon\Autre")).is_empty());
        let wow = entries.iter().find(|e| e.name == "Steam App 570").unwrap();
        assert_eq!((wow.hive, wow.display_name.as_deref()), (Hive::Hklm, Some("Dota 2")));
    }

    #[test]
    fn splits_uninstall_commands_without_a_shell() {
        let sys = Path::new(r"C:\Windows\System32");
        let files = [r"C:\Program Files\Jeu\unins000.exe", r"C:\Program Files\Jeu Deux\uninstall.exe", r"C:\Windows\System32\msiexec.exe", r"C:\Windows\System32\cmd.exe", r"C:\Jeu\run.bat"];
        let exists = |p: &Path| files.iter().any(|f| Path::new(f) == p);
        let ok = |t: &str| parse_command(t, sys, exists).unwrap();
        assert_eq!(ok(r#""C:\Program Files\Jeu\unins000.exe""#), Command { exe: r"C:\Program Files\Jeu\unins000.exe".into(), args: String::new() });
        assert_eq!(ok(r#"  "C:\Program Files\Jeu\unins000.exe" /SILENT & del x"#).args, "/SILENT & del x", "passé tel quel, jamais interprété");
        assert_eq!(ok(r"C:\Program Files\Jeu Deux\uninstall.exe --mode remove").exe, PathBuf::from(r"C:\Program Files\Jeu Deux\uninstall.exe"));
        assert_eq!(ok("MsiExec.exe /X{1234-ABCD}"), Command { exe: r"C:\Windows\System32\msiexec.exe".into(), args: "/X{1234-ABCD}".into() });
        assert_eq!(ok("msiexec /I{1234}").args, "/I{1234}");

        let err = |t: &str| parse_command(t, sys, exists).unwrap_err();
        assert!(err("uninstall.exe").contains("chemin complet"));
        assert!(err(r"\\serveur\partage\unins.exe").contains("chemin complet"));
        assert!(err(r#""C:\Windows\System32\cmd.exe" /c rmdir /s"#).contains("cmd.exe"));
        assert!(err(r"C:\Windows\System32\RunDll32.exe x.dll,Remove").contains("rundll32.exe"));
        assert!(err(r#""C:\Jeu\run.bat""#).contains("n'est pas un programme"));
        assert!(err(r#""C:\Absent\unins000.exe""#).contains("introuvable"));
        assert!(err("C:\\Jeu\\unins.exe\n/X").contains("illisible"));
        assert!(err(r#""C:\Jeu\unins.exe"#).contains("illisible"), "guillemet jamais fermé");
        assert!(err("   ").contains("Aucune"));
        assert!(err(r"C:\Jeu\notes.txt").contains("aucun programme"));
    }

    #[test]
    fn lists_the_registry_keys_read_by_each_source() {
        use Value::Text;
        let mut reg = Fake::default();
        reg.add(Hive::Hkcu, r"Software\Valve\Steam\Apps\570", &[("Installed", Value::Dword(1))]);
        reg.add(Hive::Hklm, r"SOFTWARE\WOW6432Node\GOG.com\Games\1207658924", &[]);
        reg.add(Hive::Hklm, r"SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs\635", &[]);
        reg.add(Hive::Hklm, r"SOFTWARE\WOW6432Node\Rockstar Games\Grand Theft Auto V", &[("InstallFolder", Text(r"D:\Rockstar\GTAV"))]);
        reg.add(Hive::Hklm, r"SOFTWARE\WOW6432Node\Rockstar Games\Launcher", &[("InstallFolder", Text(r"C:\Rockstar\Launcher"))]);
        let packages = r"Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages";
        reg.add(Hive::Hkcu, &format!(r"{packages}\Microsoft.Minecraft_1.0.0.0_x64__8wekyb3d8bbwe"), &[]);
        reg.add(Hive::Hkcu, &format!(r"{packages}\Microsoft.Other_1.0.0.0_x64__8wekyb3d8bbwe"), &[]);
        reg.add(Hive::Hklm, r"SOFTWARE\WOW6432Node\EA Sports\FIFA 21", &[("Install Dir", Text(r"E:\EA\FIFA 21\"))]);

        let keys = |g: &Game| source_keys(g, &reg).into_iter().map(|k| k.path).collect::<Vec<_>>();
        assert_eq!(keys(&game(Platform::Steam, "570", r"E:\S\common\Dota")), [r"Software\Valve\Steam\Apps\570"]);
        assert!(keys(&game(Platform::Steam, "730", r"E:\S\common\CS")).is_empty(), "clé absente : pas listée");
        assert_eq!(keys(&game(Platform::Gog, "1207658924", r"D:\GOG\W")).len(), 1);
        assert_eq!(keys(&game(Platform::Ubisoft, "635", r"D:\Ubi\AC")).len(), 1);
        assert!(keys(&game(Platform::Ubisoft, r"635\..\..", r"D:\Ubi\AC")).is_empty(), "id refusé");
        assert_eq!(keys(&game(Platform::Rockstar, "grand-theft-auto-v", r"d:\rockstar\gtav\")), [r"SOFTWARE\WOW6432Node\Rockstar Games\Grand Theft Auto V"]);
        assert_eq!(keys(&game(Platform::Xbox, "Microsoft.Minecraft_8wekyb3d8bbwe", r"C:\XboxGames\Minecraft")), [format!(r"{packages}\Microsoft.Minecraft_1.0.0.0_x64__8wekyb3d8bbwe")]);
        assert_eq!(keys(&game(Platform::Ea, "fifa", r"E:\EA\FIFA 21")), [r"SOFTWARE\WOW6432Node\EA Sports\FIFA 21"]);
        assert!(keys(&game(Platform::Other, "x", r"E:\Jeux\X")).is_empty());
    }

    /// Faux PC : une bibliothèque Steam avec un jeu, son cache de shaders, ses mods, ses données de
    /// compte et une sauvegarde, puis les restes après la désinstallation.
    #[test]
    fn shows_what_a_game_leaves_then_its_leftovers() {
        let root = temp("steam");
        let steamapps = root.join("steamapps");
        let install = steamapps.join(r"common\Jeu");
        fs::create_dir_all(install.join("saves")).unwrap();
        fs::write(install.join("jeu.exe"), vec![0u8; 100]).unwrap();
        fs::write(install.join(r"saves\slot1.sav"), vec![0u8; 7]).unwrap();
        fs::create_dir_all(steamapps.join(r"shadercache\4242")).unwrap();
        fs::write(steamapps.join(r"shadercache\4242\cache.bin"), vec![0u8; 30]).unwrap();
        fs::create_dir_all(steamapps.join(r"workshop\content\4242\123")).unwrap();
        fs::write(steamapps.join(r"workshop\content\4242\123\mod.pak"), vec![0u8; 50]).unwrap();
        let account = root.join(r"userdata\99");
        fs::create_dir_all(account.join(r"4242\remote")).unwrap();
        fs::write(account.join(r"4242\remote\cloud.sav"), vec![0u8; 5]).unwrap();
        let docs_save = root.join(r"Documents\Jeu\profile.cfg");
        fs::create_dir_all(docs_save.parent().unwrap()).unwrap();
        fs::write(&docs_save, vec![0u8; 3]).unwrap();

        let mut reg = Fake::default();
        let entry = format!(r"{WOW_UNINSTALL}\Steam App 4242");
        reg.add(
            Hive::Hklm,
            &entry,
            &[
                ("DisplayName", Value::Text("Jeu")),
                ("Publisher", Value::Text("Studio")),
                ("InstallLocation", Value::Text(Box::leak(install.to_string_lossy().into_owned().into_boxed_str()))),
                ("UninstallString", Value::Text(r#""C:\Program Files (x86)\Steam\steam.exe" steam://uninstall/4242"#)),
                ("InstallDate", Value::Text("20240115")),
                ("EstimatedSize", Value::Dword(2048)),
            ],
        );
        reg.add(Hive::Hkcu, r"Software\Valve\Steam\Apps\4242", &[]);

        let mut g = game(Platform::Steam, "4242", &install.to_string_lossy());
        g.size_on_disk = Some(9_000);
        let saves = Saves { paths: vec![install.join("saves"), docs_save.clone(), install.clone(), account.join(r"4242\remote")], registry: true, note: None };
        let t = targets(&g, &reg, &uninstall_entries(&reg), saves, Some(&account));
        let items: Vec<&str> = t.folders.iter().map(|f| f.item.as_str()).collect();
        assert_eq!(items, ["install", "shaders", "workshop", "cloud", "save-0", "save-1", "save-2"], "dossier du jeu pas repris en sauvegarde");
        assert_eq!(t.uninstall, [(Hive::Hklm, entry.clone())]);
        assert!(t.steam && t.saves_registry);

        let steam_exe = |p: &Path| p.ends_with("steam.exe");
        let f = check(&t, &reg, Some(&g), size_of, steam_exe);
        assert!(f.installed && f.steam && !f.store_app);
        assert_eq!(f.leftovers, 0);
        let by = |item: &str| f.folders.iter().find(|x| x.item == item).unwrap().clone();
        assert_eq!(by("install").bytes, Some(9_000), "taille du launcher, pas remesurée");
        assert_eq!((by("shaders").bytes, by("workshop").bytes, by("cloud").bytes), (Some(30), Some(50), Some(5)));
        assert_eq!((by("save-0").inside.as_deref(), by("save-1").inside.as_deref()), (Some("install"), None));
        assert_eq!(by("save-2").inside.as_deref(), Some("cloud"), "sauvegarde Steam sous userdata/<appid>");
        assert!(by("save-1").file && by("save-1").exists);
        let u = &f.uninstall[0];
        assert_eq!(u.key, format!(r"HKLM\{entry}"));
        assert_eq!((u.declared_size, u.install_date.as_deref(), u.publisher.as_deref()), (Some(2048 * 1024), Some("2024-01-15"), Some("Studio")));
        assert_eq!(u.action, Action::Steam, "passe par Steam, jamais steam.exe lancé par Ludex");
        assert_eq!(f.keys, [Key { label: "État du jeu chez Steam".into(), key: r"HKCU\Software\Valve\Steam\Apps\4242".into(), exists: true }]);

        // Désinstallé par Steam : dossier du jeu et entrée partis, le reste est encore là.
        fs::remove_dir_all(&install).unwrap();
        reg.remove(Hive::Hklm, &entry);
        let after = check(&t, &reg, None, size_of, steam_exe);
        assert!(!after.installed && !after.steam);
        let left: Vec<&str> = after.folders.iter().filter(|f| f.exists).map(|f| f.item.as_str()).collect();
        assert_eq!(left, ["shaders", "workshop", "cloud", "save-1", "save-2"]);
        assert!(!after.uninstall[0].exists && after.keys[0].exists);
        assert_eq!(after.leftovers, 5, "4 dossiers ou fichiers (save-2 compté avec cloud) et la clé de Steam");
        assert_eq!(after.uninstall[0].action, Action::None);

        // Dossier du jeu encore là (fichiers ajoutés par le jeu) : ses sauvegardes comptent avec lui.
        fs::create_dir_all(install.join("saves")).unwrap();
        fs::write(install.join(r"saves\slot1.sav"), vec![0u8; 7]).unwrap();
        let again = check(&t, &reg, None, size_of, steam_exe);
        assert_eq!(again.folders[0].bytes, Some(7), "mesuré : le jeu n'est plus là pour donner sa taille");
        assert_eq!(again.leftovers, 6);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn offers_the_official_uninstaller_only_when_it_is_safe() {
        let root = temp("other");
        let dir = root.join("Jeu");
        fs::create_dir_all(&dir).unwrap();
        let unins = dir.join("unins000.exe");
        fs::write(&unins, b"MZ").unwrap();
        let unins_text: &'static str = Box::leak(format!(r#""{}" /LOG"#, unins.display()).into_boxed_str());
        let mut reg = Fake::default();
        reg.add(Hive::Hkcu, &format!(r"{HKCU_UNINSTALL}\Jeu"), &[("UninstallString", Value::Text(unins_text))]);
        reg.add(Hive::Hkcu, &format!(r"{HKCU_UNINSTALL}\Script"), &[
            ("InstallLocation", Value::Text(Box::leak(dir.to_string_lossy().into_owned().into_boxed_str()))),
            ("UninstallString", Value::Text(r#"C:\Windows\System32\cmd.exe /c rd /s /q C:\"#)),
        ]);
        reg.add(Hive::Hkcu, &format!(r"{HKCU_UNINSTALL}\Vide"), &[("InstallLocation", Value::Text(Box::leak(dir.to_string_lossy().into_owned().into_boxed_str())))]);
        let g = game(Platform::Other, "jeu", &dir.to_string_lossy());
        let t =targets(&g, &reg, &uninstall_entries(&reg), Saves { note: Some("Inconnu".into()), ..Saves::default() }, None);
        assert_eq!(t.uninstall.len(), 3);
        let f = check(&t, &reg, Some(&g), size_of, |p: &Path| p.is_file() || p.ends_with("cmd.exe"));
        assert!(!f.steam);
        assert_eq!(f.folders[0].bytes, None, "installation pas encore mesurée");
        let by = |key: &str| f.uninstall.iter().find(|u| u.key.ends_with(key)).unwrap();
        assert_eq!((by(r"\Jeu").action, by(r"\Jeu").refusal.as_deref()), (Action::Run, None));
        assert_eq!(by(r"\Script").action, Action::None);
        assert!(by(r"\Script").refusal.as_deref().unwrap().contains("cmd.exe"));
        assert!(by(r"\Vide").refusal.as_deref().unwrap().contains("pas de commande"));
        assert_eq!(f.saves_note.as_deref(), Some("Inconnu"));
        let _ = fs::remove_dir_all(&root);
    }

    /// Relevé réel en lecture seule : `cargo test footprint_this_pc -- --ignored --nocapture`.
    /// Aucun fichier ni clé n'est modifié, aucun désinstalleur lancé ; sauvegardes non relues (la
    /// base de l'app n'est pas touchée). Puis les restes de jeux Steam déjà désinstallés.
    #[test]
    #[ignore]
    fn footprint_this_pc() {
        let started = std::time::Instant::now();
        let games = crate::library::scan().games;
        let scanned = started.elapsed();
        let t0 = std::time::Instant::now();
        let entries = uninstall_entries(&System);
        println!("{} jeux (scan {:.1} s), {} entrées de désinstallation lues en {} ms", games.len(), scanned.as_secs_f32(), entries.len(), t0.elapsed().as_millis());
        let account = crate::library::steam_account_dir();
        let mut seen: Vec<Platform> = Vec::new();
        let mut without = Vec::new();
        let mut by_platform: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        for g in &games {
            let t = targets(g, &System, &entries, Saves::default(), account.as_deref());
            let stat = by_platform.entry(format!("{:?}", g.platform)).or_default();
            stat.0 += 1;
            if t.uninstall.is_empty() {
                without.push(format!("{} ({:?})", g.title, g.platform));
            } else {
                stat.1 += 1;
            }
            // Détail : les deux premiers jeux de chaque source.
            if seen.iter().filter(|p| **p == g.platform).count() >= 2 {
                continue;
            }
            seen.push(g.platform);
            let t1 = std::time::Instant::now();
            let f = check(&t, &System, Some(g), size_of, |p: &Path| p.is_file());
            println!("\n{} ({:?}, {} ms)", f.title, f.platform, t1.elapsed().as_millis());
            for d in &f.folders {
                let size = d.bytes.map_or("?".to_string(), |b| format!("{:.1} Mo", b as f64 / 1e6));
                println!("  [{}] {} : {} ({size}{})", d.item, d.label, d.path, if d.exists { "" } else { ", absent" });
            }
            for u in &f.uninstall {
                println!(
                    "  Désinstallation : {} | {} | éditeur {:?} | {:?} Mo déclarés | {:?} -> {:?} {}",
                    u.key,
                    u.name.as_deref().unwrap_or("?"),
                    u.publisher,
                    u.declared_size.map(|b| b / 1_000_000),
                    u.command,
                    u.action,
                    u.refusal.as_deref().unwrap_or("")
                );
            }
            for k in &f.keys {
                println!("  Registre : {} ({})", k.key, k.label);
            }
        }
        println!("\nEntrée de désinstallation trouvée, par source (jeux avec / total) :");
        for (p, (total, with)) in &by_platform {
            println!("  {p} : {with} / {total}");
        }
        println!("Sans entrée : {without:?}");

        // Restes de jeux Steam désinstallés : caches de shaders et mods d'appids absents.
        let installed: Vec<&str> = games.iter().filter(|g| g.platform == Platform::Steam).map(|g| g.platform_id.as_str()).collect();
        let mut total = 0u64;
        let mut count = 0;
        for steamapps in crate::library::watched_dirs().steamapps {
            for sub in ["shadercache", r"workshop\content"] {
                let Ok(list) = fs::read_dir(steamapps.join(sub)) else { continue };
                for e in list.flatten() {
                    let name = e.file_name().to_string_lossy().into_owned();
                    if name.bytes().all(|b| b.is_ascii_digit()) && !installed.contains(&name.as_str()) {
                        let size = size_of(&e.path()).unwrap_or(0);
                        total += size;
                        count += 1;
                        println!("  Reste Steam : {} ({:.1} Mo)", e.path().display(), size as f64 / 1e6);
                    }
                }
            }
        }
        println!("{count} restes Steam d'appids désinstallés, {:.1} Mo", total as f64 / 1e6);
    }
}
