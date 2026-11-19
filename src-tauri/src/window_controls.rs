//! Boutons de la barre de titre maison (fenêtre principale sans décorations Windows).
//!
//! Commandes sans argument : la fenêtre visée est celle qui appelle, et seule la fenêtre
//! principale est acceptée (la recherche rapide n'a pas de barre de titre). Le front n'a ainsi
//! besoin d'aucune permission `core:window:allow-minimize`, `allow-toggle-maximize` ou
//! `allow-close`. Le déplacement passe par `data-tauri-drag-region` (permission
//! `core:window:allow-start-dragging`, capability `main-window`) et le double clic par
//! `internal_toggle_maximize`, déjà dans `core:default`.
//!
//! Mode canapé : trois commandes sans argument plutôt que les permissions
//! `core:window:allow-maximize` / `allow-set-fullscreen`. Même raison que les boutons : la fenêtre
//! visée est fixée côté Rust (jamais la recherche rapide), le front reste à `core:default` plus le
//! déplacement, et le vrai plein écran dépend d'un réglage lu ici (`couchFullscreen`, faux par
//! défaut) : certains écrans VRR clignotent dès qu'une appli passe en plein écran.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{Runtime, State, WebviewWindow};

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

/// La fenêtre a été agrandie par le mode canapé : elle est rendue à sa taille à la sortie.
/// Une seule fenêtre principale, donc un seul drapeau.
static COUCH_MAXIMIZED: AtomicBool = AtomicBool::new(false);

/// Entrée dans le mode canapé : la fenêtre principale est agrandie (pas de plein écran, voir le
/// réglage `couchFullscreen`). Fenêtre cachée (démarrage avec `--minimized`) : laissée telle quelle,
/// l'agrandir l'afficherait.
#[tauri::command]
pub fn window_enter_couch<R: Runtime>(window: WebviewWindow<R>) -> Result<(), String> {
    main_only(&window)?;
    if !window.is_visible().unwrap_or(false) || window.is_fullscreen().unwrap_or(false) {
        return Ok(());
    }
    if window.is_minimized().unwrap_or(false) {
        window.unminimize().map_err(|e| format!("Fenêtre non rouverte : {e}"))?;
    }
    if !window.is_maximized().unwrap_or(true) {
        window.maximize().map_err(|e| format!("Fenêtre non agrandie : {e}"))?;
        COUCH_MAXIMIZED.store(true, Ordering::Relaxed);
    }
    Ok(())
}

/// Sortie du mode canapé : quitte le plein écran s'il a été demandé, puis rend sa taille à la
/// fenêtre si le mode l'avait agrandie.
#[tauri::command]
pub fn window_leave_couch<R: Runtime>(window: WebviewWindow<R>) -> Result<(), String> {
    main_only(&window)?;
    if window.is_fullscreen().unwrap_or(false) {
        window.set_fullscreen(false).map_err(|e| format!("Sortie du plein écran impossible : {e}"))?;
    }
    if COUCH_MAXIMIZED.swap(false, Ordering::Relaxed) && window.is_maximized().unwrap_or(false) {
        window.unmaximize().map_err(|e| format!("Taille de la fenêtre non rendue : {e}"))?;
    }
    Ok(())
}

/// F11 dans le mode canapé : bascule le vrai plein écran si le réglage `couchFullscreen` le
/// permet. `None` : plein écran désactivé (le front quitte alors le mode, comme F11 y est entré),
/// sinon le nouvel état.
#[tauri::command]
pub fn window_couch_fullscreen<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, crate::AppState>,
) -> Result<Option<bool>, String> {
    main_only(&window)?;
    if !crate::lock(&state.data)?.settings.couch_fullscreen {
        return Ok(None);
    }
    let on = !window.is_fullscreen().unwrap_or(false);
    window.set_fullscreen(on).map_err(|e| format!("Plein écran impossible : {e}"))?;
    Ok(Some(on))
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
