//! Préférences de l'utilisateur, gardées dans un fichier JSON du dossier de données de l'app :
//! favoris, jeux masqués ou ré-affichés, date du dernier lancement depuis Ludex, temps de jeu
//! mesuré par Ludex et réglages de l'application (zone de notification, lancement avec Windows).

use crate::library::{Game, PlaytimeSource};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UserData {
    pub favorites: BTreeSet<String>,
    /// Jeux masqués par l'utilisateur.
    pub hidden: BTreeSet<String>,
    /// Outils (masqués par défaut) que l'utilisateur a choisi d'afficher.
    pub shown: BTreeSet<String>,
    /// Dernier lancement depuis Ludex, en secondes Unix.
    pub launched: BTreeMap<String, u64>,
    /// Temps de jeu mesuré par Ludex, en secondes, pour les jeux dont le launcher ne compte rien.
    pub playtime: BTreeMap<String, u64>,
    pub settings: Settings,
}

/// Réglages de l'application, modifiés depuis le menu de la zone de notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Fermer la fenêtre la cache dans la zone de notification au lieu de quitter : le suivi des
    /// parties continue. Actif par défaut, « Quitter » du menu quitte vraiment.
    pub keep_in_tray: bool,
    /// Dernier choix « Lancer avec Windows ». L'entrée du registre fait foi, ce champ la reflète.
    pub launch_at_startup: bool,
    /// L'explication « Ludex tourne encore » a déjà été montrée à la première fermeture.
    pub tray_hint_shown: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { keep_in_tray: true, launch_at_startup: false, tray_hint_shown: false }
    }
}

impl UserData {
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Écrit dans un fichier temporaire puis le renomme : jamais de fichier à moitié écrit.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("Dossier de données inaccessible : {e}"))?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, text).map_err(|e| format!("Enregistrement impossible : {e}"))?;
        fs::rename(&tmp, path).map_err(|e| format!("Enregistrement impossible : {e}"))
    }

    pub fn set_favorite(&mut self, id: &str, value: bool) {
        if value {
            self.favorites.insert(id.to_string());
        } else {
            self.favorites.remove(id);
        }
    }

    pub fn set_hidden(&mut self, game: &Game, value: bool) {
        let id = game.id.clone();
        self.hidden.remove(&id);
        self.shown.remove(&id);
        match (value, game.is_tool) {
            (true, false) => drop(self.hidden.insert(id)),
            (false, true) => drop(self.shown.insert(id)),
            _ => {} // état par défaut : rien à retenir
        }
    }

    pub fn record_launch(&mut self, id: &str) -> u64 {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        self.launched.insert(id.to_string(), now);
        now
    }

    /// Ajoute une portion de session mesurée par Ludex.
    pub fn add_playtime(&mut self, id: &str, secs: u64) {
        if secs > 0 {
            let total = self.playtime.entry(id.to_string()).or_insert(0);
            *total = total.saturating_add(secs);
        }
    }

    /// Reporte les préférences sur un jeu détecté.
    pub fn apply(&self, game: &mut Game) {
        game.favorite = self.favorites.contains(&game.id);
        game.hidden = if game.is_tool { !self.shown.contains(&game.id) } else { self.hidden.contains(&game.id) };
        if let Some(&ts) = self.launched.get(&game.id) {
            game.last_played = Some(game.last_played.map_or(ts, |l| l.max(ts)));
        }
        // Le chiffre du launcher prime : celui de Ludex ne s'y ajoute jamais.
        if game.playtime_source != Some(PlaytimeSource::Launcher)
            && let Some(&secs) = self.playtime.get(&game.id)
        {
            game.playtime_minutes = Some((secs + 30) / 60);
            game.playtime_source = Some(PlaytimeSource::Ludex);
        }
    }
}

