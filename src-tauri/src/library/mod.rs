//! Détection des jeux installés, launcher par launcher.
//! Chaque source lit uniquement des fichiers locaux : aucune connexion à un compte n'est nécessaire.

mod epic;
mod riot;
mod steam;
mod vdf;

use serde::Serialize;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Steam,
    Epic,
    Riot,
}

/// Comment demander au launcher de lancer le jeu.
#[derive(Clone, Debug)]
pub enum Launch {
    /// Protocole enregistré par le launcher (steam://, com.epicgames.launcher://...).
    Uri(String),
    /// Exécutable du launcher avec ses arguments.
    Exec { path: PathBuf, args: Vec<String> },
}

impl Launch {
    pub fn label(&self) -> String {
        match self {
            Launch::Uri(uri) => uri.clone(),
            Launch::Exec { path, args } => {
                let exe = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                format!("{} {}", exe, args.join(" "))
            }
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    /// Identifiant stable : "<plateforme>:<id plateforme>".
    pub id: String,
    pub title: String,
    pub platform: Platform,
    pub platform_id: String,
    pub install_dir: Option<String>,
    pub cover_url: Option<String>,
    pub hero_url: Option<String>,
    pub logo_url: Option<String>,
    /// Dernière session connue du launcher (secondes Unix).
    pub last_played: Option<u64>,
    /// Taille sur le disque en octets, si le launcher la fournit.
    pub size_on_disk: Option<u64>,
    /// Application ou outil plutôt qu'un jeu (masqué par défaut).
    pub is_tool: bool,
    /// Préférences de l'utilisateur, appliquées par `lib.rs` depuis le stockage local.
    pub favorite: bool,
    pub hidden: bool,
    pub launch_label: String,
    #[serde(skip)]
    pub launch: Launch,
}

impl Game {
    pub fn new(platform: Platform, platform_id: &str, title: &str, launch: Launch) -> Self {
        let prefix = match platform {
            Platform::Steam => "steam",
            Platform::Epic => "epic",
            Platform::Riot => "riot",
        };
        Game {
            id: format!("{prefix}:{platform_id}"),
            title: title.trim().to_string(),
            platform,
            platform_id: platform_id.to_string(),
            install_dir: None,
            cover_url: None,
            hero_url: None,
            logo_url: None,
            last_played: None,
            size_on_disk: None,
            is_tool: false,
            favorite: false,
            hidden: false,
            launch_label: launch.label(),
            launch,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceReport {
    pub platform: Platform,
    /// Le launcher est-il installé sur ce PC ?
    pub installed: bool,
    pub count: usize,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub games: Vec<Game>,
    pub sources: Vec<SourceReport>,
}

/// `Ok(None)` : launcher absent. `Ok(Some(..))` : launcher présent, liste (éventuellement vide).
type SourceFn = fn() -> Result<Option<Vec<Game>>, String>;

pub fn scan() -> ScanResult {
    let sources: [(Platform, SourceFn); 3] = [
        (Platform::Steam, steam::scan),
        (Platform::Epic, epic::scan),
        (Platform::Riot, riot::scan),
    ];

    let mut games = Vec::new();
    let mut reports = Vec::new();
    for (platform, source) in sources {
        match source() {
            Ok(Some(found)) => {
                reports.push(SourceReport { platform, installed: true, count: found.len(), error: None });
                games.extend(found);
            }
            Ok(None) => reports.push(SourceReport { platform, installed: false, count: 0, error: None }),
            Err(e) => reports.push(SourceReport { platform, installed: true, count: 0, error: Some(e) }),
        }
    }
    games.sort_by_key(|g| g.title.to_lowercase());
    ScanResult { games, sources: reports }
}

#[cfg(test)]
mod tests {
    /// Scan réel de ce PC : `cargo test scan_this_pc -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn scan_this_pc() {
        let result = super::scan();
        for s in &result.sources {
            println!("{:?}: installé={} jeux={} erreur={:?}", s.platform, s.installed, s.count, s.error);
        }
        for g in &result.games {
            println!("  [{:?}] {}  ->  {}", g.platform, g.title, g.launch_label);
        }
    }
}

/// Dossier ProgramData de Windows.
pub(crate) fn program_data() -> PathBuf {
    std::env::var_os("PROGRAMDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
}
