//! Émulateurs connus : où les trouver, comment les lancer, et les dossiers de ROMs que leur
//! configuration déclare. Tout est lu en lecture seule ; aucun fichier de l'émulateur n'est écrit.

use super::systems;
use std::fs;
use std::path::{Path, PathBuf};

/// Arguments placés avant le chemin de la ROM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Args {
    /// `<exe> <rom>`.
    Plain,
    /// `<exe> <options…> <rom>`.
    Before(&'static [&'static str]),
    /// RetroArch : `<exe> -L <cœur> <rom>`.
    RetroArch,
}

/// Où l'émulateur garde les dossiers de jeux qu'on lui a montrés.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Config {
    None,
    RetroArch,
    Dolphin,
    Pcsx2,
    DuckStation,
    Ppsspp,
    Cemu,
    Rpcs3,
    Xemu,
    Ryujinx,
    /// Famille Qt de yuzu et Citra : `qt-config.ini`, dossier `%APPDATA%\<nom>` hors portable.
    Qt(&'static str),
    MelonDs,
    Mgba,
}

pub struct Emulator {
    /// Identifiant stable (réglages) : `dolphin`, `retroarch`…
    pub key: &'static str,
    pub name: &'static str,
    /// Noms d'exécutables en minuscules ; `*` final = préfixe (`duckstation-qt*`).
    pub exes: &'static [&'static str],
    pub systems: &'static [&'static str],
    pub args: Args,
    pub config: Config,
}

/// Table des émulateurs, dans l'ordre de préférence quand plusieurs savent lancer une console
/// (RetroArch en dernier : un émulateur dédié est en général préféré).
pub const EMULATORS: &[Emulator] = &[
    Emulator { key: "dolphin", name: "Dolphin", exes: &["dolphin.exe"], systems: &["gc", "wii"], args: Args::Before(&["-b", "-e"]), config: Config::Dolphin },
    Emulator { key: "pcsx2", name: "PCSX2", exes: &["pcsx2-qt*"], systems: &["ps2"], args: Args::Before(&["-batch", "--"]), config: Config::Pcsx2 },
    Emulator { key: "pcsx2-legacy", name: "PCSX2 1.6", exes: &["pcsx2.exe", "pcsx2x64.exe"], systems: &["ps2"], args: Args::Plain, config: Config::None },
    Emulator { key: "duckstation", name: "DuckStation", exes: &["duckstation-qt*", "duckstation*"], systems: &["ps1"], args: Args::Before(&["-batch", "--"]), config: Config::DuckStation },
    Emulator { key: "ppsspp", name: "PPSSPP", exes: &["ppssppwindows64.exe", "ppssppwindows.exe"], systems: &["psp"], args: Args::Plain, config: Config::Ppsspp },
    Emulator { key: "cemu", name: "Cemu", exes: &["cemu.exe"], systems: &["wiiu"], args: Args::Before(&["-g"]), config: Config::Cemu },
    Emulator { key: "rpcs3", name: "RPCS3", exes: &["rpcs3.exe"], systems: &["ps3"], args: Args::Before(&["--no-gui"]), config: Config::Rpcs3 },
    Emulator { key: "xemu", name: "xemu", exes: &["xemu.exe"], systems: &["xbox"], args: Args::Before(&["-dvd_path"]), config: Config::Xemu },
    Emulator { key: "ryujinx", name: "Ryujinx", exes: &["ryujinx.exe", "ryujinx.ava.exe"], systems: &["switch"], args: Args::Plain, config: Config::Ryujinx },
    Emulator { key: "eden", name: "Eden", exes: &["eden.exe"], systems: &["switch"], args: Args::Before(&["-g"]), config: Config::Qt("eden") },
    Emulator { key: "citron", name: "Citron", exes: &["citron.exe"], systems: &["switch"], args: Args::Before(&["-g"]), config: Config::Qt("citron") },
    Emulator { key: "sudachi", name: "Sudachi", exes: &["sudachi.exe"], systems: &["switch"], args: Args::Before(&["-g"]), config: Config::Qt("sudachi") },
    Emulator { key: "suyu", name: "suyu", exes: &["suyu.exe"], systems: &["switch"], args: Args::Before(&["-g"]), config: Config::Qt("suyu") },
    Emulator { key: "yuzu", name: "yuzu", exes: &["yuzu.exe"], systems: &["switch"], args: Args::Before(&["-g"]), config: Config::Qt("yuzu") },
    Emulator { key: "azahar", name: "Azahar", exes: &["azahar.exe", "azahar-qt.exe"], systems: &["n3ds"], args: Args::Plain, config: Config::Qt("Azahar") },
    Emulator { key: "lime3ds", name: "Lime3DS", exes: &["lime3ds-gui.exe", "lime3ds-qt.exe", "lime3ds.exe"], systems: &["n3ds"], args: Args::Plain, config: Config::Qt("Lime3DS") },
    Emulator { key: "citra", name: "Citra", exes: &["citra-qt.exe", "citra.exe"], systems: &["n3ds"], args: Args::Plain, config: Config::Qt("Citra") },
    Emulator { key: "melonds", name: "melonDS", exes: &["melonds.exe"], systems: &["nds"], args: Args::Plain, config: Config::MelonDs },
    Emulator { key: "mgba", name: "mGBA", exes: &["mgba.exe"], systems: &["gba", "gb", "gbc"], args: Args::Plain, config: Config::Mgba },
    Emulator { key: "mesen", name: "Mesen", exes: &["mesen.exe"], systems: &["nes", "snes", "gb", "gbc", "pce", "sms", "gg"], args: Args::Plain, config: Config::None },
    Emulator { key: "snes9x", name: "Snes9x", exes: &["snes9x-x64.exe", "snes9x.exe"], systems: &["snes"], args: Args::Plain, config: Config::None },
    Emulator { key: "project64", name: "Project64", exes: &["project64.exe"], systems: &["n64"], args: Args::Plain, config: Config::None },
    Emulator { key: "flycast", name: "Flycast", exes: &["flycast.exe"], systems: &["dreamcast"], args: Args::Plain, config: Config::None },
    Emulator { key: "retroarch", name: "RetroArch", exes: &["retroarch.exe"], systems: &[], args: Args::RetroArch, config: Config::RetroArch },
];

