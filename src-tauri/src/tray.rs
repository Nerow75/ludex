//! Menu de l'icône de la zone de notification : derniers jeux joués (comme la rangée « Reprendre »
//! du front), ouverture de Ludex, réglages (lancer avec Windows, garder Ludex en fond à la
//! fermeture) et sortie. Le menu est décrit par des `Entry`, construits sans Tauri pour être testés.

use crate::library::Game;
use std::cmp::Reverse;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::{AppHandle, Runtime};

pub const TRAY_ID: &str = "ludex";
/// Jeux récents proposés dans le menu.
pub const RECENT_COUNT: usize = 5;
/// Argument passé par le lancement avec Windows : Ludex démarre caché dans la zone de notification.
pub const MINIMIZED_ARG: &str = "--minimized";

const GAME_PREFIX: &str = "game:";
const OPEN: &str = "open";
const AUTOSTART: &str = "autostart";
const KEEP_IN_TRAY: &str = "keep-in-tray";
const QUIT: &str = "quit";
const NO_RECENT: &str = "no-recent";
/// Au-delà, le titre est coupé : le menu garde une largeur raisonnable.
const MAX_LABEL: usize = 48;

/// Élément du menu, dans l'ordre d'affichage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Item { id: String, label: String, enabled: bool },
    Check { id: String, label: String, checked: bool },
    Separator,
}

/// Choix fait dans le menu.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Launch(String),
    Open,
    ToggleAutostart,
    ToggleKeepInTray,
    Quit,
}

/// Titre sans ® ni ™, en minuscules et sans accents : départage les jeux lancés à la même seconde,
/// comme le tri par titre du front.
fn title_key(title: &str) -> String {
    title.replace(['®', '™'], "").trim().chars().flat_map(char::to_lowercase).map(fold_accent).collect()
}

fn fold_accent(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        _ => c,
    }
}

/// Les `n` derniers jeux joués, hors jeux masqués : même logique que `recentGames` du front.
pub fn recent(games: &[Game], n: usize) -> Vec<&Game> {
    let mut list: Vec<&Game> = games.iter().filter(|g| !g.hidden && g.last_played.is_some_and(|t| t > 0)).collect();
    list.sort_by_cached_key(|g| (Reverse(g.last_played), title_key(&g.title)));
    list.truncate(n);
    list
}

/// Libellé d'un jeu : `&` doublé (sinon Windows le prend pour un raccourci clavier), titre coupé
/// s'il est trop long.
fn game_label(title: &str) -> String {
    let title = title.trim();
    let short = if title.chars().count() > MAX_LABEL {
        format!("{}…", title.chars().take(MAX_LABEL - 1).collect::<String>().trim_end())
    } else {
        title.to_string()
    };
    short.replace('&', "&&")
}

fn item(id: &str, label: &str) -> Entry {
    Entry::Item { id: id.to_string(), label: label.to_string(), enabled: true }
}

/// Contenu du menu : jeux récents, puis Ludex et ses réglages, puis « Quitter ».
pub fn entries(recent: &[&Game], autostart: bool, keep_in_tray: bool) -> Vec<Entry> {
    let mut list: Vec<Entry> = recent
        .iter()
        .map(|g| Entry::Item { id: format!("{GAME_PREFIX}{}", g.id), label: game_label(&g.title), enabled: true })
        .collect();
    if list.is_empty() {
        list.push(Entry::Item { id: NO_RECENT.to_string(), label: "Aucune partie récente".to_string(), enabled: false });
    }
    list.extend([
        Entry::Separator,
        item(OPEN, "Ouvrir Ludex"),
        Entry::Check { id: AUTOSTART.to_string(), label: "Lancer avec Windows".to_string(), checked: autostart },
        Entry::Check {
            id: KEEP_IN_TRAY.to_string(),
            label: "Garder Ludex dans la zone de notification à la fermeture".to_string(),
            checked: keep_in_tray,
        },
        Entry::Separator,
        item(QUIT, "Quitter"),
    ]);
    list
}

/// Action correspondant à l'identifiant d'un élément cliqué.
pub fn action(id: &str) -> Option<Action> {
    if let Some(game) = id.strip_prefix(GAME_PREFIX) {
        return Some(Action::Launch(game.to_string()));
    }
    match id {
        OPEN => Some(Action::Open),
        AUTOSTART => Some(Action::ToggleAutostart),
        KEEP_IN_TRAY => Some(Action::ToggleKeepInTray),
        QUIT => Some(Action::Quit),
        _ => None,
    }
}

