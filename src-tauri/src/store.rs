//! Préférences de l'utilisateur, gardées dans un fichier JSON du dossier de données de l'app :
//! favoris, jeux masqués ou ré-affichés, date du dernier lancement depuis Ludex, temps de jeu
//! mesuré par Ludex, jeux ajoutés à la main, exécutables et titres choisis par l'utilisateur,
//! progression (statut, note), collections perso, et réglages de l'application (zone de
//! notification, lancement avec Windows, copie des sauvegardes).

use crate::collections::{self, Collection, Status};
use crate::covers::{CustomArt, Kind, Picked};
use crate::library::manual::{self, Custom, ManualGame};
use crate::library::{Game, Platform, PlaytimeSource};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    /// Jeux ajoutés à la main (« Autres »).
    pub manual: Vec<ManualGame>,
    /// Exécutable choisi à la place de celui détecté, par id (jeux « Autres » détectés).
    pub exes: BTreeMap<String, PathBuf>,
    /// Titre choisi par l'utilisateur, par id (jeux détectés ; un jeu manuel garde le sien).
    pub titles: BTreeMap<String, String>,
    /// Jaquette et image de fond choisies par l'utilisateur, par id (`covers.rs`). Gardées même
    /// si le jeu quitte la bibliothèque : il les retrouve en revenant.
    pub covers: BTreeMap<String, CustomArt>,
    /// Réglages de l'application (zone de notification, lancement avec Windows).
    pub settings: Settings,
    /// Progression choisie par l'utilisateur, par id de jeu.
    #[serde(deserialize_with = "known_statuses")]
    pub status: BTreeMap<String, Status>,
    /// Note perso, par id de jeu (nettoyée, `collections::MAX_NOTE` caractères au plus).
    pub notes: BTreeMap<String, String>,
    /// Collections de l'utilisateur, dans l'ordre de création.
    pub collections: Vec<Collection>,
    /// Options de lancement par id de jeu, déjà validées (`launch_args.rs`).
    pub launch_args: BTreeMap<String, String>,
}

/// Statuts relus en ignorant une valeur inconnue (fichier d'une version future) : sinon le
/// fichier entier serait illisible, donc remplacé par des préférences vides au prochain
/// enregistrement.
fn known_statuses<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<String, Status>, D::Error> {
    let raw = BTreeMap::<String, serde_json::Value>::deserialize(d)?;
    Ok(raw.into_iter().filter_map(|(id, v)| Some((id, serde_json::from_value(v).ok()?))).collect())
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
    /// Dossier des copies de sauvegardes, choisi avec le sélecteur natif. `None` : `Documents\Ludex\Sauvegardes`.
    pub saves_dir: Option<PathBuf>,
    /// Dossier des données lourdes (« D:\Ludex ») : cache des images et index des sauvegardes dans
    /// `Cache`, copies de sauvegardes dans `Sauvegardes` (`relocate.rs`). `None` : `%APPDATA%` et
    /// `Documents\Ludex\Sauvegardes`. Propre à ce PC (jamais repris d'un import).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_dir: Option<PathBuf>,
    /// Copie des sauvegardes à la fin de chaque partie (une minute au moins). Active par défaut.
    pub auto_saves: bool,
    /// Raccourci clavier global qui ouvre la recherche rapide (`quick.rs`), au format du plugin
    /// (« Alt+Space », « Ctrl+Shift+L »). Vide : désactivé. Modifiable plus tard par un écran Paramètres.
    pub quick_shortcut: String,
    /// Vrai plein écran permis dans le mode canapé (F11 une fois dedans). Faux par défaut : le mode
    /// occupe la fenêtre agrandie, car certains écrans VRR (Samsung Odyssey G9) clignotent dès qu'une
    /// appli passe en plein écran. Modifiable plus tard par un écran Paramètres.
    pub couch_fullscreen: bool,
    /// Écran de premier lancement déjà vu. `None` : fichier d'avant ce réglage, tranché au
    /// démarrage par `welcome.rs` (un habitué ne le voit pas).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub welcome_shown: Option<bool>,
    /// Thème de l'interface (écran Paramètres). Valeur inconnue : thème par défaut, sans perdre le reste.
    #[serde(deserialize_with = "or_default")]
    pub theme: Theme,
    /// Launchers dont les jeux sont écartés du scan (écran Paramètres). Valeur inconnue ignorée.
    #[serde(deserialize_with = "known_platforms")]
    pub disabled_sources: Vec<Platform>,
    /// Notifications Windows activées, par type (`notify.rs`). Type inconnu ignoré.
    pub notifications: crate::notify::Prefs,
}

