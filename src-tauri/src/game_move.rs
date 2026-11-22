//! Déplacer un jeu installé vers un autre disque.
//!
//! Qui déplace dépend du launcher (`mode`) :
//! - Steam : Steam lui-même (page Stockage de ses paramètres, `steam://open/settings/storage`).
//!   Ludex n'écrit jamais dans les fichiers de Steam (`libraryfolders.vdf`, manifestes).
//! - Xbox : Windows (Paramètres > Applications installées > Déplacer).
//! - Epic, GOG, Ubisoft, EA, Battle.net, Rockstar, Riot, Amazon, itch.io : leur launcher seulement
//!   (registre, manifestes et bases pointent vers l'ancien dossier) ; le front l'explique.
//! - « Autres » rangés dans un dossier « Jeux » / « Games » et jeux ajoutés à la main : Ludex.
//!   Refusés : lanceurs tiers à emplacement fixe, dossier trop large ou qui contient un autre jeu,
//!   jeu qu'une entrée de « Applications installées » désigne (son désinstalleur casserait), jeu
//!   « Autres » hors d'un dossier « Jeux » (il ne serait plus retrouvé au scan).
//!
//! Déplacement par Ludex : jeu arrêté, un seul déplacement à la fois, place vérifiée, cible absente
//! ou vide, jamais l'une dans l'autre. Même disque : simple renommage. Autre disque : copie dans
//! `<cible>.ludex-tmp` (morceaux de 4 Mo, dates gardées, annulable), vérification (mêmes fichiers,
//! mêmes tailles), puis attente de la confirmation de l'utilisateur. Confirmé : ancien dossier
//! renommé `<ancien>.ludex-old` (échoue si un fichier est ouvert), copie renommée à sa place,
//! préférences enregistrées (sinon tout est défait), bibliothèque relue, puis `.ludex-old` supprimé.
//! Au moindre échec avant la bascule, la copie est effacée et l'original n'a pas bougé.
//!
//! Un jeu « Autres » a un id tiré de son dossier : déplacé, il garde son id d'origine par la table
//! `movedIds` de `user-data.json` (`apply_moved`, juste après le scan), donc temps de jeu,
//! collections, sessions, sauvegardes et jaquettes perso le suivent. Exécutable choisi et
//! exécutable d'un jeu manuel : chemins réécrits vers le nouveau dossier.
//!
//! Dossiers de travail (`.ludex-tmp`, `.ludex-old`) notés dans `deplacement-en-cours.json` du
//! dossier de données tant qu'ils existent, et supprimés au démarrage suivant s'ils sont restés
//! (Ludex fermé pendant une copie, fichier verrouillé). Jamais pris pour un jeu par le scan.
//!
//! Le front n'envoie qu'un id de jeu et la clé d'une destination relue ici (ou « le dossier choisi
//! avec le sélecteur natif », gardé côté Rust) : jamais un chemin.

use crate::library::{self, Game, Platform, ART_ORIGIN};
use crate::store::UserData;
use crate::{blocking, footprint, lock, relocate, rescan, saves, session, storage, wait_scan, AppState};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

/// Suffixe de la copie en cours.
pub const TMP_SUFFIX: &str = library::MOVE_SUFFIXES[0];
/// Suffixe de l'ancien dossier, le temps de la bascule.
pub const OLD_SUFFIX: &str = library::MOVE_SUFFIXES[1];
/// Place laissée libre sur le disque de destination, en plus du jeu.
pub const MARGIN: u64 = 1_000_000_000;
/// Taille des morceaux copiés.
const CHUNK: usize = 4 * 1024 * 1024;
/// Événement de progression.
pub const EVENT: &str = "game-move";
/// Dossier créé sur un disque qui n'a pas encore de dossier de jeux.
pub const NEW_LIBRARY: &str = "Jeux";
const LEFTOVERS_FILE: &str = "deplacement-en-cours.json";
/// Page Stockage des paramètres de Steam (route lue dans `steamui\sp.js`).
pub const STEAM_STORAGE: &str = "steam://open/settings/storage";

// ------------------------------------------------------------------------------------------------
// Qui déplace

/// Voie de déplacement d'un jeu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    /// Ludex copie et bascule lui-même.
    Ludex,
    /// Page Stockage de Steam.
    Steam,
    /// Paramètres de Windows (jeux du Microsoft Store).
    Windows,
    /// Le launcher seulement : le front explique comment.
    Launcher,
    /// Impossible : `reason` dit pourquoi.
    Refused,
}

/// Clé de comparaison d'un chemin : séparateurs Windows, minuscules, sans `\` final.
fn key(path: &Path) -> String {
    let s = path.to_string_lossy().replace('/', "\\").to_lowercase();
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    s.trim_end_matches('\\').to_string()
}

fn same_path(a: &Path, b: &Path) -> bool {
    key(a) == key(b)
}

/// Voie de déplacement d'un jeu, et la raison d'un refus. `libraries` : dossiers « Jeux » /
/// « Games » lus par Ludex ; `entries` : entrées de « Applications installées ».
pub fn mode(game: &Game, games: &[Game], libraries: &[PathBuf], entries: &[footprint::UninstallEntry]) -> (Mode, Option<String>) {
    let refused = |reason: String| (Mode::Refused, Some(reason));
    let Some(dir) = game.install_dir.as_deref() else {
        return refused(tr!("Ludex ne connaît pas le dossier de ce jeu.", "Ludex doesn't know this game's folder."));
    };
    match game.platform {
        Platform::Steam => return (Mode::Steam, None),
        Platform::Xbox => return (Mode::Windows, None),
        Platform::Other => {}
        _ => return (Mode::Launcher, None),
    }
    if game.via_launcher || (!game.manual && session::root_key(dir).is_some() && is_fixed_launcher(game)) {
        return refused(tr!(
            "Ce jeu passe par son propre lanceur, installé à un emplacement fixe : déplace-le depuis ce lanceur.",
            "This game runs through its own launcher, installed in a fixed place: move it from that launcher."
        ));
    }
    if session::root_key(dir).is_none() {
        return refused(tr!("Le dossier de ce jeu est trop large pour être déplacé ({dir}).", "This game's folder is too broad to be moved ({dir})."));
    }
    let here = Path::new(dir);
    let other = games
        .iter()
        .filter(|g| g.id != game.id)
        .filter_map(|g| g.install_dir.as_deref())
        .find(|d| saves::is_within(Path::new(d), here) || saves::is_within(here, Path::new(d)));
    if let Some(other) = other {
        return refused(tr!(
            "Ce dossier contient un autre jeu, ou se trouve dans le sien ({other}) : déplacement refusé.",
            "This folder holds another game, or sits inside one ({other}): move refused."
        ));
    }
    if !footprint::matching(game, entries).is_empty() {
        return refused(tr!(
            "Ce jeu figure dans les Applications installées de Windows : déplacé, son désinstalleur ne marcherait plus. Désinstalle-le, puis réinstalle-le sur l'autre disque.",
            "This game is listed in Windows' Installed apps: once moved, its uninstaller would break. Uninstall it, then reinstall it on the other drive."
        ));
    }
    if !game.manual && !here.parent().is_some_and(|p| libraries.iter().any(|l| same_path(l, p))) {
        return refused(tr!(
            "Ludex ne déplace que les jeux rangés dans un dossier « Jeux » ou « Games » : ailleurs, il ne les retrouverait plus.",
            "Ludex only moves games kept in a \"Jeux\" or \"Games\" folder: elsewhere, it wouldn't find them again."
        ));
    }
    (Mode::Ludex, None)
}

/// Jeux « Autres » trouvés à un emplacement fixe d'un lanceur tiers (`local::known_launchers`).
fn is_fixed_launcher(game: &Game) -> bool {
    matches!(game.platform_id.as_str(), "fivem" | "redm" | "ragemp" | "roblox" | "pokeisland") || game.platform_id.starts_with("ankama-")
}

// ------------------------------------------------------------------------------------------------
// Destinations

