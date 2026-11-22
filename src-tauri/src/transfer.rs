//! Export et import des données perso de Ludex, pour changer de PC.
//!
//! Exporté : ce que l'utilisateur a saisi ou que Ludex a mesuré, et qu'on ne peut relire nulle part
//! ailleurs : `user-data.json` (favoris, masqués, statuts, notes, collections, titres, options de
//! lancement, jeux ajoutés à la main, exécutables choisis, temps mesuré, réglages), `sessions.json`
//! (historique des parties) et les jaquettes perso (`images/custom/`). Jamais les caches (jaquettes
//! en ligne, fiches, durées de vie, tailles, manifeste des sauvegardes) ni les copies de sauvegardes.
//!
//! Format : une archive `.zip` sans compression (`ludex-export-AAAA-MM-JJ.zip`), lisible avec
//! l'Explorateur : `manifest.json` (format, version de Ludex, date, liste des jaquettes),
//! `user-data.json`, `sessions.json`, puis `jaquettes/<n>.<png|jpg|webp>`. Plutôt qu'un JSON unique
//! avec les images en base64 : pas de tiers de poids en plus pour les images, chaque partie lue
//! avec sa propre limite de taille, et la crate `zip` est déjà compilée sans aucune fonction pour
//! le plugin de mises à jour (méthode « stockée » seulement, aucun code de décompression).
//!
//! Import en deux temps, tout vérifié avant d'écrire : `import_data` ouvre le sélecteur natif (aucun
//! chemin ne passe par le front), lit et valide l'archive en mémoire (taille, noms attendus
//! seulement, jamais de chemin tiré de l'archive, JSON lisible, textes renettoyés comme à la
//! saisie, jaquettes vérifiées par `covers::check_bytes`) et renvoie un aperçu ; `apply_import`
//! fait une copie de sécurité de l'état actuel (`<données>/exports/avant-import-*.zip`, même
//! format, donc réimportable), puis fusionne ou remplace, écrit (tmp + renommage) et rescanne.
//! Les jeux et chemins d'un autre PC gardent leurs ids : un jeu ajouté à la main dont l'exécutable
//! manque reste enregistré et réapparaît quand le fichier existe.

use crate::collections::{self, Collection};
use crate::corrections;
use crate::covers::{self, CustomArt, Kind, Picked};
use crate::history::{self, PastSession};
use crate::library::ScanResult;
use crate::library::manual::{self, ManualGame};
use crate::store::UserData;
use crate::{AppState, blocking, lock, quick, save_and_rescan, settings, wait_scan};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::{Read, Seek, Write};
use std::path::{Component, Path, PathBuf};
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// Valeur de `format` dans `manifest.json`.
pub const FORMAT: &str = "ludex-export";
/// Version du format : un export plus récent que ce que cette version sait lire est refusé.
pub const FORMAT_VERSION: u32 = 1;
/// Taille maximale d'une archive importée.
pub const MAX_ARCHIVE: u64 = 256 * 1024 * 1024;
/// Taille maximale de `user-data.json` et de `sessions.json` dans l'archive.
const MAX_JSON: u64 = 16 * 1024 * 1024;
const MAX_MANIFEST: u64 = 1024 * 1024;
/// Nombre maximal de jaquettes perso dans une archive (deux par jeu).
const MAX_COVERS: usize = 4000;
const MAX_ID: usize = 256;
const MAX_PATH: usize = 1024;
const MAX_MANUAL: usize = 2000;
/// Au-delà, une session est jugée invalide (le suivi plafonne à 24 h, une entrée fusionnée à un jour).
const MAX_SESSION_SECS: u64 = 2 * 86_400;
/// Dossier des copies de sécurité faites avant un import, dans le dossier de données.
const BACKUP_DIR: &str = "exports";
const BACKUP_PREFIX: &str = "avant-import-";
const KEEP_BACKUPS: usize = 5;

const MANIFEST: &str = "manifest.json";
const USER_DATA: &str = "user-data.json";
const SESSIONS: &str = "sessions.json";
const COVERS_DIR: &str = "jaquettes/";

/// Contenu de `manifest.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    format: String,
    format_version: u32,
    #[serde(default)]
    ludex_version: String,
    /// Date de l'export, en secondes Unix.
    #[serde(default)]
    created: u64,
    #[serde(default)]
    covers: Vec<CoverEntry>,
}

/// Jaquette perso de l'archive : fichier, jeu et type d'image.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoverEntry {
    file: String,
    game: String,
    kind: Kind,
    /// Date du choix (millisecondes), qui versionne l'URL de l'image.
    #[serde(default)]
    at: u64,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct SessionsFile {
    sessions: Vec<PastSession>,
}

/// Jaquette perso lue et vérifiée.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoverImage {
    game: String,
    kind: Kind,
    bytes: Vec<u8>,
    ext: &'static str,
    at: u64,
}

/// Archive lue et vérifiée, en attente de confirmation.
#[derive(Debug)]
pub struct Export {
    ludex_version: String,
    created: u64,
    data: UserData,
    sessions: Vec<PastSession>,
    covers: Vec<CoverImage>,
}

/// Ce que contient un export (ou ce qui a été importé), pour l'affichage.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    favorites: usize,
    hidden: usize,
    statuses: usize,
    notes: usize,
    collections: usize,
    manual_games: usize,
    titles: usize,
    aliases: usize,
    genres: usize,
    launch_args: usize,
    wishes: usize,
    playtime_games: usize,
    sessions: usize,
    covers: usize,
}

fn summarize(data: &UserData, sessions: &[PastSession]) -> Summary {
    Summary {
        favorites: data.favorites.len(),
        hidden: data.hidden.len(),
        statuses: data.status.len(),
        notes: data.notes.len(),
        collections: data.collections.len(),
        manual_games: data.manual.len(),
        titles: data.titles.len(),
        aliases: data.aliases.len(),
        genres: data.genres.len(),
        launch_args: data.launch_args.len(),
        wishes: data.wishlist.len(),
        playtime_games: data.playtime.values().filter(|&&s| s > 0).count(),
        sessions: sessions.len(),
        covers: data.covers.values().map(|c| Kind::ALL.iter().filter(|&&k| c.get(k).is_some()).count()).sum(),
    }
}

/// Fusion (les choix de ce PC gardés, ceux de l'archive ajoutés) ou remplacement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Merge,
    Replace,
}

fn zip_err(e: zip::result::ZipError) -> String {
    tr!("Archive illisible : {e}", "Unreadable archive: {e}")
}

// ------------------------------------------------------------------------------------------------
// Écriture

