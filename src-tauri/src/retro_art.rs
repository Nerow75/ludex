//! Jaquettes des jeux d'émulateur : boîtes du projet public `libretro-thumbnails` (celles de
//! RetroArch), une par console, sans clé ni compte.
//!
//! Les noms des images d'une console sont lus dans l'API publique de GitHub (deux requêtes par
//! console : l'arbre du dépôt, puis celui de `Named_Boxarts`), gardés 30 jours dans
//! `retro-art-cache.json`. Un jeu prend l'image dont le nom est exactement celui de sa ROM, sinon
//! celle dont le titre nettoyé est le même (régions, révisions, disques ignorés), la meilleure
//! région d'abord. Rien de deviné au-delà : un nom de ROM maison (« HeartGold_Kameto_1008 ») reste
//! sans jaquette. L'image elle-même est téléchargée par le cache des images (`images.rs`) depuis
//! `raw.githubusercontent.com`, adresse reconstruite ici depuis un nom vérifié.
//!
//! Le front n'envoie rien ; seuls l'adresse publique d'un dépôt et un nom de console partent.

use crate::library::emulation::{clean_title, region_rank, systems};
use crate::library::{Game, Platform};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const CACHE_FILE: &str = "retro-art-cache.json";
const CACHE_VERSION: u32 = 1;
/// Liste d'une console relue après 30 jours.
const STALE_AFTER: u64 = 30 * 86_400;
/// Réponse de GitHub lue au plus (l'arbre des boîtes NES fait quelques Mo).
const MAX_BODY: u64 = 32 << 20;
/// Noms gardés par console, au plus.
const MAX_NAMES: usize = 60_000;
const API: &str = "https://api.github.com/repos/libretro-thumbnails";
const RAW_HOST: &str = "raw.githubusercontent.com";
const ORG: &str = "libretro-thumbnails";
const FOLDER: &str = "Named_Boxarts";

/// Une lecture tourne (diagnostic).
pub static BUSY: AtomicBool = AtomicBool::new(false);

#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Cache {
    version: u32,
    repos: BTreeMap<String, Repo>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Repo {
    checked: u64,
    names: Vec<String>,
    /// Titre comparable → noms (refait au chargement, jamais écrit).
    #[serde(skip)]
    index: HashMap<String, Vec<usize>>,
}

impl Repo {
    fn new(checked: u64, names: Vec<String>) -> Repo {
        let mut repo = Repo { checked, names, index: HashMap::new() };
        repo.reindex();
        repo
    }

    fn reindex(&mut self) {
        self.index.clear();
        for (i, name) in self.names.iter().enumerate() {
            self.index.entry(key(name)).or_default().push(i);
        }
    }

    /// Image d'une ROM : même nom, sinon même titre nettoyé avec la meilleure région.
    fn best(&self, stem: &str) -> Option<&str> {
        let exact = file_safe(stem);
        if let Some(name) = self.names.iter().find(|n| **n == exact) {
            return Some(name);
        }
        let k = key(stem);
        if k.len() < 2 {
            return None;
        }
        self.index
            .get(&k)?
            .iter()
            .map(|&i| self.names[i].as_str())
            .min_by_key(|n| (region_rank(n), n.len()))
    }
}

/// Titre comparable : titre nettoyé, lettres et chiffres en minuscules.
fn key(stem: &str) -> String {
    systems::compact(&clean_title(stem))
}

/// Nom de fichier tel que libretro l'écrit : `&*/:\`<>?\|"` remplacés par `_`.
fn file_safe(stem: &str) -> String {
    stem.chars().map(|c| if "&*/:`<>?\\|\"".contains(c) { '_' } else { c }).collect()
}

struct Loaded {
    path: PathBuf,
    cache: Cache,
}

static STATE: Mutex<Option<Loaded>> = Mutex::new(None);

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn load(dir: &Path) -> Loaded {
    let path = dir.join(CACHE_FILE);
    let mut cache = fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Cache>(&b).ok())
        .filter(|c| c.version == CACHE_VERSION)
        .unwrap_or(Cache { version: CACHE_VERSION, repos: BTreeMap::new() });
    // Noms relus vérifiés comme à la lecture (fichier modifié à la main).
    for repo in cache.repos.values_mut() {
        repo.names.retain(|n| valid_name(n));
        repo.reindex();
    }
    cache.repos.retain(|r, _| systems::SYSTEMS.iter().any(|s| s.repo == Some(r.as_str())));
    Loaded { path, cache }
}