/// Destination proposée.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    /// Clé renvoyée par le front pour choisir cette destination (le dossier parent, relu ici).
    pub key: String,
    /// Dossier final du jeu (« D:\Jeux\Frostpunk »).
    pub path: String,
    /// « D: ».
    pub letter: String,
    /// Nom du volume, vide s'il n'en a pas.
    pub name: String,
    pub free: u64,
    pub total: u64,
    /// Dossier de jeux à créer (« D:\Jeux »).
    pub create: bool,
    /// Même disque que le jeu : simple renommage, rien à copier.
    pub same_drive: bool,
    /// Assez de place (toujours vrai sur le même disque).
    pub enough: bool,
    /// Un dossier du même nom existe déjà et n'est pas vide.
    pub taken: bool,
}

fn letter(mount: &str) -> String {
    mount.trim_end_matches(['\\', '/']).to_string()
}

fn is_empty_dir(path: &Path) -> bool {
    fs::read_dir(path).is_ok_and(|mut it| it.next().is_none())
}

/// Dossier occupé : existe et n'est pas un dossier vide.
fn occupied(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok() && !is_empty_dir(path)
}

/// Un lecteur entier (« D:\ ») : seuls ceux-là ont des dossiers « Jeux » lus par Ludex.
fn is_drive_root(mount: &str) -> bool {
    let b = mount.as_bytes();
    (b.len() == 2 || (b.len() == 3 && (b[2] == b'\\' || b[2] == b'/'))) && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// Destinations d'un jeu : chaque dossier « Jeux » / « Games » lu par Ludex (sauf celui du jeu),
/// puis `<lecteur>\Jeux` à créer sur les lecteurs qui n'en ont pas.
pub fn targets(game_dir: &Path, bytes: u64, libraries: &[PathBuf], volumes: &[storage::Volume]) -> Vec<Target> {
    let Some(folder) = game_dir.file_name() else { return Vec::new() };
    let here = storage::volume_of(&game_dir.to_string_lossy(), volumes).map(|v| v.mount.clone());
    let make = |parent: &Path, create: bool| -> Option<Target> {
        let v = storage::volume_of(&parent.to_string_lossy(), volumes)?;
        let path = parent.join(folder);
        let same_drive = here.as_deref() == Some(v.mount.as_str());
        Some(Target {
            key: parent.display().to_string(),
            path: path.display().to_string(),
            letter: letter(&v.mount),
            name: v.name.clone(),
            free: v.free,
            total: v.total,
            create,
            same_drive,
            enough: same_drive || v.free >= bytes.saturating_add(MARGIN),
            taken: occupied(&path),
        })
    };
    let mut out: Vec<Target> = libraries
        .iter()
        .filter(|l| !game_dir.parent().is_some_and(|p| same_path(p, l)))
        .filter_map(|l| make(l, false))
        .collect();
    for v in volumes.iter().filter(|v| is_drive_root(&v.mount) && v.total > 0) {
        let has = libraries.iter().any(|l| storage::volume_of(&l.to_string_lossy(), volumes).is_some_and(|lv| lv.mount == v.mount));
        if !has {
            out.extend(make(&Path::new(&v.mount).join(NEW_LIBRARY), true));
        }
    }
    out.sort_by(|a, b| a.letter.cmp(&b.letter).then_with(|| a.path.to_lowercase().cmp(&b.path.to_lowercase())));
    out.dedup_by(|a, b| a.key.eq_ignore_ascii_case(&b.key));
    out
}

/// Vérifie un déplacement : source réelle, cible absente ou vide, jamais l'une dans l'autre.
pub fn check_plan(from: &Path, to: &Path) -> Result<(), String> {
    if !fs::symlink_metadata(from).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink()) {
        return Err(tr!("Le dossier du jeu est introuvable ({}).", "The game's folder can't be found ({}).", from.display()));
    }
    if saves::is_within(to, from) || saves::is_within(from, to) {
        return Err(tr!("Destination dans le dossier du jeu (ou l'inverse) : choisis-en une autre.", "Destination inside the game's folder (or the reverse): pick another one."));
    }
    if occupied(to) {
        return Err(tr!("{} existe déjà et n'est pas vide : choisis une autre destination.", "{} already exists and isn't empty: pick another destination.", to.display()));
    }
    if session::root_key(&to.to_string_lossy()).is_none() {
        return Err(tr!("Destination trop large ({}).", "Destination too broad ({}).", to.display()));
    }
    relocate::check_root(to)
}

/// Place suffisante sur le disque de destination : le jeu plus une marge d'1 Go.
pub fn check_space(bytes: u64, free: u64, disk: &str) -> Result<(), String> {
    if bytes.saturating_add(MARGIN) > free {
        return Err(tr!(
            "Pas assez de place sur {disk} : {} à déplacer, {} libres (1 Go gardé en plus).",
            "Not enough space on {disk}: {} to move, {} free (1 GB kept spare).",
            relocate::human(bytes),
            relocate::human(free)
        ));
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// Copie

/// Dossier frère : même nom, suffixe ajouté (`X` → `X.ludex-tmp`).
pub fn sibling(dir: &Path, suffix: &str) -> PathBuf {
    let mut name = dir.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(suffix);
    dir.with_file_name(name)
}

/// Raison d'arrêt d'une copie.
#[derive(Debug, PartialEq, Eq)]
pub enum CopyError {
    Cancelled,
    Failed(String),
}

impl CopyError {
    fn text(self) -> String {
        match self {
            CopyError::Cancelled => tr!("Déplacement annulé.", "Move cancelled."),
            CopyError::Failed(e) => e,
        }
    }
}

/// Ouvre le fichier de destination (vrai fichier, ou disque plein simulé dans les tests).
pub type Create<'a> = dyn FnMut(&Path) -> io::Result<Box<dyn Write>> + 'a;

fn real_file(path: &Path) -> io::Result<Box<dyn Write>> {
    Ok(Box::new(fs::File::create(path)?))
}

/// Contenu d'un dossier : fichiers (chemin relatif, taille) et dossiers, triés.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Tree {
    pub files: Vec<(PathBuf, u64)>,
    pub dirs: Vec<PathBuf>,
}

impl Tree {
    pub fn bytes(&self) -> u64 {
        self.files.iter().map(|(_, n)| n).sum()
    }
}

/// Lit un dossier entier. Refuse un lien symbolique ou une jonction : copié, il pointerait vers
/// autre chose ; ignoré, il manquerait au jeu une fois l'ancien dossier supprimé.
pub fn tree(root: &Path) -> Result<Tree, String> {
    let read_error = |path: &Path, e: io::Error| tr!("Lecture de {} impossible : {e}", "Couldn't read {}: {e}", path.display());
    let mut out = Tree::default();
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        let dir = root.join(&rel);
        for entry in fs::read_dir(&dir).map_err(|e| read_error(&dir, e))? {
            let entry = entry.map_err(|e| read_error(&dir, e))?;
            let kind = entry.file_type().map_err(|e| read_error(&entry.path(), e))?;
            let path = rel.join(entry.file_name());
            if kind.is_symlink() {
                return Err(tr!(
                    "Le jeu contient un lien symbolique ou une jonction ({}) : Ludex ne le déplace pas.",
                    "The game holds a symbolic link or a junction ({}): Ludex doesn't move it.",
                    path.display()
                ));
            } else if kind.is_dir() {
                out.dirs.push(path.clone());
                stack.push(path);
            } else {
                let len = entry.metadata().map_err(|e| read_error(&entry.path(), e))?.len();
                out.files.push((path, len));
            }
        }
    }
    out.files.sort();
    out.dirs.sort();
    Ok(out)
}

