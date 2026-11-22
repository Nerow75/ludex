//! Copie de sécurité des sauvegardes de jeux.
//!
//! Emplacements tirés de la base communautaire Ludusavi (manifeste YAML public d'environ 17 Mo,
//! <https://github.com/mtkennerly/ludusavi-manifest>) :
//! - téléchargé au plus une fois par semaine (requête conditionnelle `If-None-Match` : un manifeste
//!   inchangé ne se télécharge pas), lu une fois puis réduit à un index compact
//!   (`saves/manifeste.json` du dossier de données : règles Windows seulement, jeux qui ont des
//!   fichiers ou des clés de registre). Le YAML n'est pas gardé ;
//! - en mémoire, seulement les entrées des jeux de la bibliothèque (`Known`) : appid Steam, id GOG,
//!   titre normalisé (alias compris), sinon nom du dossier d'installation.
//!
//! Copies : `<dossier choisi>/<jeu>/<AAAA-MM-JJ_hh-mm-ss>/` (heure locale) avec `copie.json`
//! (chemins d'origine, tailles, dates) et les fichiers sous `fichiers/<lecteur>/<chemin d'origine>`.
//! Écrite dans un dossier `.tmp` renommé à la fin. `KEEP` copies gardées par jeu. Une copie
//! identique à la précédente (mêmes fichiers, tailles et dates) n'est pas refaite.
//!
//! Restauration : seulement vers les chemins d'origine enregistrés dans la copie, revérifiés contre
//! les règles du manifeste pour ce jeu ; aucun `..`, aucun lien symbolique ni jonction, écriture
//! tmp + renommage. Les fichiers présents mais absents de la copie ne sont pas supprimés.
//!
//! Limites : registre ignoré (signalé par `Entry::registry`), Windows seulement.

use crate::library::{Game, Platform};
use serde::de::{IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf, Prefix};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const MANIFEST_URL: &str = "https://raw.githubusercontent.com/mtkennerly/ludusavi-manifest/master/data/manifest.yaml";
/// Le manifeste est revérifié au plus une fois par semaine.
const REFRESH_SECS: u64 = 7 * 86_400;
/// Format de l'index compact : l'augmenter force une nouvelle lecture du manifeste.
const INDEX_VERSION: u32 = 1;
/// Manifeste refusé au-delà de cette taille (il pèse environ 17 Mo).
const MAX_MANIFEST: u64 = 64 << 20;
/// Copies gardées par jeu.
pub const KEEP: usize = 5;
/// Une copie au-delà de cette taille ou de ce nombre de fichiers est refusée : règle trop large.
const MAX_BYTES: u64 = 1 << 30;
const MAX_FILES: usize = 20_000;
/// Résultats gardés par motif du manifeste.
const MAX_MATCHES: usize = 2_000;
const INFO_FILE: &str = "copie.json";
const FILES_DIR: &str = "fichiers";
const INFO_VERSION: u32 = 1;

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Écrit dans un fichier temporaire puis le renomme : jamais de fichier à moitié écrit.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| tr!("Dossier inaccessible : {e}", "Folder not accessible: {e}"))?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).map_err(|e| tr!("Enregistrement impossible : {e}", "Couldn't save: {e}"))?;
    fs::rename(&tmp, path).map_err(|e| tr!("Enregistrement impossible : {e}", "Couldn't save: {e}"))
}

// ---------------------------------------------------------------------------------------------
// Manifeste Ludusavi
// ---------------------------------------------------------------------------------------------

/// Emplacement de sauvegarde du manifeste, pour Windows.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Rule {
    /// Motif avec variables (`<winAppData>/Jeu/*.sav`).
    pub pattern: String,
    pub save: bool,
    pub config: bool,
    /// Boutiques où la règle s'applique (noms Ludusavi : steam, epic, gog, uplay, microsoft…). Vide : toutes.
    pub stores: Vec<String>,
}

/// Jeu du manifeste, réduit à ce qui sert sous Windows.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    /// Appids Steam (principal et secondaires).
    pub steam: Vec<u64>,
    pub gog: Vec<u64>,
    /// Noms du dossier d'installation (`<game>`).
    pub install_dirs: Vec<String>,
    pub files: Vec<Rule>,
    /// Le jeu garde aussi des données dans le registre : non copiées.
    pub registry: bool,
}

/// Index compact du manifeste, gardé sur disque.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Manifest {
    pub version: u32,
    pub etag: Option<String>,
    /// Dernière vérification auprès de GitHub (secondes Unix), manifeste téléchargé ou inchangé.
    pub checked: u64,
    pub games: Vec<Entry>,
    /// Alias du manifeste : autre nom, nom de l'entrée.
    pub aliases: Vec<(String, String)>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawGame {
    alias: Option<String>,
    files: BTreeMap<String, Option<RawFile>>,
    registry: BTreeMap<String, IgnoredAny>,
    #[serde(rename = "installDir")]
    install_dir: BTreeMap<String, IgnoredAny>,
    steam: Option<RawId>,
    gog: Option<RawId>,
    id: Option<RawExtra>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawFile {
    tags: Vec<String>,
    when: Vec<RawWhen>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawWhen {
    os: Option<String>,
    store: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawId {
    id: Option<u64>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct RawExtra {
    steam_extra: Vec<u64>,
    gog_extra: Vec<u64>,
}

/// Règle utilisable sous Windows, `None` sinon (Linux, macOS).
fn windows_rule(pattern: String, raw: Option<RawFile>) -> Option<Rule> {
    let raw = raw.unwrap_or_default();
    let on_windows: Vec<&RawWhen> = raw.when.iter().filter(|w| w.os.as_deref().is_none_or(|os| os == "windows")).collect();
    if !raw.when.is_empty() && on_windows.is_empty() {
        return None;
    }
    // Chemins Linux ou macOS sans condition explicite.
    if pattern.contains("<xdg") || pattern.starts_with("<home>/Library/") {
        return None;
    }
    let mut stores: Vec<String> = Vec::new();
    let any_store = on_windows.is_empty() || on_windows.iter().any(|w| w.store.is_none());
    if !any_store {
        for s in on_windows.iter().filter_map(|w| w.store.clone()) {
            if !stores.contains(&s) {
                stores.push(s);
            }
        }
    }
    let tag = |t: &str| raw.tags.iter().any(|x| x == t);
    Some(Rule { save: tag("save"), config: tag("config"), pattern, stores })
}

/// Manifeste lu entrée par entrée : seules les entrées utiles restent en mémoire.
struct Parsed {
    games: Vec<Entry>,
    aliases: Vec<(String, String)>,
}

impl<'de> Deserialize<'de> for Parsed {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Games;
        impl<'de> Visitor<'de> for Games {
            type Value = Parsed;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str(tr_str!("la liste des jeux du manifeste", "the list of games in the manifest"))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Parsed, A::Error> {
                let mut out = Parsed { games: Vec::new(), aliases: Vec::new() };
                while let Some(name) = map.next_key::<String>()? {
                    let Some(raw) = map.next_value::<Option<RawGame>>()? else { continue };
                    if let Some(target) = raw.alias {
                        out.aliases.push((name, target));
                        continue;
                    }
                    let files: Vec<Rule> = raw.files.into_iter().filter_map(|(p, f)| windows_rule(p, f)).collect();
                    let registry = !raw.registry.is_empty();
                    if files.is_empty() && !registry {
                        continue;
                    }
                    let mut steam: Vec<u64> = raw.steam.and_then(|s| s.id).into_iter().collect();
                    let mut gog: Vec<u64> = raw.gog.and_then(|g| g.id).into_iter().collect();
                    if let Some(extra) = raw.id {
                        steam.extend(extra.steam_extra);
                        gog.extend(extra.gog_extra);
                    }
                    let install_dirs = raw.install_dir.into_keys().collect();
                    out.games.push(Entry { name, steam, gog, install_dirs, files, registry });
                }
                Ok(out)
            }
        }
        deserializer.deserialize_map(Games)
    }
}

/// Lit le manifeste YAML et n'en garde que l'index compact.
pub fn parse_manifest(yaml: &[u8]) -> Result<Manifest, String> {
    let parsed: Parsed = serde_yaml_ng::from_slice(yaml).map_err(|e| tr!("Base des sauvegardes illisible : {e}", "Unreadable save database: {e}"))?;
    Ok(Manifest { version: INDEX_VERSION, etag: None, checked: 0, games: parsed.games, aliases: parsed.aliases })
}

pub fn manifest_path(data_dir: &Path) -> PathBuf {
    data_dir.join("saves").join("manifeste.json")
}

/// Index compact enregistré, s'il est au format actuel.
pub fn load_manifest(data_dir: &Path) -> Option<Manifest> {
    let text = fs::read(manifest_path(data_dir)).ok()?;
    serde_json::from_slice::<Manifest>(&text).ok().filter(|m| m.version == INDEX_VERSION)
}

fn save_manifest(data_dir: &Path, manifest: &Manifest) -> Result<(), String> {
    let bytes = serde_json::to_vec(manifest).map_err(|e| e.to_string())?;
    write_atomic(&manifest_path(data_dir), &bytes)
}

/// Index du manifeste à jour : revérifié auprès de GitHub si la dernière vérification a plus d'une
/// semaine. Sans réseau, l'index déjà enregistré sert tel quel.
pub fn refresh_manifest(data_dir: &Path) -> Result<Manifest, String> {
    let now = unix_now();
    let current = match load_manifest(data_dir) {
        Some(m) if m.checked <= now && now - m.checked < REFRESH_SECS => return Ok(m),
        other => other,
    };
    let fetched = download(current.as_ref().and_then(|m| m.etag.as_deref()));
    match &fetched {
        Ok(_) => crate::diagnostic::ok(crate::diagnostic::Service::Saves),
        Err(e) => crate::diagnostic::failed(crate::diagnostic::Service::Saves, e),
    }
    match fetched {
        Ok(Some(Downloaded { yaml, etag })) => {
            let mut fresh = parse_manifest(&yaml)?;
            fresh.etag = etag;
            fresh.checked = now;
            save_manifest(data_dir, &fresh)?;
            Ok(fresh)
        }
        // Inchangé depuis la dernière fois : seule la date de vérification avance.
        Ok(None) => {
            let mut m = current.ok_or_else(|| tr!("Base des sauvegardes absente", "Save database missing"))?;
            m.checked = now;
            save_manifest(data_dir, &m)?;
            Ok(m)
        }
        Err(e) => match current {
            Some(m) => {
                log::warn!("Base des sauvegardes non vérifiée : {e}");
                Ok(m)
            }
            None => Err(tr!("Base des sauvegardes non téléchargée : {e}", "Save database not downloaded: {e}")),
        },
    }
}

/// Manifeste téléchargé, avec sa version (`ETag`) pour la prochaine requête conditionnelle.
struct Downloaded {
    yaml: Vec<u8>,
    etag: Option<String>,
}

/// Télécharge le manifeste. `Ok(None)` : inchangé depuis `etag`.
fn download(etag: Option<&str>) -> Result<Option<Downloaded>, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(120))
        .user_agent("Ludex")
        .build();
    let mut request = agent.get(MANIFEST_URL);
    if let Some(tag) = etag {
        request = request.set("If-None-Match", tag);
    }
    let response = request.call().map_err(|e| e.to_string())?;
    if response.status() == 304 {
        return Ok(None);
    }
    let etag = response.header("ETag").map(str::to_string);
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(MAX_MANIFEST + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| tr!("Téléchargement interrompu : {e}", "Download interrupted: {e}"))?;
    if bytes.len() as u64 > MAX_MANIFEST {
        return Err(tr!("Base des sauvegardes trop volumineuse", "Save database too large"));
    }
    Ok(Some(Downloaded { yaml: bytes, etag }))
}

/// Titre comparable : minuscules, sans accents, sans ™ ®, ponctuation remplacée par des espaces.
pub fn normalize_title(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    for c in title.chars().flat_map(char::to_lowercase) {
        let c = match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' | 'å' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' | 'ì' => 'i',
            'ô' | 'ö' | 'ó' | 'ò' | 'õ' | 'ø' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            '™' | '®' | '©' => continue,
            c if c.is_alphanumeric() => c,
            _ => ' ',
        };
        if c != ' ' || !out.ends_with(' ') {
            out.push(c);
        }
    }
    out.trim().to_string()
}