pub fn file_in(dir: PathBuf) -> PathBuf {
    dir.join("user-data.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{Launch, Platform};

    fn game(id: &str, tool: bool) -> Game {
        let mut g = Game::new(Platform::Steam, id, "Jeu", Launch::Uri(String::new()));
        g.is_tool = tool;
        g
    }

    #[test]
    fn tools_are_hidden_until_shown() {
        let mut data = UserData::default();
        let mut tool = game("431960", true);
        data.apply(&mut tool);
        assert!(tool.hidden);

        data.set_hidden(&tool, false);
        data.apply(&mut tool);
        assert!(!tool.hidden);
        assert!(data.hidden.is_empty());
    }

    #[test]
    fn games_can_be_hidden_and_favorited() {
        let mut data = UserData::default();
        let mut g = game("730", false);
        data.set_hidden(&g, true);
        data.set_favorite(&g.id, true);
        data.apply(&mut g);
        assert!(g.hidden && g.favorite);

        data.set_hidden(&g, false);
        data.apply(&mut g);
        assert!(!g.hidden);
    }

    #[test]
    fn launch_keeps_most_recent_date() {
        let mut data = UserData::default();
        let mut g = game("730", false);
        g.last_played = Some(u64::MAX);
        let ts = data.record_launch(&g.id);
        data.apply(&mut g);
        assert_eq!(g.last_played, Some(u64::MAX));

        let mut other = game("440", false);
        data.record_launch(&other.id);
        data.apply(&mut other);
        assert!(other.last_played.unwrap() >= ts);
    }

    #[test]
    fn ludex_playtime_accumulates_without_overriding_launcher() {
        let mut data = UserData::default();
        data.add_playtime("epic:Sugar", 60);
        data.add_playtime("epic:Sugar", 60);
        data.add_playtime("epic:Sugar", 29);
        data.add_playtime("epic:Sugar", 0);
        assert_eq!(data.playtime.get("epic:Sugar"), Some(&149));

        let mut epic = Game::new(Platform::Epic, "Sugar", "Rocket League", Launch::Uri(String::new()));
        data.apply(&mut epic);
        assert_eq!(epic.playtime_minutes, Some(2)); // 149 s arrondies à la minute
        assert_eq!(epic.playtime_source, Some(PlaytimeSource::Ludex));
        // Appliquer de nouveau (après une autre session) met le chiffre à jour.
        data.add_playtime("epic:Sugar", 3600);
        data.apply(&mut epic);
        assert_eq!(epic.playtime_minutes, Some(62));

        // Le launcher a son propre chiffre : celui de Ludex est ignoré, jamais additionné.
        let mut steam = game("730", false);
        steam.set_launcher_playtime(6230, None);
        data.add_playtime(&steam.id, 600);
        data.apply(&mut steam);
        assert_eq!(steam.playtime_minutes, Some(6230));
        assert_eq!(steam.playtime_source, Some(PlaytimeSource::Launcher));

        // Jamais mesuré : inconnu.
        let mut other = game("440", false);
        data.apply(&mut other);
        assert_eq!(other.playtime_minutes, None);
    }

    #[test]
    fn old_files_without_playtime_still_load() {
        let data: UserData = serde_json::from_str(r#"{"favorites":["steam:730"],"launched":{"steam:730":1}}"#).unwrap();
        assert!(data.playtime.is_empty() && data.favorites.contains("steam:730"));
    }

    #[test]
    fn settings_default_to_tray_without_autostart() {
        let s = UserData::default().settings;
        assert!(s.keep_in_tray && !s.launch_at_startup && !s.tray_hint_shown);
        // Fichier d'avant les réglages, ou réglages incomplets : valeurs par défaut.
        let old: UserData = serde_json::from_str(r#"{"favorites":["steam:730"]}"#).unwrap();
        assert_eq!(old.settings, Settings::default());
        let partial: UserData = serde_json::from_str(r#"{"settings":{"launchAtStartup":true}}"#).unwrap();
        assert!(partial.settings.keep_in_tray && partial.settings.launch_at_startup);
    }

    #[test]
    fn save_and_load_round_trip() {
        let path = std::env::temp_dir().join(format!("ludex-test-{}.json", std::process::id()));
        let mut data = UserData::default();
        data.set_favorite("steam:730", true);
        data.add_playtime("epic:Sugar", 4200);
        data.settings.keep_in_tray = false;
        data.settings.tray_hint_shown = true;
        data.save(&path).unwrap();
        let loaded = UserData::load(&path);
        assert!(loaded.favorites.contains("steam:730"));
        assert_eq!(loaded.settings, data.settings);
        assert_eq!(loaded.playtime.get("epic:Sugar"), Some(&4200));
        let _ = fs::remove_file(path);
    }
}