/// Copie `list` (chemins relatifs à `from`) dans `to`, par morceaux : `progress` reçoit les
/// octets copiés depuis l'appel précédent ; `cancel` arrête la copie entre deux morceaux. Dates de
/// modification et lecture seule recopiées.
pub fn copy_tree(from: &Path, to: &Path, list: &Tree, cancel: &AtomicBool, progress: &mut dyn FnMut(u64), create: &mut Create) -> Result<(), CopyError> {
    let failed = |e: String| CopyError::Failed(e);
    fs::create_dir_all(to).map_err(|e| failed(tr!("Impossible de créer {} : {e}", "Couldn't create {}: {e}", to.display())))?;
    for dir in &list.dirs {
        let dst = to.join(dir);
        fs::create_dir_all(&dst).map_err(|e| failed(tr!("Impossible de créer {} : {e}", "Couldn't create {}: {e}", dst.display())))?;
    }
    let mut buf = vec![0u8; CHUNK];
    for (rel, _) in &list.files {
        let src = from.join(rel);
        let dst = to.join(rel);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(|e| failed(tr!("Impossible de créer {} : {e}", "Couldn't create {}: {e}", parent.display())))?;
        }
        let write_error = |e: io::Error| match e.kind() {
            io::ErrorKind::StorageFull => failed(tr!("Disque plein pendant la copie de {}.", "Drive full while copying {}.", rel.display())),
            _ => failed(tr!("Copie de {} impossible : {e}", "Couldn't copy {}: {e}", rel.display())),
        };
        let mut input = fs::File::open(&src).map_err(|e| failed(tr!("Lecture de {} impossible : {e}", "Couldn't read {}: {e}", src.display())))?;
        let meta = input.metadata().ok();
        {
            let mut output = create(&dst).map_err(write_error)?;
            loop {
                if cancel.load(Ordering::SeqCst) {
                    return Err(CopyError::Cancelled);
                }
                let n = input.read(&mut buf).map_err(|e| failed(tr!("Lecture de {} impossible : {e}", "Couldn't read {}: {e}", src.display())))?;
                if n == 0 {
                    break;
                }
                output.write_all(&buf[..n]).map_err(write_error)?;
                progress(n as u64);
            }
            output.flush().map_err(write_error)?;
        }
        if let Some(meta) = meta {
            if let Ok(modified) = meta.modified()
                && let Ok(f) = fs::File::options().write(true).open(&dst)
            {
                let _ = f.set_modified(modified);
            }
            if meta.permissions().readonly() {
                let _ = fs::set_permissions(&dst, meta.permissions());
            }
        }
    }
    Ok(())
}

/// Les mêmes dossiers et fichiers, aux mêmes tailles, des deux côtés.
pub fn verify(list: &Tree, copy: &Path) -> Result<(), String> {
    let copied = tree(copy)?;
    if &copied != list {
        return Err(tr!(
            "La copie ne correspond pas à l'original : rien n'a été changé.",
            "The copy doesn't match the original: nothing was changed."
        ));
    }
    Ok(())
}

/// Efface un dossier de travail, lecture seule comprise (sinon Windows refuse de supprimer).
pub fn remove_tree(dir: &Path) -> io::Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    if fs::remove_dir_all(dir).is_ok() {
        return Ok(());
    }
    clear_readonly(dir);
    fs::remove_dir_all(dir)
}

fn clear_readonly(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            clear_readonly(&entry.path());
        } else if let Ok(meta) = entry.metadata() {
            let mut perms = meta.permissions();
            if perms.readonly() {
                #[allow(clippy::permissions_set_readonly_false)]
                perms.set_readonly(false);
                let _ = fs::set_permissions(entry.path(), perms);
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Bascule

/// Message d'un ancien dossier qu'on ne peut pas renommer (fichier ouvert).
fn in_use(from: &Path, e: &io::Error) -> String {
    tr!(
        "Impossible de libérer {} : un fichier est sans doute ouvert (jeu, launcher, Explorateur). Ferme-le puis réessaie. ({e})",
        "Couldn't release {}: a file is probably open (game, launcher, Explorer). Close it and try again. ({e})",
        from.display()
    )
}

/// Bascule d'un déplacement vers un autre disque : ancien dossier renommé `.ludex-old`, copie
/// renommée à sa place définitive. Renvoie l'ancien dossier renommé. En cas d'échec, tout est remis
/// comme avant (copie gardée, pour réessayer ou annuler).
pub fn swap(from: &Path, tmp: &Path, to: &Path) -> Result<PathBuf, String> {
    let old = sibling(from, OLD_SUFFIX);
    remove_tree(&old).map_err(|e| tr!("Reste d'un déplacement précédent impossible à effacer ({}) : {e}", "Couldn't delete what's left of a previous move ({}): {e}", old.display()))?;
    fs::rename(from, &old).map_err(|e| in_use(from, &e))?;
    if to.exists() && is_empty_dir(to) {
        let _ = fs::remove_dir(to);
    }
    if let Err(e) = fs::rename(tmp, to) {
        let _ = fs::rename(&old, from);
        return Err(tr!("Impossible de finaliser {} : {e}", "Couldn't finish {}: {e}", to.display()));
    }
    Ok(old)
}

/// Défait `swap` (préférences non enregistrées).
pub fn unswap(from: &Path, tmp: &Path, to: &Path, old: &Path) {
    let _ = fs::rename(to, tmp);
    let _ = fs::rename(old, from);
}

/// Déplacement sur le même disque : un renommage, instantané.
pub fn rename_in_place(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(|e| tr!("Impossible de créer {} : {e}", "Couldn't create {}: {e}", parent.display()))?;
    }
    if to.exists() && is_empty_dir(to) {
        let _ = fs::remove_dir(to);
    }
    fs::rename(from, to).map_err(|e| in_use(from, &e))
}

/// `path` déplacé avec son dossier : `from\a\b.exe` → `to\a\b.exe`. Hors de `from` : inchangé.
pub fn rebase(path: &Path, from: &Path, to: &Path) -> PathBuf {
    let (p, f) = (key(path), key(from));
    if p == f {
        return to.to_path_buf();
    }
    match p.strip_prefix(&f).filter(|rest| rest.starts_with('\\')) {
        // Reste repris dans le chemin d'origine (casse gardée).
        Some(rest) => {
            let original = path.to_string_lossy().replace('/', "\\");
            to.join(&original[original.len() - rest.len() + 1..])
        }
        None => path.to_path_buf(),
    }
}

/// Préférences après le déplacement d'un jeu : id d'origine gardé (« Autres »), exécutable du jeu
/// manuel et exécutable choisi réécrits.
pub fn record_move(data: &mut UserData, game: &Game, from: &Path, to: &Path) {
    if game.manual {
        for m in data.manual.iter_mut().filter(|m| m.id == game.id) {
            m.exe = rebase(&m.exe, from, to);
        }
    } else {
        let new_id = format!("other:{}", library::path_id(to));
        data.moved_ids.retain(|k, v| v != &game.id && k != &new_id);
        if new_id != game.id {
            data.moved_ids.insert(new_id, game.id.clone());
        }
    }
    if let Some(exe) = data.exes.get_mut(&game.id) {
        *exe = rebase(exe, from, to);
    }
}

/// Après un scan : un jeu « Autres » trouvé au nouvel endroit reprend son id d'origine (et ses
/// adresses d'images locales). Ignoré si un autre jeu porte déjà cet id.
pub fn apply_moved(games: &mut [Game], moved: &BTreeMap<String, String>) {
    if moved.is_empty() {
        return;
    }
    let mut taken: HashSet<String> = games.iter().map(|g| g.id.clone()).collect();
    for g in games.iter_mut().filter(|g| g.platform == Platform::Other && !g.manual) {
        let Some(old) = moved.get(&g.id) else { continue };
        let Some(old_pid) = old.strip_prefix("other:") else { continue };
        if old == &g.id || old_pid.is_empty() || taken.contains(old) {
            continue;
        }
        let before = format!("{ART_ORIGIN}/other/{}/", g.platform_id);
        let after = format!("{ART_ORIGIN}/other/{old_pid}/");
        for url in [&mut g.cover_url, &mut g.hero_url, &mut g.logo_url, &mut g.icon_url].into_iter().flatten() {
            if url.starts_with(&before) {
                *url = url.replacen(&before, &after, 1);
            }
        }
        taken.insert(old.clone());
        g.platform_id = old_pid.to_string();
        g.id = old.clone();
    }
}

// ------------------------------------------------------------------------------------------------
// Dossiers de travail restés sur le disque

/// Dossiers de travail notés sur disque tant qu'ils existent.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Leftovers {
    #[serde(default)]
    pub dirs: Vec<PathBuf>,
}