/// Recherche d'un jeu de la bibliothèque dans l'index.
pub struct Lookup<'a> {
    manifest: &'a Manifest,
    steam: HashMap<u64, usize>,
    gog: HashMap<u64, usize>,
    names: HashMap<String, usize>,
    /// Nom de dossier d'installation (minuscules), `None` s'il est partagé par plusieurs entrées.
    dirs: HashMap<String, Option<usize>>,
}

impl<'a> Lookup<'a> {
    pub fn new(manifest: &'a Manifest) -> Self {
        let mut l = Lookup { manifest, steam: HashMap::new(), gog: HashMap::new(), names: HashMap::new(), dirs: HashMap::new() };
        for (i, e) in manifest.games.iter().enumerate() {
            for id in &e.steam {
                l.steam.entry(*id).or_insert(i);
            }
            for id in &e.gog {
                l.gog.entry(*id).or_insert(i);
            }
            l.names.entry(normalize_title(&e.name)).or_insert(i);
            for d in &e.install_dirs {
                l.dirs.entry(d.to_lowercase()).and_modify(|v| *v = None).or_insert(Some(i));
            }
        }
        for (alias, target) in &manifest.aliases {
            if let Some(&i) = l.names.get(&normalize_title(target)) {
                l.names.entry(normalize_title(alias)).or_insert(i);
            }
        }
        l
    }

    /// Entrée du jeu : appid Steam, id GOG, titre, puis nom du dossier d'installation.
    pub fn find(&self, game: &Game) -> Option<&'a Entry> {
        let by_steam = game.steam_appid.and_then(|id| self.steam.get(&u64::from(id)));
        let by_gog = match game.platform {
            Platform::Gog => game.platform_id.parse::<u64>().ok().and_then(|id| self.gog.get(&id)),
            _ => None,
        };
        let by_name = || self.names.get(&normalize_title(&game.title));
        let by_dir = || {
            let dir = Path::new(game.install_dir.as_deref()?).file_name()?.to_str()?.to_lowercase();
            self.dirs.get(&dir).copied().flatten()
        };
        let i = by_steam.or(by_gog).or_else(by_name).copied().or_else(by_dir)?;
        self.manifest.games.get(i)
    }
}

/// État de la base des sauvegardes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Pas encore lue (premier téléchargement en cours).
    #[default]
    Loading,
    Ready,
    /// Téléchargement impossible et aucun index enregistré.
    Unavailable,
}

/// Entrées du manifeste des jeux de la bibliothèque, seules gardées en mémoire.
#[derive(Default)]
pub struct Known {
    pub status: Status,
    entries: HashMap<String, Entry>,
    /// Jeux déjà cherchés (id, titre, appid) et date de l'index utilisé.
    looked_up: HashSet<String>,
    checked: u64,
}

fn lookup_key(game: &Game) -> String {
    format!("{}|{}|{:?}", game.id, game.title, game.steam_appid)
}

impl Known {
    /// Un jeu n'a pas encore été cherché, ou la base n'est pas prête.
    pub fn needs_lookup(&self, games: &[Game]) -> bool {
        self.status != Status::Ready || games.iter().any(|g| !self.looked_up.contains(&lookup_key(g)))
    }

    /// Garde les entrées des jeux de la bibliothèque.
    pub fn select(&mut self, manifest: &Manifest, games: &[Game]) {
        let lookup = Lookup::new(manifest);
        self.entries = games.iter().filter_map(|g| Some((g.id.clone(), lookup.find(g)?.clone()))).collect();
        self.looked_up = games.iter().map(lookup_key).collect();
        self.checked = manifest.checked;
        self.status = Status::Ready;
    }

