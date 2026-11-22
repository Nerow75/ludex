//! Jeux d'émulateurs : émulateurs installés sur ce PC (RetroArch et ses cœurs, Dolphin, PCSX2,
//! DuckStation, PPSSPP, Cemu, RPCS3, xemu, Ryujinx et ses successeurs, Azahar, melonDS, mGBA…),
//! puis ROMs des dossiers que leur configuration déclare, des dossiers de jeux nommés d'après une
//! console (« Jeux Switch ») et des dossiers choisis dans les Paramètres.
//!
//! Lecture seule : aucun fichier d'émulateur n'est modifié ; ni ROM, ni BIOS, ni firmware, ni clé
//! n'est téléchargé ; une ROM n'est lue que pour reconnaître sa console (en-tête de disque).
//! Lancement : l'exécutable de l'émulateur, ses options, puis le chemin de la ROM en argument du
//! processus (jamais une chaîne de shell), reconstruits ici depuis l'état.

mod emulators;
mod roms;
pub mod systems;

use super::{art_url, Game, Launch, Platform};
use emulators::{Args, Found};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub use emulators::EMULATORS;
pub use roms::{clean_title, region_rank};

/// Dossiers de ROMs ajoutés à la main : au plus.
pub const MAX_ROM_DIRS: usize = 50;
/// Écarts notés dans le diagnostic : au plus.
const MAX_SKIPPED: usize = 200;

/// Réglages de l'émulation (`settings.emulation` de `user-data.json`).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Prefs {
    /// Dossiers de ROMs choisis dans les Paramètres (propres à ce PC).
    pub rom_dirs: Vec<PathBuf>,
    /// Émulateur choisi par console : `{ "gba": "retroarch" }`.
    pub choices: BTreeMap<String, String>,
}

impl Prefs {
    /// Réglages relus : chemins absolus sans doublon, consoles et émulateurs connus seulement.
    pub fn clean(self) -> Prefs {
        let mut seen = HashSet::new();
        let rom_dirs = self
            .rom_dirs
            .into_iter()
            .filter(|d| d.is_absolute() && !d.components().any(|c| matches!(c, std::path::Component::ParentDir)) && seen.insert(path_key(d)))
            .take(MAX_ROM_DIRS)
            .collect();
        let choices = self
            .choices
            .into_iter()
            .filter(|(s, e)| systems::find(s).is_some() && emulators::find(e).is_some_and(|def| def.args == Args::RetroArch || def.systems.contains(&s.as_str())))
            .collect();
        Prefs { rom_dirs, choices }
    }
}

/// Console et émulateur d'un jeu, envoyés au front (fiche).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmuInfo {
    pub system: &'static str,
    pub system_name: &'static str,
    pub emulator: &'static str,
    pub emulator_key: &'static str,
    /// Nom du fichier de la ROM sans extension (jaquettes libretro), jamais envoyé.
    #[serde(skip)]
    pub stem: String,
}

/// Réglages en vigueur, posés avant chaque scan (`configure`).
static PREFS: Mutex<Option<Prefs>> = Mutex::new(None);
/// Dossiers des émulateurs trouvés au dernier scan (jamais des jeux « Autres »).
static EMULATOR_DIRS: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Résumé du dernier scan (Paramètres, diagnostic).
static LAST: Mutex<Option<Summary>> = Mutex::new(None);

/// Pose les réglages lus par le prochain scan.
pub fn configure(prefs: &Prefs) {
    *PREFS.lock().unwrap_or_else(|e| e.into_inner()) = Some(prefs.clone());
}

fn prefs() -> Prefs {
    PREFS.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default()
}

/// Clé de comparaison d'un chemin : casse et séparateurs ignorés.
pub fn path_key(path: &Path) -> String {
    super::dir_key(&path.to_string_lossy())
}

/// Le dossier est-il celui d'un émulateur trouvé (ou le contient-il) ? « Autres » l'écarte.
pub fn is_emulator_dir(dir: &Path) -> bool {
    let key = path_key(dir);
    EMULATOR_DIRS.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|d| d == &key || super::is_inside(d, &key))
}

