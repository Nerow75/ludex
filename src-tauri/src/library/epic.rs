//! Epic Games : un fichier JSON `.item` par jeu installé dans
//! `ProgramData\Epic\EpicGamesLauncher\Data\Manifests`.

use super::{program_data, Game, Launch, Platform};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

pub fn scan() -> Result<Option<Vec<Game>>, String> {
    let launcher = program_data().join(r"Epic\EpicGamesLauncher");
    if !launcher.exists() {
        return Ok(None);
    }
    // Le dossier des manifestes n'apparaît qu'à la première installation d'un jeu.
    let dir = launcher.join(r"Data\Manifests");
    if !dir.exists() {
        return Ok(Some(Vec::new()));
    }
    let entries = fs::read_dir(&dir).map_err(|e| format!("Lecture des manifestes Epic impossible : {e}"))?;

    let mut games = Vec::new();
    let mut seen = HashSet::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("item") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else { continue };
        let Ok(item) = serde_json::from_str::<Value>(&text) else { continue };
        if let Some(game) = parse_item(&item)
            && seen.insert(game.platform_id.clone())
        {
            games.push(game);
        }
    }
    Ok(Some(games))
}

fn parse_item(item: &Value) -> Option<Game> {
    let s = |key: &str| item.get(key).and_then(Value::as_str).unwrap_or("");

    let app_name = s("AppName");
    let title = s("DisplayName");
    if app_name.is_empty() || title.is_empty() || item.get("bIsIncompleteInstall").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    // DLC : leur jeu principal porte un autre AppName.
    let main = s("MainGameAppName");
    if !main.is_empty() && main != app_name {
        return None;
    }
    // Ignore les applications qui ne sont pas des jeux (Unreal Engine, etc.).
    if let Some(categories) = item.get("AppCategories").and_then(Value::as_array)
        && !categories.iter().any(|c| c.as_str() == Some("games"))
    {
        return None;
    }
    let install = s("InstallLocation");
    if !install.is_empty() && !Path::new(install).exists() {
        return None;
    }

    let (namespace, catalog_id) = (s("CatalogNamespace"), s("CatalogItemId"));
    let uri = if namespace.is_empty() || catalog_id.is_empty() {
        format!("com.epicgames.launcher://apps/{app_name}?action=launch&silent=true")
    } else {
        format!("com.epicgames.launcher://apps/{namespace}%3A{catalog_id}%3A{app_name}?action=launch&silent=true")
    };

    let mut game = Game::new(Platform::Epic, app_name, title, Launch::Uri(uri));
    game.install_dir = (!install.is_empty()).then(|| install.to_string());
    game.size_on_disk = item.get("InstallSize").and_then(Value::as_u64).filter(|&n| n > 0);
    Some(game)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_dlc_and_builds_launch_uri() {
        let base = serde_json::json!({
            "AppName": "Sugar", "DisplayName": "Rocket League", "MainGameAppName": "Sugar",
            "CatalogNamespace": "ns", "CatalogItemId": "item", "AppCategories": ["public", "games", "applications"],
            "bIsIncompleteInstall": false
        });
        let game = parse_item(&base).unwrap();
        assert_eq!(game.id, "epic:Sugar");
        assert_eq!(game.launch_label, "com.epicgames.launcher://apps/ns%3Aitem%3ASugar?action=launch&silent=true");

        let mut dlc = base.clone();
        dlc["AppName"] = "SugarDLC".into();
        assert!(parse_item(&dlc).is_none());
    }
}
