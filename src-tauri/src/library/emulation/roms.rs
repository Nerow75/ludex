//! ROMs d'un dossier : console, titre lisible, disques regroupés, mises à jour écartées.
//! Seuls l'en-tête d'une image disque, une liste `.m3u`/`.cue`/`.gdi`, un `meta.xml` (Wii U) ou un
//! `PARAM.SFO` (PS3) sont lus, pour reconnaître le jeu. Rien n'est jamais écrit.

use super::systems::{self, System};
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Profondeur maximale sous un dossier de ROMs.
pub const MAX_DEPTH: usize = 4;
/// Nombre maximal d'entrées lues sous un même dossier de ROMs.
pub const MAX_ENTRIES: usize = 20_000;

/// Jeu reconnu dans un dossier de ROMs.
#[derive(Clone, Debug)]
pub struct Rom {
    /// Fichier passé à l'émulateur (ROM, `.m3u`, `.cue`, `.rpx`, `EBOOT.BIN`).
    pub path: PathBuf,
    pub system: &'static System,
    pub title: String,
    /// Nom du fichier (ou du dossier) sans extension, pour les jaquettes.
    pub stem: String,
    /// Identifiant du titre chez la console (Switch, Wii U, PS3), en minuscules.
    pub title_id: Option<String>,
    /// Rang de la version : région préférée, puis premier disque.
    pub rank: (u8, u32),
    pub size: u64,
    /// Dossier du jeu : celui de la ROM, ou le dossier du jeu (Wii U, PS3).
    pub dir: PathBuf,
}

impl Rom {
    /// Clé de doublon : même console et même titre (ou même identifiant de titre).
    pub fn key(&self) -> String {
        match &self.title_id {
            Some(id) => format!("{}#{id}", self.system.id),
            None => format!("{}:{}", self.system.id, systems::compact(&self.title)),
        }
    }
}

/// Fichier vu mais écarté, et pourquoi.
pub type Skip = (String, String);

/// Extensions de disque ou d'archive dont la console se devine par l'en-tête ou le dossier.
const AMBIGUOUS: &[&str] = &["iso", "chd", "cso", "ciso", "pbp", "rvz", "wia", "gcz", "zip", "7z", "bin", "xiso"];
/// Consoles à cartouche dont RetroArch lit les ROMs compressées (`.zip`, `.7z`).
const ZIPPED: &[&str] = &["nes", "snes", "n64", "gb", "gbc", "gba", "nds", "md", "sms", "gg", "pce", "a2600"];
/// Dossiers jamais parcourus : mods, mises à jour, sauvegardes, BIOS, firmwares, clés, médias…
const SKIP_DIRS: &[&str] = &[
    "mods", "mod", "dlc", "dlcs", "update", "updates", "upd", "patch", "patches", "saves", "save", "savestates", "savestate",
    "states", "state", "bios", "firmware", "firmwares", "keys", "system", "sysdata", "shaders", "shadercache", "cheats",
    "textures", "screenshots", "screenshot", "backup", "backups", "manuals", "manual", "covers", "boxart", "boxarts",
    "thumbnails", "media", "images", "videos", "music", "ost", "soundtrack", "logs", "cache", "nand", "sdmc", "sdcard",
    "memstick", "devflash", "devhdd1", "translations", "languages", "lang", "plugins", "cores", "assets", "autoconfig",
    "overlays", "playlists", "config", "inis", "portable", "temp", "tmp", "windowsapps", "programfiles", "programfilesx86",
];

pub fn skip_dir(name: &str) -> bool {
    name.starts_with('.') || name.starts_with('$') || SKIP_DIRS.contains(&systems::compact(name).as_str())
}

