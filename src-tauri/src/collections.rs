//! Progression des jeux (statut et note perso) et collections : celles de l'utilisateur, gardées
//! dans `user-data.json` (`store.rs`), et celles du client Steam, lues sans jamais rien écrire.
//!
//! Le front n'envoie que des ids, un statut (valeur fermée) et des textes courts, nettoyés ici.

use crate::library::manual::clean_text;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// Progression d'un jeu, choisie par l'utilisateur. Sérialisé `todo`, `playing`, `done`, `dropped`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// À faire.
    Todo,
    /// En cours.
    Playing,
    /// Terminé.
    Done,
    /// Abandonné.
    Dropped,
}

/// Longueur maximale d'une note perso, en caractères.
pub const MAX_NOTE: usize = 500;
/// Longueur maximale du nom d'une collection, en caractères.
pub const MAX_NAME: usize = 40;
/// Nombre maximal de collections de l'utilisateur.
pub const MAX_COLLECTIONS: usize = 100;
/// Texte reçu du front au-delà de cette taille (octets) : refusé avant tout traitement.
pub const MAX_INPUT: usize = 4096;

/// Collection : nom et ids des jeux, dans l'ordre d'ajout.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub games: Vec<String>,
}

pub fn clean_note(raw: &str) -> Option<String> {
    clean_text(raw, MAX_NOTE)
}

pub fn clean_name(raw: &str) -> Option<String> {
    clean_text(raw, MAX_NAME)
}

/// Refuse un texte trop long avant même de le nettoyer.
pub fn check_input(raw: &str) -> Result<(), String> {
    if raw.len() > MAX_INPUT { Err("Texte trop long.".into()) } else { Ok(()) }
}

