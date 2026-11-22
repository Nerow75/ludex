//! Écran Paramètres : réglages de `settings` dans `user-data.json`, appliqués ici quand c'est Rust
//! qui s'en sert (thème des fenêtres, raccourci global, lancement avec Windows, zone de
//! notification, launchers lus au scan). Le front n'envoie que des valeurs fermées (thème,
//! launcher), des booléens et le texte d'un raccourci, validé ici ; jamais de chemin.
//!
//! Chaque changement, d'ici ou du menu de la zone de notification, est annoncé par l'événement
//! `settings-changed` : la page ouverte et la fenêtre de recherche rapide (thème) suivent.

use crate::i18n::{self, Lang, Language};
use crate::library::{Platform, ScanResult};
use crate::store::{Accent, Settings, Theme};
use crate::{AppState, blocking, lock, quick, refresh_tray, save_and_rescan, saves_root, update_settings, wait_scan};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// Événement `settings-changed` : réglages complets, après tout changement.
pub const EVENT: &str = "settings-changed";

/// Au-delà, le texte d'un raccourci est refusé avant toute lecture.
const MAX_SHORTCUT: usize = 64;

/// Réglages tels que la page les affiche.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsInfo {
    theme: Theme,
    /// Couleur d'accent de l'interface.
    accent: Accent,
    /// Langue choisie (`system` : celle de Windows).
    language: Language,
    /// Langue effective (celle de Windows résolue), pour le front.
    language_resolved: Lang,
    /// Raccourci au format du plugin (« Alt+Space »), vide si désactivé.
    quick_shortcut: String,
    /// Le même, lisible (« Alt+Espace », « Désactivé »).
    quick_shortcut_label: String,
    launch_at_startup: bool,
    keep_in_tray: bool,
    /// Dossier des copies de sauvegardes, en texte (affichage seulement).
    saves_dir: String,
    auto_saves: bool,
    /// Écran de lancement au clic sur Jouer.
    launch_screen: bool,
    disabled_sources: Vec<Platform>,
    /// Jeux masqués par l'utilisateur (les outils masqués d'office n'y sont pas).
    hidden_games: usize,
    version: String,
    /// Notifications Windows activées, par type.
    notifications: crate::notify::Prefs,
    /// Windows a coupé les notifications de Ludex (ou toutes) : Ludex ne change pas ce réglage.
    notifications_blocked: bool,
    /// Statut Discord des parties lancées depuis Ludex.
    discord_presence: bool,
    /// Statut Discord sans le nom du jeu.
    discord_hide_name: bool,
    /// Faux tant que cette version de Ludex n'a pas d'identifiant d'application Discord.
    discord_available: bool,
}

fn info<R: Runtime>(app: &AppHandle<R>) -> Result<SettingsInfo, String> {
    let state = app.state::<AppState>();
    let saves_dir = saves_root(&state).display().to_string();
    let launch_at_startup = crate::autostart_enabled(app);
    let notifications_blocked = app.try_state::<crate::notify::Notifier>().is_some_and(|n| n.blocked());
    let data = lock(&state.data)?;
    let s: &Settings = &data.settings;
    Ok(SettingsInfo {
        theme: s.theme,
        accent: s.accent,
        language: s.language,
        language_resolved: i18n::current(),
        quick_shortcut: s.quick_shortcut.clone(),
        quick_shortcut_label: quick::label(&s.quick_shortcut),
        launch_at_startup,
        keep_in_tray: s.keep_in_tray,
        saves_dir,
        auto_saves: s.auto_saves,
        launch_screen: s.launch_screen,
        disabled_sources: s.disabled_sources.clone(),
        hidden_games: data.hidden.len(),
        version: app.package_info().version.to_string(),
        notifications: s.notifications,
        notifications_blocked,
        discord_presence: s.discord_presence,
        discord_hide_name: s.discord_hide_name,
        discord_available: crate::discord::available(),
    })
}

/// Annonce les réglages à toutes les fenêtres et les renvoie.
pub fn publish<R: Runtime>(app: &AppHandle<R>) -> Result<SettingsInfo, String> {
    let info = info(app)?;
    let _ = app.emit(EVENT, info.clone());
    Ok(info)
}

/// Thème des fenêtres natives (couleur préférée de la WebView, barres de défilement, fond avant
/// le premier rendu). `None` : celui de Windows.
///
/// Noir OLED et contraste élevé sont deux palettes sombres (fond noir) : la fenêtre est donc
/// sombre, quel que soit Windows. Suivre Windows en contraste élevé donnerait, Windows en clair,
/// un éclair blanc à l'ouverture et des contrôles natifs clairs sur un fond noir.
pub fn window_theme(theme: Theme) -> Option<tauri::Theme> {
    match theme {
        Theme::Dark | Theme::Oled | Theme::Contrast => Some(tauri::Theme::Dark),
        Theme::Light => Some(tauri::Theme::Light),
        Theme::System => None,
    }
}

