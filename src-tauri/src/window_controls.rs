//! Boutons de la barre de titre maison (fenêtre principale sans décorations Windows).
//!
//! Commandes sans argument : la fenêtre visée est celle qui appelle, et seule la fenêtre
//! principale est acceptée (la recherche rapide n'a pas de barre de titre). Le front n'a ainsi
//! besoin d'aucune permission `core:window:allow-minimize`, `allow-toggle-maximize` ou
//! `allow-close`. Le déplacement passe par `data-tauri-drag-region` (permission
//! `core:window:allow-start-dragging`, capability `main-window`) et le double clic par
//! `internal_toggle_maximize`, déjà dans `core:default`.

use tauri::{Runtime, WebviewWindow};

/// Refuse toute autre fenêtre que la principale.
fn main_only<R: Runtime>(window: &WebviewWindow<R>) -> Result<(), String> {
    if is_main(window.label()) { Ok(()) } else { Err("Fenêtre non prise en charge.".into()) }
}

fn is_main(label: &str) -> bool {
    label == crate::MAIN_WINDOW
}

/// Réduit la fenêtre principale dans la barre des tâches.
#[tauri::command]
pub fn window_minimize<R: Runtime>(window: WebviewWindow<R>) -> Result<(), String> {
    main_only(&window)?;
    window.minimize().map_err(|e| format!("Fenêtre non réduite : {e}"))
}

/// Agrandit la fenêtre principale, ou la restaure si elle l'est déjà.
#[tauri::command]
pub fn window_toggle_maximize<R: Runtime>(window: WebviewWindow<R>) -> Result<(), String> {
    main_only(&window)?;
    let result = if window.is_maximized().unwrap_or(false) { window.unmaximize() } else { window.maximize() };
    result.map_err(|e| format!("Taille de la fenêtre non changée : {e}"))
}

/// Demande la fermeture de la fenêtre principale : même chemin que Alt+F4, donc
/// `CloseRequested` décide (cachée dans la zone de notification si `keepInTray`, sinon Ludex quitte).
#[tauri::command]
pub fn window_close<R: Runtime>(window: WebviewWindow<R>) -> Result<(), String> {
    main_only(&window)?;
    window.close().map_err(|e| format!("Fenêtre non fermée : {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seule_la_fenetre_principale() {
        assert!(is_main("main"));
        assert!(!is_main(crate::quick::LABEL));
        assert!(!is_main(""));
    }
}