/// Écrit l'archive : données, historique et jaquettes perso dont le fichier existe (les autres sont
/// retirées de l'export, qui reste ainsi cohérent). Renvoie le résumé de ce qui a été écrit.
fn write_archive<W: Write + Seek>(
    out: W,
    data: &UserData,
    sessions: &[PastSession],
    images_root: &Path,
    version: &str,
    created: u64,
) -> Result<Summary, String> {
    let mut data_out: UserData = data.clone();
    data_out.covers = BTreeMap::new();
    let mut entries: Vec<CoverEntry> = Vec::new();
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for (id, art) in &data.covers {
        for kind in Kind::ALL {
            let Some(picked) = art.get(kind) else { continue };
            let Some(path) = covers::file(images_root, id, kind, picked) else { continue };
            // Fichier disparu ou abîmé : jaquette ignorée, l'export ne la mentionne pas.
            let Ok((bytes, ext)) = covers::check_image(&path) else { continue };
            let file = format!("{COVERS_DIR}{:04}.{ext}", entries.len() + 1);
            entries.push(CoverEntry { file: file.clone(), game: id.clone(), kind, at: picked.at });
            files.push((file, bytes));
            data_out.covers.entry(id.clone()).or_default().set(kind, Some(Picked { ext: ext.to_string(), at: picked.at }));
        }
    }
    let manifest =
        Manifest { format: FORMAT.into(), format_version: FORMAT_VERSION, ludex_version: version.into(), created, covers: entries };
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let mut zip = ZipWriter::new(out);
    let parts: [(&str, String); 3] = [
        (MANIFEST, pretty(&manifest)?),
        (USER_DATA, pretty(&data_out)?),
        (SESSIONS, pretty(&SessionsFile { sessions: sessions.to_vec() })?),
    ];
    for (name, text) in parts.iter() {
        zip.start_file(*name, opts).map_err(zip_err)?;
        zip.write_all(text.as_bytes()).map_err(|e| tr!("Écriture de l'archive impossible : {e}", "Couldn't write the archive: {e}"))?;
    }
    for (name, bytes) in &files {
        zip.start_file(name.as_str(), opts).map_err(zip_err)?;
        zip.write_all(bytes).map_err(|e| tr!("Écriture de l'archive impossible : {e}", "Couldn't write the archive: {e}"))?;
    }
    zip.finish().map_err(zip_err)?;
    Ok(summarize(&data_out, sessions))
}

fn pretty<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string_pretty(value).map_err(|e| e.to_string())
}

/// Écrit l'archive dans `path` : fichier temporaire, puis renommage (jamais d'archive à moitié écrite).
fn write_file(path: &Path, data: &UserData, sessions: &[PastSession], images_root: &Path, version: &str) -> Result<Summary, String> {
    let tmp = path.with_extension("zip.tmp");
    let result = (|| {
        let file = fs::File::create(&tmp).map_err(|e| tr!("Écriture impossible : {e}", "Couldn't write the file: {e}"))?;
        let mut out = std::io::BufWriter::new(file);
        let summary = write_archive(&mut out, data, sessions, images_root, version, now_secs())?;
        let file = out.into_inner().map_err(|e| tr!("Écriture impossible : {e}", "Couldn't write the file: {e}"))?;
        file.sync_all().map_err(|e| tr!("Écriture impossible : {e}", "Couldn't write the file: {e}"))?;
        drop(file);
        fs::rename(&tmp, path).map_err(|e| tr!("Écriture impossible : {e}", "Couldn't write the file: {e}"))?;
        Ok(summary)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Nom proposé dans le sélecteur : `ludex-export-AAAA-MM-JJ.zip`, date locale.
pub fn export_name(date: &str) -> String {
    format!("ludex-export-{date}.zip")
}

// ------------------------------------------------------------------------------------------------
// Lecture

/// Nom d'une jaquette dans l'archive : `jaquettes/<1 à 6 chiffres>.<png|jpg|webp>`, rien d'autre.
fn is_cover_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix(COVERS_DIR) else { return false };
    let Some((num, ext)) = rest.split_once('.') else { return false };
    (1..=6).contains(&num.len()) && num.bytes().all(|b| b.is_ascii_digit()) && crate::images::EXTENSIONS.contains(&ext)
}

/// Noms attendus seulement : jamais de chemin absolu, de `..`, de barre oblique inverse ni de
/// dossier ; un nom inconnu fait refuser toute l'archive.
fn expected_name(name: &str) -> bool {
    matches!(name, MANIFEST | USER_DATA | SESSIONS) || is_cover_name(name)
}

fn limit_for(name: &str) -> u64 {
    match name {
        MANIFEST => MAX_MANIFEST,
        USER_DATA | SESSIONS => MAX_JSON,
        _ => covers::MAX_BYTES,
    }
}

/// Lit et vérifie une archive entière, sans rien écrire. `size` : taille du fichier.
fn read_archive<R: Read + Seek>(input: R, size: u64) -> Result<Export, String> {
    if size > MAX_ARCHIVE {
        return Err(tr!("Fichier trop lourd : {} Mo au plus.", "File too large: {} MB at most.", MAX_ARCHIVE / (1024 * 1024)));
    }
    let not_export = tr_str!("Ce fichier n'est pas un export de Ludex.", "This file isn't a Ludex export.");
    let mut archive = ZipArchive::new(input).map_err(|_| not_export.to_string())?;
    if archive.len() > MAX_COVERS + 3 {
        return Err(tr!("Archive refusée : trop de fichiers.", "Archive refused: too many files."));
    }
    let mut parts: HashMap<String, Vec<u8>> = HashMap::new();
    let mut total: u64 = 0;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(zip_err)?;
        let name = entry.name().map_err(zip_err)?.into_owned();
        // `enclosed_name` en plus de la liste : un nom attendu est toujours un chemin relatif sûr.
        if !expected_name(&name) || entry.enclosed_name().is_none() || entry.is_dir() || entry.is_symlink() {
            return Err(tr!("Archive refusée : fichier inattendu « {} ».", "Archive refused: unexpected file \"{}\".", name.chars().take(80).collect::<String>()));
        }
        if entry.encrypted() || entry.compression() != CompressionMethod::Stored {
            return Err(tr!("Archive refusée : « {name} » est chiffré ou compressé.", "Archive refused: \"{name}\" is encrypted or compressed."));
        }
        if parts.contains_key(&name) {
            return Err(tr!("Archive refusée : « {name} » en double.", "Archive refused: \"{name}\" appears twice."));
        }
        let limit = limit_for(&name);
        if entry.size() > limit {
            return Err(tr!("Archive refusée : « {name} » est trop lourd.", "Archive refused: \"{name}\" is too large."));
        }
        let mut bytes = Vec::new();
        (&mut entry).take(limit + 1).read_to_end(&mut bytes).map_err(|e| tr!("Archive illisible : {e}", "Unreadable archive: {e}"))?;
        if bytes.len() as u64 > limit {
            return Err(tr!("Archive refusée : « {name} » est trop lourd.", "Archive refused: \"{name}\" is too large."));
        }
        total += bytes.len() as u64;
        if total > MAX_ARCHIVE {
            return Err(tr!("Archive refusée : contenu trop lourd.", "Archive refused: content too large."));
        }
        parts.insert(name, bytes);
    }

    let manifest: Manifest =
        serde_json::from_slice(parts.get(MANIFEST).ok_or(not_export)?).map_err(|_| tr!("manifest.json illisible.", "Unreadable manifest.json."))?;
    if manifest.format != FORMAT {
        return Err(not_export.into());
    }
    if manifest.format_version == 0 {
        return Err(tr!("manifest.json illisible.", "Unreadable manifest.json."));
    }
    if manifest.format_version > FORMAT_VERSION {
        let version = if manifest.ludex_version.is_empty() { tr_str!("inconnue", "unknown") } else { &manifest.ludex_version };
        return Err(tr!(
            "Cet export vient d'une version plus récente de Ludex ({version}) : mets Ludex à jour pour l'importer.",
            "This export comes from a newer version of Ludex ({version}): update Ludex to import it."
        ));
    }
    let mut data: UserData =
        serde_json::from_slice(parts.get(USER_DATA).ok_or_else(|| tr!("user-data.json manque dans l'archive.", "user-data.json is missing from the archive."))?)
            .map_err(|e| tr!("user-data.json illisible : {e}", "Unreadable user-data.json: {e}"))?;
    let sessions: SessionsFile =
        serde_json::from_slice(parts.get(SESSIONS).ok_or_else(|| tr!("sessions.json manque dans l'archive.", "sessions.json is missing from the archive."))?)
            .map_err(|e| tr!("sessions.json illisible : {e}", "Unreadable sessions.json: {e}"))?;

    // Jaquettes : chacune référencée une fois par le manifeste, vérifiée comme au choix d'une image.
    if manifest.covers.len() > MAX_COVERS {
        return Err(tr!("Archive refusée : trop de jaquettes.", "Archive refused: too many covers."));
    }
    let mut used: BTreeSet<&str> = BTreeSet::new();
    let mut seen: BTreeSet<(String, Kind)> = BTreeSet::new();
    let mut images: Vec<CoverImage> = Vec::new();
    for c in &manifest.covers {
        if !is_cover_name(&c.file) || !valid_id(&c.game) {
            return Err(tr!("manifest.json illisible : jaquette invalide.", "Unreadable manifest.json: invalid cover."));
        }
        let bytes = parts.get(&c.file).ok_or_else(|| tr!("Archive incomplète : « {} » manque.", "Incomplete archive: \"{}\" is missing.", c.file))?;
        if !used.insert(c.file.as_str()) || !seen.insert((c.game.clone(), c.kind)) {
            return Err(tr!("manifest.json illisible : jaquette en double.", "Unreadable manifest.json: duplicate cover."));
        }
        let ext = covers::check_bytes(&c.file, bytes).map_err(|e| tr!("Jaquette refusée : {e}", "Cover refused: {e}"))?;
        images.push(CoverImage { game: c.game.clone(), kind: c.kind, bytes: bytes.clone(), ext, at: c.at });
    }
    if let Some(stray) = parts.keys().find(|n| is_cover_name(n) && !used.contains(n.as_str())) {
        return Err(tr!("Archive refusée : fichier inattendu « {stray} ».", "Archive refused: unexpected file \"{stray}\"."));
    }

    // Jaquettes perso : seulement celles vérifiées ci-dessus, quoi que dise `user-data.json`.
    data.covers = BTreeMap::new();
    for img in &images {
        data.covers.entry(img.game.clone()).or_default().set(img.kind, Some(Picked { ext: img.ext.to_string(), at: img.at }));
    }
    Ok(Export {
        ludex_version: manifest.ludex_version.chars().take(32).collect(),
        created: manifest.created,
        data: sanitize(data),
        sessions: sanitize_sessions(sessions.sessions),
        covers: images,
    })
}

// ------------------------------------------------------------------------------------------------
// Nettoyage : une archive peut avoir été modifiée à la main, tout est revalidé comme à la saisie.

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_ID && !id.chars().any(|c| c.is_control() || manual::is_invisible(c))
}