/// Cœurs RetroArch par console, du préféré au moins bon (fichier `<nom>_libretro.dll`).
pub const CORES: &[(&str, &[&str])] = &[
    ("nes", &["mesen", "nestopia", "fceumm", "quicknes"]),
    ("snes", &["snes9x", "bsnes", "mesen-s", "snes9x2010"]),
    ("n64", &["mupen64plus_next", "parallel_n64"]),
    ("gb", &["gambatte", "sameboy", "mgba", "gearboy"]),
    ("gbc", &["gambatte", "sameboy", "mgba", "gearboy"]),
    ("gba", &["mgba", "vba_next", "vbam", "gpsp"]),
    ("nds", &["melondsds", "melonds", "desmume"]),
    ("n3ds", &["citra", "azahar"]),
    ("gc", &["dolphin"]),
    ("wii", &["dolphin"]),
    ("ps1", &["swanstation", "duckstation", "mednafen_psx_hw", "mednafen_psx", "pcsx_rearmed"]),
    ("ps2", &["pcsx2", "lrps2"]),
    ("psp", &["ppsspp"]),
    ("md", &["genesis_plus_gx", "picodrive", "blastem"]),
    ("sms", &["genesis_plus_gx", "picodrive", "gearsystem"]),
    ("gg", &["genesis_plus_gx", "gearsystem"]),
    ("saturn", &["mednafen_saturn", "kronos", "yabause"]),
    ("dreamcast", &["flycast"]),
    ("pce", &["mednafen_pce", "mednafen_pce_fast"]),
    ("a2600", &["stella", "stella2014"]),
];

pub fn find(key: &str) -> Option<&'static Emulator> {
    EMULATORS.iter().find(|e| e.key == key)
}

/// Émulateur dont un nom d'exécutable correspond.
pub fn by_exe(file: &str) -> Option<&'static Emulator> {
    let file = file.to_ascii_lowercase();
    if !file.ends_with(".exe") {
        return None;
    }
    EMULATORS.iter().find(|e| {
        e.exes.iter().any(|x| match x.strip_suffix('*') {
            Some(prefix) => file.starts_with(prefix),
            None => file == *x,
        })
    })
}

/// Émulateur trouvé sur ce PC.
#[derive(Clone, Debug)]
pub struct Found {
    pub def: &'static Emulator,
    pub exe: PathBuf,
    /// Cœurs RetroArch présents (vide pour les autres).
    pub cores: Vec<PathBuf>,
    /// Historique RetroArch : ROM et cœur avec lequel elle a été lancée.
    pub history: Vec<(PathBuf, PathBuf)>,
    /// Dossiers de jeux déclarés par la configuration, puis sous-dossiers « roms », « games »…
    pub rom_dirs: Vec<PathBuf>,
}