/// Émulateurs et dossiers vus au dernier scan, pour les Paramètres et le diagnostic.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub emulators: Vec<EmulatorView>,
    pub systems: Vec<SystemView>,
    pub folders: Vec<FolderView>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmulatorView {
    pub key: &'static str,
    pub name: &'static str,
    pub path: String,
    pub systems: Vec<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemView {
    pub id: &'static str,
    pub name: &'static str,
    pub games: usize,
    /// Émulateurs installés qui savent lancer cette console.
    pub emulators: Vec<Choice>,
    /// Émulateur choisi dans les Paramètres, `None` : choix automatique.
    pub chosen: Option<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    pub key: &'static str,
    pub name: &'static str,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderView {
    pub path: String,
    /// `user` (Paramètres), `emulator` (configuration d'un émulateur) ou `folder` (dossier nommé
    /// d'après une console).
    pub origin: &'static str,
    /// Émulateur qui déclare le dossier.
    pub emulator: Option<&'static str>,
    /// Le dossier existe et a été parcouru.
    pub found: bool,
}

/// Résumé du dernier scan, `None` avant le premier.
pub fn summary() -> Option<Summary> {
    LAST.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Dossier de ROMs à parcourir.
struct Root {
    dir: PathBuf,
    system: Option<&'static systems::System>,
    owner: Option<&'static str>,
    origin: &'static str,
}

/// Source « Émulation ». `Ok(None)` : aucun émulateur et aucun dossier de ROMs choisi.
pub fn scan() -> Result<Option<Vec<Game>>, String> {
    let prefs = prefs();
    let drives = drives();
    let libraries = super::local::library_dirs();
    let search = emulators::search_dirs(&drives, &libraries, &super::local::uninstall_locations());
    let found = find_emulators(&search);
    *EMULATOR_DIRS.lock().unwrap_or_else(|e| e.into_inner()) = found.iter().map(|f| path_key(f.dir())).collect();
    let roots = rom_roots(&found, &prefs, &drives, &libraries);
    let (games, summary) = build(&found, &roots, &prefs);
    let present = !found.is_empty() || !prefs.rom_dirs.is_empty();
    *LAST.lock().unwrap_or_else(|e| e.into_inner()) = Some(summary);
    Ok(present.then_some(games))
}

fn drives() -> Vec<PathBuf> {
    (b'A'..=b'Z').map(|d| PathBuf::from(format!("{}:\\", d as char))).filter(|p| p.is_dir()).collect()
}

/// Émulateurs des dossiers donnés, dans l'ordre de la table, sans doublon d'exécutable.
fn find_emulators(dirs: &[PathBuf]) -> Vec<Found> {
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    for dir in dirs {
        if let Some((def, exe)) = emulators::detect(dir)
            && seen.insert(path_key(&exe))
        {
            found.push(emulators::inspect(def, exe));
        }
    }
    let order = |f: &Found| EMULATORS.iter().position(|e| e.key == f.def.key).unwrap_or(usize::MAX);
    found.sort_by_key(order);
    found
}

/// Dossier de ROMs utilisable : absolu, existant, ni racine d'un disque, ni dossier système,
/// ni profil, Documents, Bureau ou AppData entiers.
fn usable_root(dir: &Path) -> bool {
    if !dir.is_absolute() || dir.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return false;
    }
    if crate::session::root_key(&dir.to_string_lossy()).is_none() {
        return false;
    }
    let key = path_key(dir);
    let wide = [dirs::home_dir(), dirs::document_dir(), dirs::desktop_dir(), dirs::config_dir(), dirs::data_local_dir()];
    if wide.iter().flatten().any(|w| path_key(w) == key) {
        return false;
    }
    std::fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir() || m.file_type().is_symlink()) && dir.is_dir()
}