    pub fn entry(&self, id: &str) -> Option<&Entry> {
        self.entries.get(id)
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

// ---------------------------------------------------------------------------------------------
// Chemins
// ---------------------------------------------------------------------------------------------

/// Dossiers du joueur qui remplacent les variables du manifeste.
#[derive(Clone, Debug, Default)]
pub struct Env {
    pub home: Option<PathBuf>,
    pub app_data: Option<PathBuf>,
    pub local_app_data: Option<PathBuf>,
    pub local_low: Option<PathBuf>,
    pub documents: Option<PathBuf>,
    pub public: Option<PathBuf>,
    pub program_data: Option<PathBuf>,
    pub win_dir: Option<PathBuf>,
    pub user_name: Option<String>,
    /// Dossier de Steam (`<root>` des jeux Steam : `userdata` s'y trouve).
    pub steam_root: Option<PathBuf>,
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

impl Env {
    pub fn system() -> Env {
        let home = dirs::home_dir();
        Env {
            app_data: dirs::config_dir().or_else(|| env_path("APPDATA")),
            local_app_data: dirs::data_local_dir().or_else(|| env_path("LOCALAPPDATA")),
            local_low: home.as_ref().map(|h| h.join("AppData").join("LocalLow")),
            // Dossier connu de Windows : suit une redirection (OneDrive).
            documents: dirs::document_dir().or_else(|| home.as_ref().map(|h| h.join("Documents"))),
            public: env_path("PUBLIC"),
            program_data: env_path("ProgramData"),
            win_dir: env_path("WINDIR"),
            user_name: std::env::var("USERNAME").ok().filter(|u| !u.is_empty()),
            steam_root: crate::library::steam_root(),
            home,
        }
    }
}

/// Dossier par défaut des copies : `Documents\Ludex\Sauvegardes`.
pub fn default_dir() -> PathBuf {
    let documents = dirs::document_dir().or_else(|| dirs::home_dir().map(|h| h.join("Documents"))).unwrap_or_else(std::env::temp_dir);
    documents.join("Ludex").join("Sauvegardes")
}

/// Dossier des copies choisi par le joueur : absolu (lecteur local ou partage réseau), dossier
/// réel (pas un lien), où Ludex peut écrire.
pub fn check_dir(dir: &Path) -> Result<PathBuf, String> {
    if !dir.is_absolute() {
        return Err(tr!("Choisis un dossier complet.", "Pick a full folder path."));
    }
    match fs::symlink_metadata(dir) {
        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {}
        _ => return Err(tr!("{} n'est pas un dossier utilisable.", "{} can't be used as a folder.", dir.display())),
    }
    let probe = dir.join(".ludex-essai");
    fs::write(&probe, b"").map_err(|e| tr!("Ludex ne peut pas écrire dans {} : {e}", "Ludex can't write to {}: {e}", dir.display()))?;
    let _ = fs::remove_file(probe);
    Ok(dir.to_path_buf())
}

/// Noms Ludusavi de la boutique d'un jeu. Vide : boutique sans équivalent, toutes les règles valent.
fn stores_of(platform: Platform) -> &'static [&'static str] {
    match platform {
        Platform::Steam => &["steam"],
        Platform::Epic => &["epic", "heroic", "legendary"],
        Platform::Gog => &["gog", "gogGalaxy"],
        Platform::Ubisoft => &["uplay"],
        Platform::Ea => &["ea", "origin"],
        Platform::Xbox => &["microsoft"],
        _ => &[],
    }
}

/// Chemin comparable : séparateurs Windows, minuscules, sans séparateur final ni préfixe `\\?\`.
fn path_key(path: &Path) -> String {
    let s = path.to_string_lossy().replace('/', "\\").to_lowercase();
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    s.trim_end_matches('\\').to_string()
}

fn key_within(path: &str, dir: &str) -> bool {
    path == dir || (path.len() > dir.len() && path.starts_with(dir) && path.as_bytes()[dir.len()] == b'\\')
}

/// `path` est `dir` ou se trouve dedans (sans tenir compte de la casse).
pub fn is_within(path: &Path, dir: &Path) -> bool {
    key_within(&path_key(path), &path_key(dir))
}

/// Chemin aux séparateurs Windows, tel qu'enregistré dans les copies.
fn clean(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().replace('/', "\\"))
}

/// Valeur d'une variable de chemin pour un motif glob : caractères spéciaux échappés.
fn path_var(path: &Path) -> String {
    glob::Pattern::escape(path.to_string_lossy().replace('\\', "/").trim_end_matches('/'))
}

/// Remplace les variables `<nom>` d'un motif. `None` : variable inconnue ou sans valeur.
fn expand(pattern: &str, var: impl Fn(&str) -> Option<String>) -> Option<String> {
    let mut out = String::with_capacity(pattern.len() + 32);
    let mut rest = pattern;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after.find('>')?;
        out.push_str(&var(&after[..end])?);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// Motif absolu sur un lecteur (`C:/…`), sans `..` ni `.` : jamais relatif au dossier courant.
fn absolute_pattern(pattern: &str) -> bool {
    let b = pattern.as_bytes();
    b.len() >= 3
        && b[0].is_ascii_alphabetic()
        && b[1] == b':'
        && (b[2] == b'/' || b[2] == b'\\')
        && !pattern.split(['/', '\\']).any(|c| c == ".." || c == ".")
}

const MATCH: glob::MatchOptions = glob::MatchOptions { case_sensitive: false, require_literal_separator: true, require_literal_leading_dot: false };

/// Emplacements des sauvegardes d'un jeu : motifs du manifeste développés pour ce PC.
pub struct Scope {
    patterns: Vec<glob::Pattern>,
    /// Dossiers jamais copiés ni restaurés en entier (profil, AppData, dossier du jeu…).
    protected: Vec<String>,
    pub registry: bool,
}

impl Scope {
    pub fn new(entry: &Entry, game: &Game, env: &Env) -> Scope {
        let base = game.install_dir.as_deref().map(PathBuf::from);
        let game_dir = base.as_deref().and_then(Path::file_name).and_then(|n| n.to_str()).map(str::to_string).or_else(|| entry.install_dirs.first().cloned());
        let root = if game.platform == Platform::Steam { env.steam_root.clone() } else { None };
        let store_game_id = match (game.platform, game.steam_appid) {
            (Platform::Steam, Some(id)) => id.to_string(),
            _ => "*".to_string(),
        };
        let p = |v: &Option<PathBuf>| v.as_deref().map(path_var);
        let var = |name: &str| -> Option<String> {
            match name {
                "base" => p(&base),
                "game" => game_dir.as_deref().map(glob::Pattern::escape),
                "root" => p(&root),
                "home" => p(&env.home),
                "winAppData" => p(&env.app_data),
                "winLocalAppData" => p(&env.local_app_data),
                "winLocalAppDataLow" => p(&env.local_low),
                "winDocuments" => p(&env.documents),
                "winPublic" => p(&env.public),
                "winProgramData" => p(&env.program_data),
                "winDir" => p(&env.win_dir),
                "osUserName" => env.user_name.as_deref().map(glob::Pattern::escape),
                // N'importe quel compte : les sauvegardes de chaque compte du PC.
                "storeUserId" => Some("*".into()),
                "storeGameId" => Some(store_game_id.clone()),
                _ => None,
            }
        };
        let stores = stores_of(game.platform);
        let patterns = entry
            .files
            .iter()
            .filter(|r| r.stores.is_empty() || stores.is_empty() || r.stores.iter().any(|s| stores.contains(&s.as_str())))
            // Réglages livrés dans le dossier du jeu : pas des sauvegardes du joueur.
            .filter(|r| r.save || !r.config || !r.pattern.starts_with("<base>"))
            .filter_map(|r| expand(&r.pattern, var))
            .filter(|p| absolute_pattern(p))
            .filter_map(|p| glob::Pattern::new(&p).ok())
            .collect();
        let mut protected: Vec<String> = [&env.home, &env.app_data, &env.local_app_data, &env.local_low, &env.documents, &env.public, &env.program_data, &env.win_dir, &base, &root]
            .into_iter()
            .flatten()
            .map(|p| path_key(p))
            .collect();
        if let Some(r) = &root {
            protected.push(path_key(&r.join("userdata")));
        }
        Scope { patterns, protected, registry: entry.registry }
    }

    /// Chemin trop large pour être copié ou restauré en entier : lecteur, dossier du profil,
    /// AppData, Documents, dossier du jeu, ou l'un de leurs parents.
    fn too_broad(&self, path: &Path) -> bool {
        let key = path_key(path);
        key.split('\\').filter(|c| !c.is_empty()).count() < 3 || self.protected.iter().any(|p| key_within(p, &key))
    }

    /// Fichiers et dossiers de sauvegarde présents sur ce PC, sans doublon (un fichier déjà dans un
    /// dossier trouvé n'est pas repris). `exclude` : dossier des copies, jamais copié dans lui-même.
    pub fn locate(&self, exclude: Option<&Path>) -> Vec<PathBuf> {
        let mut found: Vec<PathBuf> = Vec::new();
        for pattern in &self.patterns {
            let Ok(paths) = glob::glob_with(pattern.as_str(), MATCH) else { continue };
            for path in paths.flatten().take(MAX_MATCHES) {
                let path = clean(&path);
                if self.too_broad(&path) || exclude.is_some_and(|e| is_within(&path, e)) {
                    continue;
                }
                // Lien symbolique ou jonction : jamais suivi.
                if fs::symlink_metadata(&path).map_or(true, |m| m.file_type().is_symlink()) {
                    continue;
                }
                found.push(path);
            }
        }
        found.sort_by_key(|p| path_key(p).len());
        let mut kept: Vec<PathBuf> = Vec::new();
        for p in found {
            if !kept.iter().any(|k| is_within(&p, k)) {
                kept.push(p);
            }
        }
        kept.sort();
        kept
    }

    /// Dossier (ou fichier) reconnu par une règle du manifeste qui contient `path`.
    fn anchor(&self, path: &Path) -> Option<PathBuf> {
        path.ancestors()
            .find(|a| !self.too_broad(a) && self.patterns.iter().any(|p| p.matches_path_with(a, MATCH)))
            .map(Path::to_path_buf)
    }
}

// ---------------------------------------------------------------------------------------------
// Copies
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    /// Fin d'une partie lancée depuis Ludex.
    Auto,
    /// « Sauvegarder maintenant ».
    Manual,
    /// État remplacé par une restauration.
    BeforeRestore,
}

/// Fichier d'une copie.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopiedFile {
    /// Chemin d'origine, seule destination possible d'une restauration.
    pub path: PathBuf,
    /// Chemin dans la copie (`fichiers/C/Users/…`, séparateurs `/`).
    pub stored: String,
    pub size: u64,
    /// Dernière modification (secondes Unix).
    pub modified: u64,
}