/// Exécutable venu d'un autre PC : chemin absolu vers un `.exe`, sans `..`. Il peut manquer ici.
fn valid_exe(path: &Path) -> bool {
    path.is_absolute()
        && path.as_os_str().len() <= MAX_PATH
        && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"))
        && !path.components().any(|c| matches!(c, Component::ParentDir))
}

fn valid_collection_id(id: &str) -> bool {
    id.len() <= 32 && id.strip_prefix("c-").is_some_and(|r| !r.is_empty() && r.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()))
}

fn sanitize(mut data: UserData) -> UserData {
    data.favorites.retain(|id| valid_id(id));
    data.hidden.retain(|id| valid_id(id));
    data.shown.retain(|id| valid_id(id));
    data.launched.retain(|id, _| valid_id(id));
    data.playtime.retain(|id, _| valid_id(id));
    data.status.retain(|id, _| valid_id(id));
    data.exes.retain(|id, exe| valid_id(id) && valid_exe(exe));
    data.titles = std::mem::take(&mut data.titles)
        .into_iter()
        .filter(|(id, _)| valid_id(id))
        .filter_map(|(id, t)| Some((id, manual::clean_title(&t)?)))
        .collect();
    data.aliases = std::mem::take(&mut data.aliases)
        .into_iter()
        .filter(|(id, _)| valid_id(id))
        .map(|(id, list)| (id, corrections::clean_alias_list(list.iter().map(String::as_str))))
        .filter(|(_, list)| !list.is_empty())
        .collect();
    data.genres = std::mem::take(&mut data.genres)
        .into_iter()
        .filter(|(id, _)| valid_id(id))
        .map(|(id, list)| (id, corrections::known_genres(&list)))
        .filter(|(_, list)| !list.is_empty())
        .collect();
    data.notes = std::mem::take(&mut data.notes)
        .into_iter()
        .filter(|(id, _)| valid_id(id))
        .filter_map(|(id, n)| Some((id, collections::clean_note(&n)?)))
        .collect();
    data.launch_args = std::mem::take(&mut data.launch_args)
        .into_iter()
        .filter(|(id, _)| valid_id(id))
        .filter_map(|(id, a)| Some((id, crate::launch_args::clean(&a).ok()??)))
        .collect();
    data.wishlist = crate::wishlist::clean(std::mem::take(&mut data.wishlist));
    let mut ids: BTreeSet<String> = BTreeSet::new();
    data.manual = std::mem::take(&mut data.manual)
        .into_iter()
        .filter(|m| manual::is_manual(&m.id) && valid_id(&m.id) && valid_exe(&m.exe) && ids.insert(m.id.clone()))
        .take(MAX_MANUAL)
        .map(|m| ManualGame {
            title: manual::clean_title(&m.title).unwrap_or_else(|| tr!("Jeu", "Game")),
            // Aucun moyen de saisir des arguments aujourd'hui : jamais repris d'un fichier.
            args: Vec::new(),
            ..m
        })
        .collect();
    let incoming = std::mem::take(&mut data.collections);
    for c in incoming {
        add_collection(&mut data.collections, c, 0);
    }
    if !data.settings.quick_shortcut.is_empty() && quick::parse(&data.settings.quick_shortcut).is_err() {
        data.settings.quick_shortcut = crate::quick::DEFAULT_SHORTCUT.to_string();
    }
    data
}

/// Ajoute une collection nettoyée : même nom (casse ignorée) qu'une collection existante = jeux
/// ajoutés à celle-ci ; id invalide ou déjà pris = nouvel id. Rien au-delà de `MAX_COLLECTIONS`.
fn add_collection(list: &mut Vec<Collection>, c: Collection, seed: u64) {
    let Some(name) = collections::clean_name(&c.name) else { return };
    let mut games: Vec<String> = Vec::new();
    for g in c.games.into_iter().filter(|g| valid_id(g)) {
        if !games.contains(&g) {
            games.push(g);
        }
    }
    let lower = name.to_lowercase();
    if let Some(known) = list.iter_mut().find(|k| k.name.to_lowercase() == lower) {
        for g in games {
            if !known.games.contains(&g) {
                known.games.push(g);
            }
        }
        return;
    }
    if list.len() >= collections::MAX_COLLECTIONS {
        return;
    }
    let id = if valid_collection_id(&c.id) && !list.iter().any(|k| k.id == c.id) { c.id } else { collections::new_id(list, seed) };
    list.push(Collection { id, name, games });
}

