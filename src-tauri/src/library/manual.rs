//! Jeux ajoutés à la main (« Autres ») et corrections de l'utilisateur sur les jeux détectés :
//! exécutable remplacé, titre choisi.
//!
//! Le chemin d'un exécutable vient toujours du sélecteur de fichier natif ouvert côté Rust, jamais
//! du front : il est validé ici (`check_exe`) avant d'être enregistré. Un jeu ajouté à la main dont
//! l'exécutable a disparu (disque débranché, jeu supprimé) est écarté du scan mais reste enregistré :
//! il revient avec son id, donc son temps de jeu, quand le fichier réapparaît.

use super::{art_url, dir_key, is_inside, local, Game, Launch, Platform};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Longueur maximale d'un titre choisi par l'utilisateur, en caractères.
pub const MAX_TITLE: usize = 80;

/// Préfixe des ids des jeux ajoutés à la main : `other:manual-<empreinte du chemin>`.
const PREFIX: &str = "other:manual-";

/// Jeu ajouté à la main, tel qu'enregistré dans `user-data.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualGame {
    /// Id complet, fixé à l'ajout : il ne change plus, même si l'exécutable change.
    pub id: String,
    pub exe: PathBuf,
    pub title: String,
    /// Arguments de lancement (vides pour l'instant, aucun moyen de les saisir).
    #[serde(default)]
    pub args: Vec<String>,
}

/// Id stable d'un jeu ajouté depuis cet exécutable (casse et séparateurs ignorés).
pub fn manual_id(exe: &Path) -> String {
    format!("{PREFIX}{:016x}", crate::images::fnv(&dir_key(&exe.to_string_lossy())))
}

pub fn is_manual(id: &str) -> bool {
    id.starts_with(PREFIX)
}

/// Le fichier commence-t-il par un en-tête d'exécutable Windows (`MZ`, puis `PE\0\0` à l'adresse
/// indiquée en 0x3C) ?
fn is_pe(file: &mut (impl Read + Seek)) -> std::io::Result<bool> {
    let mut dos = [0u8; 64];
    file.read_exact(&mut dos)?;
    if &dos[..2] != b"MZ" {
        return Ok(false);
    }
    let offset = u32::from_le_bytes([dos[60], dos[61], dos[62], dos[63]]);
    if !(64..16 * 1024 * 1024).contains(&offset) {
        return Ok(false);
    }
    let mut sig = [0u8; 4];
    file.seek(SeekFrom::Start(u64::from(offset)))?;
    file.read_exact(&mut sig)?;
    Ok(&sig == b"PE\0\0")
}

/// Valide un fichier choisi dans le sélecteur : chemin absolu, extension `.exe`, fichier existant,
/// en-tête d'exécutable Windows.
pub fn check_exe(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("Chemin invalide.".into());
    }
    let is_exe = path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("exe"));
    if !is_exe {
        return Err("Choisis un fichier .exe.".into());
    }
    if !path.is_file() {
        return Err(format!("Fichier introuvable : {}", path.display()));
    }
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    match File::open(path).and_then(|mut f| is_pe(&mut f)) {
        Ok(true) => Ok(path.to_path_buf()),
        Ok(false) => Err(format!("{name} n'est pas un exécutable Windows.")),
        Err(e) => Err(format!("Lecture de {name} impossible : {e}")),
    }
}

/// Sous-dossiers qui ne nomment pas le jeu : on remonte au-dessus pour le titre et le dossier.
const GENERIC_DIRS: &[&str] = &["bin", "bin32", "bin64", "x64", "x86", "win32", "win64", "binaries", "release", "retail", "shipping"];

/// Dossier du jeu d'un exécutable : son dossier, ou plus haut s'il porte un nom générique
/// (`Hades\x64\Hades.exe` → `Hades`). Jamais la racine d'un lecteur, sauf exécutable posé dessus.
pub fn game_dir(exe: &Path) -> PathBuf {
    let mut dir = exe.parent().unwrap_or(exe).to_path_buf();
    for _ in 0..3 {
        let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
        match dir.parent() {
            Some(parent) if GENERIC_DIRS.contains(&name.as_str()) && parent.file_name().is_some() => dir = parent.to_path_buf(),
            _ => break,
        }
    }
    dir
}

/// Titre d'un exécutable ajouté à la main : nom du produit, sinon dossier du jeu ou nom du fichier
/// (mêmes règles que les jeux trouvés dans les dossiers « Jeux »).
pub fn guess_title(exe: &Path) -> String {
    let dir = game_dir(exe);
    let folder = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let title = local::product_name(exe).unwrap_or_else(|| local::fallback_title(folder, exe));
    clean_title(&title).unwrap_or_else(|| "Jeu".into())
}

