//! Mises à jour de Ludex (plugin `tauri-plugin-updater`). Tout se passe ici : le front n'a aucune
//! permission du plugin, il reçoit `update-available` et appelle `install_update`, sans argument.
//! Le manifeste `latest.json` est lu à l'adresse de `tauri.conf.json` (`plugins.updater`) et
//! l'installeur téléchargé n'est lancé que si sa signature correspond à la clé publique de ce fichier.

use serde::Serialize;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

/// Première vérification quelques secondes après l'ouverture : le scan passe d'abord.
const FIRST_CHECK: Duration = Duration::from_secs(8);
/// Ludex peut rester ouvert des jours : nouvelle vérification tant que rien n'est trouvé.
const RECHECK: Duration = Duration::from_secs(12 * 60 * 60);

const UPDATE_EVENT: &str = "update-available";

/// Mise à jour trouvée, en attente de l'accord de l'utilisateur.
#[derive(Default)]
pub struct PendingUpdate {
    update: Mutex<Option<Update>>,
    installing: AtomicBool,
}

/// Événement `update-available` et réponse de `available_update`.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    version: String,
    current_version: String,
    notes: Option<String>,
}

impl UpdateInfo {
    fn of(u: &Update) -> Self {
        UpdateInfo { version: u.version.clone(), current_version: u.current_version.clone(), notes: u.body.clone() }
    }
}

/// Vérifie en tâche de fond, puis prévient le front. Silencieux en cas d'échec (hors ligne, pas
/// encore de release, adresse à compléter) : un message en console, rien à l'écran.
/// Désactivé en dev (`tauri dev`) : l'appli n'y est pas installée.
pub fn spawn_check(app: AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK);
        loop {
            match tauri::async_runtime::block_on(check(&app)) {
                Ok(Some(info)) => {
                    let _ = app.emit(UPDATE_EVENT, info);
                    return;
                }
                Ok(None) => {}
                Err(e) => eprintln!("Vérification des mises à jour impossible : {e}"),
            }
            std::thread::sleep(RECHECK);
        }
    });
}

async fn check(app: &AppHandle) -> tauri_plugin_updater::Result<Option<UpdateInfo>> {
    let Some(update) = app.updater()?.check().await? else { return Ok(None) };
    let info = UpdateInfo::of(&update);
    if let Some(pending) = app.try_state::<PendingUpdate>()
        && let Ok(mut slot) = pending.update.lock()
    {
        *slot = Some(update);
    }
    Ok(Some(info))
}

/// Mise à jour déjà trouvée, pour la retrouver après un rechargement de l'interface.
#[tauri::command]
pub fn available_update(state: State<'_, PendingUpdate>) -> Option<UpdateInfo> {
    state.update.lock().ok()?.as_ref().map(UpdateInfo::of)
}

/// Recherche demandée depuis les Paramètres. Trouvée : gardée et annoncée comme celle du fond
/// (`update-available` : toast et bouton de la barre du haut). `None` : Ludex est à jour.
#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    if cfg!(debug_assertions) {
        return Err("La recherche de mises à jour est coupée en développement.".into());
    }
    let found = check(&app).await.map_err(|e| format!("Recherche impossible : {e}"))?;
    if let Some(info) = &found {
        let _ = app.emit(UPDATE_EVENT, info.clone());
    }
    Ok(found)
}

/// Télécharge, vérifie et lance l'installeur de la mise à jour trouvée par Ludex. Sous Windows,
/// le plugin ferme Ludex et l'installeur NSIS le relance une fois l'installation finie.
#[tauri::command]
pub async fn install_update(app: AppHandle, state: State<'_, PendingUpdate>) -> Result<(), String> {
    let update = state
        .update
        .lock()
        .map_err(|_| "État interne indisponible".to_string())?
        .clone()
        .ok_or("Aucune mise à jour à installer.")?;
    if state.installing.swap(true, Ordering::SeqCst) {
        return Err("La mise à jour est déjà en cours.".into());
    }
    let result = update.download_and_install(|_, _| {}, || {}).await;
    state.installing.store(false, Ordering::SeqCst);
    match result {
        // Atteint seulement hors Windows : il faut relancer soi-même.
        Ok(()) => app.restart(),
        Err(e) => Err(format!("La mise à jour a échoué : {e}")),
    }
}
