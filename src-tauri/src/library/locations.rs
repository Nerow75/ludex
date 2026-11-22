//! Emplacements que chaque source lit (dossiers, fichiers, clés du registre), pour la page de
//! diagnostic. Relus à la demande, sans scanner : seulement des tests d'existence.

use super::{amazon, epic, itch, local, program_data, riot, steam, Platform};
use serde::Serialize;
use std::path::Path;

/// Un dossier, un fichier ou une clé du registre lu par une source.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Location {
    /// Ce que c'est, dans la langue de Ludex (« Bibliothèque Steam », « Manifestes »…).
    pub label: String,
    /// Chemin complet, ou clé du registre préfixée par `HKLM\` ou `HKCU\`.
    pub path: String,
    /// Présent sur ce PC.
    pub found: bool,
}

fn path(label: &str, p: &Path) -> Location {
    Location { label: label.to_string(), path: p.to_string_lossy().into_owned(), found: p.exists() }
}

/// Clé du registre : `hive` = `HKLM` ou `HKCU`.
fn key(label: &str, hive: &str, sub: &str) -> Location {
    #[cfg(windows)]
    let found = match hive {
        "HKLM" => super::reg::hklm(sub).is_some(),
        _ => super::reg::hkcu(sub).is_some(),
    };
    #[cfg(not(windows))]
    let found = false;
    Location { label: label.to_string(), path: format!(r"{hive}\{sub}"), found }
}

/// Emplacements lus par une source, dans l'ordre où elle les lit.
pub fn locations(platform: Platform) -> Vec<Location> {
    match platform {
        Platform::Steam => {
            let mut list = vec![
                key(tr_str!("Steam (utilisateur)", "Steam (user)"), "HKCU", r"Software\Valve\Steam"),
                key("Steam (machine)", "HKLM", r"SOFTWARE\WOW6432Node\Valve\Steam"),
            ];
            if let Some(root) = steam::steam_root() {
                list.push(path(tr_str!("Dossier de Steam", "Steam folder"), &root));
                list.push(path(tr_str!("Liste des bibliothèques", "Library list"), &root.join(r"steamapps\libraryfolders.vdf")));
                for library in steam::library_paths(&root) {
                    list.push(path(tr_str!("Bibliothèque", "Library"), &library.join("steamapps")));
                }
                match steam::localconfig_path(&root) {
                    Some(config) => list.push(path(tr_str!("Temps de jeu du compte", "Account playtime"), &config)),
                    None => list.push(Location {
                        label: tr!("Temps de jeu du compte", "Account playtime"),
                        path: tr!("Aucun compte trouvé dans userdata", "No account found in userdata"),
                        found: false,
                    }),
                }
                list.push(path(tr_str!("Succès", "Achievements"), &root.join(r"appcache\stats")));
                list.push(path(tr_str!("Images de Steam", "Steam images"), &root.join(r"appcache\librarycache")));
            }
            list
        }
        Platform::Epic => vec![path("Epic Games Launcher", &program_data().join(r"Epic\EpicGamesLauncher")), path(tr_str!("Manifestes", "Manifests"), &epic::manifests_dir())],
        Platform::Gog => vec![
            key(tr_str!("Jeux", "Games"), "HKLM", r"SOFTWARE\WOW6432Node\GOG.com\Games"),
            key("GOG Galaxy", "HKLM", r"SOFTWARE\WOW6432Node\GOG.com\GalaxyClient\paths"),
            path(tr_str!("Temps de jeu de Galaxy", "Galaxy playtime"), &program_data().join(r"GOG.com\Galaxy\storage\galaxy-2.0.db")),
        ],
        Platform::Ubisoft => vec![
            key("Ubisoft Connect", "HKLM", r"SOFTWARE\WOW6432Node\Ubisoft\Launcher"),
            key(tr_str!("Jeux", "Games"), "HKLM", r"SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs"),
        ],
        Platform::Ea => vec![path(tr_str!("Appli EA", "EA app"), &program_data().join("EA Desktop")), path("Origin", &program_data().join("Origin"))],
        Platform::Battlenet => vec![path("Battle.net", &program_data().join(r"Battle.net\Agent"))],
        Platform::Rockstar => vec![key(tr_str!("Jeux", "Games"), "HKLM", r"SOFTWARE\WOW6432Node\Rockstar Games")],
        Platform::Xbox => vec![key(
            tr_str!("Paquets de l'utilisateur", "User packages"),
            "HKCU",
            r"Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages",
        )],
        Platform::Riot => {
            let root = riot::root_dir();
            vec![path(tr_str!("Client Riot", "Riot Client"), &root.join("RiotClientInstalls.json")), path(tr_str!("Jeux", "Games"), &root.join("Metadata"))]
        }
        Platform::Amazon => {
            let root = amazon::root_dir();
            vec![path(tr_str!("Appli Amazon Games", "Amazon Games app"), &root), path(tr_str!("Jeux installés", "Installed games"), &amazon::database(&root))]
        }
        Platform::Itch => {
            let root = itch::root_dir();
            vec![path(tr_str!("Appli itch", "itch app"), &root), path(tr_str!("Jeux installés (butler)", "Installed games (butler)"), &itch::database(&root))]
        }
        Platform::Other => {
            let mut list: Vec<Location> = local::library_dirs().iter().map(|d| path(tr_str!("Dossier de jeux", "Game folder"), d)).collect();
            if list.is_empty() {
                list.push(Location {
                    label: tr!("Dossier de jeux", "Game folder"),
                    path: tr!("Aucun dossier « Jeux » ou « Games » à la racine des disques", "No \"Jeux\" or \"Games\" folder at the root of the drives"),
                    found: false,
                });
            }
            list.push(key(tr_str!("Programmes installés", "Installed programs"), "HKLM", r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"));
            list.push(key(tr_str!("Programmes installés (32 bits)", "Installed programs (32-bit)"), "HKLM", r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"));
            list.push(key(tr_str!("Programmes de l'utilisateur", "User programs"), "HKCU", r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"));
            list
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_source_lists_where_it_reads() {
        for p in [
            Platform::Steam,
            Platform::Epic,
            Platform::Gog,
            Platform::Ubisoft,
            Platform::Ea,
            Platform::Battlenet,
            Platform::Rockstar,
            Platform::Xbox,
            Platform::Riot,
            Platform::Amazon,
            Platform::Itch,
            Platform::Other,
        ] {
            let list = locations(p);
            // Steam absent : au moins ses deux clés du registre.
            assert!(!list.is_empty(), "{p:?}");
            assert!(list.iter().all(|l| !l.label.is_empty() && !l.path.is_empty()), "{p:?}");
        }
        let key = key("Clé", "HKLM", r"SOFTWARE\Ludex\Absente\Pour\Le\Test");
        assert_eq!(key.path, r"HKLM\SOFTWARE\Ludex\Absente\Pour\Le\Test");
        assert!(!key.found);
    }
}
