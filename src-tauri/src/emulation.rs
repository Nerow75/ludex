//! Réglages de l'émulation (section « Émulation » des Paramètres) : dossiers de ROMs choisis et
//! émulateur par console. La source elle-même est dans `library/emulation/`.
//!
//! Le front n'envoie jamais de chemin à ouvrir ou à lire : un dossier s'ajoute par le sélecteur
//! natif ouvert ici, se retire par son texte s'il est déjà dans la liste, et un choix
//! d'émulateur est un couple d'identifiants vérifié contre les tables de Ludex.

use crate::library::emulation::{self, MAX_ROM_DIRS, Summary};
use crate::library::ScanResult;
use crate::{AppState, blocking, lock, save_and_rescan, wait_scan};
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

/// Ce que montre la section « Émulation » : émulateurs, consoles et dossiers vus au dernier scan,
/// plus les dossiers choisis (texte seulement).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmulationInfo {
    /// Faux avant le premier scan complet.
    scanned: bool,
    #[serde(flatten)]
    summary: Summary,
    rom_dirs: Vec<String>,
}

/// Réponse d'un changement : réglages relus et bibliothèque rescannée.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmulationUpdate {
    info: EmulationInfo,
    scan: ScanResult,
}

fn info(state: &AppState) -> Result<EmulationInfo, String> {
    let rom_dirs = lock(&state.data)?.settings.emulation.rom_dirs.iter().map(|d| d.display().to_string()).collect();
    let summary = emulation::summary();
    Ok(EmulationInfo { scanned: summary.is_some(), summary: summary.unwrap_or_default(), rom_dirs })
}

#[tauri::command]
pub fn emulation_info(app: AppHandle) -> Result<EmulationInfo, String> {
    info(&app.state::<AppState>())
}

/// Ajoute un dossier de ROMs par le sélecteur natif, puis rescanne. `None` : annulé.
#[tauri::command]
pub async fn add_rom_dir(app: AppHandle, window: tauri::WebviewWindow) -> Result<Option<EmulationUpdate>, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        let picked = window
            .dialog()
            .file()
            .set_title(tr!("Dossier de ROMs", "ROM folder"))
            .set_parent(&window)
            .blocking_pick_folder()
            .and_then(|f| f.into_path().ok());
        let Some(picked) = picked else { return Ok(None) };
        let dir = emulation::check_rom_dir(&picked)?;
        {
            let data = lock(&state.data)?;
            let list = &data.settings.emulation.rom_dirs;
            if list.iter().any(|d| emulation::path_key(d) == emulation::path_key(&dir)) {
                return Err(tr!("Ce dossier est déjà dans la liste.", "This folder is already in the list."));
            }
            if list.len() >= MAX_ROM_DIRS {
                return Err(tr!("{MAX_ROM_DIRS} dossiers au plus.", "{MAX_ROM_DIRS} folders at most."));
            }
        }
        let scanning = wait_scan(&state);
        let ((), scan) = save_and_rescan(&app, &state, scanning, |data| data.settings.emulation.rom_dirs.push(dir))?;
        Ok(Some(EmulationUpdate { info: info(&state)?, scan }))
    })
    .await
}

/// Retire un dossier choisi (texte reçu de `emulation_info`), puis rescanne.
#[tauri::command]
pub async fn remove_rom_dir(app: AppHandle, path: String) -> Result<EmulationUpdate, String> {
    crate::collections::check_input(&path)?;
    blocking(move || {
        let state = app.state::<AppState>();
        let key = emulation::path_key(&PathBuf::from(&path));
        if !lock(&state.data)?.settings.emulation.rom_dirs.iter().any(|d| emulation::path_key(d) == key) {
            return Err(tr!("Dossier introuvable dans la liste.", "Folder not found in the list."));
        }
        let scanning = wait_scan(&state);
        let ((), scan) = save_and_rescan(&app, &state, scanning, |data| data.settings.emulation.rom_dirs.retain(|d| emulation::path_key(d) != key))?;
        Ok(EmulationUpdate { info: info(&state)?, scan })
    })
    .await
}

/// Émulateur choisi pour une console (`None` : choix automatique), puis rescanne.
#[tauri::command]
pub async fn set_emulator_choice(app: AppHandle, system: String, emulator: Option<String>) -> Result<EmulationUpdate, String> {
    crate::collections::check_input(&system)?;
    if let Some(e) = &emulator
        && !emulation::valid_choice(&system, e)
    {
        return Err(tr!("Cet émulateur ne lance pas cette console.", "This emulator can't run this console."));
    }
    if emulation::systems::find(&system).is_none() {
        return Err(tr!("Console inconnue.", "Unknown console."));
    }
    blocking(move || {
        let state = app.state::<AppState>();
        let scanning = wait_scan(&state);
        let ((), scan) = save_and_rescan(&app, &state, scanning, |data| {
            let choices = &mut data.settings.emulation.choices;
            match emulator {
                Some(e) => choices.insert(system, e),
                None => choices.remove(&system),
            };
        })?;
        Ok(EmulationUpdate { info: info(&state)?, scan })
    })
    .await
}
