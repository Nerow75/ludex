mod library;

use library::{Game, Launch, ScanResult};
use std::sync::Mutex;
use tauri::State;
use tauri_plugin_opener::OpenerExt;

/// Derniers jeux détectés. Le front ne transmet qu'un identifiant :
/// la commande de lancement vient toujours d'ici, jamais de l'interface.
#[derive(Default)]
struct AppState {
    games: Mutex<Vec<Game>>,
}

#[tauri::command]
async fn scan_library(state: State<'_, AppState>) -> Result<ScanResult, String> {
    let result = tauri::async_runtime::spawn_blocking(library::scan)
        .await
        .map_err(|e| format!("Le scan a échoué : {e}"))?;
    *state.games.lock().map_err(|_| "État interne indisponible".to_string())? = result.games.clone();
    Ok(result)
}

fn find_game(state: &State<'_, AppState>, id: &str) -> Result<Game, String> {
    state
        .games
        .lock()
        .map_err(|_| "État interne indisponible".to_string())?
        .iter()
        .find(|g| g.id == id)
        .cloned()
        .ok_or_else(|| "Ce jeu n'est plus dans la bibliothèque. Relance un scan.".to_string())
}

#[tauri::command]
fn open_install_dir(app: tauri::AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    let dir = find_game(&state, &id)?
        .install_dir
        .ok_or("Dossier d'installation inconnu pour ce jeu.")?;
    app.opener()
        .open_path(&dir, None::<&str>)
        .map_err(|e| format!("Impossible d'ouvrir {dir} : {e}"))
}

#[tauri::command]
fn launch_game(app: tauri::AppHandle, state: State<'_, AppState>, id: String) -> Result<(), String> {
    let game = find_game(&state, &id)?;

    match &game.launch {
        Launch::Uri(uri) => app
            .opener()
            .open_url(uri, None::<&str>)
            .map_err(|e| format!("Le launcher n'a pas répondu : {e}")),
        Launch::Exec { path, args } => std::process::Command::new(path)
            .args(args)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Impossible de démarrer {} : {e}", path.display())),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![scan_library, launch_game, open_install_dir])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
