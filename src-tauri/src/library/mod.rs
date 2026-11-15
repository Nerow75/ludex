//! Détection des jeux installés, launcher par launcher.
//! Chaque source lit uniquement des fichiers locaux : aucune connexion à un compte n'est nécessaire.

mod epic;
mod gog;
mod riot;
mod rockstar;
mod steam;
mod ubisoft;
mod vdf;
mod xbox;

use serde::Serialize;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Steam,
    Epic,
    Riot,
    Gog,
    Ubisoft,
    Rockstar,
    Xbox,
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
                format!("{} {}", exe, args.join(" ")).trim_end().to_string()
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
    /// Temps de jeu en minutes. `None` : inconnu (ni le launcher ni Ludex n'ont de chiffre).
    pub playtime_minutes: Option<u64>,
    /// Provenance du temps de jeu : jamais additionnée, l'une ou l'autre.
    pub playtime_source: Option<PlaytimeSource>,
    /// Le launcher compte lui-même le temps de jeu (Steam, GOG Galaxy) : Ludex ne le mesure pas,
    /// pour ne jamais compter deux fois une session.
    #[serde(skip)]
    pub launcher_tracks_playtime: bool,
    /// Application ou outil plutôt qu'un jeu (masqué par défaut).
    pub is_tool: bool,
    /// Préférences de l'utilisateur, appliquées par `lib.rs` depuis le stockage local.
    pub favorite: bool,
    pub hidden: bool,
    pub launch_label: String,
    #[serde(skip)]
    pub launch: Launch,
    /// Images trouvées dans le cache local du launcher, servies par le protocole `ludex`.
    #[serde(skip)]
    pub local_art: LocalArt,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PlaytimeSource {
    /// Chiffre lu dans les fichiers du launcher.
    Launcher,
    /// Sessions mesurées par Ludex après un lancement depuis Ludex.
    Ludex,
}

/// Fichiers image locaux d'un jeu. Jamais envoyés au front : il ne voit qu'une URL `ludex`.
#[derive(Clone, Debug, Default)]
pub struct LocalArt {
    pub cover: Option<PathBuf>,
    pub hero: Option<PathBuf>,
    pub logo: Option<PathBuf>,
}

impl LocalArt {
    pub fn get(&self, kind: &str) -> Option<&PathBuf> {
        match kind {
            "cover" => self.cover.as_ref(),
            "hero" => self.hero.as_ref(),
            "logo" => self.logo.as_ref(),
            _ => None,
        }
    }
}

/// Origine du protocole `ludex` dans la WebView (Windows : `http://<schéma>.localhost`).
#[cfg(windows)]
pub const ART_ORIGIN: &str = "http://ludex.localhost";
#[cfg(not(windows))]
pub const ART_ORIGIN: &str = "ludex://localhost";

/// URL d'une image locale : `/<plateforme>/<id plateforme>/<type>`, résolue côté Rust depuis l'état.
pub fn art_url(game: &Game, kind: &str) -> String {
    format!("{ART_ORIGIN}/{}/{kind}", game.id.replacen(':', "/", 1))
}

impl Game {
    pub fn new(platform: Platform, platform_id: &str, title: &str, launch: Launch) -> Self {
        let prefix = match platform {
            Platform::Steam => "steam",
            Platform::Epic => "epic",
            Platform::Riot => "riot",
            Platform::Gog => "gog",
            Platform::Ubisoft => "ubisoft",
            Platform::Rockstar => "rockstar",
            Platform::Xbox => "xbox",
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
            playtime_minutes: None,
            playtime_source: None,
            launcher_tracks_playtime: false,
            is_tool: false,
            favorite: false,
            hidden: false,
            launch_label: launch.label(),
            launch,
            local_art: LocalArt::default(),
        }
    }