fn with_cache<T>(dir: &Path, f: impl FnOnce(&mut Loaded) -> T) -> T {
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let loaded = state.get_or_insert_with(|| load(dir));
    f(loaded)
}

fn save(loaded: &Loaded) -> Result<(), String> {
    let tmp = loaded.path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec(&loaded.cache).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &loaded.path).map_err(|e| e.to_string())
}

/// Dépôt des jaquettes d'un jeu d'émulateur sans jaquette, et le nom de sa ROM.
fn wanted(game: &Game) -> Option<(&'static str, &str)> {
    if game.platform != Platform::Emulation || game.cover_url.is_some() {
        return None;
    }
    let info = game.emulation.as_ref()?;
    Some((systems::find(info.system)?.repo?, info.stem.as_str()))
}

/// Pose la jaquette trouvée (cache déjà lu) sur un jeu d'émulateur qui n'en a pas. Vrai si posée.
pub fn apply(game: &mut Game, dir: &Path) -> bool {
    let Some((repo, stem)) = wanted(game) else { return false };
    let url = with_cache(dir, |l| l.cache.repos.get(repo).and_then(|r| r.best(stem)).map(|name| image_url(repo, name)));
    match url {
        Some(url) => {
            game.cover_url = Some(url);
            true
        }
        None => false,
    }
}

/// Consoles dont la liste manque ou date de plus de 30 jours, pour ces jeux.
fn to_fetch(games: &[Game], dir: &Path) -> Vec<&'static str> {
    let now = now();
    let mut repos: Vec<&'static str> = games.iter().filter_map(wanted).map(|(r, _)| r).collect();
    repos.sort_unstable();
    repos.dedup();
    with_cache(dir, |l| repos.retain(|r| l.cache.repos.get(*r).is_none_or(|c| now.saturating_sub(c.checked) > STALE_AFTER)));
    repos
}

/// Recherche de fond après un scan : listes manquantes lues, puis `done(repo)` après chacune
/// (l'appelant pose les jaquettes trouvées et prévient le front). Arrêt à la première erreur.
pub fn resolve(games: &[Game], dir: &Path, mut done: impl FnMut(&'static str)) {
    let repos = to_fetch(games, dir);
    if repos.is_empty() || BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    let agent = crate::net::agent(Duration::from_secs(30));
    for (i, repo) in repos.into_iter().enumerate() {
        if i > 0 {
            std::thread::sleep(Duration::from_secs(1));
        }
        match fetch_names(&agent, repo) {
            Ok(names) => {
                crate::diagnostic::ok(crate::diagnostic::Service::RetroArt);
                let saved = with_cache(dir, |l| {
                    l.cache.repos.insert(repo.to_string(), Repo::new(now(), names));
                    save(l)
                });
                if let Err(e) = saved {
                    log::warn!("Cache des jaquettes libretro non enregistré : {e}");
                }
                done(repo);
            }
            Err(e) => {
                crate::diagnostic::failed(crate::diagnostic::Service::RetroArt, &e);
                log::warn!("Jaquettes libretro ({repo}) : {e}");
                break;
            }
        }
    }
    BUSY.store(false, Ordering::SeqCst);
}

#[derive(Deserialize)]
struct Tree {
    #[serde(default)]
    tree: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    path: String,
    mode: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    sha: String,
}

fn get_tree(agent: &ureq::Agent, url: &str) -> Result<Tree, String> {
    let response = agent
        .get(url)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(code, _) => tr!("réponse {code} de GitHub", "GitHub answered {code}"),
            ureq::Error::Transport(t) => t.to_string(),
        })?;
    let response = crate::net::checked(response, &[])?;
    let mut body = Vec::new();
    response.into_reader().take(MAX_BODY).read_to_end(&mut body).map_err(|e| e.to_string())?;
    serde_json::from_slice(&body).map_err(|_| tr!("réponse illisible de GitHub", "unreadable GitHub response"))
}

