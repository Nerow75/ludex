//! Préférences de l'utilisateur, gardées dans un fichier JSON du dossier de données de l'app :
//! favoris, jeux masqués ou ré-affichés, et date du dernier lancement depuis Ludex.

use crate::library::Game;
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

    /// Reporte les préférences sur un jeu détecté.
    pub fn apply(&self, game: &mut Game) {
        game.favorite = self.favorites.contains(&game.id);
        game.hidden = if game.is_tool { !self.shown.contains(&game.id) } else { self.hidden.contains(&game.id) };
        if let Some(&ts) = self.launched.get(&game.id) {
            game.last_played = Some(game.last_played.map_or(ts, |l| l.max(ts)));
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
    fn save_and_load_round_trip() {
        let path = std::env::temp_dir().join(format!("ludex-test-{}.json", std::process::id()));
        let mut data = UserData::default();
        data.set_favorite("steam:730", true);
        data.save(&path).unwrap();
        assert!(UserData::load(&path).favorites.contains("steam:730"));
        let _ = fs::remove_file(path);
    }
}