/// Parcourt un dossier de ROMs. `default` : console annoncée par le dossier lui-même ou par
/// l'émulateur qui le déclare. `seen` : dossiers déjà parcourus (dossiers imbriqués).
pub fn scan_root(root: &Path, default: Option<&'static System>, seen: &mut HashSet<String>, out: &mut Vec<Rom>, skipped: &mut Vec<Skip>) {
    let hint = root.file_name().and_then(|n| n.to_str()).and_then(systems::by_folder).or(default);
    let mut budget = MAX_ENTRIES;
    walk(root, 0, hint, seen, &mut budget, out, skipped);
}

fn walk(dir: &Path, depth: usize, hint: Option<&'static System>, seen: &mut HashSet<String>, budget: &mut usize, out: &mut Vec<Rom>, skipped: &mut Vec<Skip>) {
    if *budget == 0 || !seen.insert(super::path_key(dir)) {
        return;
    }
    if let Some(found) = folder_game(dir, skipped) {
        out.extend(found);
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    for entry in entries.flatten() {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_symlink() {
            continue; // liens et jonctions jamais suivis
        }
        if kind.is_dir() {
            dirs.push(entry.path());
        } else if kind.is_file() {
            files.push(entry.path());
        }
    }
    files.sort();
    dirs.sort();
    read_files(dir, &files, hint, out, skipped);
    if depth >= MAX_DEPTH {
        return;
    }
    for sub in dirs {
        let name = sub.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if skip_dir(name) {
            continue;
        }
        let hint = systems::by_folder(name).or(hint);
        walk(&sub, depth + 1, hint, seen, budget, out, skipped);
    }
}

fn ext_of(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default()
}

fn stem_of(path: &Path) -> String {
    path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string()
}

fn size_of(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// Fichiers d'un dossier : listes `.m3u`, `.cue`, `.gdi` d'abord (leurs pistes ne sont pas des jeux
/// à part), puis les ROMs.
fn read_files(dir: &Path, files: &[PathBuf], hint: Option<&'static System>, out: &mut Vec<Rom>, skipped: &mut Vec<Skip>) {
    let mut used: HashSet<String> = HashSet::new();
    let mut lists: Vec<(&PathBuf, Vec<PathBuf>)> = Vec::new();
    for file in files {
        let ext = ext_of(file);
        if !matches!(ext.as_str(), "m3u" | "cue" | "gdi") {
            continue;
        }
        let refs = list_refs(file, &ext, dir);
        for r in &refs {
            used.insert(super::path_key(r));
        }
        lists.push((file, refs));
    }
    // Un `.cue` cité par un `.m3u` n'est pas un jeu à part.
    lists.retain(|(file, _)| !used.contains(&super::path_key(file)));
    for (file, refs) in lists {
        let ext = ext_of(file);
        let present: Vec<&PathBuf> = refs.iter().filter(|r| r.is_file()).collect();
        if present.is_empty() {
            continue;
        }
        let system = match ext.as_str() {
            "gdi" => systems::find("dreamcast"),
            // Disques d'un `.m3u` : console de la première piste.
            "m3u" => file_system(present[0], &ext_of(present[0]), hint),
            _ => hint.filter(|s| matches!(s.id, "ps1" | "ps2" | "saturn" | "pce" | "dreamcast" | "psp")),
        };
        let Some(system) = system else {
            skipped.push((file_name(file), tr!("Console inconnue (dossier sans nom de console)", "Unknown console (folder without a console name)")));
            continue;
        };
        let size = size_of(file) + present.iter().map(|r| size_of(r)).sum::<u64>();
        push(out, file, system, size, None, dir);
    }
    for file in files {
        let ext = ext_of(file);
        if ext.is_empty() || matches!(ext.as_str(), "m3u" | "cue" | "gdi") || used.contains(&super::path_key(file)) {
            continue;
        }
        let unique = systems::by_ext(&ext);
        if unique.is_none() && !AMBIGUOUS.contains(&ext.as_str()) {
            continue;
        }
        let Some(system) = file_system(file, &ext, hint) else {
            if !matches!(ext.as_str(), "zip" | "7z" | "bin") {
                skipped.push((file_name(file), tr!("Console inconnue (dossier sans nom de console)", "Unknown console (folder without a console name)")));
            }
            continue;
        };
        let mut title_id = None;
        if system.id == "switch" {
            match switch_kind(&stem_of(file)) {
                SwitchKind::Base(id) => title_id = id,
                SwitchKind::Extra => {
                    skipped.push((file_name(file), tr!("Mise à jour ou contenu additionnel", "Update or additional content")));
                    continue;
                }
            }
        }
        push(out, file, system, size_of(file), title_id, dir);
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn push(out: &mut Vec<Rom>, file: &Path, system: &'static System, size: u64, title_id: Option<String>, dir: &Path) {
    let stem = stem_of(file);
    out.push(Rom {
        path: file.to_path_buf(),
        system,
        title: clean_title(&stem),
        rank: (region_rank(&stem), disc_number(&stem)),
        stem,
        title_id,
        size,
        dir: dir.to_path_buf(),
    });
}

/// Console d'un fichier : extension propre, sinon en-tête du disque, sinon dossier.
fn file_system(file: &Path, ext: &str, hint: Option<&'static System>) -> Option<&'static System> {
    if let Some(s) = systems::by_ext(ext) {
        return Some(s);
    }
    match ext {
        "iso" | "rvz" | "wia" | "gcz" | "ciso" | "cso" | "xiso" => match disc_header(file) {
            Disc::System(id) => systems::find(id),
            // « PLAYSTATION » : PS1 ou PS2, le dossier tranche, sinon PS2 (les PS1 sont en .bin/.cue).
            Disc::PlayStation => hint.filter(|s| s.id == "ps1" || s.id == "ps2").or_else(|| systems::find("ps2")),
            Disc::Unknown => hint.filter(|s| match ext {
                "cso" | "ciso" => matches!(s.id, "psp" | "ps2"),
                "xiso" => s.id == "xbox",
                "rvz" | "wia" | "gcz" => false,
                _ => matches!(s.id, "ps1" | "ps2" | "psp" | "xbox" | "saturn" | "pce" | "gc" | "wii" | "dreamcast"),
            }).or_else(|| (ext == "cso").then(|| systems::find("psp")).flatten()),
        },
        "chd" | "cue" => hint.filter(|s| matches!(s.id, "ps1" | "ps2" | "saturn" | "pce" | "dreamcast" | "psp")),
        "pbp" => hint.filter(|s| s.id == "ps1" || s.id == "psp").or_else(|| systems::find("psp")),
        "zip" | "7z" => hint.filter(|s| ZIPPED.contains(&s.id)),
        "bin" => hint.filter(|s| matches!(s.id, "md" | "a2600")),
        _ => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Disc {
    System(&'static str),
    PlayStation,
    Unknown,
}

/// Lit le début d'une image disque (64 Ko au plus) pour reconnaître sa console.
fn disc_header(file: &Path) -> Disc {
    let mut buf = Vec::new();
    if fs::File::open(file).and_then(|f| f.take(0x10020).read_to_end(&mut buf)).is_err() {
        return Disc::Unknown;
    }
    header_system(&buf)
}

fn header_system(buf: &[u8]) -> Disc {
    let at = |off: usize, len: usize| buf.get(off..off + len);
    let be32 = |off: usize| at(off, 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let le32 = |off: usize| at(off, 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let nintendo = |base: usize| {
        if be32(base + 0x18) == Some(0x5D1C_9EA3) {
            Some("wii")
        } else if be32(base + 0x1C) == Some(0xC233_9F3D) {
            Some("gc")
        } else {
            None
        }
    };
    if let Some(id) = nintendo(0) {
        return Disc::System(id);
    }
    // RVZ et WIA (Dolphin) : type de disque à 0x48, 1 = GameCube, 2 = Wii.
    if matches!(at(0, 4), Some(b"RVZ\x01") | Some(b"WIA\x01")) {
        return match be32(0x48) {
            Some(1) => Disc::System("gc"),
            Some(2) => Disc::System("wii"),
            _ => Disc::Unknown,
        };
    }
    // GCZ (Dolphin) : sous-type 0 = GameCube, 1 = Wii.
    if le32(0) == Some(0xB10B_C001) {
        return match le32(4) {
            Some(0) => Disc::System("gc"),
            Some(1) => Disc::System("wii"),
            _ => Disc::Unknown,
        };
    }
    // CISO de Wii ou GameCube : en-tête du disque après la table de 32 Ko.
    if at(0, 4) == Some(b"CISO")
        && let Some(id) = nintendo(0x8000)
    {
        return Disc::System(id);
    }
    // Xbox : partition XDVDFS à 64 Ko.
    if at(0x10000, 20) == Some(b"MICROSOFT*XBOX*MEDIA") {
        return Disc::System("xbox");
    }
    // ISO 9660 : identifiant du système du descripteur principal.
    if at(0x8001, 5) == Some(b"CD001")
        && let Some(id) = at(0x8008, 32)
    {
        let id = String::from_utf8_lossy(id);
        let id = id.trim();
        if id.starts_with("PSP GAME") {
            return Disc::System("psp");
        }
        if id.starts_with("PLAYSTATION") {
            return Disc::PlayStation;
        }
    }
    Disc::Unknown
}

/// Pistes ou disques cités par une liste (`.m3u` : une ligne par disque ; `.cue` : `FILE "x" BINARY` ;
/// `.gdi` : cinquième champ). Chemins relatifs au dossier de la liste, sans `..` ni lecteur.
fn list_refs(file: &Path, ext: &str, dir: &Path) -> Vec<PathBuf> {
    let Some(text) = super::emulators::read_text(file, 64 << 10) else { return Vec::new() };
    let mut refs = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let name = match ext {
            "m3u" => Some(line.to_string()),
            "cue" => line.strip_prefix("FILE ").or_else(|| line.strip_prefix("file ")).and_then(|rest| {
                let rest = rest.trim();
                match rest.strip_prefix('"') {
                    Some(q) => q.split_once('"').map(|(n, _)| n.to_string()),
                    None => rest.rsplit_once(' ').map(|(n, _)| n.to_string()),
                }
            }),
            _ => {
                // `.gdi` : `1 0 4 2352 "track01.bin" 0` ; la première ligne est le nombre de pistes.
                let quoted = line.split_once('"').and_then(|(_, r)| r.split_once('"')).map(|(n, _)| n.to_string());
                quoted.or_else(|| line.split_whitespace().nth(4).map(str::to_string))
            }
        };
        let Some(name) = name else { continue };
        let rel = Path::new(&name);
        if rel.is_absolute() || rel.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
            continue;
        }
        refs.push(dir.join(rel));
    }
    refs
}

/// Jeu Switch : jeu de base (avec son identifiant de titre s'il est dans le nom), ou mise à jour
/// et contenu additionnel à écarter.
#[derive(Debug, PartialEq, Eq)]
pub enum SwitchKind {
    Base(Option<String>),
    Extra,
}

/// `[0100F43008C44000]` : jeu de base si les 13 bits bas sont nuls (`…800` = mise à jour,
/// `…1xxx` = contenu additionnel). Sans identifiant : `[UPD]`, `[DLC]`, `update` ou `v65536`.
pub fn switch_kind(stem: &str) -> SwitchKind {
    let bytes = stem.as_bytes();
    for start in 0..bytes.len().saturating_sub(15) {
        // Lettre accentuée avant l'identifiant : position hors d'un caractère, on passe.
        let Some(chunk) = stem.get(start..start + 16) else { continue };
        let before = start.checked_sub(1).map(|i| bytes[i]);
        let after = bytes.get(start + 16).copied();
        let bounded = !before.is_some_and(|b| b.is_ascii_alphanumeric()) && !after.is_some_and(|b| b.is_ascii_alphanumeric());
        if bounded && chunk.bytes().all(|b| b.is_ascii_hexdigit()) && chunk.starts_with("01") {
            let Ok(id) = u64::from_str_radix(chunk, 16) else { continue };
            return if id & 0x1FFF == 0 { SwitchKind::Base(Some(chunk.to_ascii_lowercase())) } else { SwitchKind::Extra };
        }
    }
    let lower = stem.to_lowercase();
    if lower.contains("[upd") || lower.contains("[dlc") || lower.contains("update") || lower.contains("(dlc") {
        return SwitchKind::Extra;
    }
    // `_v65536`, ` v131072` : numéro de version d'une mise à jour (`v0` = jeu de base).
    let chars: Vec<char> = lower.chars().collect();
    for i in 1..chars.len() {
        if chars[i] == 'v' && matches!(chars[i - 1], '_' | ' ' | '[' | '(' | '-') {
            let digits: String = chars[i + 1..].iter().take_while(|c| c.is_ascii_digit()).collect();
            if !digits.is_empty() && digits.parse::<u64>().is_ok_and(|v| v > 0) {
                return SwitchKind::Extra;
            }
        }
    }
    SwitchKind::Base(None)
}

/// Titre lisible tiré d'un nom de ROM : balises `(…)`, `[…]`, `{…}` retirées (région, révision,
/// disque, identifiant), `_` en espaces, numéro de scène en tête retiré, « , The » remis devant.
pub fn clean_title(stem: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for c in stem.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(if c == '_' { ' ' } else { c }),
            _ => {}
        }
    }
    let mut title = out.split_whitespace().collect::<Vec<_>>().join(" ");
    title = title.trim_matches(|c: char| c == '-' || c == '.' || c == ',' || c.is_whitespace()).to_string();
    // Numéro de scène en tête : « 0133 - Pokemon… », « 3DS0133 - … ».
    if let Some((head, rest)) = title.split_once(" - ") {
        let digits = head.trim_start_matches(|c: char| c.is_ascii_alphabetic() || c == '3');
        let numbered = head.len() <= 8 && digits.len() >= 3 && digits.chars().all(|c| c.is_ascii_digit());
        let year = head.len() == 4 && head.parse::<u32>().is_ok_and(|y| (1900..=2099).contains(&y));
        if numbered && !year && rest.len() > 2 {
            title = rest.to_string();
        }
    }
    // « Legend of Zelda, The - A Link to the Past » → « The Legend of Zelda - A Link to the Past ».
    let (main, sub) = match title.split_once(" - ") {
        Some((m, s)) => (m.to_string(), Some(s.to_string())),
        None => (title.clone(), None),
    };
    for article in ["The", "A", "An", "Le", "La", "Les", "L'", "Der", "Die", "Das"] {
        if let Some(head) = main.strip_suffix(&format!(", {article}")) {
            let main = if article.ends_with('\'') { format!("{article}{head}") } else { format!("{article} {head}") };
            title = match &sub {
                Some(s) => format!("{main} - {s}"),
                None => main,
            };
            break;
        }
    }
    if title.is_empty() { stem.trim().to_string() } else { title }
}

/// Régions citées dans les balises du nom (« (France) », « (USA, Europe) »).
fn regions(stem: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = stem;
    while let Some(open) = rest.find('(') {
        let Some(close) = rest[open..].find(')') else { break };
        for part in rest[open + 1..open + close].split(',') {
            found.push(part.trim().to_lowercase());
        }
        rest = &rest[open + close + 1..];
    }
    found
}

/// Rang de la région pour la langue de Ludex (0 = préférée). Démos, prototypes et bêtas après.
pub fn region_rank(stem: &str) -> u8 {
    let tags = regions(stem);
    let english = crate::i18n::english();
    let order: &[&str] = if english { &["usa", "world", "europe", "uk", "australia", "canada"] } else { &["france", "europe", "world", "usa", "canada", "uk"] };
    let base = tags.iter().filter_map(|t| order.iter().position(|o| o == t)).min().map_or_else(
        || if tags.iter().any(|t| t == "japan") { 8 } else { 6 },
        |p| p as u8,
    );
    let lesser = tags.iter().any(|t| ["beta", "proto", "demo", "sample", "kiosk", "unl", "pirate"].iter().any(|w| t.starts_with(w)));
    base + if lesser { 10 } else { 0 }
}

/// Numéro de disque (« (Disc 2) », « (CD 2) »), 1 sinon.
pub fn disc_number(stem: &str) -> u32 {
    regions(stem)
        .iter()
        .find_map(|t| t.strip_prefix("disc ").or_else(|| t.strip_prefix("disk ")).or_else(|| t.strip_prefix("cd ")).and_then(|n| n.split_whitespace().next()?.parse().ok()))
        .unwrap_or(1)
}

/// Jeu rangé en dossier : Wii U (`code\*.rpx` et `meta\meta.xml`), PS3 sur disque
/// (`PS3_GAME\PARAM.SFO`) ou installé (`PARAM.SFO` de catégorie `HG`).
fn folder_game(dir: &Path, skipped: &mut Vec<Skip>) -> Option<Vec<Rom>> {
    let folder = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let meta = dir.join(r"meta\meta.xml");
    if dir.join("code").is_dir() && meta.is_file() {
        let xml = super::emulators::read_text(&meta, 256 << 10).unwrap_or_default();
        let id = xml_value(&xml, "title_id").map(|s| s.to_ascii_lowercase());
        // 0005000e… = mise à jour, 0005000c… = contenu additionnel.
        if id.as_deref().is_some_and(|i| i.starts_with("0005000e") || i.starts_with("0005000c")) {
            skipped.push((folder, tr!("Mise à jour ou contenu additionnel", "Update or additional content")));
            return Some(Vec::new());
        }
        let rpx = fs::read_dir(dir.join("code")).ok()?.flatten().map(|e| e.path()).find(|p| ext_of(p) == "rpx")?;
        let title = xml_value(&xml, "longname_en").map(|t| t.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|t| !t.is_empty()).unwrap_or_else(|| clean_title(&folder));
        let system = systems::find("wiiu")?;
        return Some(vec![Rom { path: rpx, system, title, stem: folder.clone(), title_id: id, rank: (region_rank(&folder), 1), size: 0, dir: dir.to_path_buf() }]);
    }
    let (sfo, eboot) = if dir.join(r"PS3_GAME\PARAM.SFO").is_file() {
        (dir.join(r"PS3_GAME\PARAM.SFO"), dir.join(r"PS3_GAME\USRDIR\EBOOT.BIN"))
    } else if dir.join("PARAM.SFO").is_file() && dir.join(r"USRDIR\EBOOT.BIN").is_file() {
        (dir.join("PARAM.SFO"), dir.join(r"USRDIR\EBOOT.BIN"))
    } else {
        return None;
    };
    let fields = param_sfo(&fs::read(&sfo).ok().filter(|b| b.len() <= 64 << 10)?);
    let get = |k: &str| fields.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
    let disc = sfo.parent().is_some_and(|p| p.ends_with("PS3_GAME"));
    if !disc && get("CATEGORY").as_deref() != Some("HG") {
        skipped.push((folder, tr!("Mise à jour ou contenu additionnel", "Update or additional content")));
        return Some(Vec::new());
    }
    if !eboot.is_file() {
        return Some(Vec::new());
    }
    let title = get("TITLE").map(|t| t.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|t| !t.is_empty()).unwrap_or_else(|| clean_title(&folder));
    let system = systems::find("ps3")?;
    let id = get("TITLE_ID").filter(|i| i.chars().all(|c| c.is_ascii_alphanumeric())).map(|i| i.to_ascii_lowercase());
    Some(vec![Rom { path: eboot, system, stem: title.clone(), title, title_id: id, rank: (0, 1), size: 0, dir: dir.to_path_buf() }])
}

/// Texte d'un élément XML simple (`<longname_en type="string">Titre</longname_en>`).
fn xml_value(xml: &str, element: &str) -> Option<String> {
    let start = xml.find(&format!("<{element}"))?;
    let rest = &xml[start..];
    let open_end = rest.find('>')?;
    let body = &rest[open_end + 1..];
    let end = body.find(&format!("</{element}>"))?;
    Some(super::emulators::unescape(body[..end].trim()))
}

/// Champs texte d'un `PARAM.SFO` (PS3) : `TITLE`, `TITLE_ID`, `CATEGORY`…
pub fn param_sfo(data: &[u8]) -> Vec<(String, String)> {
    let u16le = |o: usize| data.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
    let u32le = |o: usize| data.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    if data.get(0..4) != Some(b"\0PSF") {
        return Vec::new();
    }
    let (Some(keys), Some(values), Some(count)) = (u32le(8), u32le(12), u32le(16)) else { return Vec::new() };
    let cstr = |o: usize, max: usize| -> Option<String> {
        let bytes = data.get(o..o.checked_add(max)?.min(data.len()))?;
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        Some(String::from_utf8_lossy(&bytes[..end]).into_owned())
    };
    let mut out = Vec::new();
    for i in 0..count.min(256) {
        let entry = 20 + i * 16;
        let (Some(key_off), Some(fmt), Some(len), Some(data_off)) = (u16le(entry), u16le(entry + 2), u32le(entry + 4), u32le(entry + 12)) else { break };
        if fmt != 0x0204 {
            continue; // seulement les chaînes UTF-8
        }
        let (Some(key), Some(value)) = (cstr(keys + key_off, 64), cstr(values + data_off, len.min(1024))) else { continue };
        out.push((key, value.trim().to_string()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_titles() {
        assert_eq!(clean_title("Pokemon - Version Or (France) (SGB Enhanced)"), "Pokemon - Version Or");
        assert_eq!(clean_title("Legend of Zelda, The - A Link to the Past (USA)"), "The Legend of Zelda - A Link to the Past");
        assert_eq!(clean_title("Pokemon Legends Z-A [0100F43008C44000][v0] (4.02 GB)"), "Pokemon Legends Z-A");
        assert_eq!(clean_title("Animal Crossing New Horizons[01006F8002326000][v0]"), "Animal Crossing New Horizons");
        assert_eq!(clean_title("0133 - Pokemon Diamond (France)"), "Pokemon Diamond");
        assert_eq!(clean_title("3DS0133 - Pokemon X"), "Pokemon X");
        assert_eq!(clean_title("1942 - Joint Strike"), "1942 - Joint Strike");
        assert_eq!(clean_title("Final_Fantasy_VII (Disc 1)"), "Final Fantasy VII");
        assert_eq!(clean_title("(Japan)"), "(Japan)");
    }

    #[test]
    fn ranks_regions_and_discs() {
        crate::i18n::with(crate::i18n::Lang::Fr, || {
            assert!(region_rank("Jeu (France)") < region_rank("Jeu (Europe)"));
            assert!(region_rank("Jeu (Europe)") < region_rank("Jeu (USA)"));
            assert!(region_rank("Jeu (USA)") < region_rank("Jeu (Japan)"));
            assert!(region_rank("Jeu (Japan)") < region_rank("Jeu (France) (Beta)"));
        });
        crate::i18n::with(crate::i18n::Lang::En, || assert!(region_rank("Jeu (USA)") < region_rank("Jeu (France)")));
        assert_eq!(disc_number("Final Fantasy VII (France) (Disc 2)"), 2);
        assert_eq!(disc_number("Final Fantasy VII (France)"), 1);
    }

    #[test]
    fn tells_switch_updates_apart() {
        assert_eq!(switch_kind("Pokemon Legends Z-A [0100F43008C44000][v0] (4.02 GB)"), SwitchKind::Base(Some("0100f43008c44000".into())));
        assert_eq!(switch_kind("Pokemon Legends Z-A [0100F43008C44800][v65536]"), SwitchKind::Extra);
        assert_eq!(switch_kind("Pokemon Legends Z-A [0100F43008C45002][v0]"), SwitchKind::Extra);
        assert_eq!(switch_kind("v-pokemon_lets_go_eevee_v65536"), SwitchKind::Extra);
        assert_eq!(switch_kind("Zelda [UPD]"), SwitchKind::Extra);
        assert_eq!(switch_kind("Super Mario Odyssey"), SwitchKind::Base(None));
        assert_eq!(switch_kind("Pokémon Let’s Go, Eevee! [0100187003A36000][v0]"), SwitchKind::Base(Some("0100187003a36000".into())));
        assert_eq!(switch_kind("Jeu v0"), SwitchKind::Base(None));
    }

    #[test]
    fn reads_disc_headers() {
        let mut wii = vec![0u8; 0x60];
        wii[0x18..0x1C].copy_from_slice(&0x5D1C_9EA3u32.to_be_bytes());
        assert_eq!(header_system(&wii), Disc::System("wii"));
        let mut gc = vec![0u8; 0x60];
        gc[0x1C..0x20].copy_from_slice(&0xC233_9F3Du32.to_be_bytes());
        assert_eq!(header_system(&gc), Disc::System("gc"));
        let mut rvz = vec![0u8; 0x60];
        rvz[0..4].copy_from_slice(b"RVZ\x01");
        rvz[0x48..0x4C].copy_from_slice(&2u32.to_be_bytes());
        assert_eq!(header_system(&rvz), Disc::System("wii"));
        let mut psp = vec![0u8; 0x8100];
        psp[0x8001..0x8006].copy_from_slice(b"CD001");
        psp[0x8008..0x8010].copy_from_slice(b"PSP GAME");
        assert_eq!(header_system(&psp), Disc::System("psp"));
        psp[0x8008..0x8013].copy_from_slice(b"PLAYSTATION");
        assert_eq!(header_system(&psp), Disc::PlayStation);
        let mut xbox = vec![0u8; 0x10020];
        xbox[0x10000..0x10014].copy_from_slice(b"MICROSOFT*XBOX*MEDIA");
        assert_eq!(header_system(&xbox), Disc::System("xbox"));
        assert_eq!(header_system(&[0u8; 16]), Disc::Unknown);
    }

    #[test]
    fn reads_param_sfo() {
        // Deux entrées : TITLE et CATEGORY.
        let mut data = vec![0u8; 20 + 32];
        data[0..4].copy_from_slice(b"\0PSF");
        let keys = b"TITLE\0CATEGORY\0";
        let key_start = data.len();
        data.extend_from_slice(keys);
        let value_start = data.len();
        data.extend_from_slice(b"Demon's Souls\0\0\0HG\0\0");
        data[8..12].copy_from_slice(&(key_start as u32).to_le_bytes());
        data[12..16].copy_from_slice(&(value_start as u32).to_le_bytes());
        data[16..20].copy_from_slice(&2u32.to_le_bytes());
        let entry = |data: &mut Vec<u8>, i: usize, key: u16, len: u32, off: u32| {
            let e = 20 + i * 16;
            data[e..e + 2].copy_from_slice(&key.to_le_bytes());
            data[e + 2..e + 4].copy_from_slice(&0x0204u16.to_le_bytes());
            data[e + 4..e + 8].copy_from_slice(&len.to_le_bytes());
            data[e + 12..e + 16].copy_from_slice(&off.to_le_bytes());
        };
        entry(&mut data, 0, 0, 14, 0);
        entry(&mut data, 1, 6, 3, 16);
        assert_eq!(param_sfo(&data), vec![("TITLE".into(), "Demon's Souls".into()), ("CATEGORY".into(), "HG".into())]);
        assert!(param_sfo(b"pas un sfo").is_empty());
    }
}
