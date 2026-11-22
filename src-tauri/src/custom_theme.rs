//! Thèmes perso et fond du mode canapé (écran Paramètres > Apparence).
//!
//! Un thème perso = un thème de base (sombre, clair, OLED, contraste élevé) et quatre couleurs
//! choisies (fond, surfaces, texte, accent). Les autres tokens (`--lx-surface-2`, bordures,
//! textes secondaires, accent corrigé pour le contraste…) sont calculés par le front
//! (`src/lib/customTheme.ts`) et posés en variables CSS sur `<html>`. Côté Rust, seulement des
//! valeurs fermées : couleurs `#RRGGBB` (jamais de CSS libre), nom nettoyé, base connue, id
//! `t-<base 36>` ; tout est revérifié à la relecture de `user-data.json` et à l'import.

use crate::library::manual::clean_text;
use crate::settings::{self, SettingsInfo};
use crate::store::Settings;
use crate::{AppState, lock};
use serde::{Deserialize, Deserializer, Serialize};
use tauri::{AppHandle, Manager};

/// Thèmes perso gardés au plus.
pub const MAX_THEMES: usize = 20;
/// Longueur d'un nom de thème, en caractères.
pub const MAX_NAME: usize = 40;
/// Au-delà, un texte venu du front est refusé avant tout nettoyage.
const MAX_INPUT: usize = 1024;

/// Thème de base d'un thème perso : point de départ des couleurs, seuils de contraste (7:1 en
/// contraste élevé) et règles en plus d'`app.css` (`data-theme`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Base {
    #[default]
    Dark,
    Light,
    Oled,
    Contrast,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomTheme {
    pub id: String,
    pub name: String,
    pub base: Base,
    /// Fond (`--lx-bg`), `#RRGGBB`.
    pub bg: String,
    /// Surfaces (`--lx-surface`).
    pub surface: String,
    /// Texte principal (`--lx-text`).
    pub text: String,
    /// Accent (`--lx-accent`, avant correction du contraste).
    pub accent: String,
}

/// Thème saisi dans l'éditeur : sans id, un nouveau thème.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeInput {
    pub id: Option<String>,
    pub name: String,
    pub base: Base,
    pub bg: String,
    pub surface: String,
    pub text: String,
    pub accent: String,
}

/// Fond animé du mode canapé. Fixe par défaut (image du jeu sans mouvement, comme avant) ; coupé
/// par `prefers-reduced-motion` côté front quel que soit le choix.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CouchBackdrop {
    #[default]
    Still,
    /// Dérive lente de l'image du jeu.
    Drift,
    /// Particules discrètes dans la couleur du jeu.
    Particles,
    /// Dégradé qui respire dans la couleur du jeu.
    Breathe,
}

/// Couleur `#RRGGBB` (dièse facultatif, casse ignorée), rendue en majuscules ; rien d'autre.
pub fn clean_hex(raw: &str) -> Option<String> {
    let digits = raw.trim().strip_prefix('#').unwrap_or(raw.trim());
    (digits.len() == 6 && digits.bytes().all(|b| b.is_ascii_hexdigit())).then(|| format!("#{}", digits.to_ascii_uppercase()))
}

/// Id d'un thème : `t-` puis 1 à 13 chiffres ou lettres minuscules (base 36 d'un `u64`).
pub fn valid_id(id: &str) -> bool {
    id.strip_prefix("t-").is_some_and(|rest| (1..=13).contains(&rest.len()) && rest.bytes().all(|b| b.is_ascii_digit() || b.is_ascii_lowercase()))
}

/// Nom nettoyé (contrôles et invisibles retirés, espaces regroupés, `MAX_NAME` caractères).
pub fn clean_name(raw: &str) -> Option<String> {
    clean_text(raw, MAX_NAME)
}

/// Thème entièrement revérifié ; `None` s'il manque quoi que ce soit.
pub fn clean(t: CustomTheme) -> Option<CustomTheme> {
    Some(CustomTheme {
        name: clean_name(&t.name)?,
        bg: clean_hex(&t.bg)?,
        surface: clean_hex(&t.surface)?,
        text: clean_hex(&t.text)?,
        accent: clean_hex(&t.accent)?,
        base: t.base,
        id: valid_id(&t.id).then_some(t.id)?,
    })
}

/// Liste revérifiée : entrées illisibles ou invalides écartées, ids uniques, `MAX_THEMES` au plus.
pub fn clean_list(list: impl IntoIterator<Item = CustomTheme>) -> Vec<CustomTheme> {
    let mut out: Vec<CustomTheme> = Vec::new();
    for t in list.into_iter().filter_map(clean) {
        if out.len() < MAX_THEMES && !out.iter().any(|k| k.id == t.id) {
            out.push(t);
        }
    }
    out
}