/// Menu Tauri construit à partir des éléments.
pub fn build<R: Runtime>(app: &AppHandle<R>, entries: &[Entry]) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    for entry in entries {
        match entry {
            Entry::Item { id, label, enabled } => menu.append(&MenuItem::with_id(app, id, label, *enabled, None::<&str>)?)?,
            Entry::Check { id, label, checked } => {
                menu.append(&CheckMenuItem::with_id(app, id, label, true, *checked, None::<&str>)?)?
            }
            Entry::Separator => menu.append(&PredefinedMenuItem::separator(app)?)?,
        }
    }
    Ok(menu)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{Launch, Platform};

    fn game(id: &str, title: &str, last: Option<u64>) -> Game {
        let mut g = Game::new(Platform::Steam, id, title, Launch::Uri(String::new()));
        g.last_played = last;
        g
    }

    fn ids(list: &[&Game]) -> Vec<String> {
        list.iter().map(|g| g.id.clone()).collect()
    }

    #[test]
    fn recent_keeps_five_most_recent_visible_games() {
        let mut games: Vec<Game> = (1..=7).map(|i| game(&i.to_string(), &format!("Jeu {i}"), Some(i * 100))).collect();
        games.push(game("0", "Jamais lancé", None));
        games.push(game("zero", "Date nulle", Some(0)));
        let mut hidden = game("99", "Masqué", Some(10_000));
        hidden.hidden = true;
        games.push(hidden);

        let list = recent(&games, RECENT_COUNT);
        assert_eq!(ids(&list), ["steam:7", "steam:6", "steam:5", "steam:4", "steam:3"]);
    }

    #[test]
    fn recent_ties_are_sorted_by_title() {
        let games = vec![game("b", "Zelda", Some(50)), game("a", "Éclipse™", Some(50)), game("c", "alan wake", Some(50))];
        assert_eq!(ids(&recent(&games, 5)), ["steam:c", "steam:a", "steam:b"]);
    }

    #[test]
    fn labels_escape_ampersand_and_cut_long_titles() {
        assert_eq!(game_label("Ratchet & Clank"), "Ratchet && Clank");
        let long = game_label(&"a".repeat(80));
        assert_eq!(long.chars().count(), MAX_LABEL);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn menu_lists_games_then_settings_then_quit() {
        let g = game("730", "Counter-Strike 2", Some(1));
        let list = entries(&[&g], true, false);
        assert_eq!(list[0], Entry::Item { id: "game:steam:730".into(), label: "Counter-Strike 2".into(), enabled: true });
        assert_eq!(list[1], Entry::Separator);
        assert_eq!(list[2], item(OPEN, "Ouvrir Ludex"));
        assert!(matches!(&list[3], Entry::Check { id, checked: true, .. } if id == AUTOSTART));
        assert!(matches!(&list[4], Entry::Check { id, checked: false, .. } if id == KEEP_IN_TRAY));
        assert_eq!(list[5], Entry::Separator);
        assert_eq!(list[6], item(QUIT, "Quitter"));
        assert_eq!(list.len(), 7);
    }

    #[test]
    fn empty_menu_says_no_recent_game() {
        let list = entries(&[], false, true);
        assert!(matches!(&list[0], Entry::Item { enabled: false, .. }));
        assert_eq!(list.len(), 7);
    }

    #[test]
    fn every_entry_maps_to_its_action() {
        let g = game("730", "CS2", Some(1));
        let actions: Vec<Option<Action>> = entries(&[&g], false, true)
            .into_iter()
            .filter_map(|e| match e {
                Entry::Item { id, .. } | Entry::Check { id, .. } => Some(action(&id)),
                Entry::Separator => None,
            })
            .collect();
        assert_eq!(
            actions,
            [
                Some(Action::Launch("steam:730".into())),
                Some(Action::Open),
                Some(Action::ToggleAutostart),
                Some(Action::ToggleKeepInTray),
                Some(Action::Quit)
            ]
        );
        assert_eq!(action(NO_RECENT), None);
    }
}
