//! Recherche rapide : un raccourci clavier global (Alt+Espace par défaut, réglage `quickShortcut`)
//! ouvre, par-dessus tout, une petite fenêtre de recherche (`quick.html`). Quelques lettres, flèches,
//! Entrée lance le jeu, Échap ou un clic ailleurs la cache. Le plugin `global-shortcut` n'est
//! utilisé que d'ici : le front n'a aucune de ses permissions.
//!
//! La fenêtre est créée cachée au démarrage (premier affichage immédiat), sans décorations,
//! toujours au premier plan, absente de la barre des tâches, et cachée dès qu'elle perd le focus.

use tauri::webview::Color;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Modifiers, Shortcut, ShortcutEvent, ShortcutState};

/// Identifiant de la fenêtre (capability `default`, comme la fenêtre principale : `core:default` seul).
pub const LABEL: &str = "quick";
/// Raccourci par défaut, au format du plugin.
pub const DEFAULT_SHORTCUT: &str = "Alt+Space";
/// Événement `quick-show` : la fenêtre vient d'apparaître, le front relit les jeux et vide la recherche.
pub const SHOW_EVENT: &str = "quick-show";

/// Taille de la fenêtre, en pixels logiques.
const WIDTH: f64 = 640.0;
const HEIGHT: f64 = 448.0;
/// Hauteur du haut de la fenêtre dans la zone de travail de l'écran : un peu au-dessus du centre.
const TOP_RATIO: f64 = 0.22;

/// Raccourci lu dans les réglages : `None` si vide (désactivé). Refusé sans Ctrl, Alt ni Windows :
/// une lettre seule ou Maj+lettre capturerait la frappe dans toutes les applis.
pub fn parse(text: &str) -> Result<Option<Shortcut>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let shortcut: Shortcut = text.parse().map_err(|_| format!("Raccourci {} non reconnu.", label(text)))?;
    if !shortcut.mods.intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER) {
        return Err(format!("Le raccourci {} doit contenir Ctrl, Alt ou Windows.", label(text)));
    }
    Ok(Some(shortcut))
}