/// Caractères invisibles (largeur nulle, sens d'écriture) : jamais gardés dans un titre.
pub(crate) fn is_invisible(c: char) -> bool {
    matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{206F}' | '\u{FEFF}')
}

/// Titre saisi par l'utilisateur : caractères de contrôle et invisibles retirés, espaces
/// regroupés, `MAX_TITLE` caractères au plus. `None` : rien de lisible.
pub fn clean_title(raw: &str) -> Option<String> {
    clean_text(raw, MAX_TITLE)
}

/// Texte court saisi par l'utilisateur (titre, note, nom de collection), sur une ligne :
/// caractères de contrôle et invisibles retirés, espaces regroupés, `max` caractères au plus.
/// `None` : rien de lisible.
pub fn clean_text(raw: &str, max: usize) -> Option<String> {
    let visible: String = raw.chars().filter(|&c| !is_invisible(c)).collect();
    let words: Vec<&str> = visible.split(|c: char| c.is_whitespace() || c.is_control()).filter(|w| !w.is_empty()).collect();
    let text: String = words.join(" ").chars().take(max).collect();
    let text = text.trim_end().to_string();
    (!text.is_empty()).then_some(text)
}

/// Remplace l'exécutable lancé, et l'icône si elle venait de l'exécutable.
fn set_exe(game: &mut Game, exe: &Path, args: Vec<String>) {
    game.launch = Launch::Exec { path: exe.to_path_buf(), args };
    game.launch_label = game.launch.label();
    if game.local_art.icon.is_some() {
        game.local_art.icon = Some(exe.to_path_buf());
    }
}

/// Jeu de la bibliothèque depuis un jeu ajouté à la main. `None` : exécutable absent.
pub fn build(m: &ManualGame) -> Option<Game> {
    if !m.exe.is_file() {
        return None;
    }
    let platform_id = m.id.strip_prefix("other:")?;
    let mut game = Game::new(Platform::Other, platform_id, &m.title, Launch::Exec { path: m.exe.clone(), args: m.args.clone() });
    game.install_dir = Some(game_dir(&m.exe).to_string_lossy().into_owned());
    game.manual = true;
    // Jamais de jaquette locale : l'icône de l'exécutable sert en attendant `art.rs`.
    game.local_art.icon = Some(m.exe.clone());
    game.icon_url = Some(art_url(&game, "icon"));
    Some(game)
}

/// Clé de l'exécutable lancé par un jeu, pour repérer les doublons.
fn exe_key(game: &Game) -> Option<String> {
    match &game.launch {
        Launch::Exec { path, .. } => Some(dir_key(&path.to_string_lossy())),
        Launch::Uri(_) => None,
    }
}

/// `exe` est-il dans le dossier `dir` (ou en dessous) ?
pub fn is_in_dir(exe: &Path, dir: &str) -> bool {
    is_inside(&dir_key(&exe.to_string_lossy()), &dir_key(dir))
}

/// Jeu de la bibliothèque (autre que `except`) qui lance déjà cet exécutable.
pub fn same_exe<'a>(games: &'a [Game], exe: &Path, except: Option<&str>) -> Option<&'a Game> {
    let key = dir_key(&exe.to_string_lossy());
    games.iter().find(|g| Some(g.id.as_str()) != except && exe_key(g).as_deref() == Some(key.as_str()))
}

/// Jeu « Autres » détecté dont le dossier contient cet exécutable : mieux vaut corriger son
/// exécutable que créer un second jeu.
pub fn folder_owner<'a>(games: &'a [Game], exe: &Path) -> Option<&'a Game> {
    games.iter().find(|g| g.platform == Platform::Other && !g.manual && g.install_dir.as_deref().is_some_and(|d| is_in_dir(exe, d)))
}

/// Corrections de l'utilisateur, appliquées après chaque scan (`store.rs` les garde).
pub struct Custom<'a> {
    pub manual: &'a [ManualGame],
    /// Exécutable choisi à la place de celui détecté, par id (jeux « Autres » détectés).
    pub exes: &'a BTreeMap<String, PathBuf>,
    /// Titre choisi, par id (jeux détectés ; un jeu manuel garde le sien dans `manual`).
    pub titles: &'a BTreeMap<String, String>,
}