impl std::fmt::Debug for Emulator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.key)
    }
}

impl Found {
    pub fn dir(&self) -> &Path {
        self.exe.parent().unwrap_or(&self.exe)
    }

    /// Consoles que cet émulateur sait lancer (RetroArch : celles dont un cœur est installé).
    pub fn systems(&self) -> Vec<&'static str> {
        if self.def.args == Args::RetroArch {
            return CORES.iter().filter(|(s, _)| self.core_for(s).is_some()).map(|(s, _)| *s).collect();
        }
        self.def.systems.to_vec()
    }

    pub fn supports(&self, system: &str) -> bool {
        self.systems().contains(&system)
    }

    /// Cœur RetroArch préféré pour une console, parmi ceux installés.
    pub fn core_for(&self, system: &str) -> Option<&PathBuf> {
        let (_, names) = CORES.iter().find(|(s, _)| *s == system)?;
        names.iter().find_map(|name| {
            let file = format!("{name}_libretro.dll");
            self.cores.iter().find(|c| c.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.eq_ignore_ascii_case(&file)))
        })
    }

    /// Cœur à utiliser pour une ROM : celui de l'historique RetroArch s'il est installé, sinon
    /// le préféré de sa console.
    pub fn core_for_rom(&self, rom: &Path, system: &str) -> Option<PathBuf> {
        let key = super::path_key(rom);
        let from_history = self
            .history
            .iter()
            .find(|(r, _)| super::path_key(r) == key)
            .and_then(|(_, core)| self.cores.iter().find(|c| super::path_key(c) == super::path_key(core)));
        from_history.or_else(|| self.core_for(system)).cloned()
    }
}

/// Dossiers où chercher des émulateurs : Program Files, `%LOCALAPPDATA%\Programs`, dossiers à la
/// racine des disques (et un niveau plus bas pour « Logiciels », « Émulateurs »…), sous-dossiers
/// des dossiers de jeux, emplacements de la désinstallation Windows, applis Scoop.
pub fn search_dirs(drives: &[PathBuf], libraries: &[PathBuf], uninstall: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let env = |name: &str| std::env::var_os(name).map(PathBuf::from);
    for base in [env("ProgramFiles"), env("ProgramFiles(x86)"), env("LOCALAPPDATA").map(|d| d.join("Programs"))].into_iter().flatten() {
        dirs.extend(subdirs(&base));
    }
    for root in drives {
        for dir in subdirs(root) {
            let name = dir.file_name().and_then(|n| n.to_str()).map(systems::compact).unwrap_or_default();
            if is_container(&name) {
                dirs.extend(subdirs(&dir));
            }
            dirs.push(dir);
        }
    }
    for lib in libraries {
        dirs.extend(subdirs(lib));
    }
    dirs.extend(uninstall.iter().cloned());
    if let Some(home) = dirs::home_dir() {
        dirs.extend(subdirs(&home.join(r"scoop\apps")).into_iter().map(|d| d.join("current")));
    }
    dirs
}

/// Dossier qui range des logiciels (« Logiciel SSD », « Emulators », « Apps »…) : ses sous-dossiers
/// sont lus aussi.
fn is_container(name: &str) -> bool {
    const WORDS: &[&str] = &["emul", "logiciel", "software", "program", "apps", "tools", "outils", "portable", "retro"];
    WORDS.iter().any(|w| name.contains(w))
}

/// Sous-dossiers triés, liens et jonctions écartés.
pub fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let mut list: Vec<PathBuf> = fs::read_dir(dir)
        .map(|it| {
            it.flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir() && !t.is_symlink()))
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default();
    list.sort();
    list
}

