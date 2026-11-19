//! Écran de premier lancement : montré une seule fois, après le premier scan. Le drapeau est
//! `settings.welcomeShown` de `user-data.json` ; un utilisateur qui avait déjà Ludex avant ce
//! drapeau (préférences, caches dans le dossier de données) ne le voit jamais.

use crate::store::UserData;
use crate::{AppState, library};
use serde::Serialize;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Manager};

/// Variable d'environnement qui force l'écran (`LUDEX_WELCOME=1`), en debug ou avec la fonction
/// `welcome-preview` seulement, sans rien enregistrer : pour le voir sans toucher aux vraies données.
const FORCE_VAR: &str = "LUDEX_WELCOME";

/// Fichiers qu'une version précédente de Ludex a laissés dans le dossier de données.
const PREVIOUS_RUN: [&str; 7] = [
    "user-data.json",
    "sessions.json",
    "art-cache.json",
    "metadata-cache.json",
    "sizes-cache.json",
    "hltb-cache.json",
    "images",
];

pub struct Welcome {
    /// L'écran reste à montrer.
    pending: AtomicBool,
    /// Affichage forcé : « C'est parti » n'enregistre rien.
    forced: bool,
}

impl Welcome {
    /// À appeler au démarrage, avant toute écriture dans le dossier de données (le réglage du
    /// lancement avec Windows enregistre `user-data.json`, le suivi des sessions ses fichiers).
    pub fn detect(state: &AppState) -> Self {
        if forced() {
            return Welcome { pending: AtomicBool::new(true), forced: true };
        }
        let Ok(mut data) = state.data.lock() else {
            return Welcome { pending: AtomicBool::new(false), forced: false };
        };
        let pending = match data.settings.welcome_shown {
            Some(shown) => !shown,
            None => {
                let new = !returning(&data, &state.data_dir);
                data.settings.welcome_shown = Some(!new);
                // Nouvel utilisateur : la décision est gardée tout de suite, même si Ludex est fermé
                // avant la fin du premier scan (ses caches feraient sinon croire à un habitué).
                // Un habitué l'enregistrera avec la prochaine écriture des préférences.
                if new && let Err(e) = data.save(&state.data_path) {
                    eprintln!("Premier lancement non enregistré : {e}");
                }
                new
            }
        };
        Welcome { pending: AtomicBool::new(pending), forced: false }
    }
}

fn forced() -> bool {
    (cfg!(debug_assertions) || cfg!(feature = "welcome-preview")) && std::env::var(FORCE_VAR).is_ok_and(|v| v == "1")
}

/// Ludex a déjà servi sur ce PC : préférences non vides, ou fichiers d'une version précédente.
fn returning(data: &UserData, dir: &Path) -> bool {
    data.has_history() || PREVIOUS_RUN.iter().any(|name| dir.join(name).exists())
}

/// Ce que le front ne sait pas déjà par le scan.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WelcomeInfo {
    /// Dossiers « Jeux », « Games » (et « Jeux Epic » dedans) lus à la racine des disques.
    folders: Vec<String>,
}

/// `null` : écran déjà vu.
#[tauri::command]
pub async fn welcome(app: AppHandle) -> Result<Option<WelcomeInfo>, String> {
    if !app.state::<Welcome>().pending.load(Ordering::SeqCst) {
        return Ok(None);
    }
    let folders = tauri::async_runtime::spawn_blocking(library::game_folders).await.map_err(|e| e.to_string())?;
    Ok(Some(WelcomeInfo { folders: folders.iter().map(|d| d.display().to_string()).collect() }))
}

/// « C'est parti » : l'écran ne reviendra plus.
#[tauri::command]
pub fn welcome_done(app: AppHandle) -> Result<(), String> {
    let welcome = app.state::<Welcome>();
    welcome.pending.store(false, Ordering::SeqCst);
    if welcome.forced {
        return Ok(());
    }
    let state = app.state::<AppState>();
    let mut data = state.data.lock().map_err(|_| "État interne indisponible".to_string())?;
    data.settings.welcome_shown = Some(true);
    data.save(&state.data_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ludex-welcome-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn empty_data_dir_is_a_new_user() {
        let dir = temp_dir("new");
        assert!(!returning(&UserData::default(), &dir));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn caches_or_preferences_mean_a_returning_user() {
        let dir = temp_dir("back");
        fs::write(dir.join("art-cache.json"), "{}").unwrap();
        assert!(returning(&UserData::default(), &dir));
        fs::remove_dir_all(&dir).unwrap();

        let empty = temp_dir("prefs");
        let mut data = UserData::default();
        data.settings.tray_hint_shown = true;
        assert!(returning(&data, &empty));
        let mut data = UserData::default();
        data.set_favorite("steam:620", true);
        assert!(returning(&data, &empty));
        let mut data = UserData::default();
        data.add_playtime("epic:fn", 60);
        assert!(returning(&data, &empty));
        fs::remove_dir_all(&empty).unwrap();
    }
}
