//! Raccourci d'un jeu sur le Bureau : fichier `.url` qui ouvre `ludex://launch/<id>` (voir
//! `deeplink.rs`). Le Bureau est le dossier connu de Windows (OneDrive compris), le nom vient du
//! titre du jeu, nettoyé pour Windows ; un raccourci existant n'est jamais écrasé.

use crate::library::{manual, Game, Launch};
use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

/// Longueur du nom du fichier (sans `.url`) : le chemin complet reste loin de la limite de Windows.
const MAX_NAME: usize = 80;
/// Raccourcis du même nom essayés (« Hades (2) »…) avant d'abandonner.
const MAX_COPIES: u32 = 99;
const EXT: &str = "url";

/// Noms réservés par Windows, même suivis d'une extension (`CON.url`).
const RESERVED: [&str; 4] = ["CON", "PRN", "AUX", "NUL"];

fn is_reserved(stem: &str) -> bool {
    let base = stem.split('.').next().unwrap_or(stem).trim_end().to_ascii_uppercase();
    if RESERVED.contains(&base.as_str()) {
        return true;
    }
    // COM1 à COM9, LPT1 à LPT9, et leurs variantes en exposant (COM¹…).
    let digit = |s: &str| matches!(s, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³");
    ["COM", "LPT"].iter().any(|p| base.strip_prefix(p).is_some_and(digit))
}

/// Nom du raccourci (sans extension) tiré du titre : caractères interdits par Windows remplacés
/// par une espace, contrôles et invisibles retirés, espaces regroupés, points et espaces finaux
/// retirés, 80 caractères au plus ; nom réservé suffixé ; « Jeu » si rien ne reste.
pub fn file_stem(title: &str) -> String {
    let allowed: String = title.chars().map(|c| if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') { ' ' } else { c }).collect();
    let mut stem = manual::clean_text(&allowed, MAX_NAME).unwrap_or_default();
    while stem.ends_with(['.', ' ']) {
        stem.pop();
    }
    if stem.is_empty() {
        return "Jeu".to_string();
    }
    if is_reserved(&stem) {
        stem.push_str(" (jeu)");
    }
    stem
}

/// Chemin utilisable dans un `.url` : ces fichiers sont lus dans la page de code de Windows, un
/// chemin hors ASCII imprimable y serait illisible.
fn plain_ascii(path: &Path) -> bool {
    path.to_str().is_some_and(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii() && !c.is_ascii_control()))
}

/// Exécutable du jeu lui-même, s'il est dans son dossier d'installation (pas un launcher comme
/// `explorer.exe` pour Xbox ou le lanceur Ankama) et existe encore.
pub fn game_exe(game: &Game) -> Option<&Path> {
    let Launch::Exec { path, .. } = &game.launch else { return None };
    let dir = game.install_dir.as_deref()?;
    let inside = path.to_string_lossy().to_lowercase().starts_with(&dir.trim_end_matches(['\\', '/']).to_lowercase());
    (inside && path.is_file()).then_some(path.as_path())
}

/// Icône du raccourci : l'exécutable du jeu, sinon celui de Ludex ; aucune si le chemin ne peut
/// pas s'écrire dans le fichier (Windows prend alors l'icône du lien `ludex:`).
pub fn icon_for(game: &Game, ludex_exe: Option<&Path>) -> Option<PathBuf> {
    [game_exe(game), ludex_exe].into_iter().flatten().find(|p| plain_ascii(p)).map(Path::to_path_buf)
}

/// Contenu du fichier `.url`.
pub fn contents(url: &str, icon: Option<&Path>) -> String {
    let mut text = format!("[InternetShortcut]\r\nURL={url}\r\n");
    if let Some(icon) = icon.filter(|p| plain_ascii(p)) {
        text.push_str(&format!("IconFile={}\r\nIconIndex=0\r\n", icon.display()));
    }
    text
}

/// Écrit le raccourci dans `dir` sans jamais en écraser un : « Hades.url », sinon « Hades (2).url »…
/// Renvoie le nom du fichier créé.
pub fn write(dir: &Path, stem: &str, contents: &str) -> Result<String, String> {
    if !dir.is_dir() {
        return Err("Bureau introuvable.".to_string());
    }
    for n in 1..=MAX_COPIES {
        let name = if n == 1 { format!("{stem}.{EXT}") } else { format!("{stem} ({n}).{EXT}") };
        let path = dir.join(&name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                return match file.write_all(contents.as_bytes()) {
                    Ok(()) => Ok(name),
                    Err(e) => {
                        drop(file);
                        let _ = std::fs::remove_file(&path);
                        Err(format!("Raccourci non écrit : {e}"))
                    }
                };
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("Raccourci non créé sur le Bureau : {e}")),
        }
    }
    Err(format!("Trop de raccourcis « {stem} » sur le Bureau."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Platform;

    #[test]
    fn names_follow_windows_rules() {
        assert_eq!(file_stem("Half-Life: Alyx"), "Half-Life Alyx");
        assert_eq!(file_stem("  Who? <Me> | \"You\" * a/b\\c  "), "Who Me You a b c");
        assert_eq!(file_stem("Tom Clancy’s Rainbow Six® Siege"), "Tom Clancy’s Rainbow Six® Siege");
        assert_eq!(file_stem("Jeu\u{200B}\u{0007} caché..."), "Jeu caché");
        assert_eq!(file_stem("???"), "Jeu");
        assert_eq!(file_stem(""), "Jeu");
        assert_eq!(file_stem("CON"), "CON (jeu)");
        assert_eq!(file_stem("nul.txt"), "nul.txt (jeu)");
        assert_eq!(file_stem("com1"), "com1 (jeu)");
        assert_eq!(file_stem("LPT²"), "LPT² (jeu)");
        assert_eq!(file_stem("Control"), "Control");
        assert_eq!(file_stem("COM10"), "COM10");
        let long = file_stem(&"a".repeat(200));
        assert_eq!(long.chars().count(), MAX_NAME);
    }

    #[test]
    fn url_file_points_to_the_link() {
        let text = contents("ludex://launch/steam:730", Some(Path::new(r"E:\Jeux\Hades\Hades.exe")));
        assert_eq!(text, "[InternetShortcut]\r\nURL=ludex://launch/steam:730\r\nIconFile=E:\\Jeux\\Hades\\Hades.exe\r\nIconIndex=0\r\n");
        assert_eq!(contents("ludex://launch/a", None), "[InternetShortcut]\r\nURL=ludex://launch/a\r\n");
        // Chemin illisible dans la page de code de Windows : pas d'icône plutôt qu'une icône cassée.
        assert_eq!(contents("ludex://launch/a", Some(Path::new(r"E:\Jeux\Pokémon\p.exe"))), "[InternetShortcut]\r\nURL=ludex://launch/a\r\n");
    }

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ludex-shortcut-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn never_overwrites_a_shortcut() {
        let d = dir("copies");
        assert_eq!(write(&d, "Hades", "a").unwrap(), "Hades.url");
        assert_eq!(write(&d, "Hades", "b").unwrap(), "Hades (2).url");
        assert_eq!(write(&d, "Hades", "c").unwrap(), "Hades (3).url");
        assert_eq!(std::fs::read_to_string(d.join("Hades.url")).unwrap(), "a");
        assert_eq!(std::fs::read_to_string(d.join("Hades (2).url")).unwrap(), "b");
        assert!(write(&d.join("absent"), "Hades", "a").is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn icon_is_the_game_exe_when_it_is_its_own() {
        let d = dir("icon");
        let exe = d.join("Jeu.exe");
        std::fs::write(&exe, b"MZ").unwrap();
        let ludex = Path::new(r"C:\Ludex\ludex.exe");

        let mut game = Game::new(Platform::Other, "x", "Jeu", Launch::Exec { path: exe.clone(), args: Vec::new() });
        game.install_dir = Some(d.to_string_lossy().into_owned());
        let expected = if plain_ascii(&exe) { exe.clone() } else { ludex.to_path_buf() };
        assert_eq!(icon_for(&game, Some(ludex)), Some(expected));

        // Lancé par un autre programme (explorer.exe pour Xbox) : icône de Ludex.
        game.launch = Launch::Exec { path: PathBuf::from(r"C:\Windows\explorer.exe"), args: Vec::new() };
        assert_eq!(icon_for(&game, Some(ludex)), Some(ludex.to_path_buf()));
        // Lancé par une URI (Steam) : icône de Ludex, ou aucune.
        let steam = Game::new(Platform::Steam, "730", "CS2", Launch::Uri("steam://rungameid/730".into()));
        assert_eq!(icon_for(&steam, Some(ludex)), Some(ludex.to_path_buf()));
        assert_eq!(icon_for(&steam, None), None);
        let _ = std::fs::remove_dir_all(&d);
    }
}
