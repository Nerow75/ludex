//! Détection des jeux installés, launcher par launcher.
//! Chaque source lit uniquement des fichiers locaux : aucune connexion à un compte n'est nécessaire.

mod achievements;
mod amazon;
mod battlenet;
mod ea;
mod epic;
mod gog;
mod itch;
mod kvbin;
mod local;
mod locations;
pub use locations::{locations, Location};
pub use local::exe_icon;
pub mod manual;
mod riot;
mod rockstar;
mod steam;
mod ubisoft;
pub(crate) mod vdf;
mod xbox;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Steam,
    Epic,
    Riot,
    Gog,
    Ubisoft,
    Rockstar,
    Xbox,
    Battlenet,
    Ea,
    /// Amazon Games (Prime Gaming).
    Amazon,
    /// Appli itch (itch.io).
    Itch,
    /// Lanceurs tiers et jeux sans launcher, trouvés dans les dossiers de jeux.
    Other,
}

/// Comment demander au launcher de lancer le jeu.
#[derive(Clone, Debug)]
pub enum Launch {
    /// Protocole enregistré par le launcher (steam://, com.epicgames.launcher://...).
    Uri(String),
    /// Exécutable du launcher avec ses arguments.
    Exec { path: PathBuf, args: Vec<String> },
}

impl Platform {
    /// Nom du launcher, en français.
    pub fn label(self) -> &'static str {
        match self {
            Platform::Steam => "Steam",
            Platform::Epic => "Epic Games",
            Platform::Riot => "Riot",
            Platform::Gog => "GOG",
            Platform::Ubisoft => "Ubisoft",
            Platform::Rockstar => "Rockstar",
            Platform::Xbox => "Xbox",
            Platform::Battlenet => "Battle.net",
            Platform::Ea => "EA",
            Platform::Amazon => "Amazon Games",
            Platform::Itch => "itch.io",
            Platform::Other => "Dossiers de jeux (Autres)",
        }
    }
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
    /// Icône de l'exécutable, montrée sur la jaquette générée quand il n'y a pas de jaquette.
    pub icon_url: Option<String>,
    /// Dernière session connue du launcher (secondes Unix).
    pub last_played: Option<u64>,
    /// Taille sur le disque en octets, si le launcher la fournit.
    pub size_on_disk: Option<u64>,
    /// Taille mesurée par Ludex (dossier d'installation, `storage.rs`) faute de chiffre du launcher.
    #[serde(skip)]
    pub size_measured: bool,
    /// Temps de jeu en minutes. `None` : inconnu (ni le launcher ni Ludex n'ont de chiffre).
    pub playtime_minutes: Option<u64>,
    /// Provenance du temps de jeu : jamais additionnée, l'une ou l'autre.
    pub playtime_source: Option<PlaytimeSource>,
    /// Succès débloqués sur le total (Steam seulement). `None` : pas de succès ou inconnu.
    pub achievements: Option<Achievements>,
    /// Mise à jour ou téléchargement en attente chez le launcher (Steam, Epic), `Ready` sinon.
    pub install_state: InstallState,
    /// Le launcher compte lui-même le temps de jeu (Steam, GOG Galaxy) : Ludex ne le mesure pas,
    /// pour ne jamais compter deux fois une session.
    #[serde(skip)]
    pub launcher_tracks_playtime: bool,
    /// Application ou outil plutôt qu'un jeu (masqué par défaut).
    pub is_tool: bool,
    /// Ajouté à la main par l'utilisateur (`manual.rs`) : il peut le retirer.
    pub manual: bool,
    /// Préférences de l'utilisateur, appliquées par `lib.rs` depuis le stockage local.
    pub favorite: bool,
    pub hidden: bool,
    pub launch_label: String,
    #[serde(skip)]
    pub launch: Launch,
    /// L'exécutable lancé est un lanceur tiers (Ankama, Pokeisland) qui ne transmet pas
    /// d'options au jeu (`launch_args.rs`).
    #[serde(skip)]
    pub via_launcher: bool,
    /// Options de lancement choisies par l'utilisateur, déjà validées (`launch_args.rs`).
    /// Absentes si ce launcher n'en accepte pas.
    pub launch_args: Option<String>,
    /// Pourquoi ce jeu ne peut pas recevoir d'options, `None` s'il le peut.
    pub launch_args_refusal: Option<String>,
    /// Images trouvées dans le cache local du launcher, servies par le protocole `ludex`.
    #[serde(skip)]
    pub local_art: LocalArt,
    /// Appid Steam : jeu Steam, table Rockstar, ou jeu retrouvé sur la boutique par `art.rs`.
    #[serde(skip)]
    pub steam_appid: Option<u32>,
    /// Genres, studio, éditeur, sortie et description (`metadata.rs`), `None` tant qu'inconnus.
    /// Genres corrigés par l'utilisateur posés par-dessus (`corrections.rs`).
    pub details: Option<crate::metadata::Details>,
    /// Les genres de `details` sont ceux choisis par l'utilisateur, pas ceux de Steam.
    pub custom_genres: bool,
    /// Alias de recherche choisis par l'utilisateur (`corrections.rs`), déjà nettoyés.
    pub aliases: Vec<String>,
    /// Progression choisie par l'utilisateur (`collections.rs`), `None` : aucun statut.
    pub status: Option<crate::collections::Status>,
    /// Note perso de l'utilisateur, déjà nettoyée.
    pub note: Option<String>,
    /// Images choisies par l'utilisateur (`covers.rs`) : jaquette, image de fond.
    pub custom_art: Vec<crate::covers::Kind>,
    /// Durée de vie moyenne (HowLongToBeat, `hltb.rs`), `None` tant qu'inconnue.
    pub hltb: Option<crate::hltb::Hltb>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PlaytimeSource {
    /// Chiffre lu dans les fichiers du launcher.
    Launcher,
    /// Sessions mesurées par Ludex après un lancement depuis Ludex.
    Ludex,
}