/// Noms des boîtes d'une console (sans `.png`), liens symboliques écartés.
fn fetch_names(agent: &ureq::Agent, repo: &str) -> Result<Vec<String>, String> {
    let root = get_tree(agent, &format!("{API}/{repo}/git/trees/master"))?;
    let folder = root
        .tree
        .iter()
        .find(|e| e.path == FOLDER && e.kind == "tree" && e.sha.len() == 40 && e.sha.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| tr!("dossier {FOLDER} absent", "{FOLDER} folder missing"))?;
    let tree = get_tree(agent, &format!("{API}/{repo}/git/trees/{}", folder.sha))?;
    Ok(names_of(tree))
}

fn names_of(tree: Tree) -> Vec<String> {
    let mut names: Vec<String> = tree
        .tree
        .into_iter()
        // 100644 = fichier ; 120000 = lien vers une autre image (doublon de région), écarté.
        .filter(|e| e.kind == "blob" && e.mode == "100644")
        .filter_map(|e| e.path.strip_suffix(".png").map(str::to_string))
        .filter(|n| valid_name(n))
        .take(MAX_NAMES)
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

/// Nom d'image acceptable : texte imprimable d'une seule partie de chemin, 200 octets au plus.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && !name.contains(['/', '\\'])
        && !name.starts_with('.')
        && !name.chars().any(|c| c.is_control())
}

/// Adresse de la boîte : nom encodé (tout sauf `A-Za-z0-9-_.~` en `%XX` UTF-8).
fn image_url(repo: &str, name: &str) -> String {
    let mut encoded = String::new();
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            encoded.push(b as char);
        } else {
            encoded.push_str(&format!("%{b:02X}"));
        }
    }
    format!("https://{RAW_HOST}/{ORG}/{repo}/master/{FOLDER}/{encoded}.png")
}