/// Thème des réglages, pour une fenêtre créée après le démarrage (recherche rapide).
pub fn current_theme<R: Runtime>(app: &AppHandle<R>) -> Theme {
    app.try_state::<AppState>().and_then(|s| lock(&s.data).ok().map(|d| d.settings.theme)).unwrap_or_default()
}

/// Applique le thème à toutes les fenêtres ouvertes.
pub fn apply_theme<R: Runtime>(app: &AppHandle<R>, theme: Theme) {
    for window in app.webview_windows().values() {
        if let Err(e) = window.set_theme(window_theme(theme)) {
            eprintln!("Thème de la fenêtre « {} » non appliqué : {e}", window.label());
        }
    }
}

#[tauri::command]
pub fn settings(app: AppHandle) -> Result<SettingsInfo, String> {
    info(&app)
}

#[tauri::command]
pub fn set_theme(app: AppHandle, theme: Theme) -> Result<SettingsInfo, String> {
    update_settings(&app, |s| s.theme = theme);
    apply_theme(&app, theme);
    publish(&app)
}

/// Couleur d'accent : rien à appliquer côté Rust (aucune couleur native), les fenêtres suivent
/// par `settings-changed`.
#[tauri::command]
pub fn set_accent(app: AppHandle, accent: Accent) -> Result<SettingsInfo, String> {
    update_settings(&app, |s| s.accent = accent);
    publish(&app)
}

/// Applique la langue des réglages à Rust (textes, menu de la zone de notification, caches des
/// fiches et des configurations requises, propres à une langue). Vrai si elle a changé.
pub fn apply_language<R: Runtime>(app: &AppHandle<R>) -> bool {
    let Some(state) = app.try_state::<AppState>() else { return false };
    let Ok(language) = lock(&state.data).map(|d| d.settings.language) else { return false };
    let lang = i18n::resolve(language);
    if lang == i18n::current() {
        return false;
    }
    i18n::set(lang);
    crate::reload_language_caches(&state);
    refresh_tray(app, true);
    true
}

/// Langue de l'interface, appliquée tout de suite : réglages publiés (les fenêtres suivent), puis
/// rescan (textes calculés au scan, tags et descriptions dans la nouvelle langue) ; la
/// bibliothèque relue est renvoyée.
#[tauri::command]
pub async fn set_language(app: AppHandle, language: Language) -> Result<ScanResult, String> {
    update_settings(&app, |s| s.language = language);
    apply_language(&app);
    publish(&app)?;
    let handle = app.clone();
    blocking(move || {
        let state = handle.state::<AppState>();
        let scanning = wait_scan(&state);
        let ((), scan) = save_and_rescan(&handle, &state, scanning, |_| ())?;
        Ok(scan)
    })
    .await
}

/// Nouveau raccourci de la recherche rapide, enregistré tout de suite. Vide : désactivé.
#[tauri::command]
pub fn set_quick_shortcut(app: AppHandle, text: String) -> Result<SettingsInfo, String> {
    if text.len() > MAX_SHORTCUT {
        return Err(tr!("Raccourci trop long.", "Shortcut too long."));
    }
    let text = text.trim().to_string();
    let previous = lock(&app.state::<AppState>().data)?.settings.quick_shortcut.clone();
    quick::replace(&app, &text, &previous)?;
    update_settings(&app, |s| s.quick_shortcut = text);
    publish(&app)
}

/// Saisie d'un raccourci en cours (`true`) ou finie (`false`) : le raccourci actuel est coupé
/// pendant la saisie, puis remis.
#[tauri::command]
pub fn pause_quick_shortcut(app: AppHandle, paused: bool) -> Result<(), String> {
    if paused {
        quick::pause(&app);
    } else {
        let text = lock(&app.state::<AppState>().data)?.settings.quick_shortcut.clone();
        quick::resume(&app, &text);
    }
    Ok(())
}

/// Lancement avec Windows (même réglage que la case du menu de la zone de notification).
#[tauri::command]
pub fn set_launch_at_startup(app: AppHandle, value: bool) -> Result<SettingsInfo, String> {
    let result = crate::set_autostart(&app, value);
    refresh_tray(&app, true);
    let info = publish(&app)?;
    result.map(|()| info)
}

/// Fermer la fenêtre la cache dans la zone de notification (même réglage que la case du menu).
#[tauri::command]
pub fn set_keep_in_tray(app: AppHandle, value: bool) -> Result<SettingsInfo, String> {
    update_settings(&app, |s| s.keep_in_tray = value);
    refresh_tray(&app, true);
    publish(&app)
}