/// État de l'installation chez le launcher. Le jeu reste lançable dans tous les cas :
/// le launcher termine lui-même la mise à jour ou la réparation au lancement.
/// Sérialisé `{ "kind": "updating", "progress": 42 }`, progression en % si connue.
#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InstallState {
    #[default]
    Ready,
    /// Mise à jour disponible, pas encore lancée.
    UpdateRequired,
    /// Mise à jour lancée, en attente dans la file de téléchargement.
    Queued,
    /// Téléchargement, installation ou vérification des fichiers en cours.
    Updating { progress: Option<u8> },
    /// Mise à jour mise en pause.
    Paused { progress: Option<u8> },
    /// Fichiers manquants ou à vérifier, installation interrompue.
    Incomplete,
}

/// Progression des succès d'un jeu, lue chez le launcher.
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Achievements {
    pub unlocked: u32,
    pub total: u32,
    /// Date du dernier succès débloqué (secondes Unix), si le launcher la garde.
    pub last_unlocked: Option<u64>,
    /// Date de chaque succès débloqué qui en a une (secondes Unix, croissantes), pour le récap.
    /// Jamais envoyé au front : il n'a besoin que des compteurs.
    #[serde(skip)]
    pub unlock_times: Vec<u64>,
}

/// Fichiers image locaux d'un jeu. Jamais envoyés au front : il ne voit qu'une URL `ludex`.
#[derive(Clone, Debug, Default)]
pub struct LocalArt {
    pub cover: Option<PathBuf>,
    pub hero: Option<PathBuf>,
    pub logo: Option<PathBuf>,
    /// Exécutable dont l'icône est extraite à la demande.
    pub icon: Option<PathBuf>,
}

impl LocalArt {
    pub fn get(&self, kind: &str) -> Option<&PathBuf> {
        match kind {
            "cover" => self.cover.as_ref(),
            "hero" => self.hero.as_ref(),
            "logo" => self.logo.as_ref(),
            "icon" => self.icon.as_ref(),
            _ => None,
        }
    }