/// Adresse téléchargeable par le cache des images : celle que `image_url` construit, pour un
/// dépôt connu, un nom encodé proprement qui ne sort pas du dossier.
pub fn allowed(url: &str) -> bool {
    let Some(rest) = url.strip_prefix(&format!("https://{RAW_HOST}/{ORG}/")) else { return false };
    let Some((repo, rest)) = rest.split_once('/') else { return false };
    let Some(file) = rest.strip_prefix(&format!("master/{FOLDER}/")) else { return false };
    let Some(encoded) = file.strip_suffix(".png") else { return false };
    if !systems::SYSTEMS.iter().any(|s| s.repo == Some(repo)) {
        return false;
    }
    let bytes = encoded.as_bytes();
    let mut decoded = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hex = bytes.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok());
                let Some(b) = hex else { return false };
                decoded.push(b);
                i += 3;
            }
            b if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) => {
                decoded.push(b);
                i += 1;
            }
            _ => return false,
        }
    }
    String::from_utf8(decoded).is_ok_and(|name| valid_name(&name) && name != ".." && image_url(repo, &name) == url)
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPO: &str = "Nintendo_-_Game_Boy_Color";

    fn repo(names: &[&str]) -> Repo {
        Repo::new(0, names.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn picks_the_same_game_with_the_best_region() {
        crate::i18n::with(crate::i18n::Lang::Fr, || {
            let r = repo(&[
                "Pokemon - Gold Version (USA, Europe) (SGB Enhanced) (GB Compatible)",
                "Pokemon - Version Or (France) (SGB Enhanced) (GB Compatible)",
                "Pokemon - Edicion Oro (Spain) (SGB Enhanced) (GB Compatible)",
                "Legend of Zelda, The - Link's Awakening DX (USA, Europe) (Rev 2) (SGB Enhanced)",
                "Legend of Zelda, The - Link's Awakening DX (France) (SGB Enhanced)",
                "Mario Tennis (USA)",
                "Mario Tennis (Japan)",
                "Tom & Jerry (USA)",
            ]);
            // Même nom exact.
            assert_eq!(r.best("Mario Tennis (Japan)"), Some("Mario Tennis (Japan)"));
            // Même titre, région française d'abord.
            assert_eq!(r.best("Pokemon - Version Or (France)"), Some("Pokemon - Version Or (France) (SGB Enhanced) (GB Compatible)"));
            assert_eq!(r.best("Legend of Zelda, The - Link's Awakening DX (Germany)"), Some("Legend of Zelda, The - Link's Awakening DX (France) (SGB Enhanced)"));
            assert_eq!(r.best("Mario Tennis"), Some("Mario Tennis (USA)"));
            // Caractère interdit dans un nom de fichier : écrit `_` par libretro.
            let r2 = repo(&["Tom _ Jerry (USA)"]);
            assert_eq!(r2.best("Tom & Jerry (USA)"), Some("Tom _ Jerry (USA)"));
            // Rien de deviné.
            assert_eq!(r.best("HeartGold_Kameto_1008"), None);
            assert_eq!(r.best("Pokemon Gold"), None);
        });
        crate::i18n::with(crate::i18n::Lang::En, || {
            let r = repo(&["Mario Tennis (Japan)", "Mario Tennis (USA)", "Mario Tennis (Europe)"]);
            assert_eq!(r.best("Mario Tennis"), Some("Mario Tennis (USA)"));
        });
    }

    #[test]
    fn reads_trees_and_drops_links() {
        let tree: Tree = serde_json::from_str(
            r#"{"sha":"x","tree":[
                {"path":"Tetris (World).png","mode":"100644","type":"blob","sha":"a"},
                {"path":"Tetris (Europe).png","mode":"120000","type":"blob","sha":"b"},
                {"path":"readme.txt","mode":"100644","type":"blob","sha":"c"},
                {"path":"sub","mode":"040000","type":"tree","sha":"d"},
                {"path":"..png","mode":"100644","type":"blob","sha":"e"},
                {"path":"Bad\u0001.png","mode":"100644","type":"blob","sha":"f"}
            ],"truncated":false}"#,
        )
        .unwrap();
        assert_eq!(names_of(tree), ["Tetris (World)"]);
    }

    #[test]
    fn builds_and_checks_image_urls() {
        let url = image_url(REPO, "Pokemon - Version Or (France) (SGB Enhanced)");
        assert_eq!(
            url,
            "https://raw.githubusercontent.com/libretro-thumbnails/Nintendo_-_Game_Boy_Color/master/Named_Boxarts/Pokemon%20-%20Version%20Or%20%28France%29%20%28SGB%20Enhanced%29.png"
        );
        assert!(allowed(&url));
        assert!(allowed(&image_url(REPO, "Pokémon Édition Cristal")));
        // Autre dépôt, autre dossier, chemin qui sort, encodage cassé ou détourné : refusés.
        assert!(!allowed(&url.replace("Nintendo_-_Game_Boy_Color", "evil")));
        assert!(!allowed(&url.replace("Named_Boxarts", "Named_Snaps")));
        assert!(!allowed(&url.replace("raw.githubusercontent.com", "evil.example")));
        assert!(!allowed(&format!("https://{RAW_HOST}/{ORG}/{REPO}/master/{FOLDER}/..%2Fx.png")));
        assert!(!allowed(&format!("https://{RAW_HOST}/{ORG}/{REPO}/master/{FOLDER}/a%2.png")));
        assert!(!allowed(&format!("https://{RAW_HOST}/{ORG}/{REPO}/master/{FOLDER}/a b.png")));
        assert!(!allowed(&format!("https://{RAW_HOST}/{ORG}/{REPO}/master/{FOLDER}/%61.png"))); // forme non canonique
        assert!(!allowed(&format!("{url}?x=1")));
        assert!(!allowed(&url.replace(".png", ".jpg")));
    }

    #[test]
    fn cache_survives_a_reload_and_drops_unknown_repos() {
        let dir = std::env::temp_dir().join(format!("ludex-retro-art-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let mut repos = BTreeMap::new();
        repos.insert(REPO.to_string(), Repo::new(5, vec!["Tetris (World)".into(), "../x".into()]));
        repos.insert("Inconnu".to_string(), Repo::new(5, vec!["Jeu".into()]));
        let loaded = Loaded { path: dir.join(CACHE_FILE), cache: Cache { version: CACHE_VERSION, repos } };
        save(&loaded).unwrap();
        let again = load(&dir);
        assert_eq!(again.cache.repos.keys().collect::<Vec<_>>(), [REPO]);
        let r = &again.cache.repos[REPO];
        assert_eq!(r.names, ["Tetris (World)"]);
        assert_eq!(r.best("Tetris"), Some("Tetris (World)"));
        let _ = fs::remove_dir_all(dir);
    }
}