fn sanitize_sessions(sessions: Vec<PastSession>) -> Vec<PastSession> {
    let mut seen: BTreeSet<(String, u64)> = BTreeSet::new();
    let mut list: Vec<PastSession> = sessions
        .into_iter()
        .filter(|s| valid_id(&s.game_id) && s.start > 0 && s.secs <= MAX_SESSION_SECS)
        .filter(|s| seen.insert((s.game_id.clone(), s.start)))
        .collect();
    list.sort_by_key(|s| s.start);
    history::compact_to_max(&mut list);
    list
}

// ------------------------------------------------------------------------------------------------
// Fusion et remplacement (pures)

/// Données après import. Remplacement : celles de l'archive, sauf les réglages propres à ce PC
/// (dossier des copies, lancement avec Windows, écrans déjà vus). Fusion : celles de ce PC, plus
/// ce que l'archive a en plus ; en cas de conflit, le choix de ce PC reste (temps mesuré et date
/// du dernier lancement : le plus grand des deux, jamais la somme).
pub fn combine(current: &UserData, incoming: UserData, mode: Mode, seed: u64) -> UserData {
    match mode {
        Mode::Replace => {
            let mut out = incoming;
            let here = &current.settings;
            out.settings.saves_dir = here.saves_dir.clone();
            out.settings.data_dir = here.data_dir.clone();
            out.settings.launch_at_startup = here.launch_at_startup;
            out.settings.welcome_shown = here.welcome_shown.or(Some(true));
            out.settings.tray_hint_shown = here.tray_hint_shown;
            out
        }
        Mode::Merge => {
            let mut out = current.clone();
            out.favorites.extend(incoming.favorites);
            out.hidden.extend(incoming.hidden);
            out.shown.extend(incoming.shown);
            for (id, t) in incoming.launched {
                let v = out.launched.entry(id).or_insert(0);
                *v = (*v).max(t);
            }
            for (id, s) in incoming.playtime {
                let v = out.playtime.entry(id).or_insert(0);
                *v = (*v).max(s);
            }
            for m in incoming.manual {
                if !out.manual.iter().any(|k| k.id == m.id) && out.manual.len() < MAX_MANUAL {
                    out.manual.push(m);
                }
            }
            fill(&mut out.exes, incoming.exes);
            fill(&mut out.titles, incoming.titles);
            fill(&mut out.aliases, incoming.aliases);
            fill(&mut out.genres, incoming.genres);
            fill(&mut out.status, incoming.status);
            fill(&mut out.notes, incoming.notes);
            fill(&mut out.launch_args, incoming.launch_args);
            for (id, art) in incoming.covers {
                let mine: &mut CustomArt = out.covers.entry(id).or_default();
                for kind in Kind::ALL {
                    if mine.get(kind).is_none() {
                        mine.set(kind, art.get(kind).cloned());
                    }
                }
            }
            for c in incoming.collections {
                add_collection(&mut out.collections, c, seed);
            }
            crate::wishlist::merge(&mut out.wishlist, incoming.wishlist);
            out
        }
    }
}

fn fill<V>(mine: &mut BTreeMap<String, V>, theirs: BTreeMap<String, V>) {
    for (k, v) in theirs {
        mine.entry(k).or_insert(v);
    }
}

/// Historique après import. Fusion : sessions des deux, sans doublon (même jeu, même début).
pub fn combine_sessions(current: &[PastSession], incoming: Vec<PastSession>, mode: Mode) -> Vec<PastSession> {
    let mut list = match mode {
        Mode::Replace => incoming,
        Mode::Merge => current.iter().cloned().chain(incoming).collect(),
    };
    let mut seen: BTreeSet<(String, u64)> = BTreeSet::new();
    list.retain(|s| seen.insert((s.game_id.clone(), s.start)));
    list.sort_by_key(|s| s.start);
    history::compact_to_max(&mut list);
    list
}

/// Jaquettes de l'archive à écrire : celles que `result` a gardées (même date de choix).
fn covers_to_write<'a>(images: &'a [CoverImage], result: &UserData) -> Vec<&'a CoverImage> {
    images
        .iter()
        .filter(|img| result.covers.get(&img.game).and_then(|c| c.get(img.kind)).is_some_and(|p| p.at == img.at && p.ext == img.ext))
        .collect()
}

// ------------------------------------------------------------------------------------------------
// Copie de sécurité

/// Garde les `KEEP_BACKUPS` copies les plus récentes (noms horodatés, triés).
fn rotate_backups(dir: &Path) {
    let mut names: Vec<PathBuf> = fs::read_dir(dir)
        .map(|it| {
            it.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(BACKUP_PREFIX) && n.ends_with(".zip")))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    let excess = names.len().saturating_sub(KEEP_BACKUPS);
    for old in &names[..excess] {
        let _ = fs::remove_file(old);
    }
}

/// Copie de sécurité de l'état actuel avant un import, dans `<données>/exports/`. Renvoie son nom.
fn backup(data_dir: &Path, stamp: &str, data: &UserData, sessions: &[PastSession], images_root: &Path, version: &str) -> Result<String, String> {
    let dir = data_dir.join(BACKUP_DIR);
    fs::create_dir_all(&dir).map_err(|e| tr!("Copie de sécurité impossible : {e}", "Couldn't make a safety copy: {e}"))?;
    let mut path = dir.join(format!("{BACKUP_PREFIX}{stamp}.zip"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{BACKUP_PREFIX}{stamp}-{n}.zip"));
        n += 1;
    }
    write_file(&path, data, sessions, images_root, version).map_err(|e| tr!("Copie de sécurité impossible : {e}", "Couldn't make a safety copy: {e}"))?;
    rotate_backups(&dir);
    Ok(path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
}

// ------------------------------------------------------------------------------------------------
// Commandes

/// Export terminé : nom du fichier écrit (sans son dossier) et contenu.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportInfo {
    file: String,
    summary: Summary,
}

/// Archive choisie et vérifiée, en attente de confirmation.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    file: String,
    ludex_version: String,
    /// Date de l'export, en secondes Unix (0 : inconnue).
    created: u64,
    summary: Summary,
}

/// Import terminé : bibliothèque relue, contenu importé et nom de la copie de sécurité.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDone {
    scan: ScanResult,
    summary: Summary,
    backup: String,
}

fn local_stamp(fmt: &str) -> String {
    chrono::Local::now().format(fmt).to_string()
}

/// Exporte les données perso dans une archive choisie avec le sélecteur natif. `None` : annulé.
#[tauri::command]
pub async fn export_data(app: AppHandle, window: WebviewWindow) -> Result<Option<ExportInfo>, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let name = export_name(&local_stamp("%Y-%m-%d"));
        let picked = window
            .dialog()
            .file()
            .set_title(tr_str!("Exporter mes données Ludex", "Export my Ludex data"))
            .add_filter(tr_str!("Export Ludex", "Ludex export"), &["zip"])
            .set_file_name(&name)
            .set_parent(&window)
            .blocking_save_file()
            .and_then(|f| f.into_path().ok());
        let Some(mut path) = picked else { return Ok(None) };
        if !path.is_absolute() {
            return Err(tr!("Chemin invalide.", "Invalid path."));
        }
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
            let mut text = path.into_os_string();
            text.push(".zip");
            path = PathBuf::from(text);
        }
        let data = lock(&state.data)?.clone();
        let sessions = state.tracker.sessions();
        let root = lock(&state.images)?.root().to_path_buf();
        let summary = write_file(&path, &data, &sessions, &root, &app.package_info().version.to_string())?;
        let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        Ok(Some(ExportInfo { file, summary }))
    })
    .await
}