/// Dossiers de ROMs : choisis dans les Paramètres, déclarés par les émulateurs, puis dossiers de
/// jeux nommés d'après une console (« E:\Jeux Switch ») et dossiers « ROMs » à la racine des disques.
fn rom_roots(found: &[Found], prefs: &Prefs, drives: &[PathBuf], libraries: &[PathBuf]) -> Vec<Root> {
    let mut roots = Vec::new();
    for dir in &prefs.rom_dirs {
        roots.push(Root { dir: dir.clone(), system: None, owner: None, origin: "user" });
    }
    for f in found {
        let single = (f.def.systems.len() == 1).then(|| systems::find(f.def.systems[0])).flatten();
        for dir in &f.rom_dirs {
            roots.push(Root { dir: dir.clone(), system: single, owner: Some(f.def.key), origin: "emulator" });
        }
    }
    let emulator_dirs: Vec<String> = found.iter().map(|f| path_key(f.dir())).collect();
    let named = |dir: PathBuf, roots: &mut Vec<Root>| {
        let key = path_key(&dir);
        if emulator_dirs.iter().any(|e| e == &key || super::is_inside(e, &key)) {
            return; // dossier d'un émulateur : ses dossiers de jeux viennent de lui
        }
        if let Some(system) = dir.file_name().and_then(|n| n.to_str()).and_then(systems::by_folder) {
            roots.push(Root { dir, system: Some(system), owner: None, origin: "folder" });
        }
    };
    for lib in libraries {
        named(lib.clone(), &mut roots);
        for sub in emulators::subdirs(lib) {
            named(sub, &mut roots);
        }
    }
    const ROM_DIRS: &[&str] = &["roms", "rom", "emulation", "emulateurs", "emulators", "retrogaming"];
    for drive in drives {
        for dir in emulators::subdirs(drive) {
            let name = dir.file_name().and_then(|n| n.to_str()).map(systems::compact).unwrap_or_default();
            if ROM_DIRS.contains(&name.as_str()) {
                roots.push(Root { dir, system: None, owner: None, origin: "folder" });
            }
        }
    }
    roots
}

/// ROM et émulateur qui la déclare.
type Owned = (roms::Rom, Option<&'static str>);

/// Parcourt les dossiers, regroupe les doublons et fabrique les jeux. Écarts notés par `skip`.
fn build(found: &[Found], roots: &[Root], prefs: &Prefs) -> (Vec<Game>, Summary) {
    let mut seen_dirs = HashSet::new();
    let mut all: Vec<Owned> = Vec::new();
    let mut skipped: Vec<roms::Skip> = Vec::new();
    let mut folders = Vec::new();
    let mut listed = HashSet::new();
    for root in roots {
        let usable = usable_root(&root.dir);
        if listed.insert(path_key(&root.dir)) {
            folders.push(FolderView {
                path: root.dir.to_string_lossy().into_owned(),
                origin: root.origin,
                emulator: root.owner.and_then(emulators::find).map(|e| e.name),
                found: usable,
            });
        }
        if !usable {
            continue;
        }
        let mut out = Vec::new();
        roms::scan_root(&root.dir, root.system, &mut seen_dirs, &mut out, &mut skipped);
        all.extend(out.into_iter().map(|r| (r, root.owner)));
    }
    // Une version par jeu : région préférée, premier disque, chemin le plus court.
    all.sort_by(|(a, _), (b, _)| a.key().cmp(&b.key()).then(a.rank.cmp(&b.rank)).then(a.path.as_os_str().len().cmp(&b.path.as_os_str().len())).then(a.path.cmp(&b.path)));
    let mut keys = HashSet::new();
    let mut games = Vec::new();
    let mut ids = HashSet::new();
    let mut per_system: BTreeMap<&'static str, usize> = BTreeMap::new();
    for (rom, owner) in all {
        let name = rom.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if !keys.insert(rom.key()) {
            skipped.push((name, tr!("Autre version du même jeu déjà retenue", "Another version of the same game already kept")));
            continue;
        }
        let Some(emulator) = pick(found, rom.system.id, owner, prefs) else {
            skipped.push((name, tr!("Aucun émulateur installé pour {}", "No emulator installed for {}", rom.system.name)));
            continue;
        };
        match make_game(&rom, emulator) {
            Some(game) if ids.insert(game.id.clone()) => {
                *per_system.entry(rom.system.id).or_default() += 1;
                games.push(game);
            }
            Some(_) => skipped.push((name, tr!("Même identifiant qu'un autre jeu", "Same identifier as another game"))),
            None => skipped.push((name, tr!("Chemin illisible", "Unreadable path"))),
        }
    }
    let total = skipped.len();
    for (title, reason) in skipped.iter().take(MAX_SKIPPED) {
        super::skip(title, reason);
    }
    if total > MAX_SKIPPED {
        super::skip(&tr!("{} autres fichiers", "{} more files", total - MAX_SKIPPED), tr_str!("Écarts suivants non détaillés", "Further skips not listed"));
    }
    (games, summarize(found, prefs, &per_system, folders))
}

/// Émulateur d'une console : celui choisi dans les Paramètres, sinon celui dont la configuration
/// déclare le dossier, sinon le premier de la table.
fn pick<'a>(found: &'a [Found], system: &str, owner: Option<&str>, prefs: &Prefs) -> Option<&'a Found> {
    let able: Vec<&Found> = found.iter().filter(|f| f.supports(system)).collect();
    let by_key = |key: &str| able.iter().copied().find(|f| f.def.key == key);
    prefs.choices.get(system).and_then(|k| by_key(k)).or_else(|| owner.and_then(by_key)).or_else(|| able.first().copied())
}