    /// Emplacement d'une image (`cover`, `hero`, `logo`), pour y poser un fichier.
    pub fn slot(&mut self, kind: &str) -> Option<&mut Option<PathBuf>> {
        match kind {
            "cover" => Some(&mut self.cover),
            "hero" => Some(&mut self.hero),
            "logo" => Some(&mut self.logo),
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
            Platform::Battlenet => "battlenet",
            Platform::Ea => "ea",
            Platform::Amazon => "amazon",
            Platform::Itch => "itch",
            Platform::Other => "other",
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
            icon_url: None,
            last_played: None,
            size_on_disk: None,
            size_measured: false,
            playtime_minutes: None,
            playtime_source: None,
            achievements: None,
            install_state: InstallState::Ready,
            launcher_tracks_playtime: false,
            is_tool: false,
            manual: false,
            favorite: false,
            hidden: false,
            launch_label: launch.label(),
            launch,
            via_launcher: false,
            launch_args: None,
            launch_args_refusal: None,
            local_art: LocalArt::default(),
            steam_appid: None,
            details: None,
            status: None,
            note: None,
            custom_genres: false,
            aliases: Vec::new(),
            custom_art: Vec::new(),
            hltb: None,
        }
    }

    /// Adresse donnée au front pour une image (`cover`, `hero`, `logo`).
    pub fn image_url(&mut self, kind: &str) -> Option<&mut Option<String>> {
        match kind {
            "cover" => Some(&mut self.cover_url),
            "hero" => Some(&mut self.hero_url),
            "logo" => Some(&mut self.logo_url),
            _ => None,
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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceReport {
    pub platform: Platform,
    /// Le launcher est-il installé sur ce PC ?
    pub installed: bool,
    pub count: usize,
    pub error: Option<String>,
    /// Désactivé dans les Paramètres : ses jeux (`count`) sont écartés de la bibliothèque.
    pub disabled: bool,
    /// Durée de lecture de la source en millisecondes (page de diagnostic, pas envoyée au scan).
    #[serde(skip)]
    pub millis: u64,
    /// Jeux vus mais écartés, et pourquoi (page de diagnostic, pas envoyés au scan).
    #[serde(skip)]
    pub skipped: Vec<Skipped>,
}

/// Jeu vu par une source mais écarté de la bibliothèque, et pourquoi.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Skipped {
    pub title: String,
    pub reason: String,
}

thread_local! {
    /// Écarts notés par la source en cours de lecture. `None` hors de `gather` : rien n'est gardé.
    static SKIPPED: std::cell::RefCell<Option<Vec<Skipped>>> = const { std::cell::RefCell::new(None) };
}

/// Note un jeu écarté par la source en cours de lecture (sans effet hors d'un scan).
pub(crate) fn skip(title: &str, reason: &str) {
    SKIPPED.with(|s| {
        if let Some(list) = s.borrow_mut().as_mut() {
            list.push(Skipped { title: title.trim().to_string(), reason: reason.to_string() });
        }
    });
}

/// Rapports du dernier scan et sa date (secondes Unix), pour la page de diagnostic.
static LAST_SCAN: std::sync::Mutex<Option<(u64, Vec<SourceReport>)>> = std::sync::Mutex::new(None);

/// Rapports du dernier scan, `None` avant le premier.
pub fn last_scan() -> Option<(u64, Vec<SourceReport>)> {
    LAST_SCAN.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub games: Vec<Game>,
    pub sources: Vec<SourceReport>,
    /// Premier affichage du démarrage : sources lentes (EA, Battle.net, « Autres ») encore en
    /// lecture, absentes de `games` et de `sources`. Le complément arrive par `library-changed`.
    pub partial: bool,
}

/// `Ok(None)` : launcher absent. `Ok(Some(..))` : launcher présent, liste (éventuellement vide).
type SourceFn = fn() -> Result<Option<Vec<Game>>, String>;

/// Dossiers où les launchers déclarent leurs jeux, surveillés pour rescanner tout seul.
/// Les sources lues dans le registre (GOG, Ubisoft, Rockstar, Xbox) n'y sont pas.
pub struct WatchedDirs {
    /// `steamapps` de chaque bibliothèque Steam : un `appmanifest_<appid>.acf` par jeu.
    pub steamapps: Vec<PathBuf>,
    /// Manifestes Epic (`*.item`), même absent.
    pub epic: PathBuf,
    /// Dossier Riot (`RiotClientInstalls.json`, `Metadata`), même absent.
    pub riot: PathBuf,
    /// Dossiers dont chaque sous-dossier est un jeu (« Jeux », « Games », « Jeux Epic »…).
    pub libraries: Vec<PathBuf>,
}

pub fn watched_dirs() -> WatchedDirs {
    WatchedDirs { steamapps: steam::steamapps_dirs(), epic: epic::manifests_dir(), riot: riot::root_dir(), libraries: local::library_dirs() }
}

/// Dossiers de jeux lus à la racine des disques (écran de premier lancement).
pub fn game_folders() -> Vec<PathBuf> {
    local::library_dirs()
}

pub use achievements::Achievement;

/// Lecture à la demande du détail des succès Steam de ce PC (jamais pendant le scan).
pub struct SteamStats {
    dir: PathBuf,
    account: u64,
}

impl SteamStats {
    /// `None` : Steam absent ou aucun compte connu.
    pub fn locate() -> Option<Self> {
        steam::stats_location().map(|(dir, account)| SteamStats { dir, account })
    }

    /// Tous les succès d'un jeu Steam (appid), dans l'ordre du schéma, sans pourcentage mondial.
    pub fn details(&self, appid: &str) -> Option<Vec<Achievement>> {
        achievements::details(&self.dir, self.account, appid)
    }
}

pub use epic::item_installed;
pub use steam::account_dir as steam_account_dir;
pub use steam::{cloud_storage_file, manifest_installed, read_manifest as read_steam_manifest, steam_root, ManifestState};

/// Scan de toutes les sources (relevés réels des tests).
#[cfg(test)]
pub fn scan() -> ScanResult {
    scan_except(&[])
}

/// Sources dans l'ordre de priorité : un jeu vu par deux launchers (même dossier) reste au premier.
/// « Autres » passe en dernier : un dossier déjà lu par un launcher n'y est jamais repris.
const SOURCES: [(Platform, SourceFn); 12] = [
    (Platform::Steam, steam::scan),
    (Platform::Epic, epic::scan),
    (Platform::Gog, gog::scan),
    (Platform::Ubisoft, ubisoft::scan),
    (Platform::Ea, ea::scan),
    (Platform::Battlenet, battlenet::scan),
    (Platform::Rockstar, rockstar::scan),
    (Platform::Xbox, xbox::scan),
    (Platform::Riot, riot::scan),
    (Platform::Amazon, amazon::scan),
    (Platform::Itch, itch::scan),
    (Platform::Other, local::scan),
];

/// Sources lentes, lues après le premier affichage au démarrage (`scan_fast` puis `finish`) :
/// EA, Battle.net et « Autres » parcourent les dossiers de jeux de tous les disques
/// (`local::candidates`, partagé : la première des trois paie le parcours).
const SLOW: [Platform; 3] = [Platform::Ea, Platform::Battlenet, Platform::Other];

/// Scan sans les jeux des launchers désactivés dans les Paramètres. Un launcher désactivé est lu
/// quand même : ses dossiers restent réservés, donc « Autres » ne reprend pas ses jeux.
pub fn scan_except(disabled: &[Platform]) -> ScanResult {
    // Un jeu peut avoir été installé depuis le dernier scan : les disques sont relus.
    local::forget_candidates();
    let (games, reports) = gather(&SOURCES, disabled);
    complete(games, reports)
}

/// Scan progressif, première moitié : les sources rapides seulement, résultat marqué `partial`.
/// Les lectures sont gardées dans `Pending` pour que `finish` donne exactement le résultat d'un
/// scan complet (même ordre de priorité, mêmes écarts).
pub fn scan_fast(disabled: &[Platform]) -> (ScanResult, Pending) {
    local::forget_candidates();
    let reads = read_fast(&SOURCES, &SLOW);
    let (mut games, sources) = combine(reads.clone(), disabled);
    games.iter_mut().for_each(add_icon);
    games.sort_by_key(|g| g.title.to_lowercase());
    (ScanResult { games, sources, partial: true }, Pending { reads, disabled: disabled.to_vec() })
}

/// Lectures des sources rapides, en attendant les sources lentes.
pub struct Pending {
    reads: Vec<Read>,
    disabled: Vec<Platform>,
}

/// Scan progressif, seconde moitié : lit les sources lentes, puis refait l'agrégation de toutes
/// les sources dans l'ordre de priorité. Identique à `scan_except` avec les mêmes lectures.
pub fn finish(pending: Pending) -> ScanResult {
    let reads = read_rest(&SOURCES, &SLOW, pending.reads);
    let (games, reports) = combine(reads, &pending.disabled);
    complete(games, reports)
}

/// Fin d'un scan complet : rapports gardés pour le diagnostic, icônes, tri par titre.
fn complete(mut games: Vec<Game>, reports: Vec<SourceReport>) -> ScanResult {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    *LAST_SCAN.lock().unwrap_or_else(|e| e.into_inner()) = Some((now, reports.clone()));
    games.iter_mut().for_each(add_icon);
    games.sort_by_key(|g| g.title.to_lowercase());
    ScanResult { games, sources: reports, partial: false }
}

/// Lecture brute d'une source, avant l'écart des dossiers déjà vus.
#[derive(Clone)]
struct Read {
    platform: Platform,
    result: Result<Option<Vec<Game>>, String>,
    millis: u64,
    skipped: Vec<Skipped>,
}

/// Lit une source en notant sa durée et les jeux qu'elle écarte elle-même.
fn read(platform: Platform, source: SourceFn) -> Read {
    SKIPPED.with(|s| *s.borrow_mut() = Some(Vec::new()));
    let start = std::time::Instant::now();
    let result = source();
    let millis = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    let skipped = SKIPPED.with(|s| s.borrow_mut().take()).unwrap_or_default();
    Read { platform, result, millis, skipped }
}

/// Sources hors de `slow`, dans l'ordre de priorité.
fn read_fast(sources: &[(Platform, SourceFn)], slow: &[Platform]) -> Vec<Read> {
    sources.iter().filter(|(p, _)| !slow.contains(p)).map(|&(p, s)| read(p, s)).collect()
}

/// Toutes les sources dans l'ordre de priorité : celles de `slow` lues maintenant, les autres
/// reprises de `fast` (lues par `read_fast`, dans le même ordre).
fn read_rest(sources: &[(Platform, SourceFn)], slow: &[Platform], fast: Vec<Read>) -> Vec<Read> {
    let mut fast = fast.into_iter();
    sources
        .iter()
        .map(|&(p, s)| if slow.contains(&p) { read(p, s) } else { fast.next().filter(|r| r.platform == p).unwrap_or_else(|| read(p, s)) })
        .collect()
}

/// Lit les sources dans l'ordre de priorité et écarte les jeux dont le dossier est déjà vu.
/// Note aussi la durée de chaque source et les jeux écartés (page de diagnostic).
fn gather(sources: &[(Platform, SourceFn)], disabled: &[Platform]) -> (Vec<Game>, Vec<SourceReport>) {
    combine(sources.iter().map(|&(p, s)| read(p, s)).collect(), disabled)
}

/// Agrège des lectures déjà faites, dans leur ordre : un jeu dont le dossier (ou un dossier parent
/// ou enfant) est déjà pris par une lecture précédente est écarté.
fn combine(reads: Vec<Read>, disabled: &[Platform]) -> (Vec<Game>, Vec<SourceReport>) {
    let mut games = Vec::new();
    let mut reports = Vec::new();
    // Dossier déjà vu, avec le launcher et le titre du jeu qui l'occupe.
    let mut dirs: Vec<(String, Platform, String)> = Vec::new();
    for Read { platform, result: read, millis, mut skipped } in reads {
        let off = disabled.contains(&platform);
        let report = |installed: bool, count: usize, error: Option<String>, skipped: Vec<Skipped>| SourceReport {
            platform,
            installed,
            count,
            error,
            disabled: off,
            millis,
            skipped,
        };
        match read {
            Ok(Some(mut found)) => {
                found.retain(|g| {
                    let Some(dir) = g.install_dir.as_deref() else { return true };
                    let key = dir_key(dir);
                    // Même dossier, ou dossier qui contient (ou est contenu dans) un jeu déjà vu.
                    let owner = dirs.iter().find(|(d, _, _)| d == &key || is_inside(&key, d) || is_inside(d, &key));
                    match owner {
                        Some((_, by, title)) => {
                            skipped.push(Skipped { title: g.title.clone(), reason: format!("Même dossier que {title} ({})", by.label()) });
                            false
                        }
                        None => {
                            dirs.push((key, platform, g.title.clone()));
                            true
                        }
                    }
                });
                reports.push(report(true, found.len(), None, skipped));
                if !off {
                    games.extend(found);
                }
            }
            Ok(None) => reports.push(report(false, 0, None, skipped)),
            Err(e) => reports.push(report(true, 0, Some(e), skipped)),
        }
    }
    (games, reports)
}

/// Jeu sans jaquette : l'icône de son exécutable (celui qu'on lance s'il est dans le dossier du
/// jeu, sinon l'exécutable principal du dossier) servira sur la jaquette générée.
fn add_icon(game: &mut Game) {
    if game.cover_url.is_some() {
        return;
    }
    let Some(dir) = game.install_dir.as_deref().map(std::path::Path::new) else { return };
    let exe = match &game.launch {
        Launch::Exec { path, .. } if path.starts_with(dir) => Some(path.clone()),
        _ => local::main_exe(dir),
    };
    if let Some(exe) = exe {
        game.local_art.icon = Some(exe);
        game.icon_url = Some(art_url(game, "icon"));
    }
}

/// Date de création du dossier d'un jeu (secondes Unix) : son installation, sa copie sur ce disque ou
/// l'ajout du dossier. `None` si le dossier est illisible ou si le système ne garde pas cette date.
pub fn install_date(dir: &str) -> Option<i64> {
    let created = std::fs::metadata(dir).ok()?.created().ok()?;
    let secs = created.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    i64::try_from(secs).ok().filter(|&s| s > 0)
}

#[cfg(test)]
mod tests {
    use super::{Game, Launch, Platform, SourceFn};

    fn at(platform: Platform, id: &str, dir: &str) -> Game {
        let mut g = Game::new(platform, id, id, Launch::Uri(String::new()));
        g.install_dir = Some(dir.to_string());
        g
    }
    fn fake_steam() -> Result<Option<Vec<Game>>, String> {
        Ok(Some(vec![at(Platform::Steam, "620", r"D:\SteamLibrary\steamapps\common\Portal 2")]))
    }
    fn fake_epic() -> Result<Option<Vec<Game>>, String> {
        Ok(None)
    }
    fn fake_other() -> Result<Option<Vec<Game>>, String> {
        // Même dossier que le jeu Steam (vu aussi par « Autres »), puis un jeu à lui.
        Ok(Some(vec![at(Platform::Other, "portal", r"D:\SteamLibrary\steamapps\common\Portal 2"), at(Platform::Other, "dofus", r"E:\Jeux\Dofus")]))
    }
    const FAKE: [(Platform, SourceFn); 3] = [(Platform::Steam, fake_steam), (Platform::Epic, fake_epic), (Platform::Other, fake_other)];

    #[test]
    fn disabled_source_keeps_its_folders_but_not_its_games() {
        let ids = |games: &[Game]| games.iter().map(|g| g.id.clone()).collect::<Vec<_>>();
        let (games, reports) = super::gather(&FAKE, &[]);
        assert_eq!(ids(&games), ["steam:620", "other:dofus"]);
        assert!(reports.iter().all(|r| !r.disabled));

        // Steam désactivé : Portal 2 n'apparaît pas non plus sous « Autres ».
        let (games, reports) = super::gather(&FAKE, &[Platform::Steam, Platform::Epic]);
        assert_eq!(ids(&games), ["other:dofus"]);
        let steam = &reports[0];
        assert!(steam.disabled && steam.installed && steam.count == 1);
        assert!(reports[1].disabled && !reports[1].installed);
        assert!(!reports[2].disabled && reports[2].count == 1);
    }

    fn fake_ea() -> Result<Option<Vec<Game>>, String> {
        Ok(Some(vec![at(Platform::Ea, "gta", r"E:\Jeux\GTA V")]))
    }
    fn fake_rockstar() -> Result<Option<Vec<Game>>, String> {
        // Sous-dossier du jeu EA : écarté par le scan complet, mais montré au premier affichage.
        Ok(Some(vec![at(Platform::Rockstar, "gtav", r"E:\Jeux\GTA V\Grand Theft Auto V"), at(Platform::Rockstar, "rdr2", r"E:\Jeux\RDR2")]))
    }
    fn fake_battlenet() -> Result<Option<Vec<Game>>, String> {
        Err("illisible".to_string())
    }
    const PROGRESSIVE: [(Platform, SourceFn); 6] = [
        (Platform::Steam, fake_steam),
        (Platform::Ea, fake_ea),
        (Platform::Battlenet, fake_battlenet),
        (Platform::Rockstar, fake_rockstar),
        (Platform::Epic, fake_epic),
        (Platform::Other, fake_other),
    ];
    const SLOW: [Platform; 3] = [Platform::Ea, Platform::Battlenet, Platform::Other];

    #[test]
    fn progressive_scan_ends_like_a_full_scan() {
        let ids = |games: &[Game]| games.iter().map(|g| g.id.clone()).collect::<Vec<_>>();
        let summary = |reports: &[super::SourceReport]| {
            reports.iter().map(|r| (r.platform, r.installed, r.count, r.error.clone(), r.disabled, r.skipped.clone())).collect::<Vec<_>>()
        };
        for disabled in [vec![], vec![Platform::Ea], vec![Platform::Rockstar, Platform::Other]] {
            let (full_games, full_reports) = super::gather(&PROGRESSIVE, &disabled);

            let fast = super::read_fast(&PROGRESSIVE, &SLOW);
            assert_eq!(fast.iter().map(|r| r.platform).collect::<Vec<_>>(), [Platform::Steam, Platform::Rockstar, Platform::Epic]);
            let (first, first_reports) = super::combine(fast.clone(), &disabled);
            assert!(first_reports.iter().all(|r| !SLOW.contains(&r.platform)));
            if !disabled.contains(&Platform::Rockstar) {
                // Sans EA, le sous-dossier de GTA V n'est pas encore pris.
                assert!(ids(&first).contains(&"rockstar:gtav".to_string()));
            }

            let (games, reports) = super::combine(super::read_rest(&PROGRESSIVE, &SLOW, fast), &disabled);
            assert_eq!(ids(&games), ids(&full_games), "désactivés : {disabled:?}");
            assert_eq!(summary(&reports), summary(&full_reports), "désactivés : {disabled:?}");
        }
        let (games, _) = super::gather(&PROGRESSIVE, &[]);
        assert_eq!(ids(&games), ["steam:620", "ea:gta", "rockstar:rdr2", "other:dofus"]);
    }

    /// Scan progressif réel : `cargo test progressive_this_pc -- --ignored --nocapture`
    /// (temps jusqu'au premier affichage et jusqu'au scan complet, puis scan complet seul).
    #[test]
    #[ignore]
    fn progressive_this_pc() {
        let ids = |games: &[Game]| games.iter().map(|g| g.id.clone()).collect::<Vec<_>>();
        let start = std::time::Instant::now();
        let (first, pending) = super::scan_fast(&[]);
        let shown = start.elapsed();
        let done = super::finish(pending);
        let total = start.elapsed();
        println!("Premier affichage : {} ms, {} jeux", shown.as_millis(), first.games.len());
        println!("Scan complet : {} ms, {} jeux", total.as_millis(), done.games.len());
        for s in &done.sources {
            println!("  {:?} : {} ms, {} jeux", s.platform, s.millis, s.count);
        }
        let start = std::time::Instant::now();
        let full = super::scan();
        println!("Scan complet d'un bloc : {} ms", start.elapsed().as_millis());
        assert!(!done.partial && first.partial);
        assert_eq!(ids(&done.games), ids(&full.games));
    }

    #[test]
    fn install_date_of_a_folder() {
        let dir = std::env::temp_dir().join(format!("ludex-install-date-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
        let date = super::install_date(dir.to_str().unwrap()).expect("date de création lue");
        assert!((date - now).abs() < 3600, "date {date}, maintenant {now}");
        std::fs::remove_dir(&dir).unwrap();
        assert_eq!(super::install_date(dir.to_str().unwrap()), None); // dossier disparu
    }

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
    fn serializes_install_state_for_the_front() {
        use super::InstallState;
        let json = |s: InstallState| serde_json::to_string(&s).unwrap();
        assert_eq!(json(InstallState::Ready), r#"{"kind":"ready"}"#);
        assert_eq!(json(InstallState::UpdateRequired), r#"{"kind":"update_required"}"#);
        assert_eq!(json(InstallState::Updating { progress: Some(42) }), r#"{"kind":"updating","progress":42}"#);
        assert_eq!(json(InstallState::Paused { progress: None }), r#"{"kind":"paused","progress":null}"#);
    }

    #[test]
    fn nested_folders_overlap() {
        assert!(super::is_inside(r"d:\jeux ssd\gta v\grand theft auto v", r"d:\jeux ssd\gta v"));
        assert!(!super::is_inside(r"d:\jeux ssd\gta v2", r"d:\jeux ssd\gta v"));
        assert!(!super::is_inside(r"d:\jeux ssd\gta v", r"d:\jeux ssd\gta v"));
    }

    #[test]
    #[ignore]
    fn scan_this_pc() {
        let start = std::time::Instant::now();
        let result = super::scan();
        let total = start.elapsed();
        for s in &result.sources {
            println!("{:?}: installé={} jeux={} erreur={:?} durée={} ms", s.platform, s.installed, s.count, s.error, s.millis);
        }
        println!("Scan complet : {} ms", total.as_millis());
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
        let steam: Vec<_> = result.games.iter().filter(|g| g.platform == super::Platform::Steam).collect();
        let with: Vec<_> = steam.iter().filter_map(|g| Some((g, g.achievements.as_ref()?))).collect();
        println!("Succès : {} jeux Steam sur {}", with.len(), steam.len());
        for (g, a) in &with {
            let last = a.last_unlocked.map_or("aucun".to_string(), |t| format!("{t}"));
            println!("  {}  ·  {} / {}  ·  dernier {last}", g.title, a.unlocked, a.total);
        }
        for g in steam.iter().filter(|g| g.achievements.is_none()) {
            println!("  sans succès : {}", g.title);
        }
        let pending: Vec<_> = result.games.iter().filter(|g| g.install_state != super::InstallState::Ready).collect();
        println!("État d'installation : {} jeux pas prêts sur {}", pending.len(), result.games.len());
        for g in pending {
            println!("  [{:?}] {}  ·  {:?}", g.platform, g.title, g.install_state);
        }
    }
}

/// `dir` est-il un sous-dossier de `parent` ? (clés de `dir_key`)
fn is_inside(dir: &str, parent: &str) -> bool {
    dir.len() > parent.len() && dir.starts_with(parent) && dir.as_bytes()[parent.len()] == b'\\'
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