/// Raccourci au format du plugin rendu lisible : « Ctrl+Maj+L », « Alt+Espace ». Vide : « Désactivé ».
pub fn label(text: &str) -> String {
    let text = text.trim();
    if text.is_empty() {
        return "Désactivé".into();
    }
    text.split('+')
        .map(|part| {
            let upper = part.trim().to_ascii_uppercase();
            let named = match upper.as_str() {
                "CTRL" | "CONTROL" => "Ctrl",
                "ALT" | "OPTION" => "Alt",
                "SHIFT" => "Maj",
                "SUPER" | "META" | "CMD" | "COMMAND" => "Windows",
                "SPACE" => "Espace",
                "ENTER" => "Entrée",
                "ESCAPE" | "ESC" => "Échap",
                "BACKSPACE" => "Retour arrière",
                "DELETE" => "Suppr",
                "INSERT" => "Inser",
                "HOME" => "Début",
                "END" => "Fin",
                "PAGEUP" => "Page préc.",
                "PAGEDOWN" => "Page suiv.",
                "ARROWUP" | "UP" => "Haut",
                "ARROWDOWN" | "DOWN" => "Bas",
                "ARROWLEFT" | "LEFT" => "Gauche",
                "ARROWRIGHT" | "RIGHT" => "Droite",
                _ => "",
            };
            if !named.is_empty() {
                return named.to_string();
            }
            let p = part.trim();
            for prefix in ["Key", "Digit"] {
                if let Some(rest) = p.get(..prefix.len()).filter(|s| s.eq_ignore_ascii_case(prefix)).and(p.get(prefix.len()..))
                    && rest.len() == 1
                {
                    return rest.to_ascii_uppercase();
                }
            }
            if let Some(rest) = p.get(..6).filter(|s| s.eq_ignore_ascii_case("Numpad")).and(p.get(6..)) {
                return format!("Pavé {rest}");
            }
            p.to_string()
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// Change le raccourci à chaud : l'ancien est retiré, le nouveau enregistré. Refusé (illisible, ou
/// déjà pris par une autre appli ou par Windows) : l'ancien est remis et l'erreur renvoyée.
/// `previous` : raccourci des réglages, remis en cas d'échec.
pub fn replace<R: Runtime>(app: &AppHandle<R>, text: &str, previous: &str) -> Result<(), String> {
    let shortcut = parse(text)?;
    let manager = app.global_shortcut();
    let _ = manager.unregister_all();
    let Some(shortcut) = shortcut else { return Ok(()) };
    if manager.register(shortcut).is_err() {
        resume(app, previous);
        return Err(format!(
            "Le raccourci {} est déjà pris par une autre application ou par Windows. Essaie une autre combinaison.",
            label(text)
        ));
    }
    Ok(())
}

/// Coupe le raccourci pendant sa saisie dans les Paramètres : sinon la combinaison actuelle
/// ouvrirait la recherche rapide au lieu d'arriver dans la page.
pub fn pause<R: Runtime>(app: &AppHandle<R>) {
    let _ = app.global_shortcut().unregister_all();
}

/// Remet le raccourci des réglages (fin de la saisie, ou échec d'un changement).
pub fn resume<R: Runtime>(app: &AppHandle<R>, text: &str) {
    let manager = app.global_shortcut();
    let _ = manager.unregister_all();
    if let Ok(Some(shortcut)) = parse(text)
        && let Err(e) = manager.register(shortcut)
    {
        eprintln!("Recherche rapide désactivée : raccourci « {text} » non remis ({e}).");
    }
}

/// Position du coin haut gauche de la fenêtre (`w` × `h`) dans la zone de travail d'un écran
/// (`x`, `y`, `aw` × `ah`), en pixels physiques : centrée en largeur, un peu haute, jamais dehors.
pub fn place(x: i32, y: i32, aw: u32, ah: u32, w: u32, h: u32) -> (i32, i32) {
    let left = x + (i64::from(aw) - i64::from(w)).max(0) as i32 / 2;
    let room = (i64::from(ah) - i64::from(h)).max(0) as f64;
    (left, y + (room * TOP_RATIO).round() as i32)
}

/// Enregistre le raccourci des réglages et prépare la fenêtre. Un raccourci illisible ou déjà pris
/// par une autre appli (PowerToys Run utilise aussi Alt+Espace) : message console, Ludex continue.
pub fn setup<R: Runtime>(app: &AppHandle<R>, text: &str) {
    let shortcut = match parse(text) {
        Ok(Some(s)) => s,
        Ok(None) => return,
        Err(e) => return eprintln!("Recherche rapide désactivée : {e}"),
    };
    if let Err(e) = app.global_shortcut().register(shortcut) {
        return eprintln!("Recherche rapide désactivée : le raccourci « {text} » est peut-être déjà pris par une autre application ({e}).");
    }
    if let Err(e) = create(app) {
        eprintln!("Fenêtre de recherche rapide non créée : {e}");
    }
}

/// Appelé par le plugin pour tout raccourci enregistré (un seul : celui des réglages).
pub fn on_shortcut<R: Runtime>(app: &AppHandle<R>, _shortcut: &Shortcut, event: ShortcutEvent) {
    if event.state() == ShortcutState::Pressed {
        toggle(app);
    }
}

fn create<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<WebviewWindow<R>> {
    WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("quick.html".into()))
        .title("Ludex : recherche rapide")
        .inner_size(WIDTH, HEIGHT)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .shadow(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .center()
        .theme(crate::settings::window_theme(crate::settings::current_theme(app)))
        .background_color(Color(0x1A, 0x1A, 0x1A, 0xFF))
        .build()
}

/// Affiche la fenêtre sur l'écran de la souris, ou la cache si elle est déjà là.
pub fn toggle<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(LABEL) else {
        // Fenêtre fermée par Windows ou jamais créée : recréée hors du fil principal (une création
        // depuis le fil de la boucle d'événements peut bloquer sous Windows), puis affichée.
        let app = app.clone();
        std::thread::spawn(move || match create(&app) {
            Ok(window) => show(&app, &window),
            Err(e) => eprintln!("Fenêtre de recherche rapide non créée : {e}"),
        });
        return;
    };
    if window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false) {
        let _ = window.hide();
    } else {
        show(app, &window);
    }
}

fn show<R: Runtime>(app: &AppHandle<R>, window: &WebviewWindow<R>) {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    match monitor {
        Some(m) => {
            let area = m.work_area();
            let scale = m.scale_factor();
            let (w, h) = ((WIDTH * scale).round() as u32, (HEIGHT * scale).round() as u32);
            let (x, y) = place(area.position.x, area.position.y, area.size.width, area.size.height, w, h);
            let _ = window.set_position(PhysicalPosition::new(x, y));
            // Écran d'une autre échelle : la taille logique est reprise après le déplacement.
            let _ = window.set_size(LogicalSize::new(WIDTH, HEIGHT));
        }
        None => {
            let _ = window.center();
        }
    }
    let _ = window.show();
    let _ = window.set_focus();
    let _ = app.emit_to(LABEL, SHOW_EVENT, ());
}

/// Cache la fenêtre (Échap, jeu lancé, perte du focus).
pub fn hide<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri_plugin_global_shortcut::Code;

    #[test]
    fn default_shortcut_is_alt_space() {
        let s = parse(DEFAULT_SHORTCUT).unwrap().unwrap();
        assert_eq!(s.mods, Modifiers::ALT);
        assert_eq!(s.key, Code::Space);
    }

    #[test]
    fn shortcuts_are_read_whatever_the_case() {
        let s = parse(" ctrl+shift+L ").unwrap().unwrap();
        assert_eq!(s.mods, Modifiers::CONTROL | Modifiers::SHIFT);
        assert_eq!(s.key, Code::KeyL);
        assert_eq!(parse("Super+G").unwrap().unwrap().mods, Modifiers::SUPER);
        assert_eq!(parse("Alt+F1").unwrap().unwrap().key, Code::F1);
    }

    #[test]
    fn empty_setting_disables_the_shortcut() {
        assert_eq!(parse("").unwrap(), None);
        assert_eq!(parse("   ").unwrap(), None);
    }

    #[test]
    fn unreadable_shortcuts_are_refused() {
        for text in ["Alt+", "Alt+Truc", "Ctrl+A+B", "Ctrl++"] {
            assert!(parse(text).is_err(), "{text}");
        }
    }

    #[test]
    fn shortcuts_without_ctrl_alt_or_windows_are_refused() {
        for text in ["A", "Space", "Shift+A", "F5"] {
            let err = parse(text).unwrap_err();
            assert!(err.contains("Ctrl, Alt ou Windows"), "{text} : {err}");
        }
    }

    #[test]
    fn labels_are_readable() {
        assert_eq!(label("Alt+Space"), "Alt+Espace");
        assert_eq!(label("Ctrl+Shift+KeyL"), "Ctrl+Maj+L");
        assert_eq!(label("super+digit1"), "Windows+1");
        assert_eq!(label("Control+Alt+ArrowUp"), "Ctrl+Alt+Haut");
        assert_eq!(label("Alt+Numpad5"), "Alt+Pavé 5");
        assert_eq!(label("Ctrl+F12"), "Ctrl+F12");
        assert_eq!(label("  "), "Désactivé");
        // Format envoyé par la page Paramètres (`KeyboardEvent.code`) : lu par le plugin.
        assert_eq!(parse("Ctrl+Alt+KeyG").unwrap().unwrap().key, Code::KeyG);
        assert_eq!(parse("Alt+Digit3").unwrap().unwrap().key, Code::Digit3);
        assert_eq!(parse("Ctrl+Shift+Comma").unwrap().unwrap().key, Code::Comma);
    }

    #[test]
    fn window_is_centered_and_a_bit_high() {
        // Écran 1920 × 1040 utiles, fenêtre 640 × 448 : 1040 - 448 = 592 de marge, 22 % au-dessus.
        assert_eq!(place(0, 0, 1920, 1040, 640, 448), (640, 130));
        // Second écran à gauche, avec une zone de travail décalée (barre des tâches en haut).
        assert_eq!(place(-2560, 40, 2560, 1400, 960, 672), (-1760, 40 + 160));
    }

    #[test]
    fn window_never_leaves_a_small_screen() {
        assert_eq!(place(100, 50, 600, 400, 640, 448), (100, 50));
    }
}