/// Choisit une archive avec le sélecteur natif, la lit et la vérifie entièrement, puis la garde en
/// attente de confirmation (`apply_import`). Rien n'est écrit. `None` : annulé.
#[tauri::command]
pub async fn import_data(app: AppHandle, window: WebviewWindow) -> Result<Option<ImportPreview>, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        *lock(&state.pending_import)? = None;
        let picked = window
            .dialog()
            .file()
            .set_title(tr_str!("Importer des données Ludex", "Import Ludex data"))
            .add_filter(tr_str!("Export Ludex", "Ludex export"), &["zip"])
            .set_parent(&window)
            .blocking_pick_file()
            .and_then(|f| f.into_path().ok());
        let Some(path) = picked else { return Ok(None) };
        if !path.is_absolute() {
            return Err(tr!("Chemin invalide.", "Invalid path."));
        }
        let meta = fs::metadata(&path).map_err(|_| tr!("Fichier introuvable.", "File not found."))?;
        if !meta.is_file() {
            return Err(tr!("Fichier introuvable.", "File not found."));
        }
        let file = fs::File::open(&path).map_err(|e| tr!("Lecture impossible : {e}", "Couldn't read the file: {e}"))?;
        let export = read_archive(std::io::BufReader::new(file), meta.len())?;
        let preview = ImportPreview {
            file: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            ludex_version: export.ludex_version.clone(),
            created: export.created,
            summary: summarize(&export.data, &export.sessions),
        };
        *lock(&state.pending_import)? = Some(export);
        Ok(Some(preview))
    })
    .await
}

/// Oublie l'archive en attente.
#[tauri::command]
pub fn cancel_import(app: AppHandle) -> Result<(), String> {
    *lock(&app.state::<AppState>().pending_import)? = None;
    Ok(())
}

