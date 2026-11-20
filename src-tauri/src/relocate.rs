//! Emplacement des données lourdes de Ludex : cache des images (`images/`, jaquettes perso
//! comprises) et index de la base des sauvegardes (`saves/`), plus les copies de sauvegardes quand
//! leur dossier n'a pas été choisi à part (`savesDir`).
//!
//! Par défaut : cache dans le dossier de données (`%APPDATA%\com.ludex.app`), copies dans
//! `Documents\Ludex\Sauvegardes`. Réglage `dataDir` (« D:\Ludex ») : cache dans `<dataDir>\Cache`,
//! copies dans `<dataDir>\Sauvegardes`.
//!
//! Restent toujours dans `%APPDATA%` : `user-data.json` (il porte ce réglage : lu avant tout le
//! reste), `sessions.json`, les petits caches JSON et les copies de sécurité d'import. Moins d'un
//! mégaoctet en tout, et jamais perdus si le disque choisi est débranché.
//!
//! Déplacement : refusé pendant une partie ; scans, téléchargements d'images, lecture de la base
//! des sauvegardes et copies arrêtés le temps de l'opération. Copie dans `<cible>.ludex-tmp`,
//! vérification (mêmes fichiers, mêmes tailles), renommage, réglage enregistré, bibliothèque
//! relue, puis anciens dossiers supprimés en dernier. Un échec avant l'enregistrement laisse tout
//! comme avant (copies partielles effacées). Le front n'envoie jamais de chemin : un disque de la
//! liste (point de montage relu ici), « l'emplacement d'origine », ou le dossier choisi avec le
//! sélecteur natif et gardé côté Rust.

use crate::{blocking, lock, rescan, saves, storage, wait_scan, AppState};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

/// Dossier créé dans le dossier ou sur le disque choisi.
pub const FOLDER: &str = "Ludex";
/// Sous-dossier du cache quand `dataDir` est réglé.
pub const CACHE: &str = "Cache";
/// Sous-dossier des copies de sauvegardes quand `dataDir` est réglé.
pub const COPIES: &str = "Sauvegardes";
/// Parties lourdes du cache, dans sa racine.
pub const HEAVY: [&str; 2] = ["images", "saves"];
/// Suffixe des copies en cours.
const TMP: &str = ".ludex-tmp";
/// Place laissée libre sur le disque de destination, en plus des données.
pub const MARGIN: u64 = 64 * 1024 * 1024;

/// Racine du cache : le dossier de données, ou `<dataDir>\Cache`.
pub fn cache_root(app_data: &Path, data_dir: Option<&Path>) -> PathBuf {
    data_dir.map_or_else(|| app_data.to_path_buf(), |d| d.join(CACHE))
}

/// Dossier des copies de sauvegardes : choisi à part, sinon `<dataDir>\Sauvegardes`, sinon le défaut.
pub fn copies_root(saves_dir: Option<&Path>, data_dir: Option<&Path>, default: impl FnOnce() -> PathBuf) -> PathBuf {
    match (saves_dir, data_dir) {
        (Some(dir), _) => dir.to_path_buf(),
        (None, Some(root)) => root.join(COPIES),
        (None, None) => default(),
    }
}

/// Dossier racine pour un dossier choisi : lui-même s'il s'appelle déjà « Ludex », sinon
/// `<choisi>\Ludex` (jamais les fichiers de Ludex en vrac à la racine d'un disque).
pub fn root_for(picked: &Path) -> PathBuf {
    match picked.file_name().and_then(|n| n.to_str()) {
        Some(name) if name.eq_ignore_ascii_case(FOLDER) => picked.to_path_buf(),
        _ => picked.join(FOLDER),
    }
}

/// Dossier racine acceptable : chemin absolu sur un lecteur de ce PC (pas un partage réseau),
/// pas la racine d'un disque, et le premier dossier existant du chemin est un vrai dossier.
pub fn check_root(root: &Path) -> Result<(), String> {
    let text = root.to_string_lossy();
    if text.starts_with(r"\\") || text.starts_with("//") {
        return Err("Choisis un dossier sur un disque de ce PC (pas un partage réseau).".into());
    }
    if !root.is_absolute() || root.parent().is_none() {
        return Err("Choisis un dossier complet.".into());
    }
    if root.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err("Chemin refusé.".into());
    }
    let existing = root.ancestors().find(|d| fs::symlink_metadata(d).is_ok()).ok_or_else(|| format!("Le disque de {} est introuvable.", root.display()))?;
    match fs::symlink_metadata(existing) {
        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => Ok(()),
        _ => Err(format!("{} n'est pas un dossier utilisable.", existing.display())),
    }
}