/// Émulateur posé dans ce dossier, ou un niveau plus bas si le sous-dossier porte un nom
/// d'émulateur (archive décompressée dans un dossier du même nom : `RetroArch-Win64\RetroArch-Win64`).
pub fn detect(dir: &Path) -> Option<(&'static Emulator, PathBuf)> {
    let direct = |d: &Path| -> Option<(&'static Emulator, PathBuf)> {
        let mut files: Vec<PathBuf> = fs::read_dir(d)
            .ok()?
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .map(|e| e.path())
            .collect();
        files.sort();
        // Ordre de la table : `pcsx2-qt` avant `pcsx2`, `duckstation-qt` avant le reste.
        EMULATORS.iter().find_map(|def| {
            files.iter().find(|f| f.file_name().and_then(|n| n.to_str()).and_then(by_exe).is_some_and(|e| e.key == def.key)).map(|f| (def, f.clone()))
        })
    };
    direct(dir).or_else(|| {
        subdirs(dir).into_iter().find_map(|sub| {
            let name = sub.file_name().and_then(|n| n.to_str()).map(systems::compact).unwrap_or_default();
            let named = EMULATORS.iter().any(|e| name.contains(&e.key.replace('-', "")) || name.contains(&systems::compact(e.name)));
            if named { direct(&sub) } else { None }
        })
    })
}

/// Lit la configuration de l'émulateur trouvé : cœurs, historique et dossiers de jeux.
pub fn inspect(def: &'static Emulator, exe: PathBuf) -> Found {
    let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut found = Found { def, exe, cores: Vec::new(), history: Vec::new(), rom_dirs: Vec::new() };
    let mut dirs: Vec<PathBuf> = Vec::new();
    let appdata = dirs::config_dir().unwrap_or_default();
    let local = dirs::data_local_dir().unwrap_or_default();
    let documents = dirs::document_dir().unwrap_or_default();
    match def.config {
        Config::None => {}
        Config::RetroArch => read_retroarch(&dir, &appdata, &mut found, &mut dirs),
        Config::Dolphin => {
            let user = if dir.join("portable.txt").is_file() {
                vec![dir.join("User")]
            } else {
                vec![appdata.join("Dolphin Emulator"), documents.join("Dolphin Emulator"), dir.join("User")]
            };
            for u in user {
                let ini = read_ini(&u.join(r"Config\Dolphin.ini"));
                dirs.extend(ini.iter().filter(|(s, k, _)| s == "general" && k.starts_with("isopath") && k != "isopaths").map(|(_, _, v)| PathBuf::from(v)));
            }
        }
        Config::Pcsx2 => {
            for ini in [dir.join(r"inis\PCSX2.ini"), documents.join(r"PCSX2\inis\PCSX2.ini")] {
                dirs.extend(game_list_paths(&read_ini(&ini)));
            }
        }
        Config::DuckStation => {
            for ini in [dir.join("settings.ini"), documents.join(r"DuckStation\settings.ini"), local.join(r"DuckStation\settings.ini")] {
                dirs.extend(game_list_paths(&read_ini(&ini)));
            }
        }
        Config::Ppsspp => {
            for ini in [dir.join(r"memstick\PSP\SYSTEM\ppsspp.ini"), documents.join(r"PPSSPP\PSP\SYSTEM\ppsspp.ini")] {
                for (s, k, v) in read_ini(&ini) {
                    if (s == "general" && k == "currentdirectory") || (s == "recent" && k.starts_with("filename")) {
                        let p = PathBuf::from(v);
                        dirs.push(if p.extension().is_some() { p.parent().map(Path::to_path_buf).unwrap_or(p) } else { p });
                    }
                }
            }
        }
        Config::Cemu => {
            for xml in [dir.join(r"portable\settings.xml"), dir.join("settings.xml"), appdata.join(r"Cemu\settings.xml")] {
                if let Some(text) = read_text(&xml, 1 << 20) {
                    dirs.extend(xml_entries(&text, "GamePaths").into_iter().map(PathBuf::from));
                }
            }
        }
        Config::Rpcs3 => {
            for yml in [dir.join(r"config\games.yml"), dir.join("games.yml")] {
                if let Some(text) = read_text(&yml, 1 << 20) {
                    // `BLES01234: D:/Jeux PS3/Jeu/` : dossier du jeu, son parent est un dossier de jeux.
                    for line in text.lines() {
                        if let Some((_, path)) = line.split_once(": ") {
                            let p = PathBuf::from(path.trim().trim_matches(['"', '\'']));
                            dirs.extend(p.parent().map(Path::to_path_buf));
                        }
                    }
                }
            }
            dirs.push(dir.join(r"dev_hdd0\game"));
        }
        Config::Xemu => {
            for toml in [dir.join("xemu.toml"), appdata.join(r"xemu\xemu\xemu.toml")] {
                for (_, k, v) in read_ini(&toml) {
                    if k == "dvd_path" {
                        dirs.extend(PathBuf::from(v).parent().map(Path::to_path_buf));
                    }
                }
            }
        }
        Config::Ryujinx => {
            for json in [dir.join(r"portable\Config.json"), appdata.join(r"Ryujinx\Config.json")] {
                if let Some(value) = read_text(&json, 4 << 20).and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                    && let Some(list) = value.get("game_dirs").and_then(|v| v.as_array())
                {
                    dirs.extend(list.iter().filter_map(|v| v.as_str()).map(PathBuf::from));
                }
            }
        }
        Config::Qt(name) => {
            for ini in [dir.join(r"user\config\qt-config.ini"), appdata.join(name).join(r"config\qt-config.ini")] {
                for (s, k, v) in read_ini(&ini) {
                    // `Paths\gamedirs\1\path=D:/Switch` (sans `\default`).
                    if s == "ui" && k.starts_with(r"paths\gamedirs\") && k.ends_with(r"\path") {
                        dirs.push(PathBuf::from(v));
                    }
                }
            }
        }
        Config::MelonDs => {
            for (_, k, v) in read_ini(&dir.join("melonDS.toml")).into_iter().chain(read_ini(&dir.join("melonDS.ini"))) {
                if k == "lastromfolder" {
                    dirs.push(PathBuf::from(v));
                }
            }
        }
        Config::Mgba => {
            for ini in [dir.join("config.ini"), appdata.join(r"mGBA\config.ini")] {
                dirs.extend(read_ini(&ini).into_iter().filter(|(_, k, _)| k == "lastdirectory").map(|(_, _, v)| PathBuf::from(v)));
            }
        }
    }
    // Dossiers de jeux rangés à côté de l'émulateur.
    const NAMES: &[&str] = &["roms", "rom", "games", "game", "isos", "iso", "jeux"];
    dirs.extend(subdirs(&dir).into_iter().filter(|d| d.file_name().and_then(|n| n.to_str()).is_some_and(|n| NAMES.contains(&n.to_lowercase().as_str()))));
    let mut seen = std::collections::HashSet::new();
    found.rom_dirs = dirs.into_iter().filter(|d| d.is_absolute() && seen.insert(super::path_key(d))).collect();
    found
}

