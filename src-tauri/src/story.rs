//! Story du récap : succès le plus rare de la période (sans requête) et enregistrement d'une page en
//! PNG.
//!
//! L'image est dessinée par le front dans un canvas ; Rust ne reçoit que ses octets (corps brut de
//! l'appel, jamais un chemin), vérifie que c'est bien un PNG de 1080 × 1920 de taille raisonnable,
//! ouvre lui-même le sélecteur d'enregistrement natif puis écrit le fichier (tmp + renommage).

use crate::library::{AchievementStore, Achievement, Game, SteamStats};
use crate::recap::{self, Period, Zone};
use crate::{AppState, blocking, lock};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::ipc::{InvokeBody, Request};
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

/// Dimensions imposées de l'image (format story 9:16).
pub const WIDTH: u32 = 1080;
pub const HEIGHT: u32 = 1920;
/// Taille maximale acceptée : une page dessinée pèse quelques Mo au plus.
pub const MAX_BYTES: usize = 16 * 1024 * 1024;
const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

/// Vérifie les octets : signature PNG, premier bloc `IHDR` de 13 octets, 1080 × 1920, taille.
pub fn check_png(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > MAX_BYTES {
        return Err(tr!("Image trop lourde.", "Image too large."));
    }
    if bytes.len() < 33 || bytes[..8] != SIGNATURE {
        return Err(tr!("Ce n'est pas une image PNG.", "This isn't a PNG image."));
    }
    let be = |at: usize| u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    if be(8) != 13 || &bytes[12..16] != b"IHDR" {
        return Err(tr!("Image PNG illisible.", "Unreadable PNG image."));
    }
    if be(16) != WIDTH || be(20) != HEIGHT {
        return Err(tr!("L'image doit mesurer {WIDTH} × {HEIGHT} pixels.", "The image must be {WIDTH} × {HEIGHT} pixels."));
    }
    // Fin du fichier : le bloc `IEND` (image complète).
    if bytes.len() < 12 || &bytes[bytes.len() - 8..bytes.len() - 4] != b"IEND" {
        return Err(tr!("Image PNG incomplète.", "Incomplete PNG image."));
    }
    Ok(())
}

/// Écrit les octets dans `path` : fichier temporaire, puis renommage.
fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("png.tmp");
    let result = fs::write(&tmp, bytes).and_then(|_| fs::rename(&tmp, path));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map_err(|e| tr!("Écriture impossible : {e}", "Couldn't write the file: {e}"))
}

/// Nom proposé dans le sélecteur.
fn file_name(stamp: &str) -> String {
    format!("ludex-recap-{stamp}.png")
}

/// Enregistre la page affichée de la story. Corps de l'appel = octets du PNG (rien d'autre). Le
/// sélecteur natif est ouvert ici ; renvoie le nom du fichier écrit (sans dossier), `None` si annulé.
#[tauri::command]
pub async fn save_story_image(window: WebviewWindow, request: Request<'_>) -> Result<Option<String>, String> {
    let InvokeBody::Raw(bytes) = request.body() else { return Err(tr!("Image attendue.", "Image expected.")) };
    check_png(bytes)?;
    let bytes = bytes.clone();
    blocking(move || {
        let name = file_name(&chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string());
        let picked = window
            .dialog()
            .file()
            .set_title(tr!("Enregistrer la page en image", "Save the page as an image"))
            .add_filter(tr!("Image PNG", "PNG image"), &["png"])
            .set_file_name(&name)
            .set_parent(&window)
            .blocking_save_file()
            .and_then(|f| f.into_path().ok());
        let Some(mut path) = picked else { return Ok(None) };
        if !path.is_absolute() {
            return Err(tr!("Chemin invalide.", "Invalid path."));
        }
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) {
            let mut text = path.into_os_string();
            text.push(".png");
            path = PathBuf::from(text);
        }
        write_file(&path, &bytes)?;
        Ok(Some(path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()))
    })
    .await
}

/// Succès le plus rare débloqué pendant la période.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rare {
    /// Boutique du succès (« des joueurs Steam », « des joueurs GOG »).
    pub store: AchievementStore,
    pub game_id: String,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub percent: f32,
    pub unlocked_at: u64,
}

/// Parmi les succès débloqués entre deux jours locaux (compris) dont le pourcentage mondial est
/// connu, le plus rare (à égalité, le plus récent, puis le premier jeu).
pub fn rarest(lists: &[(String, Vec<Achievement>)], first: i64, last: i64, zone: &impl Zone) -> Option<Rare> {
    let mut best: Option<Rare> = None;
    for (game_id, list) in lists {
        for a in list {
            let (true, Some(at), Some(percent)) = (a.unlocked, a.unlocked_at, a.global_percent) else { continue };
            if a.name.is_empty() || !percent.is_finite() {
                continue;
            }
            let day = recap::local_day(i64::try_from(at).unwrap_or(i64::MAX / 2), zone);
            if !(first..=last).contains(&day) {
                continue;
            }
            let better = best.as_ref().is_none_or(|b| percent < b.percent || (percent == b.percent && at > b.unlocked_at));
            if better {
                best = Some(Rare {
                    store: a.store,
                    game_id: game_id.clone(),
                    name: a.name.clone(),
                    description: a.description.clone(),
                    icon: a.icon.clone(),
                    percent,
                    unlocked_at: at,
                });
            }
        }
    }
    best
}