/// Crée le dossier racine et vérifie que Ludex peut y écrire.
fn prepare_root(root: &Path) -> Result<(), String> {
    fs::create_dir_all(root).map_err(|e| format!("Impossible de créer {} : {e}", root.display()))?;
    let probe = root.join(".ludex-essai");
    fs::write(&probe, b"").map_err(|e| format!("Ludex ne peut pas écrire dans {} : {e}", root.display()))?;
    let _ = fs::remove_file(probe);
    Ok(())
}

/// Emplacements d'un réglage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Places {
    pub cache: PathBuf,
    pub copies: PathBuf,
}

impl Places {
    pub fn new(app_data: &Path, data_dir: Option<&Path>, saves_dir: Option<&Path>) -> Self {
        Places { cache: cache_root(app_data, data_dir), copies: copies_root(saves_dir, data_dir, saves::default_dir) }
    }
}

/// Un dossier à déplacer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub from: PathBuf,
    pub to: PathBuf,
}

fn tmp_of(dir: &Path) -> PathBuf {
    let mut name = dir.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(TMP);
    dir.with_file_name(name)
}

fn real_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
}

fn is_empty_dir(path: &Path) -> bool {
    fs::read_dir(path).is_ok_and(|mut it| it.next().is_none())
}

/// Dossiers à déplacer d'un emplacement à l'autre. Les copies ne suivent que si leur dossier n'a
/// pas été choisi à part. Rien à faire pour un dossier absent ou déjà au bon endroit ; refusé si
/// une cible est dans sa source (ou l'inverse), ou existe déjà avec des fichiers.
pub fn plan(from: &Places, to: &Places, copies_follow: bool) -> Result<Vec<Part>, String> {
    let mut pairs: Vec<Part> = HEAVY.iter().map(|name| Part { from: from.cache.join(name), to: to.cache.join(name) }).collect();
    if copies_follow {
        pairs.push(Part { from: from.copies.clone(), to: to.copies.clone() });
    }
    let mut parts = Vec::new();
    for part in pairs {
        if saves::is_within(&part.from, &part.to) && saves::is_within(&part.to, &part.from) {
            continue; // même dossier
        }
        if !real_dir(&part.from) {
            continue;
        }
        if saves::is_within(&part.to, &part.from) || saves::is_within(&part.from, &part.to) {
            return Err(format!("{} et {} sont l'un dans l'autre : choisis un autre dossier.", part.from.display(), part.to.display()));
        }
        if part.to.exists() && !is_empty_dir(&part.to) {
            return Err(format!("{} existe déjà et n'est pas vide : choisis un autre dossier.", part.to.display()));
        }
        parts.push(part);
    }
    Ok(parts)
}

/// Fichiers d'un dossier (chemin relatif, taille), liens symboliques et jonctions jamais suivis.
pub fn files(root: &Path) -> io::Result<Vec<(PathBuf, u64)>> {
    let mut out = Vec::new();
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        for entry in fs::read_dir(root.join(&rel))? {
            let entry = entry?;
            let kind = entry.file_type()?;
            let path = rel.join(entry.file_name());
            if kind.is_symlink() {
                continue;
            } else if kind.is_dir() {
                stack.push(path);
            } else if kind.is_file() {
                out.push((path, entry.metadata()?.len()));
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Taille d'un dossier (0 s'il est absent ou illisible).
pub fn size_of(dir: &Path) -> u64 {
    files(dir).map(|list| list.iter().map(|(_, n)| n).sum()).unwrap_or(0)
}

/// « 512 Ko », « 6,3 Mo », « 1,2 Go » (unités de 1 000, comme le reste de Ludex).
pub fn human(bytes: u64) -> String {
    let (value, unit) = match bytes {
        b if b >= 1_000_000_000 => (b as f64 / 1e9, "Go"),
        b if b >= 1_000_000 => (b as f64 / 1e6, "Mo"),
        b => (b as f64 / 1e3, "Ko"),
    };
    let text = if value < 10.0 { format!("{value:.1}").replace('.', ",") } else { format!("{value:.0}") };
    format!("{}\u{a0}{unit}", text.trim_end_matches(",0"))
}

/// Place suffisante : les données plus une marge.
pub fn check_space(needed: u64, free: u64, disk: &str) -> Result<(), String> {
    if needed.saturating_add(MARGIN) > free {
        return Err(format!("Pas assez de place sur {disk} : {} à déplacer, {} libres.", human(needed), human(free)));
    }
    Ok(())
}

/// Copie un dossier fichier par fichier avec `write` (`fs::copy`, ou un disque plein simulé).
fn copy_tree(from: &Path, to: &Path, list: &[(PathBuf, u64)], write: &mut dyn FnMut(&Path, &Path) -> io::Result<u64>) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| format!("Impossible de créer {} : {e}", to.display()))?;
    for (rel, _) in list {
        let target = to.join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Impossible de créer {} : {e}", parent.display()))?;
        }
        write(&from.join(rel), &target).map_err(|e| match e.kind() {
            io::ErrorKind::StorageFull => format!("Disque plein pendant la copie de {}.", rel.display()),
            _ => format!("Copie de {} impossible : {e}", from.join(rel).display()),
        })?;
    }
    Ok(())
}