/// Applique les corrections à une bibliothèque scannée : exécutables remplacés (s'ils existent
/// encore et restent dans le dossier du jeu), jeux manuels ajoutés (un jeu détecté qui lance le même
/// exécutable s'efface devant eux), titres remplacés, puis tri par titre comme `scan`.
pub fn customize(games: &mut Vec<Game>, custom: &Custom) {
    for g in games.iter_mut().filter(|g| g.platform == Platform::Other) {
        let Some(exe) = custom.exes.get(&g.id) else { continue };
        let inside = g.install_dir.as_deref().is_some_and(|d| is_in_dir(exe, d));
        if inside && exe.is_file() {
            set_exe(g, exe, Vec::new());
        }
    }
    let manual: Vec<Game> = custom.manual.iter().filter_map(build).collect();
    let keys: Vec<Option<String>> = manual.iter().map(exe_key).collect();
    games.retain(|g| exe_key(g).is_none() || !keys.contains(&exe_key(g)));
    games.extend(manual);
    for g in games.iter_mut() {
        if let Some(title) = custom.titles.get(&g.id) {
            g.title = title.clone();
        }
    }
    games.sort_by_key(|g| g.title.to_lowercase());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Cursor;

    /// Faux exécutable : en-tête DOS, puis signature PE à l'adresse 0x80.
    fn fake_exe() -> Vec<u8> {
        let mut bytes = vec![0u8; 0x100];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[60..64].copy_from_slice(&0x80u32.to_le_bytes());
        bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
        bytes
    }

    fn dir(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("ludex-manual-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn other(id: &str, dir: &Path, exe: &Path) -> Game {
        let mut g = Game::new(Platform::Other, id, id, Launch::Exec { path: exe.to_path_buf(), args: Vec::new() });
        g.install_dir = Some(dir.to_string_lossy().into_owned());
        g
    }

    #[test]
    fn recognizes_windows_executables() {
        assert!(is_pe(&mut Cursor::new(fake_exe())).unwrap());
        let mut text = b"MZ ceci n'est pas un programme".to_vec();
        text.resize(64, b' ');
        assert!(!is_pe(&mut Cursor::new(text)).unwrap());
        assert!(!is_pe(&mut Cursor::new(b"#!/bin/sh".to_vec())).unwrap_or(false));
        let mut far = fake_exe();
        far[60..64].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(!is_pe(&mut Cursor::new(far)).unwrap());
    }

    #[test]
    fn validates_the_picked_file() {
        let root = dir("check");
        let exe = root.join("Hades.exe");
        fs::write(&exe, fake_exe()).unwrap();
        let fake = root.join("Faux.EXE");
        fs::write(&fake, b"pas un programme").unwrap();
        let text = root.join("lisez-moi.txt");
        fs::write(&text, fake_exe()).unwrap();

        assert_eq!(check_exe(&exe).unwrap(), exe);
        assert!(check_exe(&fake).is_err());
        assert_eq!(check_exe(&text).unwrap_err(), "Choisis un fichier .exe.");
        assert!(check_exe(&root.join("Absent.exe")).unwrap_err().starts_with("Fichier introuvable"));
        assert!(check_exe(&root).is_err()); // dossier
        assert_eq!(check_exe(Path::new(r"jeux\Hades.exe")).unwrap_err(), "Chemin invalide.");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ids_are_stable_and_ignore_case() {
        let a = manual_id(Path::new(r"D:\Emu\Dolphin\Dolphin.exe"));
        assert_eq!(a, manual_id(Path::new(r"d:/emu/dolphin/DOLPHIN.EXE")));
        assert_ne!(a, manual_id(Path::new(r"D:\Emu\Dolphin\DolphinTool.exe")));
        assert!(a.starts_with("other:manual-") && a.len() == "other:manual-".len() + 16);
        assert!(is_manual(&a) && !is_manual("other:e-jeux-frostpunk"));
        // L'URL des images (`/other/manual-…/icon`) ne contient que des caractères sûrs.
        assert_eq!(crate::images::clean_id(a.strip_prefix("other:").unwrap()), a.strip_prefix("other:").unwrap());
    }

    #[test]
    fn titles_are_cleaned_and_bounded() {
        assert_eq!(clean_title("  Crusader\tKings   III \n").as_deref(), Some("Crusader Kings III"));
        assert_eq!(clean_title("Ha\u{200B}des\u{202E}").as_deref(), Some("Hades"));
        assert_eq!(clean_title("Jeu\u{0}\u{7}2").as_deref(), Some("Jeu 2"));
        assert_eq!(clean_title(" \u{FEFF} \r\n"), None);
        let long = "é".repeat(200);
        assert_eq!(clean_title(&long).unwrap().chars().count(), MAX_TITLE);
        assert_eq!(clean_title(&format!("{} fin", "a".repeat(MAX_TITLE - 1))).unwrap().len(), MAX_TITLE - 1);
    }

    #[test]
    fn game_folder_skips_generic_names() {
        assert_eq!(game_dir(Path::new(r"D:\Jeux\Hades\x64\Hades.exe")), Path::new(r"D:\Jeux\Hades"));
        assert_eq!(game_dir(Path::new(r"D:\Foo\Binaries\Win64\Foo.exe")), Path::new(r"D:\Foo"));
        assert_eq!(game_dir(Path::new(r"D:\Emu\Dolphin\Dolphin.exe")), Path::new(r"D:\Emu\Dolphin"));
        assert_eq!(game_dir(Path::new(r"D:\bin\Jeu.exe")), Path::new(r"D:\bin")); // jamais la racine
        assert_eq!(guess_title(Path::new(r"D:\Jeux\CrusaderKing\bin\ck.exe")), "Crusader King");
    }

    #[test]
    fn finds_duplicates_and_folder_owners() {
        let frostpunk = other("e-jeux-frostpunk", Path::new(r"E:\Jeux\Frostpunk"), Path::new(r"E:\Jeux\Frostpunk\Frostpunk.exe"));
        let games = vec![frostpunk];
        assert_eq!(same_exe(&games, Path::new(r"e:\jeux\frostpunk\FROSTPUNK.exe"), None).unwrap().id, "other:e-jeux-frostpunk");
        assert!(same_exe(&games, Path::new(r"E:\Jeux\Frostpunk\Frostpunk.exe"), Some("other:e-jeux-frostpunk")).is_none());
        assert!(folder_owner(&games, Path::new(r"E:\Jeux\Frostpunk\bin\Launcher.exe")).is_some());
        assert!(folder_owner(&games, Path::new(r"E:\Jeux\Frostpunk2\Frostpunk2.exe")).is_none());
    }

    #[test]
    fn corrections_survive_a_rescan() {
        let root = dir("custom");
        let (game, emu) = (root.join("Frostpunk"), root.join("Emu"));
        fs::create_dir_all(game.join("bin")).unwrap();
        fs::create_dir_all(&emu).unwrap();
        let (auto, fixed, dolphin) = (game.join("Launcher.exe"), game.join(r"bin\Frostpunk.exe"), emu.join("Dolphin.exe"));
        for exe in [&auto, &fixed, &dolphin] {
            fs::write(exe, fake_exe()).unwrap();
        }
        // Ce que donnerait le scan, à chaque fois.
        let scanned = || {
            let mut g = other("frostpunk", &game, &auto);
            g.local_art.icon = Some(auto.clone());
            vec![g]
        };
        let manual = vec![ManualGame { id: manual_id(&dolphin), exe: dolphin.clone(), title: "Dolphin".into(), args: Vec::new() }];
        let mut exes = BTreeMap::new();
        exes.insert("other:frostpunk".to_string(), fixed.clone());
        let mut titles = BTreeMap::new();
        titles.insert("other:frostpunk".to_string(), "Frostpunk".to_string());
        let custom = Custom { manual: &manual, exes: &exes, titles: &titles };

        for _ in 0..2 {
            let mut games = scanned();
            customize(&mut games, &custom);
            assert_eq!(games.len(), 2);
            let (dol, fp) = (&games[0], &games[1]);
            assert_eq!((dol.title.as_str(), fp.title.as_str()), ("Dolphin", "Frostpunk"));
            assert!(matches!(&fp.launch, Launch::Exec { path, .. } if path == &fixed));
            assert_eq!(fp.local_art.icon.as_ref(), Some(&fixed));
            assert!(dol.manual && dol.id == manual_id(&dolphin) && dol.platform == Platform::Other);
            assert_eq!(dol.install_dir.as_deref(), Some(emu.to_string_lossy().as_ref()));
            assert!(dol.icon_url.as_deref().is_some_and(|u| u.ends_with("/icon")));
        }

        // Exécutable choisi hors du dossier du jeu, ou disparu : celui détecté reste.
        for wrong in [dolphin.clone(), game.join("Absent.exe")] {
            let exes = BTreeMap::from([("other:frostpunk".to_string(), wrong)]);
            let mut games = scanned();
            customize(&mut games, &Custom { manual: &[], exes: &exes, titles: &titles });
            assert!(matches!(&games[0].launch, Launch::Exec { path, .. } if path == &auto));
        }

        // Jeu manuel dont l'exécutable a disparu : écarté, mais il reviendra avec le même id.
        fs::remove_file(&dolphin).unwrap();
        let mut games = scanned();
        customize(&mut games, &custom);
        assert_eq!(games.len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn manual_game_replaces_a_detected_duplicate() {
        let root = dir("dup");
        let exe = root.join("Jeu.exe");
        fs::write(&exe, fake_exe()).unwrap();
        let mut games = vec![other("jeu", &root, &exe)];
        let manual = vec![ManualGame { id: manual_id(&exe), exe: exe.clone(), title: "Mon jeu".into(), args: Vec::new() }];
        customize(&mut games, &Custom { manual: &manual, exes: &BTreeMap::new(), titles: &BTreeMap::new() });
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].title, "Mon jeu");
        assert!(games[0].manual);
        let _ = fs::remove_dir_all(&root);
    }
}