    /// Temps de jeu lu chez le launcher, et dernière session si elle est plus récente.
    pub fn set_launcher_playtime(&mut self, minutes: u64, last_played: Option<u64>) {
        self.playtime_minutes = Some(minutes);
        self.playtime_source = Some(PlaytimeSource::Launcher);
        if let Some(ts) = last_played.filter(|&t| t > 0) {
            self.last_played = Some(self.last_played.map_or(ts, |l| l.max(ts)));
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
    // Ordre = priorité : un jeu vu par deux launchers (même dossier) reste au premier.
    let sources: [(Platform, SourceFn); 7] = [
        (Platform::Steam, steam::scan),
        (Platform::Epic, epic::scan),
        (Platform::Gog, gog::scan),
        (Platform::Ubisoft, ubisoft::scan),
        (Platform::Rockstar, rockstar::scan),
        (Platform::Xbox, xbox::scan),
        (Platform::Riot, riot::scan),
    ];

    let mut games = Vec::new();
    let mut reports = Vec::new();
    let mut dirs = std::collections::HashSet::new();
    for (platform, source) in sources {
        match source() {
            Ok(Some(mut found)) => {
                found.retain(|g| g.install_dir.as_deref().map_or(true, |d| dirs.insert(dir_key(d))));
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
    fn reads_xml_attributes() {
        let xml = r#"<Applications><Application Id="Game" Executable="a.exe"/></Applications>
            <ShellVisuals DefaultDisplayName="Tom &amp; Jerry" StoreLogo="s.png" />"#;
        assert_eq!(super::xml_attr(xml, "Application", "Id").as_deref(), Some("Game"));
        assert_eq!(super::xml_attr(xml, "ShellVisuals", "DefaultDisplayName").as_deref(), Some("Tom & Jerry"));
        assert_eq!(super::xml_attr(xml, "ShellVisuals", "Logo"), None); // pas StoreLogo
        assert_eq!(super::xml_attr(xml, "Missing", "Id"), None);
    }

    #[test]
    #[ignore]
    fn scan_this_pc() {
        let result = super::scan();
        for s in &result.sources {
            println!("{:?}: installé={} jeux={} erreur={:?}", s.platform, s.installed, s.count, s.error);
        }
        let hours = |g: &super::Game| match (g.playtime_minutes, g.playtime_source) {
            (Some(m), Some(source)) => format!("{:.1} h ({source:?})", m as f64 / 60.0),
            _ => "non mesuré".to_string(),
        };
        for g in &result.games {
            println!("  [{:?}] {}  ->  {}  ·  {}", g.platform, g.title, g.launch_label, hours(g));
        }
        let mut most_played: Vec<_> = result.games.iter().filter(|g| g.playtime_minutes.unwrap_or(0) > 0).collect();
        most_played.sort_by_key(|g| std::cmp::Reverse(g.playtime_minutes));
        println!("Les plus joués :");
        for g in most_played.iter().take(5) {
            println!("  {}  ·  {}", g.title, hours(g));
        }
    }
}

/// Clé de comparaison d'un dossier : casse et séparateurs ignorés.
fn dir_key(dir: &str) -> String {
    dir.replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

/// Valeur d'un attribut dans la première balise `<element …>` d'un XML simple
/// (MicrosoftGame.config, AppxManifest.xml) : pas besoin d'un parseur complet.
pub(crate) fn xml_attr(xml: &str, element: &str, attr: &str) -> Option<String> {
    let open = format!("<{element}");
    let mut from = 0;
    while let Some(i) = xml[from..].find(&open) {
        let start = from + i;
        let after = start + open.len();
        from = after;
        if !xml[after..].starts_with(|c: char| c.is_whitespace() || c == '>' || c == '/') {
            continue; // <Applications> quand on cherche <Application
        }
        let tag = &xml[after..after + xml[after..].find('>')?];
        let key = format!("{attr}=\"");
        let mut at = 0;
        while let Some(j) = tag[at..].find(&key) {
            let k = at + j;
            at = k + key.len();
            if k == 0 || tag.as_bytes()[k - 1].is_ascii_whitespace() {
                let value = &tag[at..at + tag[at..].find('"')?];
                return Some(
                    value
                        .replace("&quot;", "\"")
                        .replace("&apos;", "'")
                        .replace("&lt;", "<")
                        .replace("&gt;", ">")
                        .replace("&amp;", "&"),
                );
            }
        }
        return None;
    }
    None
}

/// Lecture du registre Windows, en lecture seule.
#[cfg(windows)]
pub(crate) mod reg {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
    use winreg::RegKey;

    pub fn hklm(path: &str) -> Option<RegKey> {
        RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(path, KEY_READ).ok()
    }

    pub fn hkcu(path: &str) -> Option<RegKey> {
        RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(path, KEY_READ).ok()
    }

    /// Sous-clés (nom, clé) d'une clé.
    pub fn children(key: &RegKey) -> Vec<(String, RegKey)> {
        key.enum_keys()
            .flatten()
            .filter_map(|name| key.open_subkey_with_flags(&name, KEY_READ).ok().map(|k| (name, k)))
            .collect()
    }

    /// Valeur texte non vide.
    pub fn string(key: &RegKey, name: &str) -> Option<String> {
        key.get_value::<String, _>(name).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
    }
}

/// Dossier ProgramData de Windows.
pub(crate) fn program_data() -> PathBuf {
    std::env::var_os("PROGRAMDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
}