impl Leftovers {
    pub fn load(file: &Path) -> Self {
        fs::read_to_string(file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    fn save(&self, file: &Path) {
        if self.dirs.is_empty() {
            let _ = fs::remove_file(file);
            return;
        }
        let tmp = file.with_extension("json.tmp");
        if let Ok(bytes) = serde_json::to_vec_pretty(self)
            && fs::write(&tmp, bytes).is_ok()
        {
            let _ = fs::rename(&tmp, file);
        }
    }

    pub fn add(file: &Path, dir: &Path) {
        let mut list = Leftovers::load(file);
        if !list.dirs.iter().any(|d| same_path(d, dir)) {
            list.dirs.push(dir.to_path_buf());
        }
        list.save(file);
    }

    pub fn forget(file: &Path, dir: &Path) {
        let mut list = Leftovers::load(file);
        list.dirs.retain(|d| !same_path(d, dir));
        list.save(file);
    }

    /// Supprime les dossiers de travail restés (seulement ceux qui portent un suffixe de travail).
    /// Ceux qu'on ne peut pas encore supprimer restent notés.
    pub fn clean(file: &Path) {
        let list = Leftovers::load(file);
        let kept: Vec<PathBuf> = list
            .dirs
            .into_iter()
            .filter(|d| library::is_move_leftover(d) && d.exists())
            .filter(|d| match remove_tree(d) {
                Ok(()) => {
                    log::info!("Dossier de travail d'un déplacement supprimé : {}", d.display());
                    false
                }
                Err(e) => {
                    log::warn!("Dossier de travail d'un déplacement pas encore supprimé ({}) : {e}", d.display());
                    true
                }
            })
            .collect();
        Leftovers { dirs: kept }.save(file);
    }
}

// ------------------------------------------------------------------------------------------------
// État et commandes

/// Étape d'un déplacement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Preparing,
    Copying,
    Verifying,
    /// Copie vérifiée : attend la confirmation de l'utilisateur.
    Ready,
    Finishing,
    Done,
    Failed,
    Cancelled,
}

impl Phase {
    fn active(self) -> bool {
        matches!(self, Phase::Preparing | Phase::Copying | Phase::Verifying | Phase::Ready | Phase::Finishing)
    }
}

/// Progression envoyée au front (`game-move`).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub id: String,
    pub title: String,
    pub phase: Phase,
    pub copied: u64,
    pub total: u64,
    /// Octets par seconde, sur les dernières secondes.
    pub speed: Option<u64>,
    /// Secondes restantes estimées.
    pub remaining: Option<u64>,
    pub from: String,
    pub to: String,
    pub error: Option<String>,
    pub warnings: Vec<String>,
}

struct Job {
    progress: Progress,
    from: PathBuf,
    to: PathBuf,
    tmp: PathBuf,
}

/// Déplacement en cours (un seul à la fois), géré par Tauri.
pub struct Mover {
    job: Mutex<Option<Job>>,
    cancel: AtomicBool,
    /// Dossier choisi par le sélecteur natif pour un jeu manuel : (id, dossier parent).
    picked: Mutex<Option<(String, PathBuf)>>,
    leftovers: PathBuf,
}

impl Mover {
    pub fn new(data_dir: &Path) -> Self {
        Mover { job: Mutex::new(None), cancel: AtomicBool::new(false), picked: Mutex::new(None), leftovers: data_dir.join(LEFTOVERS_FILE) }
    }

    /// Supprime, dans un fil à part, les dossiers de travail restés d'un lancement précédent.
    pub fn clean_leftovers(&self) {
        let file = self.leftovers.clone();
        let _ = std::thread::Builder::new().name("ludex-deplacement".into()).spawn(move || Leftovers::clean(&file));
    }

    /// Ce jeu est en cours de déplacement : ni le lancer ni le déplacer de nouveau.
    pub fn busy_with(&self, id: &str) -> bool {
        self.job.lock().ok().is_some_and(|j| j.as_ref().is_some_and(|j| j.progress.id == id && j.progress.phase.active()))
    }

    fn active(&self) -> bool {
        self.job.lock().ok().is_some_and(|j| j.as_ref().is_some_and(|j| j.progress.phase.active()))
    }

    fn progress(&self) -> Option<Progress> {
        self.job.lock().ok()?.as_ref().map(|j| j.progress.clone())
    }

    /// Change la progression puis la publie.
    fn update(&self, app: &AppHandle, change: impl FnOnce(&mut Progress)) {
        let sent = {
            let Ok(mut job) = self.job.lock() else { return };
            let Some(job) = job.as_mut() else { return };
            change(&mut job.progress);
            job.progress.clone()
        };
        let _ = app.emit(EVENT, sent);
    }
}

/// Refuse si le jeu tourne (suivi de Ludex, puis relevé des processus).
fn check_stopped(state: &AppState, game: &Game) -> Result<(), String> {
    if state.tracker.active().iter().any(|(g, _)| *g == game.id) || session::game_running(game) {
        return Err(tr!("{} est ouvert : ferme le jeu avant de le déplacer.", "{} is running: close the game before moving it.", game.title));
    }
    Ok(())
}

fn find(state: &AppState, id: &str) -> Result<(Game, Vec<Game>), String> {
    let games = lock(&state.games)?.clone();
    let game = games
        .iter()
        .find(|g| g.id == id)
        .cloned()
        .ok_or_else(|| tr!("Ce jeu n'est plus dans la bibliothèque. Relance un scan.", "This game is no longer in the library. Run a new scan."))?;
    Ok((game, games))
}

fn mode_of(game: &Game, games: &[Game]) -> (Mode, Option<String>) {
    // Les entrées de Windows ne servent qu'aux jeux que Ludex pourrait déplacer.
    let entries = if game.platform == Platform::Other { footprint::uninstall_entries(&footprint::System) } else { Vec::new() };
    mode(game, games, &library::game_folders(), &entries)
}

/// Taille du jeu : celle du launcher ou de la mesure, sinon les fichiers du dossier.
fn game_bytes(game: &Game, dir: &Path) -> u64 {
    game.size_on_disk.filter(|&b| b > 0).unwrap_or_else(|| relocate::size_of(dir))
}

/// Panneau « Déplacer » d'un jeu.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    id: String,
    title: String,
    platform: Platform,
    mode: Mode,
    reason: Option<String>,
    /// Dossier actuel.
    from: Option<String>,
    /// Lecteur actuel (« E: »).
    drive: Option<String>,
    bytes: u64,
    targets: Vec<Target>,
    /// « Autre dossier… » permis (jeux ajoutés à la main).
    can_pick: bool,
    /// Dossier choisi avec le sélecteur, pour ce jeu.
    picked: Option<Target>,
    /// Déplacement en cours ou fini (ce jeu ou un autre).
    job: Option<Progress>,
}

fn picked_target(mover: &Mover, game: &Game, bytes: u64, volumes: &[storage::Volume]) -> Option<Target> {
    let (id, parent) = mover.picked.lock().ok()?.clone()?;
    if id != game.id {
        return None;
    }
    let dir = Path::new(game.install_dir.as_deref()?);
    let mut list = targets(dir, bytes, &[parent], volumes);
    list.retain(|t| !t.create);
    list.pop()
}

fn options(app: &AppHandle, id: &str) -> Result<Options, String> {
    let state = app.state::<AppState>();
    let mover = app.state::<Mover>();
    let (game, games) = find(&state, id)?;
    let (mode, reason) = mode_of(&game, &games);
    let volumes = storage::volumes();
    let dir = game.install_dir.as_deref().map(PathBuf::from);
    let bytes = match (&dir, mode) {
        (Some(d), Mode::Ludex) => game_bytes(&game, d),
        _ => game.size_on_disk.unwrap_or(0),
    };
    let list = match (&dir, mode) {
        (Some(d), Mode::Ludex) if !game.manual => targets(d, bytes, &library::game_folders(), &volumes),
        (Some(d), Mode::Ludex) => targets(d, bytes, &library::game_folders(), &volumes),
        _ => Vec::new(),
    };
    Ok(Options {
        title: game.title.clone(),
        platform: game.platform,
        mode,
        reason,
        drive: dir.as_deref().and_then(|d| storage::volume_of(&d.to_string_lossy(), &volumes)).map(|v| letter(&v.mount)),
        from: game.install_dir.clone(),
        bytes,
        targets: list,
        can_pick: mode == Mode::Ludex && game.manual,
        picked: picked_target(&mover, &game, bytes, &volumes),
        job: mover.progress(),
        id: game.id,
    })
}