/// Les mêmes fichiers, aux mêmes tailles, des deux côtés.
fn verify(list: &[(PathBuf, u64)], copy: &Path) -> Result<(), String> {
    let copied = files(copy).map_err(|e| format!("Vérification de {} impossible : {e}", copy.display()))?;
    if copied != list {
        return Err(format!("La copie de {} ne correspond pas à l'original : rien n'a été changé.", copy.display()));
    }
    Ok(())
}

/// Résultat de la copie.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Copied {
    pub bytes: u64,
    pub files: usize,
}

/// Copie et vérifie chaque dossier dans `<cible>.ludex-tmp`, puis renomme les copies. À la
/// moindre erreur, les copies et les dossiers déjà renommés sont effacés : les sources restent.
pub fn copy_all(parts: &[Part], write: &mut dyn FnMut(&Path, &Path) -> io::Result<u64>) -> Result<Copied, String> {
    let mut done = Copied::default();
    let mut tmps: Vec<PathBuf> = Vec::new();
    let result = (|| {
        for part in parts {
            let tmp = tmp_of(&part.to);
            if tmp.exists() {
                fs::remove_dir_all(&tmp).map_err(|e| format!("Reste d'une copie précédente impossible à effacer ({}) : {e}", tmp.display()))?;
            }
            tmps.push(tmp.clone());
            let list = files(&part.from).map_err(|e| format!("Lecture de {} impossible : {e}", part.from.display()))?;
            copy_tree(&part.from, &tmp, &list, write)?;
            verify(&list, &tmp)?;
            done.bytes += list.iter().map(|(_, n)| n).sum::<u64>();
            done.files += list.len();
        }
        Ok(())
    })();
    if let Err(e) = result {
        for tmp in &tmps {
            let _ = fs::remove_dir_all(tmp);
        }
        return Err(e);
    }
    let mut renamed: Vec<&Path> = Vec::new();
    for (part, tmp) in parts.iter().zip(&tmps) {
        // Dossier cible vide laissé là (vérifié par `plan`) : retiré pour le renommage.
        if part.to.exists() {
            let _ = fs::remove_dir(&part.to);
        }
        if let Err(e) = fs::rename(tmp, &part.to) {
            for dir in renamed {
                let _ = fs::remove_dir_all(dir);
            }
            for tmp in &tmps {
                let _ = fs::remove_dir_all(tmp);
            }
            return Err(format!("Impossible de finaliser {} : {e}", part.to.display()));
        }
        renamed.push(&part.to);
    }
    Ok(done)
}

/// Annule une copie finalisée (réglage non enregistré) : les cibles sont effacées.
pub fn undo(parts: &[Part]) {
    for part in parts {
        let _ = fs::remove_dir_all(&part.to);
    }
}

/// Supprime les anciens dossiers, puis les dossiers parents devenus vides (`prune`, dans l'ordre,
/// seulement s'ils sont vides). Renvoie les avertissements : les données sont déjà à leur nouvelle
/// place, un reste n'est pas une erreur.
pub fn remove_sources(parts: &[Part], prune: &[PathBuf]) -> Vec<String> {
    let mut warnings = Vec::new();
    for part in parts {
        if let Err(e) = fs::remove_dir_all(&part.from) {
            warnings.push(format!("Ancien dossier {} pas entièrement supprimé : {e}", part.from.display()));
        }
    }
    for dir in prune {
        if real_dir(dir) && is_empty_dir(dir) {
            let _ = fs::remove_dir(dir);
        }
    }
    warnings
}

