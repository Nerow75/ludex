//! Riot : le client est indiqué dans `ProgramData\Riot Games\RiotClientInstalls.json`,
//! chaque jeu installé a un dossier `Metadata\<produit>.<patchline>`.

use super::{program_data, Game, Launch, Platform};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

fn title_for(product: &str) -> String {
    match product {
        "league_of_legends" => "League of Legends".into(),
        "valorant" => "VALORANT".into(),
        "bacon" => "Legends of Runeterra".into(),
        "lion" => "2XKO".into(),
        "teamfighttactics" => "Teamfight Tactics".into(),
        other => other
            .split('_')
            .map(|w| {
                let mut c = w.chars();
                c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn riot_client(root: &Path) -> Option<PathBuf> {
    let text = fs::read_to_string(root.join("RiotClientInstalls.json")).ok()?;
    let json: Value = serde_json::from_str(&text).ok()?;
    ["rc_live", "rc_default"]
        .iter()
        .filter_map(|k| json.get(*k).and_then(Value::as_str))
        .map(|p| PathBuf::from(p.replace('/', "\\")))
        .find(|p| p.exists())
}

/// Lit `product_install_full_path` dans le fichier YAML de réglages du produit.
fn install_path(settings: &Path) -> Option<PathBuf> {
    let text = fs::read_to_string(settings).ok()?;
    text.lines().find_map(|line| {
        let value = line.trim().strip_prefix("product_install_full_path:")?;
        let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
        (!value.is_empty()).then(|| PathBuf::from(value.replace('/', "\\")))
    })
}

/// Dossier de `RiotClientInstalls.json`, avec `Metadata\<produit>.<patchline>` pour chaque jeu.
pub fn root_dir() -> PathBuf {
    program_data().join("Riot Games")
}

pub fn scan() -> Result<Option<Vec<Game>>, String> {
    let root = root_dir();
    let Some(client) = riot_client(&root) else { return Ok(None) };

    let mut games = Vec::new();
    let Ok(entries) = fs::read_dir(root.join("Metadata")) else { return Ok(Some(games)) };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some((product, patchline)) = name.split_once('.') else { continue };
        if product == "riot_client" {
            continue;
        }
        let settings = entry.path().join(format!("{name}.product_settings.yaml"));
        let Some(dir) = install_path(&settings).filter(|p| p.exists()) else {
            super::skip(&title_for(product), "Dossier d'installation introuvable");
            continue;
        };

        let launch = Launch::Exec {
            path: client.clone(),
            args: vec![format!("--launch-product={product}"), format!("--launch-patchline={patchline}")],
        };
        let mut game = Game::new(Platform::Riot, &name, &title_for(product), launch);
        game.install_dir = Some(dir.to_string_lossy().into_owned());
        games.push(game);
    }
    Ok(Some(games))
}