#[tauri::command]
pub async fn move_options(app: AppHandle, id: String) -> Result<Options, String> {
    blocking(move || options(&app, &id)).await
}

/// Sélecteur de dossier natif (jeux ajoutés à la main) : le jeu ira dans `<choisi>\<son dossier>`.
/// Le chemin reste côté Rust. `None` : annulé.
#[tauri::command]
pub async fn move_pick_folder(app: AppHandle, window: tauri::WebviewWindow, id: String) -> Result<Option<Options>, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let (game, games) = find(&state, &id)?;
        if !game.manual || mode_of(&game, &games).0 != Mode::Ludex {
            return Err(tr!("Choisis une destination de la liste.", "Pick a destination from the list."));
        }
        let dialog = window.dialog().file().set_title(tr_str!("Déplacer le jeu dans…", "Move the game to…")).set_parent(&window);
        let Some(parent) = dialog.blocking_pick_folder().and_then(|f| f.into_path().ok()) else { return Ok(None) };
        let dir = PathBuf::from(game.install_dir.clone().unwrap_or_default());
        let to = parent.join(dir.file_name().unwrap_or_default());
        check_plan(&dir, &to)?;
        *lock(&app.state::<Mover>().picked)? = Some((game.id.clone(), parent));
        options(&app, &id).map(Some)
    })
    .await
}

/// Destination choisie par le front : une clé de la liste, ou le dossier choisi au sélecteur.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Choice {
    Folder { key: String },
    Picked,
}

/// Lance le déplacement d'un jeu par Ludex. Même disque : fait tout de suite. Autre disque : copie
/// dans un fil, progression par `game-move`, puis attente de `move_game_finish`.
#[tauri::command]
pub async fn move_game(app: AppHandle, id: String, target: Choice) -> Result<Progress, String> {
    blocking(move || start(&app, &id, target)).await
}

fn start(app: &AppHandle, id: &str, choice: Choice) -> Result<Progress, String> {
    let state = app.state::<AppState>();
    let mover = app.state::<Mover>();
    if mover.active() {
        return Err(tr!("Un déplacement est déjà en cours : attends sa fin.", "A move is already in progress: wait for it to end."));
    }
    let (game, games) = find(&state, id)?;
    let (mode, reason) = mode_of(&game, &games);
    if mode != Mode::Ludex {
        return Err(reason.unwrap_or_else(|| tr!("Ce jeu se déplace depuis son launcher.", "This game is moved from its launcher.")));
    }
    check_stopped(&state, &game)?;
    let from = PathBuf::from(game.install_dir.clone().unwrap_or_default());
    let bytes = game_bytes(&game, &from);
    let volumes = storage::volumes();
    let target = match choice {
        Choice::Folder { key } => targets(&from, bytes, &library::game_folders(), &volumes)
            .into_iter()
            .find(|t| t.key == key)
            .ok_or_else(|| tr!("Cette destination n'est plus disponible : rouvre le panneau.", "This destination is no longer available: reopen the panel."))?,
        Choice::Picked => picked_target(&mover, &game, bytes, &volumes).ok_or_else(|| tr!("Choisis d'abord un dossier.", "Pick a folder first."))?,
    };
    let to = PathBuf::from(&target.path);
    check_plan(&from, &to)?;
    if !target.same_drive {
        check_space(bytes, target.free, &target.letter)?;
    }
    if target.create {
        fs::create_dir_all(Path::new(&target.key)).map_err(|e| tr!("Impossible de créer {} : {e}", "Couldn't create {}: {e}", target.key))?;
    }
    let progress = Progress {
        id: game.id.clone(),
        title: game.title.clone(),
        phase: if target.same_drive { Phase::Finishing } else { Phase::Preparing },
        copied: 0,
        total: bytes,
        speed: None,
        remaining: None,
        from: from.display().to_string(),
        to: to.display().to_string(),
        error: None,
        warnings: Vec::new(),
    };
    let tmp = sibling(&to, TMP_SUFFIX);
    *lock(&mover.job)? = Some(Job { progress: progress.clone(), from: from.clone(), to: to.clone(), tmp: tmp.clone() });
    mover.cancel.store(false, Ordering::SeqCst);
    *lock(&mover.picked)? = None;
    log::info!("Déplacement de {} : {} vers {}", game.title, from.display(), to.display());

    if target.same_drive {
        let result = (|| {
            let scanning = wait_scan(&state);
            check_stopped(&state, &game)?;
            rename_in_place(&from, &to)?;
            if let Err(e) = save_move(&state, &game, &from, &to) {
                let _ = fs::rename(&to, &from);
                return Err(e);
            }
            publish(app, &state, scanning);
            Ok(())
        })();
        return finish_with(app, &mover, result, Vec::new());
    }

    let handle = app.clone();
    std::thread::Builder::new()
        .name("ludex-deplacement".into())
        .spawn(move || copy_job(&handle, &from, &tmp))
        .map_err(|e| tr!("Impossible de lancer la copie : {e}", "Couldn't start the copy: {e}"))?;
    mover.progress().ok_or_else(|| tr!("État interne indisponible", "Internal state unavailable"))
}

/// Préférences mises à jour et enregistrées (rien de changé si l'enregistrement échoue).
fn save_move(state: &AppState, game: &Game, from: &Path, to: &Path) -> Result<(), String> {
    let mut data = lock(&state.data)?;
    let before = data.clone();
    record_move(&mut data, game, from, to);
    if let Err(e) = data.save(&state.data_path) {
        *data = before;
        return Err(tr!("Préférences non enregistrées, déplacement défait : {e}", "Preferences not saved, move undone: {e}"));
    }
    Ok(())
}

/// Bibliothèque relue (le jeu garde son id), puis `library-changed`.
fn publish(app: &AppHandle, state: &AppState, scanning: std::sync::MutexGuard<'_, ()>) {
    match rescan(app, state, scanning) {
        Ok((scan, changes)) => {
            let _ = app.emit(crate::LIBRARY_EVENT, crate::LibraryEvent { added: changes.added, removed: changes.removed, scan, complement: false });
        }
        Err(e) => log::error!("Rescan après un déplacement impossible : {e}"),
    }
}

/// Fin d'un déplacement : `Done` ou `Failed`, journal, événement.
fn finish_with(app: &AppHandle, mover: &Mover, result: Result<(), String>, warnings: Vec<String>) -> Result<Progress, String> {
    match result {
        Ok(()) => {
            mover.update(app, |p| {
                p.phase = Phase::Done;
                p.copied = p.total;
                p.speed = None;
                p.remaining = None;
                p.warnings = warnings;
            });
            if let Some(p) = mover.progress() {
                log::info!("Déplacement de {} terminé : {}", p.title, p.to);
            }
            mover.progress().ok_or_else(|| tr!("État interne indisponible", "Internal state unavailable"))
        }
        Err(e) => {
            log::error!("Déplacement impossible : {e}");
            mover.update(app, |p| {
                p.phase = Phase::Failed;
                p.error = Some(e.clone());
            });
            Err(e)
        }
    }
}

