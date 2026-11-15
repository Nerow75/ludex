mod library;
mod store;

use library::{Game, Launch, ScanResult};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use store::UserData;
use tauri::{Manager, State};
use tauri_plugin_opener::OpenerExt;

/// Derniers jeux détectés et préférences de l'utilisateur. Le front ne transmet qu'un identifiant :
/// la commande de lancement vient toujours d'ici, jamais de l'interface.
struct AppState {
    games: Mutex<Vec<Game>>,
    data: Mutex<UserData>,
    data_path: PathBuf,
}

fn lock<T>(m: &Mutex<T>) -> Result<MutexGuard<'_, T>, String> {
    m.lock().map_err(|_| "État interne indisponible".to_string())
}

#[tauri::command]
async fn scan_library(state: State<'_, AppState>) -> Result<ScanResult, String> {
    let mut result = tauri::async_runtime::spawn_blocking(library::scan)
        .await
        .map_err(|e| format!("Le scan a échoué : {e}"))?;
    let data = lock(&state.data)?;
    result.games.iter_mut().for_each(|g| data.apply(g));
    *lock(&state.games)? = result.games.clone();
    Ok(result)
}

/// Modifie les préférences d'un jeu connu, puis les enregistre.
fn update_data(state: &State<'_, AppState>, id: &str, change: impl FnOnce(&mut UserData, &Game)) -> Result<(), String> {
    let game = find_game(state, id)?;
    let mut data = lock(&state.data)?;
    change(&mut data, &game);
    data.save(&state.data_path)?;
    if let Some(g) = lock(&state.games)?.iter_mut().find(|g| g.id == id) {
        data.apply(g);
    }
    Ok(())
}

#[tauri::command]
fn set_favorite(state: State<'_, AppState>, id: String, value: bool) -> Result<(), String> {
    update_data(&state, &id, |data, game| data.set_favorite(&game.id, value))
}

#[tauri::command]
fn set_hidden(state: State<'_, AppState>, id: String, value: bool) -> Result<(), String> {
    update_data(&state, &id, |data, game| data.set_hidden(game, value))
}

fn find_game(state: &State<'_, AppState>, id: &str) -> Result<Game, String> {
    lock(&state.games)?
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

/// Lance le jeu et renvoie l'heure du lancement (secondes Unix).
#[tauri::command]
fn launch_game(app: tauri::AppHandle, state: State<'_, AppState>, id: String) -> Result<u64, String> {
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
    }?;

    let mut data = lock(&state.data)?;
    let now = data.record_launch(&id);
    // Un historique non enregistré ne doit pas faire croire à un échec du lancement.
    let _ = data.save(&state.data_path);
    Ok(now)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let data_path = store::file_in(dir);
            app.manage(AppState {
                games: Mutex::default(),
                data: Mutex::new(UserData::load(&data_path)),
                data_path,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![scan_library, launch_game, open_install_dir, set_favorite, set_hidden])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
