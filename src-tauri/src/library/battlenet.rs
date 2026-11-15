//! Battle.net : chaque jeu Blizzard (et Call of Duty) garde à sa racine un `.product.db` et un
//! `.build.info`, dont la colonne `Product` (ou, sur les anciennes versions, la fin de `CDN Path`)
//! donne le code produit. Les dossiers viennent de la recherche des dossiers de jeux (`local.rs`)
//! et de la désinstallation Windows (le désinstalleur Blizzard y déclare chaque jeu).
//! Lancement par `battlenet://<code>` quand le client est installé, sinon par l'exécutable.

use super::local::{self, Kind};
use super::{program_data, Game, Launch, Platform};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// Code produit (`.build.info`) → code de lancement de `battlenet://`, et titre.
fn known(product: &str) -> Option<(&'static str, &'static str)> {
    Some(match product {
        "wow" => ("WoW", "World of Warcraft"),
        "wow_classic" | "wowc" => ("WoWC", "World of Warcraft Classic"),
        "d3" => ("D3", "Diablo III"),
        "fenris" => ("Fen", "Diablo IV"),
        "osi" => ("OSI", "Diablo II: Resurrected"),
        "anbs" => ("ANBS", "Diablo Immortal"),
        "pro" => ("Pro", "Overwatch 2"),
        "hs" | "hsb" | "wtcg" => ("WTCG", "Hearthstone"),
        "hero" | "heroes" => ("Hero", "Heroes of the Storm"),
        "s2" | "sc2" => ("S2", "StarCraft II"),
        "s1" | "sc1" => ("S1", "StarCraft Remastered"),
        "w3" | "war3" => ("W3", "Warcraft III: Reforged"),
        "rtro" => ("RTRO", "Blizzard Arcade Collection"),
        "wlby" => ("WLBY", "Crash Bandicoot 4"),
        "odin" => ("ODIN", "Call of Duty: Modern Warfare"),
        "lazr" | "lazarus" => ("LAZR", "Call of Duty: Modern Warfare II"),
        "viper" | "vipr" => ("VIPR", "Call of Duty: Black Ops 4"),
        "zeus" => ("ZEUS", "Call of Duty: Black Ops Cold War"),
        "fore" => ("FORE", "Call of Duty: Vanguard"),
        "auks" => ("AUKS", "Call of Duty"),
        _ => return None,
    })
}

/// Code produit lu dans `.build.info` : tableau `Nom!TYPE:taille|…`, une ligne par branche.
fn product_code(build_info: &str) -> Option<String> {
    let mut lines = build_info.lines().filter(|l| !l.trim().is_empty());
    let header: Vec<&str> = lines.next()?.split('|').map(|c| c.split('!').next().unwrap_or(c)).collect();
    let row: Vec<&str> = lines.next()?.split('|').collect();
    let column = |name: &str| header.iter().position(|h| h.eq_ignore_ascii_case(name)).and_then(|i| row.get(i)).map(|v| v.trim());
    column("Product")
        .filter(|v| !v.is_empty())
        .or_else(|| column("CDN Path").and_then(|p| p.rsplit('/').next()).filter(|v| !v.is_empty()))
        .map(str::to_lowercase)
}

fn build(dir: &Path, client: bool) -> Option<Game> {
    let code = product_code(&fs::read_to_string(dir.join(".build.info")).ok()?)?;
    let folder = dir.file_name().and_then(|n| n.to_str()).unwrap_or("Battle.net");
    let (title, launch) = match known(&code) {
        Some((launch_code, title)) if client => (title.to_string(), Launch::Uri(format!("battlenet://{launch_code}"))),
        known => {
            let exe = local::main_exe(dir)?;
            (known.map_or_else(|| local::pretty_folder(folder), |(_, t)| t.to_string()), Launch::Exec { path: exe, args: Vec::new() })
        }
    };
    let mut game = Game::new(Platform::Battlenet, &code, &title, launch);
    game.install_dir = Some(dir.to_string_lossy().into_owned());
    Some(game)
}

pub fn scan() -> Result<Option<Vec<Game>>, String> {
    let client = program_data().join(r"Battle.net\Agent").is_dir();
    let mut seen = HashSet::new();
    let games: Vec<Game> = local::candidates()
        .into_iter()
        .filter(|d| local::kind(d) == Kind::Battlenet)
        .filter_map(|d| build(&d, client))
        .filter(|g| seen.insert(g.platform_id.clone()))
        .collect();
    Ok((client || !games.is_empty()).then_some(games))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_product_code_and_builds_launch() {
        let old = "Branch!STRING:0|Active!DEC:1|CDN Path!STRING:0|Version!STRING:0\neu|1|tpr/sc2|5.0.14\n";
        assert_eq!(product_code(old).as_deref(), Some("sc2"));
        let new = "Branch!STRING:0|Product!STRING:0|CDN Path!STRING:0\neu|fenris|tpr/fenris\n";
        assert_eq!(product_code(new).as_deref(), Some("fenris"));
        assert_eq!(product_code("vide"), None);

        let dir = std::env::temp_dir().join(format!("ludex-bnet-{}", std::process::id())).join("StarCraft II");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(".build.info"), old).unwrap();
        fs::write(dir.join("StarCraft II.exe"), b"").unwrap();

        let with_client = build(&dir, true).unwrap();
        assert_eq!(with_client.id, "battlenet:sc2");
        assert_eq!(with_client.title, "StarCraft II");
        assert_eq!(with_client.launch_label, "battlenet://S2");

        let without = build(&dir, false).unwrap();
        assert_eq!(without.launch_label, "StarCraft II.exe");
        let _ = fs::remove_dir_all(dir.parent().unwrap());
    }
}