/// Fil de copie : liste, copie par morceaux, vérification, puis `Ready`.
fn copy_job(app: &AppHandle, from: &Path, tmp: &Path) {
    let mover = app.state::<Mover>();
    Leftovers::add(&mover.leftovers, tmp);
    let result = (|| -> Result<u64, CopyError> {
        remove_tree(tmp).map_err(|e| CopyError::Failed(tr!("Reste d'une copie précédente impossible à effacer ({}) : {e}", "Couldn't delete what's left of a previous copy ({}): {e}", tmp.display())))?;
        let list = tree(from).map_err(CopyError::Failed)?;
        let total = list.bytes();
        mover.update(app, |p| {
            p.phase = Phase::Copying;
            p.total = total;
        });
        let started = Instant::now();
        let mut copied = 0u64;
        let mut last_sent = Instant::now();
        let mut window: Vec<(Instant, u64)> = vec![(started, 0)];
        let mut on_bytes = |n: u64| {
            copied += n;
            if last_sent.elapsed() < Duration::from_millis(250) {
                return;
            }
            last_sent = Instant::now();
            window.push((last_sent, copied));
            window.retain(|(t, _)| t.elapsed() <= Duration::from_secs(5));
            let (t0, c0) = window[0];
            let secs = t0.elapsed().as_secs_f64();
            let speed = (secs >= 1.0).then(|| ((copied - c0) as f64 / secs) as u64).filter(|&s| s > 0);
            let done = copied;
            mover.update(app, |p| {
                p.copied = done;
                p.speed = speed;
                p.remaining = speed.map(|s| total.saturating_sub(done) / s);
            });
        };
        copy_tree(from, tmp, &list, &mover.cancel, &mut on_bytes, &mut real_file)?;
        mover.update(app, |p| {
            p.phase = Phase::Verifying;
            p.copied = total;
            p.speed = None;
            p.remaining = None;
        });
        verify(&list, tmp).map_err(CopyError::Failed)?;
        Ok(total)
    })();
    match result {
        Ok(total) => {
            log::info!("Copie vérifiée ({}) : {}", relocate::human(total), tmp.display());
            mover.update(app, |p| p.phase = Phase::Ready);
        }
        Err(e) => {
            let cancelled = e == CopyError::Cancelled;
            let text = e.text();
            if remove_tree(tmp).is_ok() {
                Leftovers::forget(&mover.leftovers, tmp);
            }
            if cancelled {
                log::info!("Déplacement annulé pendant la copie : {}", from.display());
            } else {
                log::error!("Copie de {} impossible : {text}", from.display());
            }
            mover.update(app, |p| {
                p.phase = if cancelled { Phase::Cancelled } else { Phase::Failed };
                p.error = (!cancelled).then_some(text);
                p.speed = None;
                p.remaining = None;
            });
        }
    }
}

/// Copie vérifiée et confirmée : bascule, préférences, bibliothèque relue, ancien dossier supprimé.
/// Un échec avant la suppression laisse la copie prête (réessayer ou annuler).
#[tauri::command]
pub async fn move_game_finish(app: AppHandle) -> Result<Progress, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let mover = app.state::<Mover>();
        let (id, from, to, tmp) = {
            let job = lock(&mover.job)?;
            let job = job.as_ref().filter(|j| j.progress.phase == Phase::Ready).ok_or_else(|| tr!("Aucune copie prête à finir.", "No copy ready to finish."))?;
            (job.progress.id.clone(), job.from.clone(), job.to.clone(), job.tmp.clone())
        };
        let back_to_ready = |e: String| {
            log::warn!("Fin du déplacement refusée : {e}");
            mover.update(&app, |p| {
                p.phase = Phase::Ready;
                p.error = Some(e.clone());
            });
            Err(e)
        };
        let (game, _) = match find(&state, &id) {
            Ok(g) => g,
            Err(e) => return back_to_ready(e),
        };
        if let Err(e) = check_stopped(&state, &game) {
            return back_to_ready(e);
        }
        mover.update(&app, |p| {
            p.phase = Phase::Finishing;
            p.error = None;
        });
        let scanning = wait_scan(&state);
        let old = match swap(&from, &tmp, &to) {
            Ok(old) => old,
            Err(e) => {
                drop(scanning);
                return back_to_ready(e);
            }
        };
        if let Err(e) = save_move(&state, &game, &from, &to) {
            unswap(&from, &tmp, &to, &old);
            drop(scanning);
            return back_to_ready(e);
        }
        Leftovers::forget(&mover.leftovers, &tmp);
        Leftovers::add(&mover.leftovers, &old);
        publish(&app, &state, scanning);
        let mut warnings = Vec::new();
        match remove_tree(&old) {
            Ok(()) => Leftovers::forget(&mover.leftovers, &old),
            Err(e) => {
                log::warn!("Ancien dossier pas entièrement supprimé ({}) : {e}", old.display());
                warnings.push(tr!(
                    "Ancien dossier pas entièrement supprimé ({}) : Ludex réessaiera au prochain démarrage. ({e})",
                    "Old folder not fully deleted ({}): Ludex will try again at next start. ({e})",
                    old.display()
                ));
            }
        }
        finish_with(&app, &mover, Ok(()), warnings)
    })
    .await
}

/// Annule : pendant la copie (arrêt au prochain morceau, copie effacée) ou une fois la copie prête
/// (copie effacée). L'original n'a jamais bougé.
#[tauri::command]
pub async fn move_game_cancel(app: AppHandle) -> Result<Option<Progress>, String> {
    blocking(move || {
        let mover = app.state::<Mover>();
        let ready = {
            let job = lock(&mover.job)?;
            match job.as_ref().map(|j| (j.progress.phase, j.tmp.clone())) {
                Some((Phase::Ready, tmp)) => Some(tmp),
                Some((Phase::Preparing | Phase::Copying | Phase::Verifying, _)) => {
                    mover.cancel.store(true, Ordering::SeqCst);
                    None
                }
                _ => None,
            }
        };
        if let Some(tmp) = ready {
            remove_tree(&tmp).map_err(|e| tr!("Copie impossible à effacer ({}) : {e}", "Couldn't delete the copy ({}): {e}", tmp.display()))?;
            Leftovers::forget(&mover.leftovers, &tmp);
            log::info!("Déplacement annulé, copie effacée : {}", tmp.display());
            mover.update(&app, |p| {
                p.phase = Phase::Cancelled;
                p.error = None;
            });
        }
        Ok(mover.progress())
    })
    .await
}

/// Oublie un déplacement fini, raté ou annulé.
#[tauri::command]
pub fn move_game_dismiss(app: AppHandle) -> Result<(), String> {
    let mover = app.state::<Mover>();
    let mut job = lock(&mover.job)?;
    if job.as_ref().is_some_and(|j| !j.progress.phase.active()) {
        *job = None;
    }
    Ok(())
}

/// Déplacement en cours ou fini (pour un panneau rouvert).
#[tauri::command]
pub fn move_game_status(app: AppHandle) -> Option<Progress> {
    app.state::<Mover>().progress()
}