/// Active ou coupe l'écran de lancement montré au clic sur Jouer.
#[tauri::command]
pub fn set_launch_screen(app: AppHandle, value: bool) -> Result<SettingsInfo, String> {
    update_settings(&app, |s| s.launch_screen = value);
    publish(&app)
}

/// Active ou désactive un type de notification Windows.
#[tauri::command]
pub fn set_notification(app: AppHandle, kind: crate::notify::Kind, value: bool) -> Result<SettingsInfo, String> {
    update_settings(&app, |s| s.notifications.set(kind, value));
    publish(&app)
}

/// Statut Discord : réglage enregistré puis repris par le fil `ludex-discord` (activité posée ou
/// effacée tout de suite si une partie est en cours).
#[tauri::command]
pub fn set_discord_presence(app: AppHandle, value: bool) -> Result<SettingsInfo, String> {
    update_settings(&app, |s| s.discord_presence = value);
    refresh_discord(&app);
    publish(&app)
}

/// Statut Discord sans le nom du jeu (« En jeu »).
#[tauri::command]
pub fn set_discord_hide_name(app: AppHandle, value: bool) -> Result<SettingsInfo, String> {
    update_settings(&app, |s| s.discord_hide_name = value);
    refresh_discord(&app);
    publish(&app)
}

/// Le fil du statut Discord relit les réglages (aussi après un import).
pub fn refresh_discord<R: Runtime>(app: &AppHandle<R>) {
    if let Some(presence) = app.try_state::<crate::discord::Presence>() {
        presence.refresh();
    }
}

/// Active ou désactive un launcher, puis rescanne : la bibliothèque relue est renvoyée.
#[tauri::command]
pub async fn set_source_enabled(app: AppHandle, platform: Platform, value: bool) -> Result<ScanResult, String> {
    let handle = app.clone();
    let scan = blocking(move || {
        let state = handle.state::<AppState>();
        let scanning = wait_scan(&state);
        let ((), scan) = save_and_rescan(&handle, &state, scanning, |data| {
            let list = &mut data.settings.disabled_sources;
            list.retain(|p| *p != platform);
            if !value {
                list.push(platform);
            }
        })?;
        Ok(scan)
    })
    .await?;
    publish(&app)?;
    Ok(scan)
}

/// Ré-affiche tous les jeux masqués par l'utilisateur ; renvoie leurs ids, pour le front.
#[tauri::command]
pub fn show_all_hidden(app: AppHandle) -> Result<Vec<String>, String> {
    let state = app.state::<AppState>();
    let ids = {
        let mut data = lock(&state.data)?;
        let ids = data.show_all_hidden();
        if ids.is_empty() {
            return Ok(ids);
        }
        data.save(&state.data_path)?;
        ids
    };
    for g in lock(&state.games)?.iter_mut().filter(|g| ids.contains(&g.id)) {
        g.hidden = false;
    }
    refresh_tray(&app, false);
    publish(&app)?;
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn themes_map_to_window_themes() {
        assert_eq!(window_theme(Theme::Dark), Some(tauri::Theme::Dark));
        assert_eq!(window_theme(Theme::Light), Some(tauri::Theme::Light));
        assert_eq!(window_theme(Theme::System), None);
        assert_eq!(window_theme(Theme::Oled), Some(tauri::Theme::Dark));
        assert_eq!(window_theme(Theme::Contrast), Some(tauri::Theme::Dark));
    }

    #[test]
    fn info_is_sent_in_camel_case() {
        let info = SettingsInfo {
            theme: Theme::System,
            accent: Accent::Pink,
            language: Language::En,
            language_resolved: Lang::En,
            quick_shortcut: "Alt+Space".into(),
            quick_shortcut_label: quick::label("Alt+Space"),
            launch_at_startup: false,
            keep_in_tray: true,
            saves_dir: String::new(),
            auto_saves: true,
            launch_screen: false,
            disabled_sources: vec![Platform::Epic],
            hidden_games: 2,
            version: "0.2.0".into(),
            notifications: crate::notify::Prefs { news: false, ..Default::default() },
            notifications_blocked: true,
            discord_presence: true,
            discord_hide_name: false,
            discord_available: false,
        };
        let text = serde_json::to_string(&info).unwrap();
        for part in [r#""theme":"system""#, r#""accent":"pink""#, r#""language":"en""#, r#""languageResolved":"en""#, r#""quickShortcutLabel":"Alt+Espace""#, r#""disabledSources":["epic"]"#, r#""hiddenGames":2"#, r#""launchScreen":false"#, r#""news":false"#, r#""notificationsBlocked":true"#, r#""discordPresence":true"#, r#""discordHideName":false"#, r#""discordAvailable":false"#] {
            assert!(text.contains(part), "{text}");
        }
    }
}