/// Identifiant lisible tiré du titre : `zelda-a-link-to-the-past`.
pub fn slug(title: &str) -> String {
    let mut out = String::new();
    for c in title.chars() {
        match systems::fold(c) {
            Some(c) if c.is_ascii_alphanumeric() => out.push(c),
            _ if !out.ends_with('-') && !out.is_empty() => out.push('-'),
            _ => {}
        }
        if out.len() >= 60 {
            break;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() { format!("{:016x}", crate::images::fnv(title)) } else { out }
}

/// Jeu de la bibliothèque : émulateur, options puis ROM en arguments du processus.
fn make_game(rom: &roms::Rom, emulator: &Found) -> Option<Game> {
    let rom_path = rom.path.to_str()?.to_string();
    let mut args: Vec<String> = match emulator.def.args {
        Args::Plain => Vec::new(),
        Args::Before(before) => before.iter().map(|s| s.to_string()).collect(),
        Args::RetroArch => {
            let core = emulator.core_for_rom(&rom.path, rom.system.id)?;
            vec!["-L".to_string(), core.to_str()?.to_string()]
        }
    };
    args.push(rom_path);
    let id = match &rom.title_id {
        Some(t) => format!("{}-{t}", rom.system.id),
        None => format!("{}-{}", rom.system.id, slug(&rom.title)),
    };
    let exe = emulator.exe.clone();
    let mut game = Game::new(Platform::Emulation, &id, &rom.title, Launch::Exec { path: exe.clone(), args });
    game.install_dir = Some(rom.dir.to_string_lossy().into_owned());
    game.size_on_disk = (rom.size > 0).then_some(rom.size);
    let exe_name = exe.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let rom_name = rom.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    game.launch_label = format!("{exe_name} {rom_name}");
    game.local_art.icon = Some(exe);
    game.icon_url = Some(art_url(&game, "icon"));
    game.emulation = Some(EmuInfo {
        system: rom.system.id,
        system_name: rom.system.name,
        emulator: emulator.def.name,
        emulator_key: emulator.def.key,
        stem: rom.stem.clone(),
    });
    Some(game)
}

fn summarize(found: &[Found], prefs: &Prefs, per_system: &BTreeMap<&'static str, usize>, folders: Vec<FolderView>) -> Summary {
    let emulators = found
        .iter()
        .map(|f| EmulatorView { key: f.def.key, name: f.def.name, path: f.exe.to_string_lossy().into_owned(), systems: f.systems() })
        .collect();
    let systems = systems::SYSTEMS
        .iter()
        .filter_map(|s| {
            let mut choices: Vec<Choice> = Vec::new();
            for f in found.iter().filter(|f| f.supports(s.id)) {
                if !choices.iter().any(|c| c.key == f.def.key) {
                    choices.push(Choice { key: f.def.key, name: f.def.name });
                }
            }
            let games = per_system.get(s.id).copied().unwrap_or(0);
            if choices.is_empty() && games == 0 {
                return None;
            }
            let chosen = prefs.choices.get(s.id).and_then(|k| choices.iter().find(|c| c.key == k)).map(|c| c.key);
            Some(SystemView { id: s.id, name: s.name, games, emulators: choices, chosen })
        })
        .collect();
    Summary { emulators, systems, folders }
}

/// Dossier de ROMs choisi dans les Paramètres : refusé s'il est trop large (disque, profil,
/// Documents, Windows…) ou s'il n'existe pas. Renvoie le chemin gardé.
pub fn check_rom_dir(dir: &Path) -> Result<PathBuf, String> {
    if usable_root(dir) {
        Ok(dir.to_path_buf())
    } else {
        Err(tr!(
            "Choisis un dossier de ROMs précis : ni la racine d'un disque, ni ton profil, Documents ou Windows.",
            "Pick a specific ROM folder: not the root of a drive, your profile, Documents or Windows."
        ))
    }
}

/// Un émulateur choisi pour une console est-il connu et capable de la lancer ? (Paramètres)
pub fn valid_choice(system: &str, emulator: &str) -> bool {
    systems::find(system).is_some() && emulators::find(emulator).is_some_and(|e| e.args == Args::RetroArch || e.systems.contains(&system))
}

#[cfg(test)]
mod tests;