/// RetroArch : `retroarch.cfg` (dossier de l'appli, sinon `%APPDATA%\RetroArch`), cœurs,
/// listes de lecture et historique. `:` en tête d'un chemin = dossier de RetroArch.
fn read_retroarch(dir: &Path, appdata: &Path, found: &mut Found, dirs: &mut Vec<PathBuf>) {
    let cfg_path = [dir.join("retroarch.cfg"), appdata.join(r"RetroArch\retroarch.cfg")].into_iter().find(|p| p.is_file());
    let cfg = cfg_path.as_deref().map(read_ini).unwrap_or_default();
    let base = cfg_path.as_deref().and_then(Path::parent).unwrap_or(dir).to_path_buf();
    let value = |key: &str| -> Option<PathBuf> {
        let v = cfg.iter().find(|(_, k, _)| k == key)?.2.as_str();
        if v.is_empty() || v.eq_ignore_ascii_case("default") {
            return None;
        }
        Some(match v.strip_prefix(':') {
            Some(rest) => base.join(rest.trim_start_matches(['\\', '/'])),
            None => PathBuf::from(v),
        })
    };
    let cores_dir = value("libretro_directory").unwrap_or_else(|| base.join("cores"));
    found.cores = fs::read_dir(&cores_dir)
        .map(|it| {
            it.flatten()
                .map(|e| e.path())
                .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.to_ascii_lowercase().ends_with("_libretro.dll")))
                .collect()
        })
        .unwrap_or_default();
    found.cores.sort();
    dirs.extend(value("rgui_browser_directory"));
    let playlists = value("playlist_directory").unwrap_or_else(|| base.join("playlists"));
    let mut lists: Vec<PathBuf> = fs::read_dir(&playlists)
        .map(|it| it.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("lpl"))).collect())
        .unwrap_or_default();
    lists.sort();
    lists.truncate(64);
    lists.extend(value("content_history_path"));
    lists.push(base.join("content_history.lpl"));
    for list in lists {
        let Some(json) = read_text(&list, 4 << 20).and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else { continue };
        let Some(items) = json.get("items").and_then(|v| v.as_array()) else { continue };
        for item in items.iter().take(5000) {
            let Some(path) = item.get("path").and_then(|v| v.as_str()) else { continue };
            // `jeu.zip#jeu.gba` : l'archive est le fichier.
            let rom = PathBuf::from(path.split('#').next().unwrap_or(path));
            if let Some(parent) = rom.parent() {
                dirs.push(parent.to_path_buf());
            }
            if let Some(core) = item.get("core_path").and_then(|v| v.as_str()).filter(|c| c.to_ascii_lowercase().ends_with("_libretro.dll")) {
                found.history.push((rom, PathBuf::from(core)));
            }
        }
    }
}