/// Page Stockage des paramètres de Steam (« Déplacer » y est proposé pour chaque jeu).
#[tauri::command]
pub fn open_steam_storage(app: AppHandle) -> Result<(), String> {
    app.opener().open_url(STEAM_STORAGE, None::<&str>).map_err(|e| tr!("Steam n'a pas répondu : {e}", "Steam didn't respond: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{manual::ManualGame, Launch};

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ludex-move-{name}-{}", std::process::id()));
        let _ = remove_tree(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    /// Faux jeu : un exe, un gros fichier (plusieurs morceaux), un sous-dossier, un fichier vide.
    fn fake_game(root: &Path) -> PathBuf {
        let dir = root.join("Jeux").join("Ludex Essai");
        write(&dir.join("Essai.exe"), b"MZ faux");
        write(&dir.join("data").join("gros.pak"), &vec![7u8; CHUNK * 2 + 123]);
        write(&dir.join("data").join("sub").join("vide.txt"), b"");
        write(&dir.join("save").join("slot1.sav"), b"partie");
        fs::create_dir_all(dir.join("vide")).unwrap();
        dir
    }

    fn other(id: &str, dir: &Path) -> Game {
        let mut g = Game::new(Platform::Other, id, id, Launch::Exec { path: dir.join("Essai.exe"), args: Vec::new() });
        g.install_dir = Some(dir.display().to_string());
        g
    }

    fn copy(from: &Path, to: &Path, cancel: &AtomicBool, create: &mut Create) -> (Result<(), CopyError>, u64, Tree) {
        let list = tree(from).unwrap();
        let mut seen = 0;
        let r = copy_tree(from, to, &list, cancel, &mut |n| seen += n, create);
        (r, seen, list)
    }

    #[test]
    fn copies_verifies_and_swaps_a_fake_game() {
        let base = temp_dir("full");
        let from = fake_game(&base);
        let to = base.join("D").join("Jeux").join("Ludex Essai");
        check_plan(&from, &to).unwrap();
        let tmp = sibling(&to, TMP_SUFFIX);
        let (r, seen, list) = copy(&from, &tmp, &AtomicBool::new(false), &mut real_file);
        r.unwrap();
        assert_eq!(seen, list.bytes(), "chaque octet compté");
        assert!(tmp.join("vide").is_dir(), "dossier vide recopié");
        verify(&list, &tmp).unwrap();
        // Date de modification gardée.
        let when = |p: &Path| fs::metadata(p).unwrap().modified().unwrap();
        assert_eq!(when(&from.join("save").join("slot1.sav")), when(&tmp.join("save").join("slot1.sav")));

        let old = swap(&from, &tmp, &to).unwrap();
        assert!(!from.exists() && !tmp.exists() && old.ends_with("Ludex Essai.ludex-old"));
        assert_eq!(fs::read(to.join("save").join("slot1.sav")).unwrap(), b"partie");
        remove_tree(&old).unwrap();
        assert!(!old.exists());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn full_disk_mid_file_leaves_the_original_alone() {
        let base = temp_dir("diskfull");
        let from = fake_game(&base);
        let tmp = base.join("D").join("Jeux").join("Ludex Essai.ludex-tmp");
        /// Écrit `left` octets puis « disque plein ».
        struct Full {
            file: fs::File,
            left: usize,
        }
        impl Write for Full {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                if self.left == 0 {
                    return Err(io::Error::from(io::ErrorKind::StorageFull));
                }
                let n = buf.len().min(self.left);
                self.left -= n;
                self.file.write(&buf[..n])
            }
            fn flush(&mut self) -> io::Result<()> {
                self.file.flush()
            }
        }
        let mut budget = CHUNK + 10;
        let mut create = |p: &Path| -> io::Result<Box<dyn Write>> {
            let left = budget;
            budget = budget.saturating_sub(left);
            Ok(Box::new(Full { file: fs::File::create(p)?, left }))
        };
        let (r, _, _) = copy(&from, &tmp, &AtomicBool::new(false), &mut create);
        let Err(CopyError::Failed(e)) = r else { panic!("{r:?}") };
        assert!(e.contains("Disque plein"), "{e}");
        remove_tree(&tmp).unwrap();
        assert!(!tmp.exists());
        assert_eq!(relocate::size_of(&from), (CHUNK * 2 + 123 + 7 + 6) as u64, "original intact");
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn cancel_stops_between_chunks_and_truncated_copy_is_caught() {
        let base = temp_dir("cancel");
        let from = fake_game(&base);
        let tmp = base.join("copie.ludex-tmp");
        let cancel = AtomicBool::new(false);
        let list = tree(&from).unwrap();
        let mut chunks = 0;
        let r = copy_tree(&from, &tmp, &list, &cancel, &mut |_| {
            chunks += 1;
            if chunks == 1 {
                cancel.store(true, Ordering::SeqCst);
            }
        }, &mut real_file);
        assert_eq!(r, Err(CopyError::Cancelled));
        assert_eq!(CopyError::Cancelled.text(), "Déplacement annulé.");
        assert!(verify(&list, &tmp).is_err(), "copie incomplète refusée");
        remove_tree(&tmp).unwrap();
        // Copie qui « réussit » mais tronque : vérification qui la refuse.
        let mut short = |p: &Path| -> io::Result<Box<dyn Write>> { fs::File::create(p)?; Ok(Box::new(io::sink())) };
        let (r, _, list) = copy(&from, &tmp, &AtomicBool::new(false), &mut short);
        r.unwrap();
        assert!(verify(&list, &tmp).unwrap_err().contains("ne correspond pas"));
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn swap_is_undone_when_the_old_folder_is_locked() {
        let base = temp_dir("locked");
        let from = fake_game(&base);
        let to = base.join("D").join("Ludex Essai");
        let tmp = sibling(&to, TMP_SUFFIX);
        copy(&from, &tmp, &AtomicBool::new(false), &mut real_file).0.unwrap();
        // Fichier ouvert sans partage de suppression : Windows refuse de renommer le dossier.
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            let held = fs::OpenOptions::new().read(true).share_mode(0).open(from.join("Essai.exe")).unwrap();
            let err = swap(&from, &tmp, &to).unwrap_err();
            assert!(err.contains("fichier est sans doute ouvert"), "{err}");
            assert!(from.join("Essai.exe").is_file() && tmp.is_dir() && !to.exists(), "rien n'a bougé, copie gardée");
            drop(held);
        }
        // Défaire une bascule réussie : tout revient.
        let old = swap(&from, &tmp, &to).unwrap();
        unswap(&from, &tmp, &to, &old);
        assert!(from.join("Essai.exe").is_file() && tmp.is_dir() && !to.exists() && !old.exists());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn same_drive_is_a_rename_and_refuses_taken_or_nested_targets() {
        let base = temp_dir("same");
        let from = fake_game(&base);
        let taken = base.join("Games").join("Ludex Essai");
        write(&taken.join("autre.txt"), b"x");
        assert!(check_plan(&from, &taken).unwrap_err().contains("n'est pas vide"));
        assert!(check_plan(&from, &from.join("sous")).unwrap_err().contains("dans le dossier du jeu"));
        assert!(check_plan(&base.join("absent"), &base.join("x").join("y")).unwrap_err().contains("introuvable"));
        // Dossier vide du même nom : accepté.
        let empty = base.join("Jeux2").join("Ludex Essai");
        fs::create_dir_all(&empty).unwrap();
        check_plan(&from, &empty).unwrap();
        rename_in_place(&from, &empty).unwrap();
        assert!(!from.exists() && empty.join("Essai.exe").is_file());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn checks_free_space_with_a_margin() {
        assert!(check_space(5_000_000_000, 6_000_000_000, "D:").is_ok());
        let err = check_space(5_500_000_000, 6_000_000_000, "D:").unwrap_err();
        assert_eq!(err, "Pas assez de place sur D: : 5,5\u{a0}Go à déplacer, 6\u{a0}Go libres (1 Go gardé en plus).");
        crate::i18n::with(crate::i18n::Lang::En, || {
            assert_eq!(check_space(5_500_000_000, 6_000_000_000, "D:").unwrap_err(), "Not enough space on D:: 5.5\u{a0}GB to move, 6\u{a0}GB free (1 GB kept spare).");
        });
    }

    fn vol(mount: &str, free: u64) -> storage::Volume {
        storage::Volume { mount: mount.into(), name: format!("Disque {}", &mount[..1]), total: 4_000_000_000_000, free }
    }

    #[test]
    fn offers_game_folders_and_new_ones_on_bare_drives() {
        let libs = vec![PathBuf::from(r"E:\Jeux"), PathBuf::from(r"E:\Jeux\Jeux Epic"), PathBuf::from(r"D:\Games")];
        let vols = vec![vol(r"C:\", 100_000_000_000), vol(r"D:\", 3_000_000_000), vol(r"E:\", 900_000_000_000), vol(r"F:\Montage\", 1)];
        let list = targets(Path::new(r"E:\Jeux\Frostpunk"), 5_000_000_000, &libs, &vols);
        let paths: Vec<&str> = list.iter().map(|t| t.path.as_str()).collect();
        assert_eq!(paths, [r"C:\Jeux\Frostpunk", r"D:\Games\Frostpunk", r"E:\Jeux\Jeux Epic\Frostpunk"]);
        assert!(list[0].create && !list[0].same_drive && list[0].enough);
        assert!(!list[1].enough, "3 Go libres pour 5 Go");
        assert!(list[2].same_drive && list[2].enough);
        assert_eq!((list[1].letter.as_str(), list[1].key.as_str()), ("D:", r"D:\Games"));
    }

    #[test]
    fn rebases_paths_under_the_moved_folder() {
        let from = Path::new(r"E:\Jeux\Frostpunk");
        let to = Path::new(r"D:\Games\Frostpunk");
        assert_eq!(rebase(Path::new(r"e:\jeux\frostpunk\Bin\Frostpunk.exe"), from, to), PathBuf::from(r"D:\Games\Frostpunk\Bin\Frostpunk.exe"));
        assert_eq!(rebase(Path::new(r"E:\Jeux\Frostpunk"), from, to), to);
        assert_eq!(rebase(Path::new(r"E:\Jeux\Frostpunk 2\x.exe"), from, to), PathBuf::from(r"E:\Jeux\Frostpunk 2\x.exe"));
    }

    #[test]
    fn a_moved_game_keeps_its_original_id_through_several_moves() {
        let e = Path::new(r"E:\Jeux\Frostpunk");
        let d = Path::new(r"D:\Games\Frostpunk");
        let c = Path::new(r"C:\Jeux\Frostpunk");
        let mut data = UserData::default();
        data.exes.insert("other:e-jeux-frostpunk".into(), PathBuf::from(r"E:\Jeux\Frostpunk\bin\fp.exe"));
        let game = other("e-jeux-frostpunk", e);
        record_move(&mut data, &game, e, d);
        assert_eq!(data.moved_ids, BTreeMap::from([("other:d-games-frostpunk".to_string(), "other:e-jeux-frostpunk".to_string())]));
        assert_eq!(data.exes["other:e-jeux-frostpunk"], PathBuf::from(r"D:\Games\Frostpunk\bin\fp.exe"));

        // Scan suivant : le jeu trouvé sur D: reprend son id, et ses images locales suivent.
        let mut scanned = other("d-games-frostpunk", d);
        scanned.icon_url = Some(library::art_url(&scanned, "icon"));
        let mut games = vec![scanned, other("autre", Path::new(r"D:\Games\Autre"))];
        apply_moved(&mut games, &data.moved_ids);
        assert_eq!((games[0].id.as_str(), games[0].platform_id.as_str()), ("other:e-jeux-frostpunk", "e-jeux-frostpunk"));
        assert_eq!(games[0].icon_url.as_deref(), Some(format!("{ART_ORIGIN}/other/e-jeux-frostpunk/icon").as_str()));
        assert_eq!(games[1].id, "other:autre");

        // Second déplacement : toujours l'id d'origine, une seule entrée.
        record_move(&mut data, &games[0], d, c);
        assert_eq!(data.moved_ids, BTreeMap::from([("other:c-jeux-frostpunk".to_string(), "other:e-jeux-frostpunk".to_string())]));
        // Retour à sa place d'origine : plus d'entrée.
        record_move(&mut data, &games[0], c, e);
        assert!(data.moved_ids.is_empty());
        assert_eq!(data.exes["other:e-jeux-frostpunk"], PathBuf::from(r"E:\Jeux\Frostpunk\bin\fp.exe"));
    }

    #[test]
    fn the_original_id_is_never_given_twice() {
        let moved = BTreeMap::from([("other:d-games-x".to_string(), "other:e-jeux-x".to_string())]);
        // Un autre jeu occupe de nouveau l'ancien dossier : l'id n'est pas repris.
        let mut games = vec![other("d-games-x", Path::new(r"D:\Games\X")), other("e-jeux-x", Path::new(r"E:\Jeux\X"))];
        apply_moved(&mut games, &moved);
        assert_eq!((games[0].id.as_str(), games[1].id.as_str()), ("other:d-games-x", "other:e-jeux-x"));
        // Un jeu manuel n'est jamais renommé : son id est fixe.
        let mut manual = other("d-games-x", Path::new(r"D:\Games\X"));
        manual.manual = true;
        let mut games = vec![manual];
        apply_moved(&mut games, &moved);
        assert_eq!(games[0].id, "other:d-games-x");
    }

    #[test]
    fn a_manual_game_follows_with_its_exe() {
        let from = Path::new(r"E:\Jeux\Essai");
        let to = Path::new(r"D:\Mes jeux\Essai");
        let mut data = UserData::default();
        data.manual.push(ManualGame { id: "other:manual-0123".into(), exe: PathBuf::from(r"E:\Jeux\Essai\x64\Essai.exe"), title: "Essai".into(), args: Vec::new() });
        let mut game = other("manual-0123", from);
        game.manual = true;
        record_move(&mut data, &game, from, to);
        assert_eq!(data.manual[0].exe, PathBuf::from(r"D:\Mes jeux\Essai\x64\Essai.exe"));
        assert!(data.moved_ids.is_empty(), "id fixe, rien à retenir");
    }

    #[test]
    fn decides_who_moves_each_game() {
        let libs = vec![PathBuf::from(r"E:\Jeux")];
        let mut steam = Game::new(Platform::Steam, "570", "Dota 2", Launch::Uri(String::new()));
        steam.install_dir = Some(r"E:\Steam\steamapps\common\dota".into());
        let mut epic = Game::new(Platform::Epic, "x", "Epic", Launch::Uri(String::new()));
        epic.install_dir = Some(r"E:\Epic\X".into());
        let mut xbox = Game::new(Platform::Xbox, "x", "Xbox", Launch::Uri(String::new()));
        xbox.install_dir = Some(r"C:\XboxGames\X".into());
        let ok = other("e-jeux-frostpunk", Path::new(r"E:\Jeux\Frostpunk"));
        let elsewhere = other("d-outils-x", Path::new(r"D:\Outils\X"));
        let mut manual = other("manual-1", Path::new(r"D:\Outils\Y"));
        manual.manual = true;
        let mut fivem = other("fivem", Path::new(r"C:\Users\A\AppData\Local\FiveM"));
        fivem.via_launcher = false;
        let parent = other("e-jeux", Path::new(r"E:\Jeux\Pack"));
        let child = other("e-jeux-pack-sub", Path::new(r"E:\Jeux\Pack\Sub"));
        let mut none = other("none", Path::new(r"E:\Jeux\None"));
        none.install_dir = None;
        let games = vec![steam.clone(), epic.clone(), xbox.clone(), ok.clone(), elsewhere.clone(), manual.clone(), fivem.clone(), parent.clone(), child.clone()];
        let m = |g: &Game| mode(g, &games, &libs, &[]).0;
        assert_eq!([m(&steam), m(&epic), m(&xbox), m(&ok), m(&manual)], [Mode::Steam, Mode::Launcher, Mode::Windows, Mode::Ludex, Mode::Ludex]);
        assert_eq!([m(&elsewhere), m(&fivem), m(&parent), m(&child), m(&none)], [Mode::Refused; 5]);
        assert!(mode(&elsewhere, &games, &libs, &[]).1.unwrap().contains("« Jeux »"));
        // Désigné par une entrée de « Applications installées » : refusé.
        let entry = footprint::UninstallEntry {
            hive: footprint::Hive::Hkcu,
            path: r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Frostpunk_is1".into(),
            name: "Frostpunk_is1".into(),
            display_name: Some("Frostpunk".into()),
            publisher: None,
            version: None,
            install_location: Some(r"E:\Jeux\Frostpunk".into()),
            uninstall_string: None,
            install_date: None,
            estimated_kb: None,
        };
        let (mode_, reason) = mode(&ok, &games, &libs, &[entry]);
        assert_eq!(mode_, Mode::Refused);
        assert!(reason.unwrap().contains("Applications installées"));
    }

    #[cfg(windows)]
    #[test]
    fn a_junction_inside_the_game_is_refused() {
        let base = temp_dir("junction");
        let from = fake_game(&base);
        let target = base.join("ailleurs");
        fs::create_dir_all(&target).unwrap();
        let link = from.join("lien");
        let made = std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(&link).arg(&target).output().unwrap();
        assert!(made.status.success(), "{made:?}");
        let err = tree(&from).unwrap_err();
        assert!(err.contains("jonction") && err.contains("lien"), "{err}");
        fs::remove_dir(&link).unwrap();
        assert!(tree(&from).is_ok());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn leftovers_are_cleaned_only_when_they_look_like_ours() {
        let base = temp_dir("leftovers");
        let file = base.join(LEFTOVERS_FILE);
        let tmp = base.join("Jeux").join("X.ludex-tmp");
        let old = base.join("Jeux").join("Y.ludex-old");
        let foreign = base.join("Jeux").join("Z");
        for d in [&tmp, &old, &foreign] {
            write(&d.join("f.bin"), b"x");
        }
        Leftovers::add(&file, &tmp);
        Leftovers::add(&file, &old);
        Leftovers::add(&file, &tmp);
        Leftovers::add(&file, &foreign);
        assert_eq!(Leftovers::load(&file).dirs.len(), 3);
        Leftovers::clean(&file);
        assert!(!tmp.exists() && !old.exists(), "dossiers de travail supprimés");
        assert!(foreign.join("f.bin").is_file(), "jamais un dossier sans suffixe de travail");
        assert!(!file.exists(), "plus rien à noter");
        assert!(library::is_move_leftover(Path::new(r"E:\Jeux\Hades.LUDEX-TMP")));
        assert!(!library::is_move_leftover(Path::new(r"E:\Jeux\Hades")));
        fs::remove_dir_all(&base).unwrap();
    }
}
