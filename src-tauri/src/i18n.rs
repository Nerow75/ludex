//! Langue de Ludex : français (langue de référence) ou anglais. Réglage `language` de
//! `user-data.json` (`system` par défaut : la langue d'affichage de Windows, anglais si ce n'est pas
//! le français), lu au démarrage et changé par `set_language` (`settings.rs`).
//!
//! Tout texte qui atteint l'utilisateur (erreur renvoyée au front, notification, menu de la zone de
//! notification, rapport de diagnostic, titre d'un dialogue natif, nom de fichier proposé) passe par
//! `tr!` : les deux textes côte à côte, choisis au moment de l'appel. Les messages de la console
//! (`eprintln!`) restent en français. Les requêtes réseau qui ont une langue (Steam) suivent
//! `steam_language`.

use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::sync::atomic::{AtomicU8, Ordering};

/// Réglage de l'utilisateur (Paramètres, Apparence).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// Langue d'affichage de Windows.
    #[default]
    System,
    Fr,
    En,
}

/// Langue effective de l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    Fr,
    En,
}

impl Lang {
    /// Langue de Steam pour ses API (`l=`, `language=`).
    pub fn steam(self) -> &'static str {
        match self {
            Lang::Fr => "french",
            Lang::En => "english",
        }
    }

    /// Code court, pour les noms des caches propres à une langue.
    pub fn code(self) -> &'static str {
        match self {
            Lang::Fr => "fr",
            Lang::En => "en",
        }
    }
}

const FR: u8 = 0;
const EN: u8 = 1;

static CURRENT: AtomicU8 = AtomicU8::new(FR);

thread_local! {
    /// Langue imposée sur ce fil (tests : jamais de langue partagée entre deux tests parallèles).
    static FORCED: Cell<Option<Lang>> = const { Cell::new(None) };
}

/// Langue effective d'un réglage : `system` = celle de Windows.
pub fn resolve(language: Language) -> Lang {
    match language {
        Language::Fr => Lang::Fr,
        Language::En => Lang::En,
        Language::System => system(),
    }
}

/// Langue d'affichage de Windows : français s'il l'est, anglais sinon.
pub fn system() -> Lang {
    #[cfg(windows)]
    {
        // SAFETY : fonction sans argument ni effet, toujours disponible.
        let id = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
        from_langid(id)
    }
    #[cfg(not(windows))]
    {
        Lang::Fr
    }
}

/// LANGID de Windows : langue principale sur les 10 bits du bas (0x0C = français, toutes régions).
pub fn from_langid(id: u16) -> Lang {
    if id & 0x3FF == 0x0C { Lang::Fr } else { Lang::En }
}

/// Change la langue de tout Ludex (démarrage, Paramètres).
pub fn set(lang: Lang) {
    CURRENT.store(if lang == Lang::En { EN } else { FR }, Ordering::Relaxed);
}

/// Langue effective actuelle.
pub fn current() -> Lang {
    if let Some(lang) = FORCED.with(Cell::get) {
        return lang;
    }
    if CURRENT.load(Ordering::Relaxed) == EN { Lang::En } else { Lang::Fr }
}

/// Vrai si Ludex parle anglais.
pub fn english() -> bool {
    current() == Lang::En
}

/// Langue de Steam actuelle (« french » ou « english »).
pub fn steam_language() -> &'static str {
    current().steam()
}

/// Exécute `f` avec une langue imposée sur ce fil seulement (tests).
#[cfg(test)]
pub fn with<T>(lang: Lang, f: impl FnOnce() -> T) -> T {
    let before = FORCED.with(|c| c.replace(Some(lang)));
    let out = f();
    FORCED.with(|c| c.set(before));
    out
}

/// Texte dans la langue actuelle, une `String` formatée comme par `format!` dans chaque langue :
/// `tr!("Jeu introuvable.", "Game not found.")`, `tr!("{n} jeux", "{n} games", n = count)`, ou
/// avec des arguments implicites (`{title}` d'une variable `title` de la portée).
#[macro_export]
macro_rules! tr {
    ($fr:literal, $en:literal $(,)?) => {
        if $crate::i18n::english() { format!($en) } else { format!($fr) }
    };
    ($fr:literal, $en:literal, $($args:tt)+) => {
        if $crate::i18n::english() { format!($en, $($args)+) } else { format!($fr, $($args)+) }
    };
}

/// Texte fixe dans la langue actuelle, sans formatage : une `&'static str`
/// (`tr_str!("Ouvrir Ludex", "Open Ludex")`), là où une `String` ne convient pas.
#[macro_export]
macro_rules! tr_str {
    ($fr:literal, $en:literal $(,)?) => {
        if $crate::i18n::english() { $en } else { $fr }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_language_ids() {
        assert_eq!(from_langid(0x040C), Lang::Fr); // France
        assert_eq!(from_langid(0x0C0C), Lang::Fr); // Canada
        assert_eq!(from_langid(0x100C), Lang::Fr); // Suisse
        assert_eq!(from_langid(0x0409), Lang::En); // États-Unis
        assert_eq!(from_langid(0x0809), Lang::En); // Royaume-Uni
        assert_eq!(from_langid(0x0407), Lang::En); // Allemand : anglais
    }

    #[test]
    fn explicit_settings_win_over_windows() {
        assert_eq!(resolve(Language::Fr), Lang::Fr);
        assert_eq!(resolve(Language::En), Lang::En);
    }

    #[test]
    fn texts_follow_the_language() {
        let title = "Hades";
        let n = 3;
        with(Lang::Fr, || {
            assert_eq!(tr!("Jouer", "Play"), "Jouer");
            assert_eq!(tr!("Jeu « {title} » lancé", "{title} launched"), "Jeu « Hades » lancé");
            assert_eq!(tr!("{} jeux", "{} games", n), "3 jeux");
            assert_eq!(tr_str!("Annulé.", "Canceled."), "Annulé.");
            assert_eq!(steam_language(), "french");
        });
        with(Lang::En, || {
            assert_eq!(tr!("Jouer", "Play"), "Play");
            assert_eq!(tr!("Jeu « {title} » lancé", "{title} launched"), "Hades launched");
            assert_eq!(tr!("{n} jeux", "{n} games", n = n + 1), "4 games");
            assert_eq!(steam_language(), "english");
        });
    }

    #[test]
    fn language_setting_is_read_safely() {
        #[derive(Deserialize)]
        struct S {
            language: Language,
        }
        let read: S = serde_json::from_str(r#"{"language":"en"}"#).unwrap();
        assert_eq!(read.language, Language::En);
        assert_eq!(serde_json::to_string(&Language::System).unwrap(), r#""system""#);
    }
}