/// Parents à retirer s'ils sont vides après un déplacement depuis `from` : `Cache` et le dossier
/// racine d'un ancien `dataDir`, `Documents\Ludex` des copies par défaut.
pub fn prune_list(old_root: Option<&Path>, from: &Places, parts: &[Part]) -> Vec<PathBuf> {
    let mut list = Vec::new();
    if let Some(root) = old_root {
        list.push(root.join(CACHE));
    }
    if parts.iter().any(|p| p.from == from.copies)
        && let Some(parent) = from.copies.parent()
        && parent.file_name().is_some_and(|n| n.eq_ignore_ascii_case(FOLDER))
    {
        list.push(parent.to_path_buf());
    }
    if let Some(root) = old_root {
        list.push(root.to_path_buf());
    }
    list
}

// ------------------------------------------------------------------------------------------------
// Commandes

/// Disque proposé par l'écran d'accueil et les Paramètres.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskChoice {
    /// Point de montage (« D:\ »), renvoyé tel quel pour choisir ce disque.
    mount: String,
    /// « D: ».
    letter: String,
    /// Nom du volume (« Jeux »), vide s'il n'en a pas.
    name: String,
    total: u64,
    free: u64,
    removable: bool,
    /// Disque de Windows.
    system: bool,
    /// Disque où sont les données lourdes aujourd'hui.
    current: bool,
    /// Dossier qui serait utilisé : « D:\Ludex ».
    target: String,
}

#[derive(Clone, Debug)]
struct Disk {
    mount: String,
    name: String,
    total: u64,
    free: u64,
    removable: bool,
}

fn list_disks() -> Vec<Disk> {
    let Ok(disks) = sysinfo::Disks::new_with_refreshed_list() else { return Vec::new() };
    let mut out: Vec<Disk> = disks
        .iter()
        .filter(|d| d.total_space() > 0)
        .map(|d| Disk {
            mount: d.mount_point().to_string_lossy().into_owned(),
            name: d.name().to_string_lossy().trim().to_string(),
            total: d.total_space(),
            free: d.available_space(),
            removable: d.is_removable(),
        })
        .collect();
    out.sort_by_key(|a| a.mount.to_lowercase());
    out.dedup_by(|a, b| a.mount.eq_ignore_ascii_case(&b.mount));
    out
}

fn volumes_of(disks: &[Disk]) -> Vec<storage::Volume> {
    disks.iter().map(|d| storage::Volume { mount: d.mount.clone(), name: d.name.clone(), total: d.total, free: d.free }).collect()
}

/// Où sont les données de Ludex, combien elles pèsent, et les disques possibles.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    /// Dossier choisi (`dataDir`), vide = emplacement d'origine.
    root: String,
    /// Dossier choisi introuvable (disque débranché ?).
    missing: bool,
    cache_dir: String,
    cache_bytes: u64,
    copies_dir: String,
    copies_bytes: u64,
    /// Dossier des copies choisi à part dans les Paramètres : il ne bouge pas avec le reste.
    copies_separate: bool,
    /// Ce qui reste toujours dans `%APPDATA%` (préférences, historique, petits caches).
    kept_dir: String,
    kept_bytes: u64,
    /// Emplacements d'origine, pour « Revenir à l'emplacement d'origine ».
    default_cache: String,
    default_copies: String,
    disks: Vec<DiskChoice>,
    /// Dossier choisi avec le sélecteur, en attente de confirmation.
    picked: Option<String>,
    /// Aperçu de l'écran d'accueil (`LUDEX_WELCOME=1`) : rien n'est déplacé.
    preview: bool,
}

fn settings_of(state: &AppState) -> Result<(Option<PathBuf>, Option<PathBuf>), String> {
    let data = lock(&state.data)?;
    Ok((data.settings.data_dir.clone(), data.settings.saves_dir.clone()))
}

/// Racine actuelle du cache (dossier de données ou `<dataDir>\Cache`).
pub fn current_cache(state: &AppState) -> PathBuf {
    let data_dir = lock(&state.data).ok().and_then(|d| d.settings.data_dir.clone());
    cache_root(&state.data_dir, data_dir.as_deref())
}