/// Contenu de `copie.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyInfo {
    pub version: u32,
    pub game_id: String,
    pub title: String,
    pub created: u64,
    pub reason: Reason,
    pub size: u64,
    pub files: Vec<CopiedFile>,
}

/// Copie telle que montrée au front.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopySummary {
    /// Nom du dossier de la copie (`2026-11-19_21-04-12`) : seul identifiant reçu du front.
    pub id: String,
    pub created: u64,
    pub reason: Reason,
    pub size: u64,
    pub files: usize,
}

struct Copy {
    dir: PathBuf,
    id: String,
    info: CopyInfo,
}

impl Copy {
    fn summary(&self) -> CopySummary {
        CopySummary { id: self.id.clone(), created: self.info.created, reason: self.info.reason, size: self.info.size, files: self.info.files.len() }
    }
}

/// Heure d'une copie : secondes Unix et nom du dossier en heure locale.
#[derive(Clone, Debug)]
pub struct Stamp {
    pub unix: u64,
    pub label: String,
}

impl Stamp {
    pub fn now() -> Stamp {
        let t = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
        let label = format!(
            "{:04}-{:02}-{:02}_{:02}-{:02}-{:02}",
            t.year(),
            u8::from(t.month()),
            t.day(),
            t.hour(),
            t.minute(),
            t.second()
        );
        Stamp { unix: t.unix_timestamp().max(0) as u64, label }
    }
}

/// Nom de dossier de copie : `AAAA-MM-JJ_hh-mm-ss`, éventuellement suivi de `-2`, `-3`…
pub fn valid_copy_id(id: &str) -> bool {
    let (stamp, suffix) = match id.get(19..) {
        Some(rest) if !rest.is_empty() => (&id[..19], rest),
        _ => (id, ""),
    };
    let shape = stamp.len() == 19
        && stamp.bytes().enumerate().all(|(i, b)| match i {
            4 | 7 | 13 | 16 => b == b'-',
            10 => b == b'_',
            _ => b.is_ascii_digit(),
        });
    let suffix_ok = suffix.is_empty()
        || (suffix.len() <= 4 && suffix.starts_with('-') && suffix[1..].bytes().all(|b| b.is_ascii_digit()) && suffix.len() > 1);
    shape && suffix_ok
}

/// Nom de dossier sûr sous Windows pour un titre de jeu.
fn safe_name(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'))
        .collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut name: String = cleaned.chars().take(80).collect();
    name = name.trim_end_matches(['.', ' ']).to_string();
    const RESERVED: &[&str] = &["con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9"];
    if name.is_empty() {
        return "Jeu".into();
    }
    if RESERVED.contains(&name.to_lowercase().as_str()) {
        name.push('_');
    }
    name
}

fn real_dirs(dir: &Path) -> impl Iterator<Item = PathBuf> {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir() && !t.is_symlink()))
        .map(|e| e.path())
}

/// Toutes les copies du dossier, par jeu, la plus récente d'abord.
fn scan_copies(root: &Path) -> HashMap<String, Vec<Copy>> {
    let mut by_game: HashMap<String, Vec<Copy>> = HashMap::new();
    for game_dir in real_dirs(root) {
        for dir in real_dirs(&game_dir) {
            let Some(id) = dir.file_name().and_then(|n| n.to_str()).map(str::to_string) else { continue };
            if !valid_copy_id(&id) {
                continue;
            }
            let Some(info) = fs::read(dir.join(INFO_FILE)).ok().and_then(|b| serde_json::from_slice::<CopyInfo>(&b).ok()) else { continue };
            by_game.entry(info.game_id.clone()).or_default().push(Copy { dir, id, info });
        }
    }
    for list in by_game.values_mut() {
        list.sort_by(|a, b| b.info.created.cmp(&a.info.created).then_with(|| b.id.cmp(&a.id)));
    }
    by_game
}

fn copies_of(root: &Path, game_id: &str) -> Vec<Copy> {
    scan_copies(root).remove(game_id).unwrap_or_default()
}

/// Copies d'un jeu, la plus récente d'abord.
pub fn list(root: &Path, game_id: &str) -> Vec<CopySummary> {
    copies_of(root, game_id).iter().map(Copy::summary).collect()
}

/// Dossier d'une copie de ce jeu (« Ouvrir » dans l'Explorateur), `None` : copie inconnue.
pub fn copy_dir(root: &Path, game_id: &str, copy_id: &str) -> Option<PathBuf> {
    copies_of(root, game_id).into_iter().find(|c| c.id == copy_id).map(|c| c.dir)
}

/// Copies de tous les jeux, la plus récente d'abord.
pub fn list_all(root: &Path) -> HashMap<String, Vec<CopySummary>> {
    scan_copies(root).into_iter().map(|(id, list)| (id, list.iter().map(Copy::summary).collect())).collect()
}

struct FileItem {
    path: PathBuf,
    size: u64,
    modified: u64,
}