/// `[GameList] Paths` et `RecursivePaths` (PCSX2, DuckStation), une ligne par dossier.
fn game_list_paths(ini: &[(String, String, String)]) -> Vec<PathBuf> {
    ini.iter()
        .filter(|(s, k, _)| s == "gamelist" && (k == "paths" || k == "recursivepaths"))
        .map(|(_, _, v)| PathBuf::from(v))
        .collect()
}

/// Fichier texte lu s'il fait au plus `max` octets.
pub fn read_text(path: &Path, max: u64) -> Option<String> {
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > max {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// INI ou TOML simple : `(section, clé, valeur)`, section et clé en minuscules, guillemets retirés.
/// Lignes vides, commentaires et lignes sans `=` ignorés.
pub fn read_ini(path: &Path) -> Vec<(String, String, String)> {
    read_text(path, 4 << 20).map(|t| parse_ini(&t)).unwrap_or_default()
}

pub fn parse_ini(text: &str) -> Vec<(String, String, String)> {
    let mut section = String::new();
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(['#', ';']) {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_lowercase();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let value = value.trim();
        let value = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')).or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\''))).unwrap_or(value);
        out.push((section.clone(), key.trim().to_lowercase(), value.to_string()));
    }
    out
}

/// Contenu des `<Entry>` d'un élément XML simple (`<GamePaths><Entry>D:\Wii U</Entry></GamePaths>`).
pub fn xml_entries(xml: &str, element: &str) -> Vec<String> {
    let open = format!("<{element}>");
    let close = format!("</{element}>");
    let Some(start) = xml.find(&open) else { return Vec::new() };
    let Some(end) = xml[start..].find(&close) else { return Vec::new() };
    let inner = &xml[start + open.len()..start + end];
    inner
        .split("<Entry>")
        .skip(1)
        .filter_map(|part| part.split_once("</Entry>").map(|(v, _)| unescape(v.trim())))
        .filter(|v| !v.is_empty())
        .collect()
}

pub fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_executables() {
        let key = |n: &str| by_exe(n).map(|e| e.key);
        assert_eq!(key("retroarch.exe"), Some("retroarch"));
        assert_eq!(key("Ryujinx.exe"), Some("ryujinx"));
        assert_eq!(key("pcsx2-qtx64-avx2.exe"), Some("pcsx2"));
        assert_eq!(key("pcsx2.exe"), Some("pcsx2-legacy"));
        assert_eq!(key("duckstation-qt-x64-ReleaseLTCG.exe"), Some("duckstation"));
        assert_eq!(key("mGBA.exe"), Some("mgba"));
        assert_eq!(key("uninstall.exe"), None);
        assert_eq!(key("retroarch.cfg"), None);
        // Chaque clé est unique et chaque console citée existe.
        let mut keys = std::collections::HashSet::new();
        for e in EMULATORS {
            assert!(keys.insert(e.key));
            assert!(e.systems.iter().all(|s| systems::find(s).is_some()), "{}", e.key);
        }
        assert!(CORES.iter().all(|(s, _)| systems::find(s).is_some()));
    }

    #[test]
    fn reads_ini_and_xml() {
        let ini = parse_ini("; commentaire\n[GameList]\nRecursivePaths = D:\\PS2\nPaths = \"E:\\Jeux PS2\"\n[UI]\nPaths\\gamedirs\\1\\path=D:/Switch\nPaths\\gamedirs\\1\\path\\default=false\n");
        assert_eq!(game_list_paths(&ini), vec![PathBuf::from(r"D:\PS2"), PathBuf::from(r"E:\Jeux PS2")]);
        assert!(ini.contains(&("ui".into(), r"paths\gamedirs\1\path".into(), "D:/Switch".into())));
        let xml = "<content><GamePaths><Entry>D:\\Wii U</Entry><Entry>E:\\A &amp; B</Entry></GamePaths></content>";
        assert_eq!(xml_entries(xml, "GamePaths"), vec![r"D:\Wii U".to_string(), r"E:\A & B".to_string()]);
    }
}