/// Le dossier choisi est réglé mais introuvable (disque débranché) : rien n'y est écrit.
pub fn unavailable(state: &AppState) -> bool {
    lock(&state.data).ok().and_then(|d| d.settings.data_dir.clone()).is_some_and(|d| !d.is_dir())
}

fn preview(app: &AppHandle) -> bool {
    app.try_state::<crate::welcome::Welcome>().is_some_and(|w| w.forced())
}

fn location(app: &AppHandle) -> Result<Location, String> {
    let state = app.state::<AppState>();
    let (data_dir, saves_dir) = settings_of(&state)?;
    let places = Places::new(&state.data_dir, data_dir.as_deref(), saves_dir.as_deref());
    let cache_bytes: u64 = HEAVY.iter().map(|n| size_of(&places.cache.join(n))).sum();
    let all = size_of(&state.data_dir);
    let kept_bytes = if data_dir.is_none() { all.saturating_sub(cache_bytes) } else { all };
    let disks = list_disks();
    let volumes = volumes_of(&disks);
    let system = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let system_mount = storage::volume_of(&system.to_string_lossy(), &volumes).map(|v| v.mount.clone());
    let current_mount = storage::volume_of(&places.cache.to_string_lossy(), &volumes).map(|v| v.mount.clone());
    let disks = disks
        .into_iter()
        .map(|d| DiskChoice {
            letter: d.mount.trim_end_matches(['\\', '/']).to_string(),
            target: Path::new(&d.mount).join(FOLDER).display().to_string(),
            system: system_mount.as_deref() == Some(d.mount.as_str()),
            current: current_mount.as_deref() == Some(d.mount.as_str()),
            mount: d.mount,
            name: d.name,
            total: d.total,
            free: d.free,
            removable: d.removable,
        })
        .collect();
    let picked = lock(&state.picked_data_dir)?.as_ref().map(|p| p.display().to_string());
    Ok(Location {
        root: data_dir.as_ref().map(|d| d.display().to_string()).unwrap_or_default(),
        missing: data_dir.as_ref().is_some_and(|d| !d.is_dir()),
        cache_dir: places.cache.display().to_string(),
        cache_bytes,
        copies_dir: places.copies.display().to_string(),
        copies_bytes: size_of(&places.copies),
        copies_separate: saves_dir.is_some(),
        kept_dir: state.data_dir.display().to_string(),
        kept_bytes,
        default_cache: state.data_dir.display().to_string(),
        default_copies: saves::default_dir().display().to_string(),
        disks,
        picked,
        preview: preview(app),
    })
}

#[tauri::command]
pub async fn data_location(app: AppHandle) -> Result<Location, String> {
    blocking(move || location(&app)).await
}

/// Choisit un dossier avec le sélecteur natif. Le chemin reste côté Rust (déplacement confirmé
/// ensuite par `move_data` avec `picked`) ; le front ne reçoit que le texte du dossier qui sera
/// utilisé (`<choisi>\Ludex`). `None` : annulé.
#[tauri::command]
pub async fn choose_data_dir(app: AppHandle, window: tauri::WebviewWindow) -> Result<Option<String>, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let mut dialog = window.dialog().file().set_title("Dossier des données de Ludex").set_parent(&window);
        if let Some(start) = current_cache(&state).ancestors().find(|d| d.is_dir()) {
            dialog = dialog.set_directory(start);
        }
        let Some(picked) = dialog.blocking_pick_folder().and_then(|f| f.into_path().ok()) else { return Ok(None) };
        let root = root_for(&picked);
        check_root(&root)?;
        let shown = root.display().to_string();
        *lock(&state.picked_data_dir)? = Some(root);
        Ok(Some(shown))
    })
    .await
}

/// Destination demandée par le front : jamais un chemin libre.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Target {
    /// Emplacement d'origine (`%APPDATA%`, `Documents\Ludex\Sauvegardes`).
    Default,
    /// `<disque>\Ludex`, pour un point de montage de la liste.
    Drive { mount: String },
    /// Dossier choisi par `choose_data_dir`.
    Picked,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveDone {
    scan: crate::library::ScanResult,
    location: Location,
    /// Octets déplacés.
    moved: u64,
    warnings: Vec<String>,
}

/// Drapeau `*_busy` pris le temps du déplacement, rendu à la fin.
struct Held<'a>(&'a AtomicBool);