/// Thème de l'interface : sombre (par défaut, celui de Ludex depuis le début), clair, celui de
/// Windows, noir OLED (fond #000) ou contraste élevé (texte AAA, contours partout).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Light,
    System,
    Oled,
    Contrast,
}

/// Valeur relue en gardant la valeur par défaut si elle est illisible (fichier d'une version future).
fn or_default<'de, D: Deserializer<'de>, T: Default + serde::de::DeserializeOwned>(d: D) -> Result<T, D::Error> {
    let raw = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(raw).unwrap_or_default())
}

/// Launchers relus sans doublon, en ignorant ceux qu'une version future aurait ajoutés.
fn known_platforms<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Platform>, D::Error> {
    let raw = Vec::<serde_json::Value>::deserialize(d).unwrap_or_default();
    let mut list: Vec<Platform> = Vec::new();
    for p in raw.into_iter().filter_map(|v| serde_json::from_value(v).ok()) {
        if !list.contains(&p) {
            list.push(p);
        }
    }
    Ok(list)
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            keep_in_tray: true,
            launch_at_startup: false,
            tray_hint_shown: false,
            saves_dir: None,
            data_dir: None,
            auto_saves: true,
            quick_shortcut: crate::quick::DEFAULT_SHORTCUT.to_string(),
            couch_fullscreen: false,
            welcome_shown: None,
            theme: Theme::Dark,
            disabled_sources: Vec::new(),
            notifications: crate::notify::Prefs::default(),
        }
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

    /// Ludex a déjà servi : une préférence retenue, ou l'explication de la zone de notification vue.
    pub fn has_history(&self) -> bool {
        self.settings.tray_hint_shown
            || !(self.favorites.is_empty()
                && self.hidden.is_empty()
                && self.shown.is_empty()
                && self.launched.is_empty()
                && self.playtime.is_empty()
                && self.manual.is_empty()
                && self.exes.is_empty()
                && self.titles.is_empty()
                && self.covers.is_empty()
                && self.status.is_empty()
                && self.notes.is_empty()
                && self.collections.is_empty())
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

    /// Ré-affiche tous les jeux masqués par l'utilisateur et renvoie leurs ids. Les outils
    /// (masqués d'office) restent dans « Masqués » : ce ne sont pas des choix de l'utilisateur.
    pub fn show_all_hidden(&mut self) -> Vec<String> {
        std::mem::take(&mut self.hidden).into_iter().collect()
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

    /// Ajoute un jeu à la main et renvoie son id. Le même exécutable ajouté de nouveau (jeu
    /// écarté parce que son fichier avait disparu) reprend son entrée et son id.
    pub fn add_manual(&mut self, exe: PathBuf, title: String) -> String {
        let id = manual::manual_id(&exe);
        self.manual.retain(|m| m.id != id);
        self.manual.push(ManualGame { id: id.clone(), exe, title, args: Vec::new() });
        id
    }

    /// Retire un jeu ajouté à la main. Son temps de jeu et sa date de lancement restent : ajouté
    /// de nouveau depuis le même exécutable, il les retrouve.
    pub fn remove_manual(&mut self, id: &str) -> bool {
        let before = self.manual.len();
        self.manual.retain(|m| m.id != id);
        self.favorites.remove(id);
        self.hidden.remove(id);
        self.manual.len() != before
    }

    /// Nouvel exécutable : remplace celui d'un jeu manuel, ou celui détecté pour un jeu « Autres ».
    pub fn set_exe(&mut self, id: &str, exe: PathBuf) {
        match self.manual.iter_mut().find(|m| m.id == id) {
            Some(m) => m.exe = exe,
            None => drop(self.exes.insert(id.to_string(), exe)),
        }
    }

    /// Nouveau titre. `None` : retour au titre d'origine (détecté, ou deviné depuis l'exécutable
    /// pour un jeu manuel).
    pub fn rename(&mut self, id: &str, title: Option<String>) {
        if let Some(m) = self.manual.iter_mut().find(|m| m.id == id) {
            m.title = title.unwrap_or_else(|| manual::guess_title(&m.exe));
            return;
        }
        match title {
            Some(t) => drop(self.titles.insert(id.to_string(), t)),
            None => drop(self.titles.remove(id)),
        }
    }

    /// Statut de progression d'un jeu. `None` : aucun.
    pub fn set_status(&mut self, id: &str, status: Option<Status>) {
        match status {
            Some(s) => drop(self.status.insert(id.to_string(), s)),
            None => drop(self.status.remove(id)),
        }
    }

    /// Note perso d'un jeu, déjà nettoyée. `None` : note effacée.
    pub fn set_note(&mut self, id: &str, note: Option<String>) {
        match note {
            Some(n) => drop(self.notes.insert(id.to_string(), n)),
            None => drop(self.notes.remove(id)),
        }
    }

    /// Options de lancement d'un jeu, déjà validées. `None` : options effacées.
    pub fn set_launch_args(&mut self, id: &str, args: Option<String>) {
        match args {
            Some(a) => drop(self.launch_args.insert(id.to_string(), a)),
            None => drop(self.launch_args.remove(id)),
        }
    }

    fn collection_mut(&mut self, id: &str) -> Result<&mut Collection, String> {
        self.collections.iter_mut().find(|c| c.id == id).ok_or_else(|| "Cette collection n'existe plus.".to_string())
    }

    /// Nom de collection nettoyé, différent de ceux des autres collections (casse ignorée).
    fn collection_name(&self, raw: &str, except: Option<&str>) -> Result<String, String> {
        let name = collections::clean_name(raw).ok_or("Donne un nom à la collection.")?;
        let lower = name.to_lowercase();
        match self.collections.iter().find(|c| Some(c.id.as_str()) != except && c.name.to_lowercase() == lower) {
            Some(c) => Err(format!("La collection « {} » existe déjà.", c.name)),
            None => Ok(name),
        }
    }

    /// Crée une collection vide et renvoie son id, généré ici (`seed` : heure en millisecondes).
    pub fn create_collection(&mut self, raw_name: &str, seed: u64) -> Result<String, String> {
        if self.collections.len() >= collections::MAX_COLLECTIONS {
            return Err(format!("{} collections au plus.", collections::MAX_COLLECTIONS));
        }
        let name = self.collection_name(raw_name, None)?;
        let id = collections::new_id(&self.collections, seed);
        self.collections.push(Collection { id: id.clone(), name, games: Vec::new() });
        Ok(id)
    }

    pub fn rename_collection(&mut self, id: &str, raw_name: &str) -> Result<(), String> {
        let name = self.collection_name(raw_name, Some(id))?;
        self.collection_mut(id)?.name = name;
        Ok(())
    }

    pub fn delete_collection(&mut self, id: &str) -> Result<Collection, String> {
        let i = self.collections.iter().position(|c| c.id == id).ok_or("Cette collection n'existe plus.")?;
        Ok(self.collections.remove(i))
    }

    /// Ajoute un jeu à une collection (à la fin, sans doublon) ou l'en retire.
    pub fn set_in_collection(&mut self, collection: &str, game: &str, value: bool) -> Result<(), String> {
        let c = self.collection_mut(collection)?;
        let present = c.games.iter().any(|g| g == game);
        if value && !present {
            c.games.push(game.to_string());
        } else if !value {
            c.games.retain(|g| g != game);
        }
        Ok(())
    }

    /// Image perso choisie (`Some`) ou retirée (`None`). Renvoie l'ancien choix.
    pub fn set_cover(&mut self, id: &str, kind: Kind, value: Option<Picked>) -> Option<Picked> {
        let entry = self.covers.entry(id.to_string()).or_default();
        let old = entry.get(kind).cloned();
        entry.set(kind, value);
        if entry.is_empty() {
            self.covers.remove(id);
        }
        old
    }

    /// Jeux manuels et corrections de l'utilisateur, appliqués à une bibliothèque scannée.
    pub fn customize(&self, games: &mut Vec<Game>) {
        manual::customize(games, &Custom { manual: &self.manual, exes: &self.exes, titles: &self.titles });
    }

    /// Reporte les préférences sur un jeu détecté.
    pub fn apply(&self, game: &mut Game) {
        game.favorite = self.favorites.contains(&game.id);
        game.status = self.status.get(&game.id).copied();
        game.note = self.notes.get(&game.id).cloned();
        // Options gardées même si le launcher les refuse aujourd'hui (Galaxy installé depuis),
        // mais ni montrées ni utilisées tant qu'il les refuse.
        game.launch_args_refusal = crate::launch_args::refusal(game).map(String::from);
        game.launch_args = if game.launch_args_refusal.is_none() { self.launch_args.get(&game.id).cloned() } else { None };
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
    fn launch_args_are_kept_and_shown_only_where_accepted() {
        let mut data = UserData::default();
        let mut steam = game("730", false);
        data.set_launch_args(&steam.id, Some("-novid".into()));
        data.apply(&mut steam);
        assert_eq!(steam.launch_args.as_deref(), Some("-novid"));
        assert_eq!(steam.launch_args_refusal, None);

        // Launcher qui refuse : options gardées dans le fichier, ni montrées ni utilisées.
        let mut epic = Game::new(Platform::Epic, "Sugar", "Rocket League", Launch::Uri(String::new()));
        data.set_launch_args(&epic.id, Some("-novid".into()));
        data.apply(&mut epic);
        assert_eq!(epic.launch_args, None);
        assert!(epic.launch_args_refusal.is_some());
        assert!(data.launch_args.contains_key("epic:Sugar"));

        let json = serde_json::to_string(&data).unwrap();
        let back: UserData = serde_json::from_str(&json).unwrap();
        assert_eq!(back.launch_args.get("steam:730").map(String::as_str), Some("-novid"));

        data.set_launch_args(&steam.id, None);
        data.apply(&mut steam);
        assert_eq!(steam.launch_args, None);
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
    fn manual_games_and_corrections_round_trip() {
        let path = std::env::temp_dir().join(format!("ludex-test-manual-{}.json", std::process::id()));
        let mut data = UserData::default();
        let exe = PathBuf::from(r"D:\Emu\Dolphin\Dolphin.exe");
        let id = data.add_manual(exe.clone(), "Dolphin".into());
        // Même exécutable de nouveau : même id, pas de doublon.
        assert_eq!(data.add_manual(exe.clone(), "Dolphin 2".into()), id);
        assert_eq!(data.manual.len(), 1);
        data.set_favorite(&id, true);
        data.add_playtime(&id, 600);
        data.set_exe(&id, PathBuf::from(r"D:\Emu\Dolphin\DolphinQt.exe"));
        data.set_exe("other:e-jeux-frostpunk", PathBuf::from(r"E:\Jeux\Frostpunk\bin\Frostpunk.exe"));
        data.rename(&id, Some("Wii".into()));
        data.rename("other:e-jeux-ck", Some("Crusader Kings III".into()));
        data.save(&path).unwrap();

        let mut loaded = UserData::load(&path);
        assert_eq!(loaded.manual[0].id, id); // l'id ne suit pas l'exécutable
        assert_eq!(loaded.manual[0].exe, PathBuf::from(r"D:\Emu\Dolphin\DolphinQt.exe"));
        assert_eq!(loaded.manual[0].title, "Wii");
        assert!(loaded.exes.contains_key("other:e-jeux-frostpunk") && !loaded.exes.contains_key(&id));
        assert_eq!(loaded.titles.get("other:e-jeux-ck").map(String::as_str), Some("Crusader Kings III"));

        loaded.rename("other:e-jeux-ck", None);
        assert!(loaded.titles.is_empty());
        assert!(loaded.remove_manual(&id));
        assert!(!loaded.remove_manual(&id));
        assert!(loaded.manual.is_empty() && !loaded.favorites.contains(&id));
        assert_eq!(loaded.playtime.get(&id), Some(&600)); // gardé pour un nouvel ajout
        let _ = fs::remove_file(path);
    }

    #[test]
    fn files_without_manual_games_still_load() {
        let data: UserData = serde_json::from_str(r#"{"favorites":[],"playtime":{"epic:Sugar":60}}"#).unwrap();
        assert!(data.manual.is_empty() && data.exes.is_empty() && data.titles.is_empty());
        let one: UserData = serde_json::from_str(r#"{"manual":[{"id":"other:manual-1","exe":"D:\\a.exe","title":"A"}]}"#).unwrap();
        assert!(one.manual[0].args.is_empty());
    }

    #[test]
    fn settings_default_to_tray_without_autostart() {
        let s = UserData::default().settings;
        assert!(s.keep_in_tray && !s.launch_at_startup && !s.tray_hint_shown);
        assert!(s.auto_saves && s.saves_dir.is_none() && s.data_dir.is_none() && s.welcome_shown.is_none());
        assert!(!s.couch_fullscreen);
        assert_eq!(s.quick_shortcut, "Alt+Space");
        assert!(!UserData::default().has_history());
        // Fichier d'avant les réglages, ou réglages incomplets : valeurs par défaut.
        let old: UserData = serde_json::from_str(r#"{"favorites":["steam:730"]}"#).unwrap();
        assert_eq!(old.settings, Settings::default());
        let partial: UserData = serde_json::from_str(r#"{"settings":{"launchAtStartup":true}}"#).unwrap();
        assert!(partial.settings.keep_in_tray && partial.settings.launch_at_startup);
        assert_eq!(partial.settings.quick_shortcut, "Alt+Space");
        // Raccourci désactivé par l'utilisateur : la chaîne vide est gardée telle quelle.
        let off: UserData = serde_json::from_str(r#"{"settings":{"quickShortcut":""}}"#).unwrap();
        assert!(off.settings.quick_shortcut.is_empty());
    }

    #[test]
    fn theme_and_disabled_sources_survive_unknown_values() {
        let s = UserData::default().settings;
        assert_eq!(s.theme, Theme::Dark);
        assert!(s.disabled_sources.is_empty());
        let read: UserData = serde_json::from_str(r#"{"settings":{"theme":"system","disabledSources":["epic","riot","epic"]}}"#).unwrap();
        assert_eq!(read.settings.theme, Theme::System);
        assert_eq!(read.settings.disabled_sources, [Platform::Epic, Platform::Riot]);
        // Fichier d'une version future : le thème inconnu revient au défaut, le launcher inconnu est
        // oublié, et le reste du fichier (favoris, autres réglages) est gardé.
        let future: UserData = serde_json::from_str(
            r#"{"favorites":["steam:730"],"settings":{"theme":"sepia","disabledSources":["humble","gog"],"autoSaves":false,"notifications":{"promos":true,"news":false}}}"#,
        )
        .unwrap();
        assert_eq!(future.settings.theme, Theme::Dark);
        assert!(!future.settings.notifications.news && future.settings.notifications.download, "type inconnu ignoré");
        let odd: UserData = serde_json::from_str(r#"{"favorites":["a"],"settings":{"notifications":"tout"}}"#).unwrap();
        assert!(odd.favorites.contains("a") && odd.settings.notifications == crate::notify::Prefs::default());
        assert_eq!(future.settings.disabled_sources, [Platform::Gog]);
        assert!(!future.settings.auto_saves && future.favorites.contains("steam:730"));
        let text = serde_json::to_string(&read.settings).unwrap();
        assert!(text.contains(r#""theme":"system""#) && text.contains(r#""disabledSources":["epic","riot"]"#), "{text}");
        // Noir OLED et contraste élevé : relus et réécrits sous leur nom.
        for (name, theme) in [("oled", Theme::Oled), ("contrast", Theme::Contrast)] {
            let read: UserData = serde_json::from_str(&format!(r#"{{"settings":{{"theme":"{name}"}}}}"#)).unwrap();
            assert_eq!(read.settings.theme, theme);
            let text = serde_json::to_string(&read.settings).unwrap();
            assert!(text.contains(&format!(r#""theme":"{name}""#)), "{text}");
        }
        // Casse différente : inconnu, donc le défaut.
        let upper: UserData = serde_json::from_str(r#"{"settings":{"theme":"OLED"}}"#).unwrap();
        assert_eq!(upper.settings.theme, Theme::Dark);
    }

    #[test]
    fn show_all_hidden_keeps_tools_hidden() {
        let mut data = UserData::default();
        let tool = game("431960", true);
        data.set_hidden(&game("730", false), true);
        data.set_hidden(&game("440", false), true);
        data.set_hidden(&tool, false); // outil ré-affiché : reste ré-affiché
        assert_eq!(data.show_all_hidden(), ["steam:440", "steam:730"]);
        assert!(data.hidden.is_empty() && data.shown.contains("steam:431960"));
        assert!(data.show_all_hidden().is_empty());
    }

    #[test]
    fn status_and_note_are_applied_and_saved() {
        let path = std::env::temp_dir().join(format!("ludex-test-progress-{}.json", std::process::id()));
        let mut data = UserData::default();
        data.set_status("steam:730", Some(Status::Playing));
        data.set_status("steam:440", Some(Status::Done));
        data.set_note("steam:730", collections::clean_note("  Rang  Or \n visé "));
        data.save(&path).unwrap();

        let mut loaded = UserData::load(&path);
        let mut g = game("730", false);
        loaded.apply(&mut g);
        assert_eq!(g.status, Some(Status::Playing));
        assert_eq!(g.note.as_deref(), Some("Rang Or visé"));
        assert!(fs::read_to_string(&path).unwrap().contains(r#""steam:440": "done""#));

        loaded.set_status("steam:730", None);
        loaded.set_note("steam:730", None);
        loaded.apply(&mut g);
        assert_eq!((g.status, g.note), (None, None));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn custom_covers_are_set_reset_and_saved() {
        let path = std::env::temp_dir().join(format!("ludex-test-covers-{}.json", std::process::id()));
        let mut data = UserData::default();
        let picked = |at| Picked { ext: "png".into(), at };
        assert_eq!(data.set_cover("riot:valorant", Kind::Cover, Some(picked(1))), None);
        assert_eq!(data.set_cover("riot:valorant", Kind::Cover, Some(picked(2))), Some(picked(1)));
        data.set_cover("riot:valorant", Kind::Hero, Some(picked(3)));
        data.save(&path).unwrap();
        let loaded = UserData::load(&path);
        assert_eq!(loaded.covers.get("riot:valorant").and_then(|c| c.get(Kind::Hero)), Some(&picked(3)));
        // Jaquette d'origine : l'entrée disparaît quand plus rien n'est choisi.
        data.set_cover("riot:valorant", Kind::Cover, None);
        assert!(data.covers.contains_key("riot:valorant"));
        data.set_cover("riot:valorant", Kind::Hero, None);
        assert!(data.covers.is_empty());
        data.set_cover("steam:730", Kind::Hero, None);
        assert!(data.covers.is_empty());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn collections_are_created_renamed_filled_and_deleted() {
        let mut data = UserData::default();
        let rpg = data.create_collection("  RPG  ", 1000).unwrap();
        assert_eq!(data.collections[0].name, "RPG");
        // Même nom (casse ignorée) ou nom vide : refusé.
        assert!(data.create_collection("rpg", 1000).unwrap_err().contains("existe déjà"));
        assert!(data.create_collection(" \u{200B} ", 1000).is_err());
        let coop = data.create_collection("Coop", 1000).unwrap();
        assert_ne!(rpg, coop); // même graine : id suivant

        data.set_in_collection(&rpg, "steam:730", true).unwrap();
        data.set_in_collection(&rpg, "steam:440", true).unwrap();
        data.set_in_collection(&rpg, "steam:730", true).unwrap();
        assert_eq!(data.collections[0].games, ["steam:730", "steam:440"]);
        data.set_in_collection(&rpg, "steam:730", false).unwrap();
        assert_eq!(data.collections[0].games, ["steam:440"]);
        assert!(data.set_in_collection("c-inconnue", "steam:730", true).is_err());

        assert!(data.rename_collection(&coop, "RPG").is_err());
        data.rename_collection(&rpg, "rpg").unwrap(); // sa propre casse : permis
        assert_eq!(data.collections[0].name, "rpg");

        let path = std::env::temp_dir().join(format!("ludex-test-collections-{}.json", std::process::id()));
        data.save(&path).unwrap();
        let mut loaded = UserData::load(&path);
        assert_eq!(loaded.collections, data.collections);
        assert_eq!(loaded.delete_collection(&rpg).unwrap().games, ["steam:440"]);
        assert!(loaded.delete_collection(&rpg).is_err());
        assert_eq!(loaded.collections.len(), 1);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn collections_are_limited() {
        let mut data = UserData::default();
        for i in 0..collections::MAX_COLLECTIONS {
            data.create_collection(&format!("C{i}"), 0).unwrap();
        }
        assert!(data.create_collection("Une de trop", 0).is_err());
    }

    #[test]
    fn files_without_progress_still_load() {
        let data: UserData = serde_json::from_str(r#"{"favorites":["steam:730"]}"#).unwrap();
        assert!(data.status.is_empty() && data.notes.is_empty() && data.collections.is_empty());
        // Statut inconnu (version future) : ignoré, le reste du fichier est gardé.
        let future: UserData =
            serde_json::from_str(r#"{"favorites":["steam:730"],"status":{"steam:730":"wishlist","steam:440":"done"}}"#).unwrap();
        assert!(future.favorites.contains("steam:730"));
        assert_eq!(future.status.len(), 1);
        assert_eq!(future.status.get("steam:440"), Some(&Status::Done));
    }

    #[test]
    fn save_and_load_round_trip() {
        let path = std::env::temp_dir().join(format!("ludex-test-{}.json", std::process::id()));
        let mut data = UserData::default();
        data.set_favorite("steam:730", true);
        data.add_playtime("epic:Sugar", 4200);
        data.settings.keep_in_tray = false;
        data.settings.tray_hint_shown = true;
        data.settings.saves_dir = Some(PathBuf::from(r"D:\Copies"));
        data.settings.auto_saves = false;
        data.settings.quick_shortcut = "Ctrl+Shift+L".to_string();
        data.settings.welcome_shown = Some(true);
        data.settings.couch_fullscreen = true;
        data.save(&path).unwrap();
        let loaded = UserData::load(&path);
        assert!(loaded.favorites.contains("steam:730"));
        assert_eq!(loaded.settings, data.settings);
        assert_eq!(loaded.playtime.get("epic:Sugar"), Some(&4200));
        let _ = fs::remove_file(path);
    }
}