/// Fichiers sous les emplacements trouvés, sans suivre de lien symbolique. `limits` : refuse une
/// copie démesurée (règle trop large).
fn walk(roots: &[PathBuf], exclude: Option<&Path>, limits: bool) -> Result<Vec<FileItem>, String> {
    let mut files = Vec::new();
    let mut total = 0u64;
    let mut stack: Vec<PathBuf> = roots.to_vec();
    while let Some(path) = stack.pop() {
        if exclude.is_some_and(|e| is_within(&path, e)) {
            continue;
        }
        let Ok(meta) = fs::symlink_metadata(&path) else { continue };
        let kind = meta.file_type();
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            stack.extend(fs::read_dir(&path).into_iter().flatten().flatten().map(|e| e.path()));
        } else if kind.is_file() {
            total += meta.len();
            let modified = meta.modified().ok().and_then(|m| m.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
            files.push(FileItem { path: clean(&path), size: meta.len(), modified });
            if limits && (files.len() > MAX_FILES || total > MAX_BYTES) {
                return Err(tr!(
                    "Sauvegardes trop volumineuses pour une copie (plus de 1 Go ou de 20 000 fichiers).",
                    "Saves too large to copy (more than 1 GB or 20,000 files)."
                ));
            }
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// Emplacement trouvé, avec sa taille.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub path: String,
    pub size: u64,
    pub files: usize,
}

/// Taille et nombre de fichiers de chaque emplacement.
pub fn measure(found: &[PathBuf], exclude: Option<&Path>) -> Vec<Location> {
    found
        .iter()
        .map(|p| {
            let files = walk(std::slice::from_ref(p), exclude, false).unwrap_or_default();
            Location { path: p.to_string_lossy().into_owned(), size: files.iter().map(|f| f.size).sum(), files: files.len() }
        })
        .collect()
}

/// Chemin d'un fichier dans la copie : `fichiers/<lecteur>/<chemin>`. Lecteur local seulement.
fn stored_path(path: &Path) -> Option<String> {
    let mut parts = vec![FILES_DIR.to_string()];
    for c in path.components() {
        match c {
            Component::Prefix(p) => match p.kind() {
                Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => parts.push((letter as char).to_ascii_uppercase().to_string()),
                _ => return None,
            },
            Component::RootDir => {}
            Component::Normal(n) => parts.push(n.to_str()?.to_string()),
            _ => return None,
        }
    }
    (parts.len() > 2).then(|| parts.join("/"))
}

fn same_files(copied: &[CopiedFile], now: &[FileItem]) -> bool {
    copied.len() == now.len()
        && copied.iter().zip(now).all(|(c, f)| path_key(&c.path) == path_key(&f.path) && c.size == f.size && c.modified == f.modified)
}

/// Résultat d'une copie : `unchanged` = rien n'a bougé depuis la copie précédente (renvoyée).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub copy: CopySummary,
    pub unchanged: bool,
}

/// Erreur « aucune sauvegarde trouvée » : renvoyée dans la langue de Ludex et reconnue dans les deux
/// (`e == NOTHING_FOUND`), puisque les appelants la comparent au texte reçu.
pub const NOTHING_FOUND: NothingFound = NothingFound;

#[derive(Clone, Copy, Debug)]
pub struct NothingFound;

impl NothingFound {
    const FR: &'static str = "Aucune sauvegarde trouvée sur ce PC pour ce jeu.";
    const EN: &'static str = "No saves found on this PC for this game.";

    fn matches(text: &str) -> bool {
        text == Self::FR || text == Self::EN
    }
}

impl From<NothingFound> for String {
    fn from(_: NothingFound) -> String {
        (if crate::i18n::english() { NothingFound::EN } else { NothingFound::FR }).to_string()
    }
}

impl PartialEq<NothingFound> for String {
    fn eq(&self, _: &NothingFound) -> bool {
        NothingFound::matches(self)
    }
}

/// Copie les sauvegardes trouvées dans `<root>/<jeu>/<date>/`.
pub fn backup(root: &Path, game_id: &str, title: &str, found: &[PathBuf], reason: Reason, stamp: &Stamp) -> Result<Outcome, String> {
    let files = walk(found, Some(root), true)?;
    if files.is_empty() {
        return Err(NOTHING_FOUND.into());
    }
    let existing = copies_of(root, game_id);
    if let Some(latest) = existing.first()
        && same_files(&latest.info.files, &files)
    {
        return Ok(Outcome { copy: latest.summary(), unchanged: true });
    }
    // Les copies d'un jeu restent ensemble, même renommé depuis.
    let folder = existing.first().and_then(|c| c.dir.parent().map(Path::to_path_buf)).unwrap_or_else(|| root.join(safe_name(title)));
    fs::create_dir_all(&folder).map_err(|e| tr!("Dossier des sauvegardes inaccessible ({}) : {e}", "Save folder not accessible ({}): {e}", folder.display()))?;
    // Copie interrompue (Ludex fermé, disque plein) : jamais listée, retirée ici.
    for leftover in real_dirs(&folder).filter(|d| d.extension().is_some_and(|e| e == "tmp")) {
        let _ = fs::remove_dir_all(leftover);
    }
    let mut id = stamp.label.clone();
    let mut n = 2;
    while folder.join(&id).exists() || folder.join(format!("{id}.tmp")).exists() {
        id = format!("{}-{n}", stamp.label);
        n += 1;
    }
    let tmp = folder.join(format!("{id}.tmp"));
    let written = write_copy(&tmp, game_id, title, &files, reason, stamp.unix);
    let info = match written.and_then(|info| fs::rename(&tmp, folder.join(&id)).map(|_| info).map_err(|e| tr!("Copie non finalisée : {e}", "Copy not finished: {e}"))) {
        Ok(info) => info,
        Err(e) => {
            let _ = fs::remove_dir_all(&tmp);
            return Err(e);
        }
    };
    let copy = Copy { dir: folder.join(&id), id, info };
    Ok(Outcome { copy: copy.summary(), unchanged: false })
}

fn write_copy(tmp: &Path, game_id: &str, title: &str, files: &[FileItem], reason: Reason, created: u64) -> Result<CopyInfo, String> {
    let mut copied = Vec::with_capacity(files.len());
    for f in files {
        let stored = stored_path(&f.path).ok_or_else(|| tr!("Emplacement non pris en charge : {}", "Unsupported location: {}", f.path.display()))?;
        let dest = tmp.join(stored.replace('/', "\\"));
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| tr!("Copie impossible : {e}", "Couldn't copy: {e}"))?;
        }
        let size = fs::copy(&f.path, &dest).map_err(|e| tr!("Copie de {} impossible : {e}", "Couldn't copy {}: {e}", f.path.display()))?;
        copied.push(CopiedFile { path: f.path.clone(), stored, size, modified: f.modified });
    }
    let info = CopyInfo {
        version: INFO_VERSION,
        game_id: game_id.to_string(),
        title: title.to_string(),
        created,
        reason,
        size: copied.iter().map(|f| f.size).sum(),
        files: copied,
    };
    let bytes = serde_json::to_vec_pretty(&info).map_err(|e| e.to_string())?;
    fs::write(tmp.join(INFO_FILE), bytes).map_err(|e| tr!("Copie non finalisée : {e}", "Copy not finished: {e}"))?;
    Ok(info)
}

/// Garde les `keep` copies les plus récentes d'un jeu. Les copies de `protect` ne sont jamais
/// supprimées (copie qui vient d'être restaurée, copie de sécurité). Renvoie le nombre supprimé.
pub fn rotate(root: &Path, game_id: &str, keep: usize, protect: &[&str]) -> usize {
    copies_of(root, game_id)
        .iter()
        .skip(keep)
        .filter(|c| !protect.contains(&c.id.as_str()))
        .filter(|c| fs::remove_dir_all(&c.dir).is_ok())
        .count()
}

// ---------------------------------------------------------------------------------------------
// Restauration
// ---------------------------------------------------------------------------------------------

/// Destination d'un fichier restauré : chemin absolu sur un lecteur local, sans `..`, reconnu par
/// une règle du manifeste de ce jeu, sans lien symbolique ni jonction sur le chemin.
fn checked_target(path: &Path, scope: &Scope) -> Result<PathBuf, String> {
    let refused = |why: String| Err(tr!("Restauration refusée ({}) : {why}", "Restore refused ({}): {why}", path.display()));
    let mut parts = path.components();
    if !matches!(parts.next(), Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_))) || parts.next() != Some(Component::RootDir) {
        return refused(tr!("chemin non absolu", "not an absolute path"));
    }
    if !parts.all(|c| matches!(c, Component::Normal(_))) {
        return refused(tr!("chemin relatif", "relative path"));
    }
    if scope.anchor(path).is_none() {
        return refused(tr!("emplacement inconnu du manifeste pour ce jeu", "location unknown to the save database for this game"));
    }
    for (i, a) in path.ancestors().enumerate() {
        match fs::symlink_metadata(a) {
            Ok(m) if m.file_type().is_symlink() => return refused(tr!("lien symbolique ou jonction", "symbolic link or junction")),
            Ok(m) if i == 0 && !m.is_file() => return refused(tr!("un dossier porte ce nom", "a folder has this name")),
            _ => {}
        }
    }
    Ok(path.to_path_buf())
}

/// Fichier de la copie : sous `fichiers/`, sans `..`, fichier ordinaire.
fn checked_source(copy_dir: &Path, stored: &str) -> Result<PathBuf, String> {
    let refused = || Err(tr!("Copie altérée : {stored}", "Damaged copy: {stored}"));
    let rel = Path::new(stored);
    let mut parts = rel.components();
    if parts.next() != Some(Component::Normal(FILES_DIR.as_ref())) || !parts.all(|c| matches!(c, Component::Normal(_))) {
        return refused();
    }
    let source = copy_dir.join(stored.replace('/', "\\"));
    match fs::symlink_metadata(&source) {
        Ok(m) if m.file_type().is_file() => Ok(source),
        _ => refused(),
    }
}