/// Relecture de `user-data.json` : une entrée illisible (version future, fichier modifié à la
/// main) est ignorée sans perdre le reste.
pub fn read_list<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<CustomTheme>, D::Error> {
    let raw = Vec::<serde_json::Value>::deserialize(d).unwrap_or_default();
    Ok(clean_list(raw.into_iter().filter_map(|v| serde_json::from_value(v).ok())))
}

/// Id libre `t-<base 36>` à partir de `seed` (l'heure en millisecondes).
pub fn new_id(taken: &[CustomTheme], seed: u64) -> String {
    let mut n = seed;
    loop {
        let id = format!("t-{}", base36(n));
        if !taken.iter().any(|t| t.id == id) {
            return id;
        }
        n = n.wrapping_add(1);
    }
}

fn base36(mut n: u64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    loop {
        out.push(DIGITS[(n % 36) as usize]);
        n /= 36;
        if n == 0 {
            break;
        }
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// Luminance relative WCAG d'une couleur `#RRGGBB` déjà nettoyée.
fn luminance(hex: &str) -> f64 {
    let channel = |i: usize| {
        let v = u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0) as f64 / 255.0;
        if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5)
}

/// Fond sombre : le blanc y contraste plus que le noir (fenêtre native et `color-scheme` sombres).
pub fn is_dark(hex: &str) -> bool {
    let l = luminance(hex);
    (1.05 / (l + 0.05)) >= ((l + 0.05) / 0.05)
}

/// Thème perso actif des réglages (l'id retenu doit exister).
pub fn active(s: &Settings) -> Option<&CustomTheme> {
    let id = s.custom_theme.as_deref()?;
    s.custom_themes.iter().find(|t| t.id == id)
}

/// Enregistre un thème saisi : remplace celui de même id, sinon l'ajoute. Renvoie son id.
pub fn upsert(s: &mut Settings, input: ThemeInput, seed: u64) -> Result<String, String> {
    if [&input.name, &input.bg, &input.surface, &input.text, &input.accent].iter().any(|v| v.len() > MAX_INPUT)
        || input.id.as_ref().is_some_and(|id| id.len() > MAX_INPUT)
    {
        return Err(tr!("Thème trop long.", "Theme too long."));
    }
    let name = clean_name(&input.name).ok_or_else(|| tr!("Donne un nom au thème.", "Give the theme a name."))?;
    let color = |v: &str| clean_hex(v).ok_or_else(|| tr!("Couleur invalide : « {v} » (attendu : #RRGGBB).", "Invalid color: \"{v}\" (expected #RRGGBB)."));
    let (bg, surface, text, accent) = (color(&input.bg)?, color(&input.surface)?, color(&input.text)?, color(&input.accent)?);
    let id = match input.id {
        Some(id) if s.custom_themes.iter().any(|t| t.id == id) => id,
        Some(_) => return Err(tr!("Thème introuvable.", "Theme not found.")),
        None if s.custom_themes.len() >= MAX_THEMES => {
            return Err(tr!("{MAX_THEMES} thèmes au plus : supprimes-en un d'abord.", "{MAX_THEMES} themes at most: delete one first."));
        }
        None => new_id(&s.custom_themes, seed),
    };
    let theme = CustomTheme { id: id.clone(), name, base: input.base, bg, surface, text, accent };
    match s.custom_themes.iter_mut().find(|t| t.id == id) {
        Some(known) => *known = theme,
        None => s.custom_themes.push(theme),
    }
    Ok(id)
}

/// Retire un thème ; actif, Ludex revient au thème de base des réglages.
pub fn remove(s: &mut Settings, id: &str) -> bool {
    let before = s.custom_themes.len();
    s.custom_themes.retain(|t| t.id != id);
    if s.custom_theme.as_deref() == Some(id) {
        s.custom_theme = None;
    }
    s.custom_themes.len() != before
}

/// Fusion à l'import : les thèmes de ce PC restent, ceux de l'archive s'ajoutent (même id ou même
/// nom, casse ignorée : déjà là), jusqu'à `MAX_THEMES`.
pub fn merge(here: &mut Vec<CustomTheme>, incoming: Vec<CustomTheme>) {
    for t in clean_list(incoming) {
        let lower = t.name.to_lowercase();
        if here.len() < MAX_THEMES && !here.iter().any(|k| k.id == t.id || k.name.to_lowercase() == lower) {
            here.push(t);
        }
    }
}

/// Change les réglages, les enregistre (erreur renvoyée), applique le thème des fenêtres et publie.
fn change<T>(app: &AppHandle, edit: impl FnOnce(&mut Settings) -> Result<T, String>) -> Result<SettingsInfo, String> {
    let state = app.state::<AppState>();
    let settings_now = {
        let mut data = lock(&state.data)?;
        let mut copy = data.settings.clone();
        edit(&mut copy)?;
        let previous = std::mem::replace(&mut data.settings, copy);
        if let Err(e) = data.save(&state.data_path) {
            data.settings = previous;
            return Err(e);
        }
        data.settings.clone()
    };
    settings::apply_theme(app, &settings_now);
    settings::publish(app)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Enregistre un thème de l'éditeur (nouveau ou modifié) et l'applique.
#[tauri::command]
pub fn save_custom_theme(app: AppHandle, theme: ThemeInput) -> Result<SettingsInfo, String> {
    change(&app, |s| {
        let id = upsert(s, theme, now_ms())?;
        s.custom_theme = Some(id);
        Ok(())
    })
}

/// Supprime un thème perso.
#[tauri::command]
pub fn delete_custom_theme(app: AppHandle, id: String) -> Result<SettingsInfo, String> {
    change(&app, |s| remove(s, &id).then_some(()).ok_or_else(|| tr!("Thème introuvable.", "Theme not found.")))
}

/// Applique un thème perso (`null` : retour au thème de base choisi).
#[tauri::command]
pub fn set_custom_theme(app: AppHandle, id: Option<String>) -> Result<SettingsInfo, String> {
    change(&app, |s| {
        if let Some(id) = &id
            && !s.custom_themes.iter().any(|t| &t.id == id)
        {
            return Err(tr!("Thème introuvable.", "Theme not found."));
        }
        s.custom_theme = id;
        Ok(())
    })
}

/// Fond du mode canapé.
#[tauri::command]
pub fn set_couch_backdrop(app: AppHandle, backdrop: CouchBackdrop) -> Result<SettingsInfo, String> {
    change(&app, |s| {
        s.couch_backdrop = backdrop;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme(id: &str, name: &str) -> CustomTheme {
        CustomTheme { id: id.into(), name: name.into(), base: Base::Dark, bg: "#101418".into(), surface: "#1B2128".into(), text: "#F2F2F2".into(), accent: "#E0A030".into() }
    }

    fn input(id: Option<&str>, name: &str) -> ThemeInput {
        ThemeInput { id: id.map(Into::into), name: name.into(), base: Base::Light, bg: "fafafa".into(), surface: "#eeeeee".into(), text: "#111111".into(), accent: "#3366CC".into() }
    }

    #[test]
    fn colors_are_strict_hex() {
        assert_eq!(clean_hex("#0b0b0b").as_deref(), Some("#0B0B0B"));
        assert_eq!(clean_hex(" 14b8a6 ").as_deref(), Some("#14B8A6"));
        for bad in ["", "#FFF", "#GGGGGG", "#1234567", "red", "rgb(0,0,0)", "#12345;", "var(--x)", "#ffffff; background: url(x)", "##123456", "#１２３４５６"] {
            assert_eq!(clean_hex(bad), None, "{bad}");
        }
    }

    #[test]
    fn ids_and_names_are_checked() {
        assert!(valid_id("t-0") && valid_id("t-lz3k9q1a"));
        for bad in ["", "t-", "c-12", "t-ABC", "t-a/b", "t-12345678901234", "t-é"] {
            assert!(!valid_id(bad), "{bad}");
        }
        assert_eq!(clean_name("  Mon \u{200B}thème\n  nuit ").as_deref(), Some("Mon thème nuit"));
        assert_eq!(clean_name(" \u{200B} "), None);
        assert_eq!(clean_name(&"x".repeat(200)).map(|n| n.chars().count()), Some(MAX_NAME));
    }

    #[test]
    fn list_is_reread_without_losing_the_rest() {
        let text = r##"{"settings":{"theme":"light","customTheme":"t-1","customThemes":[
            {"id":"t-1","name":"Nuit","base":"oled","bg":"#000000","surface":"#0a0a0a","text":"#ffffff","accent":"#ff8800"},
            {"id":"t-1","name":"Doublon","base":"dark","bg":"#000000","surface":"#0a0a0a","text":"#ffffff","accent":"#ff8800"},
            {"id":"t-2","name":"Base future","base":"sepia","bg":"#000000","surface":"#0a0a0a","text":"#ffffff","accent":"#ff8800"},
            {"id":"t-3","name":"CSS","base":"dark","bg":"red; color: blue","surface":"#0a0a0a","text":"#ffffff","accent":"#ff8800"},
            {"id":"../x","name":"Id","base":"dark","bg":"#000000","surface":"#0a0a0a","text":"#ffffff","accent":"#ff8800"},
            "illisible",
            {"id":"t-4","name":"  ","base":"dark","bg":"#000000","surface":"#0a0a0a","text":"#ffffff","accent":"#ff8800"}
        ],"couchBackdrop":"particles"}}"##;
        let data: crate::store::UserData = serde_json::from_str(text).unwrap();
        let s = &data.settings;
        assert_eq!(s.theme, crate::store::Theme::Light);
        assert_eq!(s.custom_themes.len(), 1);
        assert_eq!(s.custom_themes[0].surface, "#0A0A0A");
        assert_eq!(s.custom_themes[0].base, Base::Oled);
        assert_eq!(active(s).map(|t| t.name.as_str()), Some("Nuit"));
        assert_eq!(s.couch_backdrop, CouchBackdrop::Particles);
        // Fond inconnu (version future) : fixe, sans perdre le reste.
        let future: crate::store::UserData = serde_json::from_str(r#"{"favorites":["a"],"settings":{"couchBackdrop":"lasers","customThemes":"x"}}"#).unwrap();
        assert_eq!(future.settings.couch_backdrop, CouchBackdrop::Still);
        assert!(future.settings.custom_themes.is_empty() && future.favorites.contains("a"));
        // Rien de choisi : le fichier ne change pas de forme.
        let plain = serde_json::to_string(&crate::store::Settings::default()).unwrap();
        assert!(!plain.contains("customTheme") && plain.contains(r#""couchBackdrop":"still""#), "{plain}");
    }

    #[test]
    fn active_theme_needs_a_known_id() {
        let mut s = Settings { custom_themes: vec![theme("t-1", "Nuit")], custom_theme: Some("t-9".into()), ..Default::default() };
        assert!(active(&s).is_none());
        s.custom_theme = Some("t-1".into());
        assert_eq!(active(&s).map(|t| t.id.as_str()), Some("t-1"));
    }

    #[test]
    fn upsert_creates_replaces_and_refuses() {
        let mut s = Settings::default();
        let id = upsert(&mut s, input(None, " Jour  doux "), 42).unwrap();
        assert_eq!(id, "t-16");
        let t = &s.custom_themes[0];
        assert_eq!((t.name.as_str(), t.bg.as_str(), t.base), ("Jour doux", "#FAFAFA", Base::Light));
        // Même id : remplacé sur place.
        upsert(&mut s, input(Some(&id), "Jour"), 0).unwrap();
        assert_eq!(s.custom_themes.len(), 1);
        assert_eq!(s.custom_themes[0].name, "Jour");
        // Id inconnu, couleur ou nom invalides : rien ne change.
        assert!(upsert(&mut s, input(Some("t-zz"), "Autre"), 0).is_err());
        assert!(upsert(&mut s, ThemeInput { accent: "blue".into(), ..input(None, "Bleu") }, 0).is_err());
        assert!(upsert(&mut s, input(None, "\u{200B}"), 0).is_err());
        assert!(upsert(&mut s, ThemeInput { bg: "#".repeat(5000), ..input(None, "Long") }, 0).is_err());
        assert_eq!(s.custom_themes.len(), 1);
        // Plafond.
        for n in 0..MAX_THEMES {
            let _ = upsert(&mut s, input(None, &format!("T{n}")), 1000 + n as u64);
        }
        assert_eq!(s.custom_themes.len(), MAX_THEMES);
        assert!(upsert(&mut s, input(None, "De trop"), 9999).is_err());
    }

    #[test]
    fn remove_falls_back_to_the_base_theme() {
        let mut s = Settings { custom_themes: vec![theme("t-1", "Nuit"), theme("t-2", "Jour")], custom_theme: Some("t-1".into()), ..Default::default() };
        assert!(remove(&mut s, "t-1"));
        assert_eq!(s.custom_theme, None);
        assert!(!remove(&mut s, "t-1"));
        assert_eq!(s.custom_themes.len(), 1);
    }

    #[test]
    fn merge_keeps_this_pc_and_adds_new_names() {
        let mut here = vec![theme("t-1", "Nuit")];
        merge(&mut here, vec![theme("t-1", "Autre nom"), theme("t-2", "NUIT"), theme("t-3", "Forêt"), CustomTheme { bg: "bleu".into(), ..theme("t-4", "Cassé") }]);
        let names: Vec<&str> = here.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Nuit", "Forêt"]);
    }

    #[test]
    fn dark_backgrounds_are_detected() {
        assert!(is_dark("#000000") && is_dark("#0B0B0B") && is_dark("#1E2A44") && is_dark("#6B6B6B"));
        assert!(!is_dark("#FFFFFF") && !is_dark("#F5F0E6") && !is_dark("#A0A0A0"));
    }
}