/// Nouvel id de collection, `c-<base 36>`, jamais déjà pris (ni réutilisé tant qu'il existe).
pub fn new_id(taken: &[Collection], seed: u64) -> String {
    let mut n = seed;
    loop {
        let id = format!("c-{}", base36(n));
        if !taken.iter().any(|c| c.id == id) {
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

/// Préfixe des clés des collections dans le stockage du client Steam.
const STEAM_PREFIX: &str = "user-collections.";
/// Collection « Masqués » de Steam : jamais proposée (Ludex a ses propres jeux masqués).
const STEAM_HIDDEN: &str = "hidden";

/// Contenu d'une collection Steam (valeur JSON rangée dans une chaîne).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SteamValue {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    added: Vec<u64>,
    #[serde(default)]
    removed: Vec<u64>,
    /// Collection dynamique (règles de filtre) : son contenu est calculé par Steam, pas listé.
    filter_spec: Option<serde_json::Value>,
}

/// Entrée du stockage : `[clé, { key, timestamp, value, version, is_deleted? }]`.
#[derive(Deserialize)]
struct SteamEntry {
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    is_deleted: bool,
}

/// Collections Steam d'un fichier `cloud-storage-namespace-1.json`, dans l'ordre du fichier (le
/// front les trie par nom, accents compris). Jeux = ids
/// Ludex `steam:<appid>` (ajoutés moins retirés). Écartées : supprimées, « Masqués », collections
/// dynamiques (leur contenu n'est pas dans le fichier) et entrées illisibles.
pub fn parse_steam(text: &str) -> Vec<Collection> {
    let Ok(entries) = serde_json::from_str::<Vec<(String, serde_json::Value)>>(text) else { return Vec::new() };
    entries
        .into_iter()
        .filter_map(|(key, raw)| {
            let key_id = key.strip_prefix(STEAM_PREFIX)?;
            let entry: SteamEntry = serde_json::from_value(raw).ok()?;
            if entry.is_deleted || key_id == STEAM_HIDDEN {
                return None;
            }
            let value: SteamValue = serde_json::from_str(entry.value.as_deref()?).ok()?;
            if value.filter_spec.is_some() {
                return None;
            }
            let id = if value.id.is_empty() { key_id.to_string() } else { value.id };
            let name = clean_name(&value.name)?;
            let removed: BTreeSet<u64> = value.removed.into_iter().collect();
            let mut seen = BTreeSet::new();
            let games = value
                .added
                .into_iter()
                .filter(|appid| !removed.contains(appid) && seen.insert(*appid))
                .map(|appid| format!("steam:{appid}"))
                .collect();
            Some(Collection { id, name, games })
        })
        .collect()
}

/// Collections Steam du compte retenu sur ce PC. Vide : Steam absent, fichier absent ou illisible.
pub fn steam_collections() -> Vec<Collection> {
    crate::library::cloud_storage_file().map(|p| read_steam(&p)).unwrap_or_default()
}

/// Fichier trop gros pour être un stockage de collections : ignoré.
const MAX_STEAM_FILE: u64 = 16 * 1024 * 1024;

pub fn read_steam(path: &Path) -> Vec<Collection> {
    if fs::metadata(path).map(|m| m.len() > MAX_STEAM_FILE).unwrap_or(true) {
        return Vec::new();
    }
    fs::read_to_string(path).map(|t| parse_steam(&t)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_is_a_closed_value() {
        assert_eq!(serde_json::to_string(&Status::Dropped).unwrap(), r#""dropped""#);
        assert_eq!(serde_json::from_str::<Status>(r#""playing""#).unwrap(), Status::Playing);
        assert!(serde_json::from_str::<Status>(r#""finished""#).is_err());
        assert!(serde_json::from_str::<Option<Status>>("null").unwrap().is_none());
    }

    #[test]
    fn notes_and_names_are_cleaned() {
        assert_eq!(clean_note("  Fini   à 100 %\n\tsauf le DLC \u{200B} ").as_deref(), Some("Fini à 100 % sauf le DLC"));
        assert_eq!(clean_note(" \u{FEFF}\n ").as_deref(), None);
        assert_eq!(clean_note(&"é".repeat(600)).unwrap().chars().count(), MAX_NOTE);
        assert_eq!(clean_name(&"x".repeat(60)).unwrap().len(), MAX_NAME);
        assert_eq!(clean_name("RPG\u{202E} à finir").as_deref(), Some("RPG à finir"));
        assert!(check_input(&"a".repeat(MAX_INPUT + 1)).is_err());
        assert!(check_input("Coop").is_ok());
    }

    #[test]
    fn new_ids_never_collide() {
        let taken = vec![Collection { id: new_id(&[], 35), name: "A".into(), games: vec![] }];
        assert_eq!(taken[0].id, "c-z");
        assert_eq!(new_id(&taken, 35), "c-10");
        assert_eq!(new_id(&[], 0), "c-0");
    }

    /// Extrait d'un vrai fichier (structure relevée sur un PC, appids et noms changés).
    const STEAM_FILE: &str = r#"[
        ["showcases.0", {"key":"showcases.0","timestamp":1,"value":"{\"nShowcaseId\":0}","version":"1"}],
        ["user-collections.favorite", {"key":"user-collections.favorite","timestamp":2,"value":"{\"id\":\"favorite\",\"name\":\"Favoris\",\"added\":[362620,1904540,526870,362620],\"removed\":[526870]}","version":"3","conflictResolutionMethod":"custom","strMethodId":"union-collections"}],
        ["user-collections.hidden", {"key":"user-collections.hidden","timestamp":2,"value":"{\"id\":\"hidden\",\"name\":\"Masqués\",\"added\":[440],\"removed\":[]}","version":"3"}],
        ["user-collections.uc-dqHc1YwCJ0qx", {"key":"user-collections.uc-dqHc1YwCJ0qx","timestamp":3,"value":"{\"id\":\"uc-dqHc1YwCJ0qx\",\"name\":\"  À finir \",\"added\":[730],\"removed\":[]}","version":"4"}],
        ["user-collections.uc-dyn", {"key":"user-collections.uc-dyn","timestamp":3,"value":"{\"id\":\"uc-dyn\",\"name\":\"Coop\",\"added\":[],\"removed\":[],\"filterSpec\":{\"nFormatVersion\":2}}","version":"4"}],
        ["user-collections.uc-gone", {"key":"user-collections.uc-gone","timestamp":3,"is_deleted":true,"version":"5"}],
        ["user-collections.uc-bad", {"key":"user-collections.uc-bad","timestamp":3,"value":"pas du json","version":"5"}]
    ]"#;

    #[test]
    fn steam_collections_are_read_from_cloud_storage() {
        let list = parse_steam(STEAM_FILE);
        let names: Vec<&str> = list.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Favoris", "À finir"]);
        assert_eq!(list[1].id, "uc-dqHc1YwCJ0qx");
        assert_eq!(list[1].games, ["steam:730"]);
        // Retirés et doublons écartés, ordre d'ajout gardé.
        assert_eq!(list[0].games, ["steam:362620", "steam:1904540"]);
    }

    #[test]
    fn unreadable_steam_files_give_nothing() {
        assert!(parse_steam("").is_empty());
        assert!(parse_steam("{}").is_empty());
        assert!(parse_steam(r#"[["user-collections.x", 3]]"#).is_empty());
        let path = std::env::temp_dir().join(format!("ludex-test-steam-collections-{}.json", std::process::id()));
        assert!(read_steam(&path).is_empty()); // absent
        fs::write(&path, STEAM_FILE).unwrap();
        assert_eq!(read_steam(&path).len(), 2);
        let _ = fs::remove_file(path);
    }

    /// Collections Steam de ce PC, en lecture seule : `cargo test steam_collections_this_pc -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn steam_collections_this_pc() {
        let Some(path) = crate::library::cloud_storage_file() else {
            println!("Steam absent ou aucun compte trouvé");
            return;
        };
        println!("Fichier : {} (existe : {})", path.display(), path.is_file());
        let list = read_steam(&path);
        println!("{} collection(s) Steam :", list.len());
        for c in &list {
            println!("  {} [{}] : {} jeu(x) {:?}", c.name, c.id, c.games.len(), c.games.iter().take(8).collect::<Vec<_>>());
        }
    }
}
