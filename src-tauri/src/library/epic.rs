//! Epic Games : un fichier JSON `.item` par jeu installé dans
//! `ProgramData\Epic\EpicGamesLauncher\Data\Manifests`.

use super::{program_data, Game, InstallState, Launch, Platform};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Dossier des manifestes `.item`. Il n'apparaît qu'à la première installation d'un jeu.
pub fn manifests_dir() -> PathBuf {
    program_data().join(r"Epic\EpicGamesLauncher\Data\Manifests")
}

/// Le manifeste `.item` décrit-il un jeu complètement installé ? `None` : illisible (en cours d'écriture).
pub fn item_installed(text: &str) -> Option<bool> {
    let item = serde_json::from_str::<Value>(text).ok()?;
    Some(item.get("bIsIncompleteInstall").and_then(Value::as_bool) != Some(true))
}

pub fn scan() -> Result<Option<Vec<Game>>, String> {
    let launcher = program_data().join(r"Epic\EpicGamesLauncher");
    if !launcher.exists() {
        return Ok(None);
    }
    let dir = manifests_dir();
    if !dir.exists() {
        return Ok(Some(Vec::new()));
    }
    let entries = fs::read_dir(&dir).map_err(|e| tr!("Lecture des manifestes Epic impossible : {e}", "Can't read the Epic manifests: {e}"))?;

    let mut games = Vec::new();
    let mut seen = HashSet::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("item") {
            continue;
        }
        let file = entry.file_name().to_string_lossy().into_owned();
        let Some(item) = fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else {
            super::skip(&file, tr_str!("Manifeste illisible", "Unreadable manifest"));
            continue;
        };
        match parse_item(&item) {
            Ok(game) if seen.insert(game.platform_id.clone()) => games.push(game),
            Ok(game) => super::skip(&game.title, tr_str!("Manifeste en double", "Duplicate manifest")),
            Err(reason) => {
                let title = item.get("DisplayName").and_then(Value::as_str).filter(|t| !t.is_empty()).unwrap_or(&file);
                super::skip(title, reason);
            }
        }
    }
    Ok(Some(games))
}

/// Jeu décrit par un manifeste, ou la raison de l'écarter.
fn parse_item(item: &Value) -> Result<Game, &'static str> {
    let s = |key: &str| item.get(key).and_then(Value::as_str).unwrap_or("");
    let flag = |key: &str| item.get(key).and_then(Value::as_bool) == Some(true);

    let app_name = s("AppName");
    let title = s("DisplayName");
    if app_name.is_empty() || title.is_empty() {
        return Err(tr_str!("Manifeste incomplet (AppName ou nom absent)", "Incomplete manifest (no AppName or name)"));
    }
    // DLC : leur jeu principal porte un autre AppName.
    let main = s("MainGameAppName");
    if !main.is_empty() && main != app_name {
        return Err(tr_str!("Contenu additionnel (DLC)", "Additional content (DLC)"));
    }
    // Ignore les applications qui ne sont pas des jeux (Unreal Engine, etc.).
    if let Some(categories) = item.get("AppCategories").and_then(Value::as_array)
        && !categories.iter().any(|c| c.as_str() == Some("games"))
    {
        return Err(tr_str!("Application, pas un jeu", "Application, not a game"));
    }
    let install = s("InstallLocation");
    if !install.is_empty() && !Path::new(install).exists() {
        return Err(tr_str!("Dossier d'installation introuvable", "Install folder not found"));
    }
    // `bIsIncompleteInstall` : installation ou mise à jour interrompue. Gardé seulement si
    // l'exécutable est déjà là (mise à jour), une première installation n'étant pas jouable.
    let incomplete = flag("bIsIncompleteInstall");
    if incomplete && (install.is_empty() || !Path::new(install).join(s("LaunchExecutable")).is_file()) {
        return Err(tr_str!("Première installation pas terminée", "First install not finished"));
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
    // `bNeedsValidation` : Epic vérifiera les fichiers avant le prochain lancement. Les mises à jour
    // disponibles ne sont pas dans le manifeste (le launcher les demande au serveur).
    if incomplete || flag("bNeedsValidation") {
        game.install_state = InstallState::Incomplete;
    }
    Ok(game)
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

        assert_eq!(game.install_state, InstallState::Ready);
        assert_eq!(item_installed(&base.to_string()), Some(true));
        let mut incomplete = base.clone();
        incomplete["bIsIncompleteInstall"] = true.into();
        assert_eq!(item_installed(&incomplete.to_string()), Some(false));
        assert_eq!(item_installed(r#"{ "AppName": "#), None);

        let mut dlc = base.clone();
        dlc["AppName"] = "SugarDLC".into();
        assert_eq!(parse_item(&dlc).err(), Some("Contenu additionnel (DLC)"));
        // Raison d'écart en anglais (Diagnostic).
        crate::i18n::with(crate::i18n::Lang::En, || assert_eq!(parse_item(&dlc).err(), Some("Additional content (DLC)")));
    }

    #[test]
    fn reads_incomplete_and_validation_states() {
        let dir = std::env::temp_dir().join(format!("ludex-epic-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("Binaries")).unwrap();
        fs::write(dir.join("Binaries").join("Game.exe"), b"x").unwrap();
        let item = |incomplete: bool, validation: bool, exe: &str| {
            serde_json::json!({
                "AppName": "Fortnite", "DisplayName": "Fortnite", "AppCategories": ["games"],
                "InstallLocation": dir.to_string_lossy(), "LaunchExecutable": exe,
                "bIsIncompleteInstall": incomplete, "bNeedsValidation": validation
            })
        };
        let state = |v: &Value| parse_item(v).ok().map(|g| g.install_state);
        assert_eq!(state(&item(false, false, "Binaries/Game.exe")), Some(InstallState::Ready));
        assert_eq!(state(&item(false, true, "Binaries/Game.exe")), Some(InstallState::Incomplete));
        // Mise à jour interrompue : l'exécutable est là, le jeu reste listé.
        assert_eq!(state(&item(true, false, "Binaries/Game.exe")), Some(InstallState::Incomplete));
        // Première installation en cours : pas encore d'exécutable, écartée.
        assert_eq!(state(&item(true, false, "Binaries/Absent.exe")), None);
        assert_eq!(parse_item(&item(true, false, "Binaries/Absent.exe")).err(), Some("Première installation pas terminée"));
        let _ = fs::remove_dir_all(&dir);
    }
}