impl Drop for Held<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn hold<'a>(flag: &'a AtomicBool, what: &str) -> Result<Held<'a>, String> {
    for _ in 0..150 {
        if !flag.swap(true, Ordering::SeqCst) {
            return Ok(Held(flag));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err(format!("{what} : réessaie dans un instant."))
}

fn same(a: Option<&Path>, b: Option<&Path>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => saves::is_within(a, b) && saves::is_within(b, a),
        _ => false,
    }
}

/// Déplace les données lourdes vers `target`, puis relit la bibliothèque (images servies depuis
/// le nouveau dossier).
#[tauri::command]
pub async fn move_data(app: AppHandle, target: Target) -> Result<MoveDone, String> {
    blocking(move || {
        if preview(&app) {
            return Err("Aperçu de l'écran d'accueil : rien n'est déplacé.".into());
        }
        let state = app.state::<AppState>();
        if !state.tracker.active().is_empty() {
            return Err("Une partie est en cours : ferme le jeu avant de déplacer les données de Ludex.".into());
        }
        let _moving = hold(&state.relocating, "Un déplacement est déjà en cours")?;
        let disks = list_disks();
        let new_root = match target {
            Target::Default => None,
            Target::Drive { mount } => {
                let disk = disks.iter().find(|d| d.mount == mount).ok_or("Ce disque n'est plus disponible.")?;
                Some(Path::new(&disk.mount).join(FOLDER))
            }
            Target::Picked => Some(lock(&state.picked_data_dir)?.take().ok_or("Choisis d'abord un dossier.")?),
        };
        let (old_root, saves_dir) = settings_of(&state)?;
        if same(old_root.as_deref(), new_root.as_deref()) {
            return Err("Les données de Ludex sont déjà à cet endroit.".into());
        }
        if let Some(root) = &new_root {
            check_root(root)?;
        }

        // Plus rien n'écrit dans les dossiers concernés pendant la copie.
        let scanning = wait_scan(&state);
        let _copies = lock(&state.saves_lock)?;
        let images_held = hold(&state.images_busy, "Des images sont en cours de téléchargement")?;
        let saves_held = hold(&state.saves_busy, "La base des sauvegardes est en cours de lecture")?;

        let from = Places::new(&state.data_dir, old_root.as_deref(), saves_dir.as_deref());
        let to = Places::new(&state.data_dir, new_root.as_deref(), saves_dir.as_deref());
        let parts = plan(&from, &to, saves_dir.is_none())?;
        let volumes = volumes_of(&disks);
        let mut by_disk: Vec<(&storage::Volume, u64)> = Vec::new();
        for part in &parts {
            let Some(volume) = storage::volume_of(&part.to.to_string_lossy(), &volumes) else { continue };
            let bytes = size_of(&part.from);
            match by_disk.iter_mut().find(|(v, _)| v.mount == volume.mount) {
                Some((_, total)) => *total += bytes,
                None => by_disk.push((volume, bytes)),
            }
        }
        for (volume, bytes) in &by_disk {
            check_space(*bytes, volume.free, volume.mount.trim_end_matches('\\'))?;
        }
        if let Some(root) = &new_root {
            prepare_root(root)?;
        }
        let copied = copy_all(&parts, &mut |a, b| fs::copy(a, b))?;

        // Réglage enregistré : à partir d'ici, les nouvelles places font foi.
        {
            let mut data = lock(&state.data)?;
            let before = data.settings.data_dir.clone();
            data.settings.data_dir = new_root.clone();
            if let Err(e) = data.save(&state.data_path) {
                data.settings.data_dir = before;
                drop(data);
                undo(&parts);
                return Err(format!("Réglage non enregistré, rien n'a été déplacé : {e}"));
            }
        }
        *lock(&state.images)? = crate::images::ImageCache::load(to.cache.join("images"));
        // Bibliothèque relue : les images en cache pointent vers le nouveau dossier.
        let (scan, _) = rescan(&app, &state, scanning)?;
        let warnings = remove_sources(&parts, &prune_list(old_root.as_deref(), &from, &parts));
        drop(images_held);
        drop(saves_held);
        crate::resolve_images(app.clone());
        crate::resolve_saves(app.clone());
        Ok(MoveDone { scan, location: location(&app)?, moved: copied.bytes, warnings })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ludex-relocate-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// Dossier de données d'origine avec un cache, et des copies par défaut.
    fn setup(name: &str) -> (PathBuf, Places) {
        let base = temp_dir(name);
        let from = Places { cache: base.join("appdata"), copies: base.join("Documents").join(FOLDER).join(COPIES) };
        write(&from.cache.join("user-data.json"), "{}");
        write(&from.cache.join("images").join("index.json"), "{\"version\":1}");
        write(&from.cache.join("images").join("steam").join("620").join("cover.jpg"), "jaquette");
        write(&from.cache.join("images").join("custom").join("620").join("hero.png"), "perso");
        write(&from.cache.join("saves").join("manifeste.json"), "{\"index\":true}");
        write(&from.copies.join("Portal 2").join("2026-11-20_10-00-00").join("copie.json"), "{}");
        (base, from)
    }

    #[test]
    fn places_follow_the_settings() {
        let app = Path::new(r"C:\Users\A\AppData\Roaming\com.ludex.app");
        let default = || PathBuf::from(r"C:\Users\A\Documents\Ludex\Sauvegardes");
        assert_eq!(cache_root(app, None), app);
        assert_eq!(cache_root(app, Some(Path::new(r"D:\Ludex"))), PathBuf::from(r"D:\Ludex\Cache"));
        assert_eq!(copies_root(None, None, default), default());
        assert_eq!(copies_root(None, Some(Path::new(r"D:\Ludex")), default), PathBuf::from(r"D:\Ludex\Sauvegardes"));
        assert_eq!(copies_root(Some(Path::new(r"E:\Copies")), Some(Path::new(r"D:\Ludex")), default), PathBuf::from(r"E:\Copies"));
        assert_eq!(root_for(Path::new(r"D:\")), PathBuf::from(r"D:\Ludex"));
        assert_eq!(root_for(Path::new(r"D:\Jeux")), PathBuf::from(r"D:\Jeux\Ludex"));
        assert_eq!(root_for(Path::new(r"D:\ludex")), PathBuf::from(r"D:\ludex"));
    }

    #[test]
    fn refuses_bad_roots() {
        assert!(check_root(Path::new(r"\\nas\partage\Ludex")).is_err());
        assert!(check_root(Path::new("relatif\\Ludex")).is_err());
        assert!(check_root(Path::new(r"C:\")).is_err());
        assert!(check_root(Path::new(r"C:\Jeux\..\Ludex")).is_err());
        let base = temp_dir("roots");
        assert!(check_root(&base.join("pas-encore").join(FOLDER)).is_ok());
        // Un fichier sur le chemin : pas un dossier utilisable.
        write(&base.join("fichier"), "x");
        assert!(check_root(&base.join("fichier").join(FOLDER)).is_err());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn moves_cache_and_copies_then_removes_the_old_ones() {
        let (base, from) = setup("move");
        let root = base.join("D").join(FOLDER);
        let to = Places { cache: root.join(CACHE), copies: root.join(COPIES) };
        let parts = plan(&from, &to, true).unwrap();
        assert_eq!(parts.len(), 3);
        assert!(parts.iter().all(|p| size_of(&p.from) > 0));
        prepare_root(&root).unwrap();
        let copied = copy_all(&parts, &mut |a, b| fs::copy(a, b)).unwrap();
        assert_eq!(copied.files, 5);
        assert_eq!(fs::read_to_string(to.cache.join("images").join("custom").join("620").join("hero.png")).unwrap(), "perso");
        assert_eq!(fs::read_to_string(to.cache.join("saves").join("manifeste.json")).unwrap(), "{\"index\":true}");
        assert!(to.copies.join("Portal 2").join("2026-11-20_10-00-00").join("copie.json").is_file());
        // Aucun reste de copie en cours.
        assert!(!tmp_of(&to.cache.join("images")).exists());

        let warnings = remove_sources(&parts, &prune_list(None, &from, &parts));
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(!from.cache.join("images").exists());
        assert!(!from.cache.join("saves").exists());
        // Les petits fichiers restent, `Documents\Ludex` vide est retiré.
        assert!(from.cache.join("user-data.json").is_file());
        assert!(!from.copies.parent().unwrap().exists());

        // Retour à l'emplacement d'origine : `Cache` et `Ludex` vides retirés aussi.
        let back = plan(&to, &from, true).unwrap();
        assert_eq!(back.len(), 3);
        copy_all(&back, &mut |a, b| fs::copy(a, b)).unwrap();
        remove_sources(&back, &prune_list(Some(&root), &to, &back));
        assert!(from.cache.join("images").join("steam").join("620").join("cover.jpg").is_file());
        assert!(!root.exists());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn copies_chosen_apart_stay() {
        let (base, from) = setup("apart");
        let to = Places { cache: base.join("D").join(FOLDER).join(CACHE), copies: from.copies.clone() };
        let parts = plan(&from, &to, false).unwrap();
        assert_eq!(parts.len(), 2);
        assert!(parts.iter().all(|p| p.from != from.copies));
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn nothing_to_move_for_missing_or_same_folders() {
        let base = temp_dir("empty");
        let from = Places { cache: base.join("appdata"), copies: base.join("copies") };
        fs::create_dir_all(&from.cache).unwrap();
        let to = Places { cache: base.join("D").join(CACHE), copies: base.join("D").join(COPIES) };
        assert!(plan(&from, &to, true).unwrap().is_empty());
        assert!(plan(&from, &from, true).unwrap().is_empty());
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn refuses_nested_or_occupied_targets() {
        let (base, from) = setup("nested");
        // Cible dans la source (dossier choisi dans le cache d'images).
        let inside = from.cache.join("images").join(FOLDER);
        let to = Places { cache: inside.join(CACHE), copies: inside.join(COPIES) };
        assert!(plan(&from, &to, true).unwrap_err().contains("l'un dans l'autre"));
        // Cible qui contient déjà des fichiers.
        let root = base.join("D").join(FOLDER);
        write(&root.join(CACHE).join("images").join("autre.jpg"), "x");
        let to = Places { cache: root.join(CACHE), copies: root.join(COPIES) };
        assert!(plan(&from, &to, true).unwrap_err().contains("n'est pas vide"));
        // Cible vide : acceptée.
        fs::remove_file(root.join(CACHE).join("images").join("autre.jpg")).unwrap();
        assert_eq!(plan(&from, &to, true).unwrap().len(), 3);
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn full_disk_leaves_everything_as_before() {
        let (base, from) = setup("full");
        let root = base.join("D").join(FOLDER);
        let to = Places { cache: root.join(CACHE), copies: root.join(COPIES) };
        let parts = plan(&from, &to, true).unwrap();
        // Disque plein simulé au quatrième fichier (dans le deuxième dossier).
        let mut count = 0;
        let mut full = |a: &Path, b: &Path| {
            count += 1;
            if count == 4 { Err(io::Error::from(io::ErrorKind::StorageFull)) } else { fs::copy(a, b) }
        };
        let err = copy_all(&parts, &mut full).unwrap_err();
        assert!(err.contains("Disque plein"), "{err}");
        // Rien de copié ne reste, les sources sont intactes.
        for part in &parts {
            assert!(!part.to.exists());
            assert!(!tmp_of(&part.to).exists());
            assert!(part.from.is_dir());
        }
        assert_eq!(fs::read_to_string(from.cache.join("saves").join("manifeste.json")).unwrap(), "{\"index\":true}");
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_bad_copy_is_caught_by_the_check() {
        let (base, from) = setup("verify");
        let to = Places { cache: base.join("D").join(CACHE), copies: base.join("D").join(COPIES) };
        let parts = plan(&from, &to, true).unwrap();
        // Copie tronquée sans erreur : la vérification la refuse.
        let mut short = |_: &Path, b: &Path| fs::write(b, b"").map(|_| 0);
        assert!(copy_all(&parts, &mut short).unwrap_err().contains("ne correspond pas"));
        assert!(parts.iter().all(|p| !p.to.exists() && p.from.is_dir()));
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn checks_free_space() {
        assert!(check_space(10_000_000, 10_000_000 + MARGIN, "D:").is_ok());
        let err = check_space(2_500_000_000, 1_200_000_000, "D:").unwrap_err();
        assert_eq!(err, "Pas assez de place sur D: : 2,5\u{a0}Go à déplacer, 1,2\u{a0}Go libres.");
        assert_eq!(human(512), "0,5\u{a0}Ko");
        assert_eq!(human(6_300_000), "6,3\u{a0}Mo");
        assert_eq!(human(19_000_000), "19\u{a0}Mo");
        assert_eq!(human(2_000_000_000), "2\u{a0}Go");
    }
}