/// Applique l'archive en attente : copie de sécurité, fusion ou remplacement, écriture, rescan.
#[tauri::command]
pub async fn apply_import(app: AppHandle, mode: Mode) -> Result<ImportDone, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        if !state.tracker.active().is_empty() {
            return Err(tr!("Une partie est en cours : importe tes données une fois le jeu fermé.", "A game is running: import your data once it's closed."));
        }
        let export = lock(&state.pending_import)?.take().ok_or_else(|| tr!("Choisis d'abord un fichier à importer.", "Pick a file to import first."))?;
        let scanning = wait_scan(&state);
        let version = app.package_info().version.to_string();
        let root = lock(&state.images)?.root().to_path_buf();
        let current = lock(&state.data)?.clone();
        let current_sessions = state.tracker.sessions();
        let backup_name = backup(&state.data_dir, &local_stamp("%Y-%m-%d_%H-%M-%S"), &current, &current_sessions, &root, &version)?;

        let summary = summarize(&export.data, &export.sessions);
        let mut result = combine(&current, export.data, mode, covers::stamp());
        let sessions = combine_sessions(&current_sessions, export.sessions, mode);

        // Jaquettes : copiées d'abord (tmp + renommage), celles que le remplacement abandonne retirées.
        for img in covers_to_write(&export.covers, &result) {
            covers::save(&root, &img.game, img.kind, &img.bytes, img.ext)?;
        }
        if mode == Mode::Replace {
            for (id, art) in &current.covers {
                for kind in Kind::ALL {
                    if art.get(kind).is_some() && result.covers.get(id).and_then(|c| c.get(kind)).is_none() {
                        covers::remove(&root, id, kind);
                    }
                }
            }
        }
        state.tracker.replace_history(sessions, |list| history::write_history(&state.data_dir, list))?;

        // Raccourci de la recherche rapide : celui de l'archive s'il est libre ici, sinon l'actuel.
        let previous = current.settings.quick_shortcut.clone();
        if result.settings.quick_shortcut != previous && quick::replace(&app, &result.settings.quick_shortcut, &previous).is_err() {
            result.settings.quick_shortcut = previous;
        }
        let theme = result.settings.theme;
        let ((), scan) = save_and_rescan(&app, &state, scanning, |data| *data = result)?;
        settings::apply_theme(&app, theme);
        settings::apply_language(&app);
        settings::publish(&app)?;
        Ok(ImportDone { scan, summary, backup: backup_name })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collections::Status;
    use std::io::Cursor;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ludex-test-transfer-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        b.extend_from_slice(&w.to_be_bytes());
        b.extend_from_slice(&h.to_be_bytes());
        b.extend_from_slice(&[8, 6, 0, 0, 0]);
        b
    }

    fn session(game: &str, start: u64, secs: u64) -> PastSession {
        PastSession { game_id: game.into(), start, secs, launcher: false, merged: false }
    }

    /// Données d'un PC : un peu de tout, plus une jaquette perso enregistrée dans `root`.
    fn sample(root: &Path) -> UserData {
        let mut d = UserData::default();
        d.set_favorite("steam:730", true);
        d.hidden.insert("steam:440".into());
        d.set_status("steam:730", Some(Status::Playing));
        d.set_note("steam:730", Some("Rang Or".into()));
        d.titles.insert("other:e-jeux-ck".into(), "Crusader Kings III".into());
        d.set_aliases("steam:1938090", vec!["cod".into(), "Call of Duty".into()]);
        d.set_genres("steam:431960", vec!["Occasionnel".into()]);
        d.launch_args.insert("steam:730".into(), "-novid".into());
        d.wishlist.push(crate::wishlist::Wish {
            appid: 1426210,
            title: "It Takes Two".into(),
            added: 1_700_000_000,
            price: Some(crate::wishlist::Price { currency: "EUR".into(), initial: 3999, now: 799, discount: 80 }),
        });
        d.add_playtime("epic:Sugar", 4200);
        d.launched.insert("epic:Sugar".into(), 100);
        d.add_manual(PathBuf::from(r"D:\Emu\Dolphin\Dolphin.exe"), "Dolphin".into());
        let rpg = d.create_collection("RPG", 1).unwrap();
        d.set_in_collection(&rpg, "steam:730", true).unwrap();
        d.settings.theme = crate::store::Theme::Oled;
        d.settings.accent = crate::store::Accent::Violet;
        d.settings.launch_screen = false;
        d.settings.notifications.set(crate::notify::Kind::News, false);
        d.settings.saves_dir = Some(PathBuf::from(r"D:\Copies"));
        let picked = Picked { ext: "png".into(), at: 42 };
        covers::save(root, "riot:valorant", Kind::Cover, &png(600, 900), "png").unwrap();
        d.set_cover("riot:valorant", Kind::Cover, Some(picked));
        // Choix dont le fichier a disparu : absent de l'export.
        d.set_cover("riot:lol", Kind::Hero, Some(Picked { ext: "jpg".into(), at: 7 }));
        d
    }

    fn export_bytes(data: &UserData, sessions: &[PastSession], root: &Path) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        write_archive(&mut out, data, sessions, root, "0.2.0", 1_700_000_000).unwrap();
        out.into_inner()
    }

    fn read(bytes: Vec<u8>) -> Result<Export, String> {
        let len = bytes.len() as u64;
        read_archive(Cursor::new(bytes), len)
    }

    /// Archive écrite à la main, pour les cas piégés.
    fn raw_zip(files: &[(&str, &[u8])], method: CompressionMethod) -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        let opts = SimpleFileOptions::default().compression_method(method);
        for (name, bytes) in files {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    fn manifest(covers: &str) -> Vec<u8> {
        format!(r#"{{"format":"ludex-export","formatVersion":1,"ludexVersion":"0.2.0","created":1,"covers":[{covers}]}}"#).into_bytes()
    }

    #[test]
    fn refusals_are_written_in_english() {
        crate::i18n::with(crate::i18n::Lang::En, || {
            assert_eq!(read(b"bonjour".to_vec()).unwrap_err(), "This file isn't a Ludex export.");
            assert_eq!(read_archive(Cursor::new(Vec::new()), MAX_ARCHIVE + 1).unwrap_err(), "File too large: 256 MB at most.");
            let m = manifest("");
            let base: [(&str, &[u8]); 4] =
                [("manifest.json", &m), ("user-data.json", b"{}"), ("sessions.json", br#"{"sessions":[]}"#), ("autre.txt", b"x")];
            let err = read(raw_zip(&base, CompressionMethod::Stored)).unwrap_err();
            assert_eq!(err, "Archive refused: unexpected file \"autre.txt\".");
            let future = br#"{"format":"ludex-export","formatVersion":9,"ludexVersion":"9.0.0"}"#;
            let err = read(raw_zip(&[("manifest.json", future), ("user-data.json", b"{}"), ("sessions.json", b"{}")], CompressionMethod::Stored)).unwrap_err();
            assert_eq!(err, "This export comes from a newer version of Ludex (9.0.0): update Ludex to import it.");
            // Le nom proposé reste le même dans les deux langues.
            assert_eq!(export_name("2026-11-21"), "ludex-export-2026-11-21.zip");
        });
    }

    #[test]
    fn export_then_import_round_trips() {
        let root = temp_dir("round");
        let data = sample(&root);
        let sessions = vec![session("epic:Sugar", 1000, 600), session("steam:730", 2000, 3600)];
        let bytes = export_bytes(&data, &sessions, &root);
        let back = read(bytes).unwrap();

        assert_eq!(back.ludex_version, "0.2.0");
        assert_eq!(back.created, 1_700_000_000);
        assert_eq!(back.sessions, sessions);
        assert_eq!(back.data.favorites, data.favorites);
        assert_eq!(back.data.status, data.status);
        assert_eq!(back.data.notes, data.notes);
        assert_eq!(back.data.titles, data.titles);
        assert_eq!(back.data.aliases, data.aliases);
        assert_eq!(back.data.genres, data.genres);
        assert_eq!(back.data.launch_args, data.launch_args);
        assert_eq!(back.data.wishlist, data.wishlist);
        assert_eq!(back.data.playtime, data.playtime);
        assert_eq!(back.data.manual, data.manual);
        assert_eq!(back.data.collections, data.collections);
        assert_eq!(back.data.settings.theme, crate::store::Theme::Oled);
        assert_eq!(back.data.settings.accent, crate::store::Accent::Violet);
        // Seule la jaquette dont le fichier existe a voyagé, avec sa date de choix.
        assert_eq!(back.covers.len(), 1);
        assert_eq!(back.covers[0].bytes, png(600, 900));
        assert_eq!(back.data.covers.len(), 1);
        assert_eq!(back.data.covers["riot:valorant"].get(Kind::Cover), Some(&Picked { ext: "png".into(), at: 42 }));
        let s = summarize(&back.data, &back.sessions);
        assert_eq!((s.favorites, s.collections, s.manual_games, s.sessions, s.covers), (1, 1, 1, 2, 1));
        assert_eq!((s.aliases, s.genres, s.wishes), (1, 1, 1));

        // Remplacement sur un autre PC : jaquette écrite là-bas, réglages propres au PC gardés.
        let other = temp_dir("round-other");
        let mut here = UserData::default();
        here.settings.saves_dir = Some(PathBuf::from(r"E:\Ailleurs"));
        here.settings.launch_at_startup = true;
        here.settings.data_dir = Some(PathBuf::from(r"F:\Ludex"));
        let mut incoming = back.data;
        incoming.settings.data_dir = Some(PathBuf::from(r"D:\Ludex"));
        let result = combine(&here, incoming, Mode::Replace, 0);
        for img in covers_to_write(&back.covers, &result) {
            covers::save(&other, &img.game, img.kind, &img.bytes, img.ext).unwrap();
        }
        let picked = result.covers["riot:valorant"].get(Kind::Cover).unwrap();
        assert!(covers::file(&other, "riot:valorant", Kind::Cover, picked).unwrap().is_file());
        assert_eq!(result.settings.saves_dir, Some(PathBuf::from(r"E:\Ailleurs")));
        assert_eq!(result.settings.data_dir, Some(PathBuf::from(r"F:\Ludex")));
        assert!(result.settings.launch_at_startup);
        assert_eq!(result.settings.theme, crate::store::Theme::Oled);
        // Couleur d'accent : préférence de l'utilisateur comme le thème, elle suit l'archive.
        assert_eq!(result.settings.accent, crate::store::Accent::Violet);
        // Écran de lancement coupé : préférence de l'utilisateur, elle suit l'archive.
        assert!(!result.settings.launch_screen);
        // Notifications : préférence de l'utilisateur, pas de ce PC (elles suivent l'archive).
        assert!(!result.settings.notifications.news && result.settings.notifications.download);
        assert!(result.favorites.contains("steam:730"));

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(other);
    }

    #[test]
    fn export_file_is_written_atomically() {
        let root = temp_dir("file");
        let path = root.join("ludex-export-2026-11-19.zip");
        let summary = write_file(&path, &sample(&root), &[], &root, "0.2.0").unwrap();
        assert_eq!(summary.covers, 1);
        assert!(path.is_file() && !path.with_extension("zip.tmp").exists());
        let len = fs::metadata(&path).unwrap().len();
        assert!(read_archive(fs::File::open(&path).unwrap(), len).is_ok());
        assert_eq!(export_name("2026-11-19"), "ludex-export-2026-11-19.zip");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn trapped_archives_are_refused() {
        let data = br#"{}"#.as_slice();
        let sessions = br#"{"sessions":[]}"#.as_slice();
        let m = manifest("");
        let ok = raw_zip(&[("manifest.json", &m), ("user-data.json", data), ("sessions.json", sessions)], CompressionMethod::Stored);
        assert!(read(ok).is_ok());

        let refused = |files: &[(&str, &[u8])], want: &str| {
            let err = read(raw_zip(files, CompressionMethod::Stored)).unwrap_err();
            assert!(err.contains(want), "{err}");
        };
        let base: [(&str, &[u8]); 3] = [("manifest.json", &m), ("user-data.json", data), ("sessions.json", sessions)];
        for evil in ["../user-data.json", "/etc/passwd", "C:/Windows/evil.exe", "jaquettes/../../evil.png", "jaquettes\\1.png", "jaquettes/1.exe", "autre.txt"] {
            let mut files = base.to_vec();
            files.push((evil, b"x"));
            refused(&files, "inattendu");
        }
        // Image dans l'archive mais absente du manifeste.
        let img = png(32, 32);
        let mut stray = base.to_vec();
        stray.push(("jaquettes/0001.png", &img));
        refused(&stray, "inattendu");
        // Image du manifeste absente de l'archive.
        let m2 = manifest(r#"{"file":"jaquettes/0001.png","game":"steam:730","kind":"cover","at":1}"#);
        refused(&[("manifest.json", &m2), ("user-data.json", data), ("sessions.json", sessions)], "manque");
        // Faux PNG.
        refused(&[("manifest.json", &m2), ("user-data.json", data), ("sessions.json", sessions), ("jaquettes/0001.png", b"pas une image")], "Jaquette refusée");
        // Image trop petite.
        let tiny = png(4, 4);
        refused(&[("manifest.json", &m2), ("user-data.json", data), ("sessions.json", sessions), ("jaquettes/0001.png", &tiny)], "trop petite");
        // Même jeu et même type deux fois.
        let m3 = manifest(
            r#"{"file":"jaquettes/0001.png","game":"steam:730","kind":"cover"},{"file":"jaquettes/0002.png","game":"steam:730","kind":"cover"}"#,
        );
        refused(
            &[("manifest.json", &m3), ("user-data.json", data), ("sessions.json", sessions), ("jaquettes/0001.png", &img), ("jaquettes/0002.png", &img)],
            "double",
        );
        // Nom de jaquette du manifeste qui sort du dossier.
        let m4 = manifest(r#"{"file":"../cover.png","game":"steam:730","kind":"cover"}"#);
        refused(&[("manifest.json", &m4), ("user-data.json", data), ("sessions.json", sessions)], "invalide");
        // Autre format, version future, JSON illisible, partie manquante.
        refused(&[("manifest.json", br#"{"format":"autre","formatVersion":1}"#), ("user-data.json", data), ("sessions.json", sessions)], "pas un export");
        refused(
            &[("manifest.json", br#"{"format":"ludex-export","formatVersion":9,"ludexVersion":"9.0.0"}"#), ("user-data.json", data), ("sessions.json", sessions)],
            "plus récente",
        );
        refused(&[("manifest.json", &m), ("user-data.json", b"{pas du json"), ("sessions.json", sessions)], "illisible");
        refused(&[("manifest.json", &m), ("sessions.json", sessions)], "manque");
        // Partie déclarée compressée (aucun décodeur ici) : refusée sans être lue. La crate n'écrit
        // que des parties stockées : méthode 8 (deflate) posée à la main dans les deux en-têtes.
        let mut deflated = raw_zip(&[("manifest.json", &m), ("user-data.json", data), ("sessions.json", sessions)], CompressionMethod::Stored);
        for i in 0..deflated.len().saturating_sub(4) {
            match &deflated[i..i + 4] {
                b"PK" => deflated[i + 8] = 8,
                b"PK" => deflated[i + 10] = 8,
                _ => {}
            }
        }
        assert!(read(deflated).is_err());
        // Pas une archive, ou trop lourde.
        assert!(read(b"bonjour".to_vec()).unwrap_err().contains("pas un export"));
        assert!(read_archive(Cursor::new(Vec::new()), MAX_ARCHIVE + 1).unwrap_err().contains("trop lourd"));
    }

    #[test]
    fn hand_edited_data_is_cleaned() {
        let data = br#"{
            "favorites": ["steam:730", "", "steam:\u0007bip"],
            "titles": {"steam:730": "  Counter \u200B Strike  ", "steam:440": "   "},
            "notes": {"steam:730": "ligne\nsuivante"},
            "aliases": {"steam:730": [" cod ", "COD", "x\u0000y"], "": ["vide"], "steam:440": [" "]},
            "genres": {"steam:730": ["rpg", "Fantaisie", "RPG"], "steam:\u0007": ["RPG"], "steam:440": ["Inconnu"]},
            "launchArgs": {"steam:730": "-novid & calc", "steam:440": "-windowed"},
            "manual": [
                {"id": "other:manual-1", "exe": "D:\\Jeux\\a.exe", "title": "A", "args": ["--danger"]},
                {"id": "other:manual-1", "exe": "D:\\Jeux\\b.exe", "title": "B"},
                {"id": "steam:730", "exe": "D:\\Jeux\\c.exe", "title": "C"},
                {"id": "other:manual-2", "exe": "D:\\Jeux\\..\\x.exe", "title": "D"},
                {"id": "other:manual-3", "exe": "relatif.exe", "title": "E"},
                {"id": "other:manual-4", "exe": "D:\\Jeux\\script.bat", "title": "F"}
            ],
            "exes": {"other:x": "D:\\Jeux\\x\\x.exe", "other:y": "C:\\Windows\\System32\\cmd.bat"},
            "collections": [
                {"id": "c-1", "name": "RPG", "games": ["steam:730", "steam:730", ""]},
                {"id": "c-1", "name": "rpg", "games": ["steam:440"]},
                {"id": "../x", "name": "Coop", "games": []},
                {"id": "c-2", "name": "   ", "games": []}
            ],
            "settings": {"quickShortcut": "Space"}
        }"#;
        let m = manifest("");
        let zip = raw_zip(&[("manifest.json", &m), ("user-data.json", data), ("sessions.json", br#"{"sessions":[{"gameId":"steam:730","start":5,"secs":60},{"gameId":"steam:730","start":5,"secs":60},{"gameId":"steam:440","start":6,"secs":999999}]}"#)], CompressionMethod::Stored);
        let e = read(zip).unwrap();
        let d = e.data;
        assert_eq!(d.favorites.iter().collect::<Vec<_>>(), ["steam:730"]);
        assert_eq!(d.titles.get("steam:730").map(String::as_str), Some("Counter Strike"));
        assert!(!d.titles.contains_key("steam:440"));
        assert_eq!(d.notes.get("steam:730").map(String::as_str), Some("ligne suivante"));
        assert_eq!(d.aliases.into_iter().collect::<Vec<_>>(), [("steam:730".to_string(), vec!["cod".to_string(), "x y".to_string()])]);
        assert_eq!(d.genres.into_iter().collect::<Vec<_>>(), [("steam:730".to_string(), vec!["RPG".to_string()])]);
        assert!(!d.launch_args.contains_key("steam:730"));
        assert_eq!(d.launch_args.get("steam:440").map(String::as_str), Some("-windowed"));
        assert_eq!(d.manual.len(), 1);
        assert_eq!(d.manual[0].exe, PathBuf::from(r"D:\Jeux\a.exe"));
        assert!(d.manual[0].args.is_empty());
        assert_eq!(d.exes.keys().collect::<Vec<_>>(), ["other:x"]);
        assert_eq!(d.collections.len(), 2);
        assert_eq!(d.collections[0].games, ["steam:730", "steam:440"]);
        assert_eq!(d.collections[1].name, "Coop");
        assert!(valid_collection_id(&d.collections[1].id));
        assert_eq!(d.settings.quick_shortcut, crate::quick::DEFAULT_SHORTCUT);
        assert_eq!(e.sessions, [session("steam:730", 5, 60)]);
    }

    #[test]
    fn merge_keeps_this_pc_and_adds_the_rest() {
        let mut here = UserData::default();
        here.set_favorite("steam:730", true);
        here.set_status("steam:730", Some(Status::Done));
        here.titles.insert("other:a".into(), "Mon titre".into());
        here.set_aliases("other:a", vec!["mien".into()]);
        here.set_genres("other:a", vec!["RPG".into()]);
        here.playtime.insert("epic:Sugar".into(), 600);
        here.launched.insert("epic:Sugar".into(), 50);
        here.settings.theme = crate::store::Theme::Light;
        here.settings.accent = crate::store::Accent::Pink;
        let coop = here.create_collection("Coop", 1).unwrap();
        here.set_in_collection(&coop, "steam:730", true).unwrap();
        here.set_cover("riot:valorant", Kind::Cover, Some(Picked { ext: "png".into(), at: 1 }));
        let dolphin = here.add_manual(PathBuf::from(r"D:\Emu\Dolphin.exe"), "Dolphin".into());

        let mut there = UserData::default();
        there.set_favorite("steam:440", true);
        there.set_status("steam:730", Some(Status::Dropped));
        there.set_status("steam:440", Some(Status::Todo));
        there.titles.insert("other:a".into(), "Autre titre".into());
        there.set_aliases("other:a", vec!["autre".into()]);
        there.set_aliases("steam:440", vec!["tf2".into()]);
        there.set_genres("other:a", vec!["Action".into()]);
        there.set_genres("steam:440", vec!["Action".into()]);
        there.playtime.insert("epic:Sugar".into(), 300);
        there.playtime.insert("epic:Fortnite".into(), 60);
        there.launched.insert("epic:Sugar".into(), 90);
        there.settings.theme = crate::store::Theme::Oled;
        there.settings.accent = crate::store::Accent::Green;
        there.collections.push(Collection { id: coop.clone(), name: "COOP".into(), games: vec!["steam:440".into()] });
        there.collections.push(Collection { id: coop.clone(), name: "Solo".into(), games: vec!["steam:620".into()] });
        there.covers.insert("riot:valorant".into(), CustomArt { cover: Some(Picked { ext: "jpg".into(), at: 9 }), hero: Some(Picked { ext: "jpg".into(), at: 9 }) });
        there.manual.push(ManualGame { id: dolphin.clone(), exe: PathBuf::from(r"F:\Dolphin.exe"), title: "Ailleurs".into(), args: vec![] });
        there.add_manual(PathBuf::from(r"D:\Emu\Pcsx2.exe"), "PCSX2".into());
        let wish = |appid: u32, title: &str| crate::wishlist::Wish { appid, title: title.into(), added: 1, price: None };
        here.wishlist.push(wish(1145360, "Hades"));
        there.wishlist = vec![wish(1145360, "Autre titre"), wish(1426210, "It Takes Two")];

        let images = vec![
            CoverImage { game: "riot:valorant".into(), kind: Kind::Cover, bytes: vec![], ext: "jpg", at: 9 },
            CoverImage { game: "riot:valorant".into(), kind: Kind::Hero, bytes: vec![], ext: "jpg", at: 9 },
        ];
        let out = combine(&here, there, Mode::Merge, 2);
        assert!(out.favorites.contains("steam:730") && out.favorites.contains("steam:440"));
        assert_eq!(out.status["steam:730"], Status::Done); // ce PC l'emporte
        assert_eq!(out.status["steam:440"], Status::Todo);
        assert_eq!(out.titles["other:a"], "Mon titre");
        assert_eq!(out.aliases["other:a"], ["mien"]);
        assert_eq!(out.aliases["steam:440"], ["tf2"]);
        assert_eq!(out.genres["other:a"], ["RPG"]);
        assert_eq!(out.genres["steam:440"], ["Action"]);
        assert_eq!(out.playtime["epic:Sugar"], 600); // le plus grand, jamais la somme
        assert_eq!(out.playtime["epic:Fortnite"], 60);
        assert_eq!(out.launched["epic:Sugar"], 90);
        assert_eq!(out.settings.theme, crate::store::Theme::Light);
        assert_eq!(out.settings.accent, crate::store::Accent::Pink); // ce PC l'emporte
        // Liste de souhaits : fusion par appid, l'entrée de ce PC reste.
        assert_eq!(out.wishlist.iter().map(|w| (w.appid, w.title.as_str())).collect::<Vec<_>>(), [(1145360, "Hades"), (1426210, "It Takes Two")]);
        // Collection de même nom : jeux réunis ; autre nom au même id : nouvel id.
        assert_eq!(out.collections.len(), 2);
        assert_eq!(out.collections[0].games, ["steam:730", "steam:440"]);
        assert_eq!(out.collections[1].name, "Solo");
        assert_ne!(out.collections[1].id, coop);
        // Jaquette : celle de ce PC gardée, l'image de fond ajoutée (seule à écrire).
        assert_eq!(out.covers["riot:valorant"].get(Kind::Cover).unwrap().at, 1);
        assert_eq!(out.covers["riot:valorant"].get(Kind::Hero).unwrap().at, 9);
        let written: Vec<Kind> = covers_to_write(&images, &out).iter().map(|i| i.kind).collect();
        assert_eq!(written, [Kind::Hero]);
        // Jeux manuels : même id gardé tel quel, nouveau ajouté.
        assert_eq!(out.manual.len(), 2);
        assert_eq!(out.manual[0].exe, PathBuf::from(r"D:\Emu\Dolphin.exe"));

        // Sessions : réunies sans doublon, dans l'ordre.
        let mine = [session("steam:730", 10, 60), session("steam:730", 30, 60)];
        let theirs = vec![session("steam:730", 30, 60), session("steam:440", 20, 120)];
        let merged = combine_sessions(&mine, theirs.clone(), Mode::Merge);
        assert_eq!(merged.iter().map(|s| s.start).collect::<Vec<_>>(), [10, 20, 30]);
        let replaced = combine_sessions(&mine, theirs, Mode::Replace);
        assert_eq!(replaced.iter().map(|s| s.start).collect::<Vec<_>>(), [20, 30]);
    }

    #[test]
    fn backups_are_kept_and_rotated() {
        let dir = temp_dir("backup");
        let root = dir.join("images");
        let data = UserData::default();
        let first = backup(&dir, "2026-11-19_10-00-00", &data, &[], &root, "0.2.0").unwrap();
        let second = backup(&dir, "2026-11-19_10-00-00", &data, &[], &root, "0.2.0").unwrap();
        assert_eq!(first, "avant-import-2026-11-19_10-00-00.zip");
        assert_eq!(second, "avant-import-2026-11-19_10-00-00-2.zip");
        for h in 11..18 {
            backup(&dir, &format!("2026-11-19_{h}-00-00"), &data, &[], &root, "0.2.0").unwrap();
        }
        let mut left: Vec<String> =
            fs::read_dir(dir.join(BACKUP_DIR)).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        assert_eq!(left.len(), KEEP_BACKUPS);
        assert_eq!(left[0], "avant-import-2026-11-19_13-00-00.zip");
        // Une copie de sécurité se réimporte comme un export.
        let path = dir.join(BACKUP_DIR).join(&left[0]);
        let len = fs::metadata(&path).unwrap().len();
        assert!(read_archive(fs::File::open(&path).unwrap(), len).is_ok());
        let _ = fs::remove_dir_all(dir);
    }
}