/// Succès le plus rare de la période du récap (`back` périodes en arrière). Détail lu seulement pour
/// les jeux Steam qui ont un déblocage dans la période, pourcentages mondiaux du cache seulement
/// (aucune requête) : `None` si aucun n'est connu.
#[tauri::command]
pub async fn recap_rarest(app: AppHandle, period: Period, back: Option<u32>) -> Result<Option<Rare>, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let zone = recap::LocalZone;
        let (first, last) = recap::span(period, now, back.unwrap_or(0), &zone);
        let in_period = |t: u64| (first..=last).contains(&recap::local_day(i64::try_from(t).unwrap_or(i64::MAX / 2), &zone));
        // Steam et GOG : détail lu dans les caches seulement.
        let wanted: Vec<Game> = lock(&state.games)?
            .iter()
            .filter(|g| g.achievements.as_ref().is_some_and(|a| a.unlock_times.iter().any(|&t| in_period(t))))
            .cloned()
            .collect();
        if wanted.is_empty() {
            return Ok(None);
        }
        let stats = SteamStats::locate();
        let cache = lock(&state.global)?;
        let lists: Vec<(String, Vec<Achievement>)> =
            wanted.iter().filter_map(|g| Some((g.id.clone(), crate::showcase::details_of(g, stats.as_ref(), &cache)?))).collect();
        Ok(rarest(&lists, first, last, &zone))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PNG minimal valide en apparence : signature, IHDR, IEND.
    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut out = SIGNATURE.to_vec();
        out.extend_from_slice(&13u32.to_be_bytes());
        out.extend_from_slice(b"IHDR");
        out.extend_from_slice(&width.to_be_bytes());
        out.extend_from_slice(&height.to_be_bytes());
        out.extend_from_slice(&[8, 6, 0, 0, 0]);
        out.extend_from_slice(&[0; 4]); // CRC (non vérifié)
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(b"IEND");
        out.extend_from_slice(&[0xAE, 0x42, 0x60, 0x82]);
        out
    }

    #[test]
    fn accepte_seulement_un_png_de_la_bonne_taille() {
        assert!(check_png(&png(1080, 1920)).is_ok());
        assert!(check_png(&png(1920, 1080)).unwrap_err().contains("1080 × 1920"));
        assert!(check_png(&png(1080, 1921)).is_err());
        let mut jpeg = png(1080, 1920);
        jpeg[0] = 0xFF;
        assert!(check_png(&jpeg).unwrap_err().contains("PNG"));
        assert!(check_png(b"").is_err());
        // Image tronquée : pas de bloc IEND.
        let full = png(1080, 1920);
        assert!(check_png(&full[..full.len() - 12]).is_err());
        // Premier bloc qui n'est pas IHDR.
        let mut other = png(1080, 1920);
        other[12..16].copy_from_slice(b"tEXt");
        assert!(check_png(&other).is_err());
    }

    #[test]
    fn refuse_une_image_trop_lourde() {
        let mut big = png(1080, 1920);
        let end = big.split_off(big.len() - 12);
        big.resize(MAX_BYTES + 1, 0);
        big.extend_from_slice(&end);
        assert_eq!(check_png(&big).unwrap_err(), "Image trop lourde.");
    }

    #[test]
    fn ecrit_par_un_fichier_temporaire() {
        let dir = std::env::temp_dir().join(format!("ludex-story-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("page.png");
        write_file(&path, &png(1080, 1920)).unwrap();
        assert_eq!(fs::read(&path).unwrap(), png(1080, 1920));
        assert!(!path.with_extension("png.tmp").exists());
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(file_name("2026-11-21_10-00-00"), "ludex-recap-2026-11-21_10-00-00.png");
    }

    struct Utc;
    impl Zone for Utc {
        fn offset(&self, _: i64) -> i64 {
            0
        }
    }

    fn succes(name: &str, at: Option<u64>, percent: Option<f32>) -> Achievement {
        Achievement {
            store: crate::library::AchievementStore::Steam,
            api_name: name.into(),
            name: name.into(),
            description: None,
            icon: None,
            icon_gray: None,
            hidden: false,
            unlocked: at.is_some(),
            unlocked_at: at,
            global_percent: percent,
        }
    }

    #[test]
    fn plus_rare_de_la_periode() {
        let day = 20_000i64; // jour local
        let t = |d: i64, h: u64| (d * 86_400) as u64 + h * 3_600;
        let lists = vec![
            ("steam:1".to_string(), vec![succes("Courant", Some(t(day, 10)), Some(40.0)), succes("Ancien", Some(t(day - 30, 1)), Some(0.1))]),
            (
                "steam:2".to_string(),
                vec![
                    succes("Rare", Some(t(day + 1, 2)), Some(1.5)),
                    succes("Inconnu", Some(t(day + 1, 3)), None),
                    succes("Pas débloqué", None, Some(0.5)),
                    succes("Même rareté, plus récent", Some(t(day + 2, 4)), Some(1.5)),
                ],
            ),
        ];
        let best = rarest(&lists, day, day + 6, &Utc).unwrap();
        assert_eq!(best.name, "Même rareté, plus récent");
        assert_eq!(best.game_id, "steam:2");
        // Rien de connu dans la période.
        assert!(rarest(&lists, day + 10, day + 16, &Utc).is_none());
    }
}