/// Copie `source` sur `target` : fichier temporaire à côté, puis renommage.
fn write_file(source: &Path, target: &Path) -> Result<(), String> {
    let parent = target.parent().ok_or_else(|| tr!("Destination invalide", "Invalid destination"))?;
    fs::create_dir_all(parent).map_err(|e| tr!("Dossier {} inaccessible : {e}", "Folder {} not accessible: {e}", parent.display()))?;
    let mut name = target.file_name().ok_or_else(|| tr!("Destination invalide", "Invalid destination"))?.to_os_string();
    name.push(".ludex-tmp");
    let tmp = target.with_file_name(name);
    let result = fs::copy(source, &tmp).and_then(|_| fs::rename(&tmp, target));
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(tr!("Restauration de {} impossible : {e}", "Couldn't restore {}: {e}", target.display()));
    }
    Ok(())
}

/// Résultat d'une restauration.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Restored {
    pub restored: usize,
    /// Copie de l'état remplacé (`None` : rien à copier, ou identique à une copie existante).
    pub safety: Option<CopySummary>,
}

/// Restaure une copie : vérifie tout d'abord, copie l'état actuel (copie de sécurité), puis écrit.
/// Le jeu doit être fermé (vérifié par l'appelant).
pub fn restore(root: &Path, game_id: &str, title: &str, copy_id: &str, scope: &Scope, stamp: &Stamp) -> Result<Restored, String> {
    if !valid_copy_id(copy_id) {
        return Err(tr!("Copie inconnue.", "Unknown copy."));
    }
    let copy = copies_of(root, game_id).into_iter().find(|c| c.id == copy_id).ok_or_else(|| tr!("Cette copie n'existe plus.", "This copy no longer exists."))?;
    let plan = copy
        .info
        .files
        .iter()
        .map(|f| Ok((checked_source(&copy.dir, &f.stored)?, checked_target(&f.path, scope)?)))
        .collect::<Result<Vec<_>, String>>()?;
    if plan.is_empty() {
        return Err(tr!("Cette copie est vide.", "This copy is empty."));
    }
    let safety = match backup(root, game_id, title, &scope.locate(Some(root)), Reason::BeforeRestore, stamp) {
        Ok(o) => (!o.unchanged).then_some(o.copy),
        Err(e) if e == NOTHING_FOUND => None,
        Err(e) => return Err(tr!("Copie de sécurité impossible, rien n'a été restauré : {e}", "Couldn't make a safety copy, nothing was restored: {e}")),
    };
    for (source, target) in &plan {
        write_file(source, target)?;
    }
    let mut protect = vec![copy_id];
    if let Some(s) = &safety {
        protect.push(s.id.as_str());
    }
    rotate(root, game_id, KEEP, &protect);
    Ok(Restored { restored: plan.len(), safety })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Launch;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ludex-test-saves-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Profil factice sous `dir`.
    fn env_in(dir: &Path) -> Env {
        let home = dir.join("Users").join("Joueur");
        Env {
            app_data: Some(home.join("AppData").join("Roaming")),
            local_app_data: Some(home.join("AppData").join("Local")),
            local_low: Some(home.join("AppData").join("LocalLow")),
            documents: Some(home.join("Documents")),
            public: None,
            program_data: Some(dir.join("ProgramData")),
            win_dir: None,
            user_name: Some("Joueur".into()),
            steam_root: Some(dir.join("Steam")),
            home: Some(home),
        }
    }

    fn rule(pattern: &str) -> Rule {
        Rule { pattern: pattern.into(), save: true, config: false, stores: Vec::new() }
    }

    fn steam_game(dir: &Path) -> Game {
        let mut g = Game::new(Platform::Steam, "620", "Portal 2", Launch::Uri(String::new()));
        g.install_dir = Some(dir.join("Steam").join("steamapps").join("common").join("Portal 2").to_string_lossy().into_owned());
        g.steam_appid = Some(620);
        g
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// Date de modification fixée : deux écritures dans la même seconde restent distinctes.
    fn touch(path: &Path, secs: u64) {
        let file = fs::File::options().write(true).open(path).unwrap();
        file.set_modified(UNIX_EPOCH + Duration::from_secs(secs)).unwrap();
    }

    fn stamp(unix: u64) -> Stamp {
        Stamp { unix, label: format!("2026-11-19_10-{:02}-{:02}", unix / 60 % 60, unix % 60) }
    }

    const YAML: &str = r#"---
Portal 2:
  files:
    "<root>/userdata/<storeUserId>/620/remote":
      tags: [save]
      when:
        - os: windows
          store: steam
    "<home>/Library/Application Support/Portal 2":
      when:
        - os: mac
    "<base>/portal2/cfg/config.cfg":
      tags: [config]
  installDir:
    Portal 2: {}
  launch:
    "<base>/portal2.exe":
      - when:
          - os: windows
  steam:
    id: 620
  id:
    steamExtra: [621]
Hades:
  files:
    "<winDocuments>/Saved Games/Hades":
      tags: [save, config]
      when:
        - os: windows
  registry:
    HKEY_CURRENT_USER/Software/Supergiant:
      tags: [config]
  gog:
    id: 1405
Linux Only:
  files:
    "<xdgData>/linux-only": {}
  steam:
    id: 1
Sans fichier:
  steam:
    id: 2
"Hades : Battle Out of Hell":
  alias: Hades
Vide: null
"#;

    #[test]
    fn parses_only_windows_entries() {
        let m = parse_manifest(YAML.as_bytes()).unwrap();
        assert_eq!(m.games.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), vec!["Portal 2", "Hades"]);
        let portal = &m.games[0];
        assert_eq!(portal.steam, vec![620, 621]);
        assert_eq!(portal.install_dirs, vec!["Portal 2".to_string()]);
        // La règle macOS est écartée, la configuration sans condition gardée.
        assert_eq!(portal.files.len(), 2);
        assert_eq!(portal.files.iter().find(|r| r.pattern.starts_with("<root>")).unwrap().stores, vec!["steam".to_string()]);
        assert!(!portal.registry);
        let hades = &m.games[1];
        assert!(hades.registry && hades.files[0].save && hades.files[0].config && hades.files[0].stores.is_empty());
        assert_eq!(hades.gog, vec![1405]);
        assert_eq!(m.aliases, vec![("Hades : Battle Out of Hell".to_string(), "Hades".to_string())]);
    }

    #[test]
    fn matches_library_games() {
        let m = parse_manifest(YAML.as_bytes()).unwrap();
        let lookup = Lookup::new(&m);
        let dir = std::env::temp_dir();
        assert_eq!(lookup.find(&steam_game(&dir)).unwrap().name, "Portal 2");
        // Appid secondaire.
        let mut extra = steam_game(&dir);
        extra.steam_appid = Some(621);
        extra.title = "Autre".into();
        assert_eq!(lookup.find(&extra).unwrap().name, "Portal 2");
        // GOG par id, Epic par titre normalisé, alias.
        let gog = Game::new(Platform::Gog, "1405", "Inconnu", Launch::Uri(String::new()));
        assert_eq!(lookup.find(&gog).unwrap().name, "Hades");
        let epic = Game::new(Platform::Epic, "Min", "HADES™", Launch::Uri(String::new()));
        assert_eq!(lookup.find(&epic).unwrap().name, "Hades");
        let alias = Game::new(Platform::Other, "x", "Hades: Battle out of Hell", Launch::Uri(String::new()));
        assert_eq!(lookup.find(&alias).unwrap().name, "Hades");
        // Dossier d'installation.
        let mut by_dir = Game::new(Platform::Other, "y", "P2", Launch::Uri(String::new()));
        by_dir.install_dir = Some(r"E:\Jeux\Portal 2".into());
        assert_eq!(lookup.find(&by_dir).unwrap().name, "Portal 2");
        let none = Game::new(Platform::Other, "z", "Rien", Launch::Uri(String::new()));
        assert!(lookup.find(&none).is_none());
        assert_eq!(normalize_title("Assassin’s Creed® Unity : Édition"), "assassin s creed unity edition");
    }

    #[test]
    fn expands_variables_and_escapes_paths() {
        let var = |name: &str| match name {
            "home" => Some(path_var(Path::new(r"C:\Users\Jo [x]"))),
            "storeUserId" => Some("*".into()),
            _ => None,
        };
        assert_eq!(expand("<home>/Saves/<storeUserId>/*.sav", var).unwrap(), "C:/Users/Jo [[]x[]]/Saves/*/*.sav");
        assert_eq!(expand("<xdgData>/jeu", var), None);
        assert_eq!(expand("<home/jeu", var), None);
        assert!(absolute_pattern("C:/Users/Jo/Saves"));
        assert!(!absolute_pattern("Saves/jeu"));
        assert!(!absolute_pattern("C:/Users/Jo/../Admin"));
        // Les crochets échappés désignent bien le dossier d'origine.
        let pattern = glob::Pattern::new(&expand("<home>/Saves", var).unwrap()).unwrap();
        assert!(pattern.matches_path_with(Path::new(r"c:\users\jo [x]\saves"), MATCH));
    }

    #[test]
    fn locates_saves_and_refuses_too_broad_paths() {
        let dir = temp_dir("locate");
        let env = env_in(&dir);
        let game = steam_game(&dir);
        let home = env.home.clone().unwrap();
        write(&dir.join(r"Steam\userdata\123\620\remote\save1.sav"), "a");
        write(&dir.join(r"Steam\userdata\456\620\remote\save2.sav"), "b");
        write(&home.join(r"Documents\My Games\Portal\slot.sav"), "c");
        write(&home.join(r"Documents\My Games\Portal\sub\slot2.sav"), "d");
        write(&home.join(r"AppData\Local\Portal\one.sav"), "e");
        write(&dir.join(r"Steam\steamapps\common\Portal 2\portal2\cfg\config.cfg"), "f");
        let entry = Entry {
            name: "Portal 2".into(),
            files: vec![
                rule("<root>/userdata/<storeUserId>/620/remote"),
                rule("<winDocuments>/My Games/Portal"),
                rule("<winDocuments>/My Games/Portal/sub/*.sav"), // déjà dans le dossier précédent
                rule("<winLocalAppData>/Portal/*.SAV"),          // casse ignorée
                rule("<home>"),                                  // trop large
                rule("<base>"),                                  // dossier du jeu entier
                rule("<winDocuments>"),                          // tout Documents
                rule("<winAppData>/../Local"),                   // remontée refusée
                Rule { config: true, save: false, ..rule("<base>/portal2/cfg/config.cfg") }, // réglage livré
                Rule { stores: vec!["epic".into()], ..rule("<home>/Epic/Portal") },
                rule("<xdgConfig>/portal"),
            ],
            ..Entry::default()
        };
        let scope = Scope::new(&entry, &game, &env);
        let found = scope.locate(None);
        let keys: Vec<String> = found.iter().map(|p| path_key(p.strip_prefix(&dir).unwrap())).collect();
        assert_eq!(
            keys,
            vec![
                r"steam\userdata\123\620\remote",
                r"steam\userdata\456\620\remote",
                r"users\joueur\appdata\local\portal\one.sav",
                r"users\joueur\documents\my games\portal",
            ]
        );
        let sizes = measure(&found, None);
        assert_eq!(sizes.iter().map(|l| l.files).sum::<usize>(), 5);
        let _ = fs::remove_dir_all(dir);
    }

    /// Jeu factice avec un dossier de sauvegardes dans Documents.
    fn setup(name: &str) -> (PathBuf, Env, Game, Scope, PathBuf) {
        let dir = temp_dir(name);
        let env = env_in(&dir);
        let game = steam_game(&dir);
        let saves = env.documents.clone().unwrap().join("My Games").join("Portal");
        write(&saves.join("slot1.sav"), "niveau 1");
        write(&saves.join(r"profils\joueur.cfg"), "réglages");
        let entry = Entry { name: "Portal 2".into(), files: vec![rule("<winDocuments>/My Games/Portal")], ..Entry::default() };
        let scope = Scope::new(&entry, &game, &env);
        (dir, env, game, scope, saves)
    }

    #[test]
    fn backs_up_then_skips_unchanged_copies() {
        let (dir, _, game, scope, saves) = setup("backup");
        let root = dir.join("Copies");
        let first = backup(&root, &game.id, &game.title, &scope.locate(Some(&root)), Reason::Manual, &stamp(1)).unwrap();
        assert!(!first.unchanged);
        assert_eq!((first.copy.files, first.copy.size), (2, ("niveau 1".len() + "réglages".len()) as u64));
        let copy_dir = root.join("Portal 2").join(&first.copy.id);
        let info: CopyInfo = serde_json::from_slice(&fs::read(copy_dir.join(INFO_FILE)).unwrap()).unwrap();
        assert!(info.files.iter().all(|f| f.stored.starts_with("fichiers/") && copy_dir.join(f.stored.replace('/', "\\")).is_file()));
        assert!(info.files.iter().any(|f| f.path == saves.join("slot1.sav")));

        // Rien n'a changé : pas de nouvelle copie.
        let again = backup(&root, &game.id, &game.title, &scope.locate(Some(&root)), Reason::Auto, &stamp(2)).unwrap();
        assert!(again.unchanged && again.copy.id == first.copy.id);
        // Un fichier change : nouvelle copie, même seconde = suffixe.
        write(&saves.join("slot2.sav"), "niveau 2");
        let third = backup(&root, &game.id, &game.title, &scope.locate(Some(&root)), Reason::Auto, &Stamp { unix: 3, label: first.copy.id.clone() }).unwrap();
        assert_eq!(third.copy.id, format!("{}-2", first.copy.id));
        assert!(valid_copy_id(&third.copy.id));
        let listed = list(&root, &game.id);
        assert_eq!(listed.iter().map(|c| c.created).collect::<Vec<_>>(), vec![3, 1]);
        assert!(list(&root, "steam:1").is_empty());

        // Rien à copier : erreur explicite.
        let empty = Scope::new(&Entry::default(), &game, &env_in(&dir));
        assert_eq!(backup(&root, &game.id, &game.title, &empty.locate(None), Reason::Manual, &stamp(4)).unwrap_err(), NOTHING_FOUND);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn restore_messages_in_english() {
        let (dir, env, game, scope, _) = setup("english");
        let root = dir.join("Copies");
        crate::i18n::with(crate::i18n::Lang::En, || {
            let empty = Scope::new(&Entry::default(), &game, &env);
            let err = backup(&root, &game.id, &game.title, &empty.locate(None), Reason::Manual, &stamp(1)).unwrap_err();
            // Reconnue dans les deux langues par les appelants.
            assert_eq!(err, "No saves found on this PC for this game.");
            assert_eq!(err, NOTHING_FOUND);
            assert_eq!(restore(&root, &game.id, &game.title, "../x", &scope, &stamp(2)).unwrap_err(), "Unknown copy.");
            assert_eq!(
                restore(&root, &game.id, &game.title, "2026-01-01_10-00-00", &scope, &stamp(2)).unwrap_err(),
                "This copy no longer exists."
            );
            assert_eq!(checked_target(Path::new("relatif.sav"), &scope).unwrap_err(), "Restore refused (relatif.sav): not an absolute path");
        });
        assert_eq!(String::from("Aucune sauvegarde trouvée sur ce PC pour ce jeu."), NOTHING_FOUND);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn keeps_the_most_recent_copies() {
        let (dir, _, game, scope, saves) = setup("rotate");
        let root = dir.join("Copies");
        let n = KEEP as u64 + 2;
        for i in 0..n {
            write(&saves.join("slot1.sav"), &format!("partie {i}"));
            touch(&saves.join("slot1.sav"), 1_000 + i);
            backup(&root, &game.id, &game.title, &scope.locate(Some(&root)), Reason::Auto, &stamp(100 + i)).unwrap();
        }
        assert_eq!(list(&root, &game.id).len(), KEEP + 2);
        let oldest = list(&root, &game.id).last().unwrap().id.clone();
        // La plus ancienne est protégée : la suivante part à sa place.
        assert_eq!(rotate(&root, &game.id, KEEP, &[&oldest]), 1);
        let left = list(&root, &game.id);
        assert_eq!(left.len(), KEEP + 1);
        assert!(left.iter().any(|c| c.id == oldest));
        assert_eq!(left[0].created, 100 + n - 1);
        assert_eq!(rotate(&root, &game.id, KEEP, &[]), 1);
        let left = list(&root, &game.id);
        assert_eq!(left.len(), KEEP);
        assert_eq!(left.last().unwrap().created, 102);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn restores_a_copy_after_a_safety_copy() {
        let (dir, _, game, scope, saves) = setup("restore");
        let root = dir.join("Copies");
        let first = backup(&root, &game.id, &game.title, &scope.locate(Some(&root)), Reason::Manual, &stamp(1)).unwrap();
        // La partie continue, puis une sauvegarde est abîmée et un dossier disparaît.
        write(&saves.join("slot1.sav"), "corrompu");
        fs::remove_dir_all(saves.join("profils")).unwrap();
        write(&saves.join("nouveau.sav"), "plus récent");

        let done = restore(&root, &game.id, &game.title, &first.copy.id, &scope, &stamp(2)).unwrap();
        assert_eq!(done.restored, 2);
        assert_eq!(fs::read_to_string(saves.join("slot1.sav")).unwrap(), "niveau 1");
        assert_eq!(fs::read_to_string(saves.join(r"profils\joueur.cfg")).unwrap(), "réglages");
        // Fichier absent de la copie : laissé en place. Aucun fichier temporaire.
        assert!(saves.join("nouveau.sav").is_file());
        assert!(!saves.join("slot1.sav.ludex-tmp").exists());
        // L'état remplacé est dans la copie de sécurité.
        let safety = done.safety.unwrap();
        assert_eq!(safety.reason, Reason::BeforeRestore);
        let safety_dir = root.join("Portal 2").join(&safety.id);
        let info: CopyInfo = serde_json::from_slice(&fs::read(safety_dir.join(INFO_FILE)).unwrap()).unwrap();
        let slot = info.files.iter().find(|f| f.path.ends_with("slot1.sav")).unwrap();
        assert_eq!(fs::read_to_string(safety_dir.join(slot.stored.replace('/', "\\"))).unwrap(), "corrompu");

        assert!(restore(&root, &game.id, &game.title, "../../Windows", &scope, &stamp(3)).is_err());
        assert!(restore(&root, &game.id, &game.title, "2020-01-01_00-00-00", &scope, &stamp(3)).is_err());
        let _ = fs::remove_dir_all(dir);
    }

    /// Réécrit le `copie.json` d'une copie.
    fn tamper(root: &Path, id: &str, change: impl FnOnce(&mut CopyInfo)) {
        let file = root.join("Portal 2").join(id).join(INFO_FILE);
        let mut info: CopyInfo = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        change(&mut info);
        fs::write(&file, serde_json::to_vec(&info).unwrap()).unwrap();
    }

    #[test]
    fn refuses_paths_outside_the_manifest() {
        let (dir, env, game, scope, saves) = setup("refuse");
        let root = dir.join("Copies");
        let copy = backup(&root, &game.id, &game.title, &scope.locate(Some(&root)), Reason::Manual, &stamp(1)).unwrap().copy;
        write(&saves.join("slot1.sav"), "actuel");
        let home = env.home.clone().unwrap();
        let outside = [
            home.join(r"AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup\x.bat"),
            saves.join(r"..\..\..\AppData\Roaming\x.bat"),
            PathBuf::from(r"slot1.sav"),
            PathBuf::from(r"\\serveur\partage\slot1.sav"),
            env.documents.clone().unwrap().join("x.sav"),
        ];
        for path in outside {
            tamper(&root, &copy.id, |info| info.files[0].path = path.clone());
            let err = restore(&root, &game.id, &game.title, &copy.id, &scope, &stamp(2)).unwrap_err();
            assert!(err.starts_with("Restauration refusée"), "{path:?} : {err}");
        }
        // Fichier de la copie hors de `fichiers/`.
        tamper(&root, &copy.id, |info| {
            info.files[0].path = saves.join("slot1.sav");
            info.files[0].stored = "../../../Users/Joueur/secret.txt".into();
        });
        assert!(restore(&root, &game.id, &game.title, &copy.id, &scope, &stamp(2)).unwrap_err().starts_with("Copie altérée"));
        // Rien n'a été écrit, aucune copie de sécurité.
        assert_eq!(fs::read_to_string(saves.join("slot1.sav")).unwrap(), "actuel");
        assert_eq!(list(&root, &game.id).len(), 1);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn refuses_symbolic_links() {
        let (dir, _, game, scope, saves) = setup("symlink");
        let root = dir.join("Copies");
        let copy = backup(&root, &game.id, &game.title, &scope.locate(Some(&root)), Reason::Manual, &stamp(1)).unwrap().copy;
        // Le dossier des sauvegardes remplacé par un lien (ou une jonction) vers un autre dossier.
        let elsewhere = dir.join("Ailleurs");
        fs::create_dir_all(&elsewhere).unwrap();
        fs::remove_dir_all(&saves).unwrap();
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_dir(&elsewhere, &saves).is_ok()
            // Sans le mode développeur, une jonction (même risque, aucun droit requis).
            || std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(&saves)
                .arg(&elsewhere)
                .output()
                .is_ok_and(|o| o.status.success());
        #[cfg(not(windows))]
        let linked = std::os::unix::fs::symlink(&elsewhere, &saves).is_ok();
        if !linked {
            eprintln!("Ni lien symbolique ni jonction possible ici : test ignoré");
            let _ = fs::remove_dir_all(dir);
            return;
        }
        let err = restore(&root, &game.id, &game.title, &copy.id, &scope, &stamp(2)).unwrap_err();
        assert!(err.contains("lien symbolique"), "{err}");
        assert!(fs::read_dir(&elsewhere).unwrap().next().is_none());
        // Un lien n'est jamais suivi à la copie non plus.
        assert!(scope.locate(None).is_empty());
        let _ = fs::remove_file(&saves).or_else(|_| fs::remove_dir(&saves));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn validates_copy_ids_and_names() {
        assert!(valid_copy_id("2026-11-19_21-04-12"));
        assert!(valid_copy_id("2026-11-19_21-04-12-2"));
        assert!(!valid_copy_id("2026-11-19_21-04-12-"));
        assert!(!valid_copy_id("2026-11-19_21-04-12.tmp"));
        assert!(!valid_copy_id("..\\2026-11-19_21-04"));
        assert!(!valid_copy_id(""));
        assert_eq!(safe_name("Half-Life 2: Episode One?"), "Half-Life 2 Episode One");
        assert_eq!(safe_name("CON"), "CON_");
        assert_eq!(safe_name("..."), "Jeu");
        assert_eq!(stored_path(Path::new(r"C:\Users\Jo\save.dat")).as_deref(), Some("fichiers/C/Users/Jo/save.dat"));
        assert_eq!(stored_path(Path::new(r"\\serveur\partage\save.dat")), None);
        assert_eq!(stored_path(Path::new(r"C:\Users\..\save.dat")), None);
    }

    /// Relevé réel : `cargo test saves_this_pc -- --ignored --nocapture`. Télécharge la base dans
    /// un dossier temporaire, cherche les sauvegardes des jeux de la bibliothèque, sans rien copier
    /// ni toucher aux dossiers de l'app : seul le dossier temporaire est écrit.
    #[test]
    #[ignore]
    fn saves_this_pc() {
        let tmp = std::env::temp_dir().join("ludex-saves-this-pc");
        let started = std::time::Instant::now();
        let manifest = refresh_manifest(&tmp).expect("base des sauvegardes");
        println!("Base : {} jeux pour Windows, {} alias ({:?})", manifest.games.len(), manifest.aliases.len(), started.elapsed());
        let index_size = fs::metadata(manifest_path(&tmp)).map(|m| m.len()).unwrap_or(0);
        println!("Index compact : {:.1} Mo", index_size as f64 / 1e6);

        let scan = crate::library::scan();
        let mut known = Known::default();
        let started = std::time::Instant::now();
        known.select(&manifest, &scan.games);
        println!("{} jeux sur {} reconnus dans la base ({:?})", known.len(), scan.games.len(), started.elapsed());

        let env = Env::system();
        let started = std::time::Instant::now();
        let mut with_saves = 0;
        let mut total = 0u64;
        for game in &scan.games {
            let Some(entry) = known.entry(&game.id) else { continue };
            let scope = Scope::new(entry, game, &env);
            let found = scope.locate(None);
            if found.is_empty() {
                continue;
            }
            with_saves += 1;
            let locations = measure(&found, None);
            let size: u64 = locations.iter().map(|l| l.size).sum();
            total += size;
            let registry = if scope.registry { ", registre non copié" } else { "" };
            println!("  {} ({}) : {} emplacement(s), {:.1} Mo{registry}", game.title, entry.name, locations.len(), size as f64 / 1e6);
            for l in &locations {
                println!("      {} ({} fichiers)", l.path, l.files);
            }
        }
        println!("{with_saves} jeux avec des sauvegardes trouvées, {:.1} Mo au total ({:?})", total as f64 / 1e6, started.elapsed());
    }
}
