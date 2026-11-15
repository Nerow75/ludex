//! Cache disque des images distantes (jaquette, image de fond et logo venant du CDN Steam) : elles
//! sont téléchargées une fois côté Rust, puis servies par le protocole `ludex` comme les images
//! locales des launchers. Affichage immédiat au démarrage et bibliothèque complète hors ligne.
//!
//! Sécurité : seules les URL de l'état (posées par les sources ou par `art.rs`) sont téléchargées,
//! seulement depuis les hôtes du CDN Steam (liste blanche stricte, redirections jamais suivies), avec
//! un plafond de taille et une vérification du type par signature (JPEG, PNG, WebP). Le front ne
//! voit jamais un chemin : il reçoit l'URL `ludex` du jeu, que `lib.rs` résout depuis l'état.
//!
//! Fichiers : `<données>/images/<plateforme>/<id nettoyé>/<cover|hero|logo>.<ext>`, plus un index
//! `images/index.json` : URL d'origine de chaque fichier (une URL qui change fait retélécharger),
//! images absentes du CDN (404 : redemandées au bout de 7 jours) et dernière présence de chaque jeu
//! dans la bibliothèque.
//!
//! Purge : le dossier d'un jeu absent de la bibliothèque depuis 60 jours est supprimé. Pas de
//! plafond global : la taille suit la bibliothèque (trois images par jeu au plus), et un jeu
//! désinstallé puis réinstallé dans les deux mois retrouve ses images sans réseau.

use crate::library::{art_url, Game};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Hôtes autorisés, comparés à l'identique (aucun sous-domaine, aucun port).
const HOSTS: &[&str] = &["cdn.cloudflare.steamstatic.com", "shared.akamai.steamstatic.com"];
const MAX_BYTES: u64 = 8 * 1024 * 1024;
const RETRY_MISSING: u64 = 7 * 24 * 3600;
const PURGE_AFTER: u64 = 60 * 24 * 3600;
/// Images gardées en cache, de la plus visible à la moins visible (ordre des téléchargements).
pub const KINDS: [&str; 3] = ["cover", "hero", "logo"];
const EXTENSIONS: [&str; 3] = ["jpg", "png", "webp"];
/// Téléchargements simultanés au plus.
pub const WORKERS: usize = 4;
/// Échecs réseau de suite avant d'abandonner le tour (hors ligne) : repris au prochain scan.
pub const MAX_FAILURES: usize = 3;
const INDEX_VERSION: u32 = 1;

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// FNV-1a 64 bits : stable d'une version de Rust à l'autre, contrairement au hasher par défaut.
fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

/// Noms réservés par Windows, même suivis d'une extension (« CON.txt »).
fn reserved(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT")) && stem.len() == 4 && stem.as_bytes()[3].is_ascii_digit())
}

/// Nom de dossier sûr pour un identifiant : lettres ASCII minuscules, chiffres, `-`, `_` et `.`
/// seulement, sans point au bord ni nom réservé. Un id modifié (ou trop long, ou avec des
/// majuscules, Windows ignorant la casse) reçoit son empreinte : deux ids ne partagent jamais un dossier.
pub fn clean_id(id: &str) -> String {
    let safe: String = id.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '_' }).collect();
    let safe = safe.trim_matches('.');
    let exact = !safe.is_empty() && safe == id && safe.len() <= 64 && !id.bytes().any(|b| b.is_ascii_uppercase()) && !reserved(safe);
    if exact {
        return safe.to_string();
    }
    // Sans point : « con.txt » garderait le nom réservé « con » devant son extension.
    let short: String = safe.chars().take(48).map(|c| if c == '.' { '_' } else { c.to_ascii_lowercase() }).collect();
    format!("{short}_{:016x}", fnv(id))
}

/// Dossier d'un jeu dans le cache, relatif à sa racine : `<plateforme>/<id nettoyé>`.
pub fn game_dir(game: &Game) -> String {
    let platform = game.id.split(':').next().unwrap_or_default();
    format!("{}/{}", clean_id(platform), clean_id(&game.platform_id))
}

/// URL téléchargeable : `https://`, hôte de la liste blanche, chemin simple (ni requête, ni `..`,
/// ni caractère encodé, ni identifiants, ni port).
pub fn allowed(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    let Some((host, path)) = rest.split_once('/') else { return false };
    HOSTS.contains(&host) && !path.is_empty() && !path.contains("..") && path.chars().all(|c| c.is_ascii_alphanumeric() || "/_-.".contains(c))
}

/// Type d'une image d'après sa signature : extension, ou `None` si ce n'est ni JPEG, ni PNG, ni WebP.
pub fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

pub fn mime(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "webp" => "image/webp",
        _ => "image/jpeg",
    }
}

/// Adresse distante d'une image du jeu, si elle est téléchargeable.
fn remote<'a>(game: &'a Game, kind: &str) -> Option<&'a str> {
    let url = match kind {
        "cover" => &game.cover_url,
        "hero" => &game.hero_url,
        "logo" => &game.logo_url,
        _ => return None,
    };
    url.as_deref().filter(|u| allowed(u))
}

/// Image à télécharger.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    pub game_id: String,
    /// Dossier du jeu dans le cache (`game_dir`).
    pub dir: String,
    pub kind: &'static str,
    pub url: String,
}

impl Job {
    fn key(&self) -> String {
        format!("{}/{}", self.dir, self.kind)
    }
}

/// Résultat d'un téléchargement.
#[derive(Debug, PartialEq, Eq)]
pub enum Fetch {
    Image(Vec<u8>, &'static str),
    /// Absente, refusée, trop lourde ou pas une image : notée, redemandée au bout de 7 jours.
    Missing,
    /// Erreur réseau ou serveur : redemandée au prochain scan.
    Failed(String),
}

fn status_outcome(code: u16) -> Fetch {
    match code {
        // Redirection (jamais suivie) comprise : redemander à chaque scan ne changerait rien.
        300..=399 | 403 | 404 | 410 => Fetch::Missing,
        _ => Fetch::Failed(format!("HTTP {code}")),
    }
}

/// Lit au plus `MAX_BYTES` et vérifie la signature.
fn read_image(reader: impl Read) -> Fetch {
    let mut bytes = Vec::new();
    if let Err(e) = reader.take(MAX_BYTES + 1).read_to_end(&mut bytes) {
        return Fetch::Failed(e.to_string());
    }
    match sniff(&bytes) {
        Some(ext) if bytes.len() as u64 <= MAX_BYTES => Fetch::Image(bytes, ext),
        _ => Fetch::Missing,
    }
}

pub fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        // Une redirection pourrait sortir de la liste blanche : jamais suivie.
        .redirects(0)
        .user_agent("Ludex (lanceur de jeux)")
        .build()
}

pub fn fetch(agent: &ureq::Agent, url: &str) -> Fetch {
    if !allowed(url) {
        return Fetch::Missing;
    }
    match agent.get(url).call() {
        Ok(resp) if resp.status() == 200 => {
            let declared = resp.header("Content-Length").and_then(|v| v.parse::<u64>().ok());
            if declared.is_some_and(|n| n > MAX_BYTES) {
                return Fetch::Missing;
            }
            read_image(resp.into_reader())
        }
        Ok(resp) => status_outcome(resp.status()),
        Err(ureq::Error::Status(code, _)) => status_outcome(code),
        Err(e) => Fetch::Failed(e.to_string()),
    }
}

/// Écrit une image dans le cache (fichier temporaire puis renommage) et retire les anciennes
/// versions du même type. Renvoie le chemin du fichier.
pub fn write(root: &Path, job: &Job, bytes: &[u8], ext: &str) -> Result<PathBuf, String> {
    let dir = root.join(&job.dir);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!("{}.tmp", job.kind));
    fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    for other in EXTENSIONS.iter().filter(|&&e| e != ext) {
        let _ = fs::remove_file(dir.join(format!("{}.{other}", job.kind)));
    }
    let file = dir.join(format!("{}.{ext}", job.kind));
    fs::rename(&tmp, &file).map_err(|e| e.to_string())?;
    Ok(file)
}

/// Pose une image téléchargée sur le jeu, si son adresse distante n'a pas changé entre-temps.
/// Renvoie vrai si le jeu a changé.
pub fn use_file(game: &mut Game, job: &Job, file: PathBuf) -> bool {
    if remote(game, job.kind) != Some(job.url.as_str()) || game.local_art.get(job.kind).is_some() {
        return false;
    }
    set_file(game, job.kind, file);
    true
}

/// Fichier servi par le protocole `ludex` pour une image du jeu.
fn set_file(game: &mut Game, kind: &str, file: PathBuf) {
    let url = art_url(game, kind);
    if let Some(local) = game.local_art.slot(kind) {
        *local = Some(file);
    }
    if let Some(shown) = game.image_url(kind) {
        *shown = Some(url);
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Index {
    #[serde(default)]
    version: u32,
    /// URL d'origine de chaque image en cache, par `<plateforme>/<id>/<type>`.
    #[serde(default)]
    sources: BTreeMap<String, String>,
    /// Images absentes du CDN : URL, date de la dernière tentative.
    #[serde(default)]
    missing: BTreeMap<String, u64>,
    /// Dernière présence de chaque jeu dans la bibliothèque, par `<plateforme>/<id>`.
    #[serde(default)]
    seen: BTreeMap<String, u64>,
}

#[derive(Debug)]
pub struct ImageCache {
    root: PathBuf,
    index: Index,
}

impl ImageCache {
    /// Cache rangé dans `root` (`<données>/images`).
    pub fn load(root: PathBuf) -> Self {
        let mut index: Index =
            fs::read_to_string(root.join("index.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        if index.version != INDEX_VERSION {
            // Fichiers sans URL d'origine connue : retéléchargés, puis remplacés.
            index = Index { version: INDEX_VERSION, ..Index::default() };
        }
        ImageCache { root, index }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn save(&self) -> Result<(), String> {
        fs::create_dir_all(&self.root).map_err(|e| e.to_string())?;
        let path = self.root.join("index.json");
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec(&self.index).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &path).map_err(|e| e.to_string())
    }

    /// Fichier en cache d'une image, s'il vient bien de cette URL.
    fn cached(&self, dir: &str, kind: &str, url: &str) -> Option<PathBuf> {
        if self.index.sources.get(&format!("{dir}/{kind}")).map(String::as_str) != Some(url) {
            return None;
        }
        let base = self.root.join(dir);
        EXTENSIONS.iter().map(|ext| base.join(format!("{kind}.{ext}"))).find(|p| p.is_file())
    }

    /// Remplace les images distantes déjà en cache par leur URL `ludex`. Les images locales du
    /// launcher restent prioritaires. Renvoie vrai si le jeu a changé.
    pub fn apply(&self, game: &mut Game) -> bool {
        let dir = game_dir(game);
        let mut changed = false;
        for kind in KINDS {
            if game.local_art.get(kind).is_some() {
                continue;
            }
            let Some(file) = remote(game, kind).and_then(|url| self.cached(&dir, kind, url)) else { continue };
            set_file(game, kind, file);
            changed = true;
        }
        changed
    }

    fn recently_missing(&self, url: &str) -> bool {
        self.index.missing.get(url).is_some_and(|&t| now().saturating_sub(t) < RETRY_MISSING)
    }

    /// Images distantes pas encore en cache (ni locales, ni absentes du CDN depuis peu), jaquettes
    /// d'abord.
    pub fn to_download(&self, games: &[Game]) -> Vec<Job> {
        let mut jobs: Vec<Job> = Vec::new();
        for kind in KINDS {
            for game in games {
                let dir = game_dir(game);
                let Some(url) = remote(game, kind) else { continue };
                if game.local_art.get(kind).is_some() || self.cached(&dir, kind, url).is_some() || self.recently_missing(url) {
                    continue;
                }
                let job = Job { game_id: game.id.clone(), dir, kind, url: url.to_string() };
                if !jobs.iter().any(|j| j.key() == job.key()) {
                    jobs.push(job);
                }
            }
        }
        jobs
    }

    pub fn record(&mut self, job: &Job) {
        self.index.sources.insert(job.key(), job.url.clone());
        self.index.missing.remove(&job.url);
    }

    pub fn note_missing(&mut self, url: &str) {
        self.index.missing.insert(url.to_string(), now());
    }

    /// Note la présence des jeux de la bibliothèque (décompte de la purge).
    pub fn touch(&mut self, games: &[Game]) {
        let t = now();
        for game in games {
            self.index.seen.insert(game_dir(game), t);
        }
    }

    /// Supprime les dossiers des jeux absents de la bibliothèque depuis plus de 60 jours. Un
    /// dossier inconnu de l'index (index perdu) commence son décompte maintenant. Renvoie le
    /// nombre de dossiers supprimés.
    pub fn purge(&mut self) -> usize {
        let t = now();
        let mut removed = 0;
        for platform in read_dirs(&self.root) {
            for game in read_dirs(&platform) {
                let (Some(p), Some(g)) = (name(&platform), name(&game)) else { continue };
                let key = format!("{p}/{g}");
                let last = *self.index.seen.entry(key.clone()).or_insert(t);
                if t.saturating_sub(last) > PURGE_AFTER && fs::remove_dir_all(&game).is_ok() {
                    removed += 1;
                    self.index.seen.remove(&key);
                    let prefix = format!("{key}/");
                    self.index.sources.retain(|k, _| !k.starts_with(&prefix));
                }
            }
        }
        self.index.seen.retain(|_, &mut last| t.saturating_sub(last) <= PURGE_AFTER);
        self.index.missing.retain(|_, &mut last| t.saturating_sub(last) <= PURGE_AFTER);
        removed
    }

    /// Images en cache et leur taille totale en octets (tests et relevé réel).
    #[cfg(test)]
    fn usage(&self) -> (usize, u64) {
        let files: Vec<u64> = read_dirs(&self.root)
            .iter()
            .flat_map(|p| read_dirs(p))
            .flat_map(|g| fs::read_dir(g).into_iter().flatten().flatten())
            .filter(|e| e.path().extension().and_then(|x| x.to_str()).is_some_and(|x| EXTENSIONS.contains(&x)))
            .filter_map(|e| e.metadata().ok().map(|m| m.len()))
            .collect();
        (files.len(), files.iter().sum())
    }

    #[cfg(test)]
    fn missing_count(&self) -> usize {
        self.index.missing.len()
    }
}

/// Sous-dossiers directs (jamais de lien suivi hors du cache : `read_dir` ne liste que ce niveau).
fn read_dirs(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect()
}

fn name(path: &Path) -> Option<&str> {
    path.file_name()?.to_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{Launch, LocalArt, Platform};
    use std::io::Cursor;

    const CDN: &str = "https://cdn.cloudflare.steamstatic.com/steam/apps/730";
    const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10];

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ludex-test-images-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn steam_game() -> Game {
        let mut g = Game::new(Platform::Steam, "730", "Counter-Strike 2", Launch::Uri(String::new()));
        g.cover_url = Some(format!("{CDN}/library_600x900_2x.jpg"));
        g.hero_url = Some(format!("{CDN}/library_hero.jpg"));
        g.logo_url = Some(format!("{CDN}/logo.png"));
        g
    }

    fn job(game: &Game, kind: &'static str, url: &str) -> Job {
        Job { game_id: game.id.clone(), dir: game_dir(game), kind, url: url.to_string() }
    }

    #[test]
    fn cleans_ids_into_safe_folder_names() {
        assert_eq!(clean_id("730"), "730");
        assert_eq!(clean_id("steam"), "steam");
        for id in ["..", "../../evil", r"C:\Jeux\Foo", "a/b", "CON", "con.txt", "Lpt1", "trailing.", "Microsoft.Minecraft_8wekyb3d8bbwe", &"x".repeat(300)] {
            let clean = clean_id(id);
            assert!(!clean.is_empty() && clean.len() <= 70, "{id} → {clean}");
            assert!(clean.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "-_.".contains(c)), "{id} → {clean}");
            assert!(!clean.starts_with('.') && !clean.ends_with('.') && !reserved(&clean), "{id} → {clean}");
        }
        // Ids proches, dossiers distincts.
        assert_ne!(clean_id("a/b"), clean_id("a_b"));
        assert_ne!(clean_id("Abc"), clean_id("abc"));
        let mut g = Game::new(Platform::Other, r"d:\jeux\cyberpunk", "Cyberpunk", Launch::Uri(String::new()));
        assert!(game_dir(&g).starts_with("other/d__jeux_cyberpunk_"));
        g.id = "../..:x".into();
        assert!(!game_dir(&g).contains(".."));
    }

    #[test]
    fn allows_only_steam_cdn_hosts() {
        assert!(allowed(&format!("{CDN}/library_hero.jpg")));
        assert!(allowed("https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/1/df4e/library_capsule_2x.jpg"));
        for url in [
            "http://cdn.cloudflare.steamstatic.com/steam/apps/730/logo.png",
            "https://cdn.cloudflare.steamstatic.com.evil.com/a.jpg",
            "https://evil.com/cdn.cloudflare.steamstatic.com/a.jpg",
            "https://user@cdn.cloudflare.steamstatic.com/a.jpg",
            "https://cdn.cloudflare.steamstatic.com:8443/a.jpg",
            "https://cdn.cloudflare.steamstatic.com/a.jpg?x=1",
            "https://cdn.cloudflare.steamstatic.com/../a.jpg",
            "https://cdn.cloudflare.steamstatic.com/%2e%2e/a.jpg",
            "https://cdn.cloudflare.steamstatic.com\\a.jpg",
            "https://cdn.cloudflare.steamstatic.com",
            "https://cdn.cloudflare.steamstatic.com/",
            "http://ludex.localhost/steam/730/cover",
        ] {
            assert!(!allowed(url), "{url}");
        }
    }

    #[test]
    fn detects_image_type_by_signature() {
        assert_eq!(sniff(JPEG), Some("jpg"));
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\n...."), Some("png"));
        assert_eq!(sniff(b"RIFF\x10\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(sniff(b"GIF89a"), None);
        assert_eq!(sniff(b"<html>404</html>"), None);
        assert_eq!(sniff(b""), None);
        assert_eq!(mime("webp"), "image/webp");
    }

    #[test]
    fn caps_size_and_classifies_errors() {
        assert_eq!(read_image(Cursor::new(JPEG.to_vec())), Fetch::Image(JPEG.to_vec(), "jpg"));
        let mut huge = JPEG.to_vec();
        huge.resize(MAX_BYTES as usize + 1, 0);
        assert_eq!(read_image(Cursor::new(huge)), Fetch::Missing);
        assert_eq!(read_image(Cursor::new(b"<html>".to_vec())), Fetch::Missing);
        assert_eq!(status_outcome(404), Fetch::Missing);
        assert_eq!(status_outcome(302), Fetch::Missing);
        assert!(matches!(status_outcome(503), Fetch::Failed(_)));
        assert_eq!(fetch(&agent(), "https://evil.com/a.jpg"), Fetch::Missing); // aucune requête
    }

    #[test]
    fn prefers_local_then_cache_then_remote() {
        let root = temp_root("choice");
        let mut cache = ImageCache::load(root.clone());
        let mut game = steam_game();
        game.local_art = LocalArt { cover: Some(PathBuf::from(r"C:\Steam\librarycache\730\cover.jpg")), ..LocalArt::default() };
        game.cover_url = Some(art_url(&game, "cover"));

        // Image de fond en cache depuis la même URL ; logo en cache, mais depuis une autre URL.
        let hero = job(&game, "hero", game.hero_url.as_deref().unwrap());
        write(&root, &hero, JPEG, "jpg").unwrap();
        cache.record(&hero);
        let old_logo = job(&game, "logo", &format!("{CDN}/ancien_logo.png"));
        write(&root, &old_logo, JPEG, "jpg").unwrap();
        cache.record(&old_logo);

        assert_eq!(cache.to_download(std::slice::from_ref(&game)), vec![job(&game, "logo", &format!("{CDN}/logo.png"))]);
        assert!(cache.apply(&mut game));
        assert_eq!(game.local_art.cover, Some(PathBuf::from(r"C:\Steam\librarycache\730\cover.jpg"))); // locale gardée
        assert_eq!(game.hero_url.as_deref(), Some(art_url(&game, "hero").as_str()));
        assert_eq!(game.local_art.hero, Some(root.join("steam/730").join("hero.jpg")));
        assert_eq!(game.logo_url, Some(format!("{CDN}/logo.png"))); // reste distant
        assert!(!cache.apply(&mut game));

        // Téléchargement fini : le jeu passe sur le cache ; une URL changée entre-temps est ignorée.
        let logo = job(&game, "logo", &format!("{CDN}/logo.png"));
        let file = write(&root, &logo, b"\x89PNG\r\n\x1a\n", "png").unwrap();
        assert!(!root.join("steam/730/logo.jpg").exists()); // ancienne version retirée
        assert!(!use_file(&mut steam_game(), &job(&game, "logo", &format!("{CDN}/autre.png")), file.clone()));
        assert!(use_file(&mut game, &logo, file));
        assert!(game.logo_url.as_deref().unwrap().starts_with(crate::library::ART_ORIGIN));
        assert!(cache.to_download(&[game]).is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn remembers_missing_images_for_a_few_days() {
        let root = temp_root("missing");
        let mut cache = ImageCache::load(root.clone());
        let game = steam_game();
        let logo = format!("{CDN}/logo.png");
        cache.note_missing(&logo);
        let jobs = cache.to_download(std::slice::from_ref(&game));
        assert_eq!(jobs.iter().map(|j| j.kind).collect::<Vec<_>>(), vec!["cover", "hero"]);
        cache.index.missing.insert(logo.clone(), now() - RETRY_MISSING - 1);
        assert_eq!(cache.to_download(std::slice::from_ref(&game)).len(), 3);
        cache.record(&job(&game, "logo", &logo));
        assert_eq!(cache.missing_count(), 0);

        // L'index survit au redémarrage.
        cache.note_missing(&logo);
        cache.save().unwrap();
        assert!(ImageCache::load(root.clone()).recently_missing(&logo));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn purges_games_gone_for_long() {
        let root = temp_root("purge");
        let mut cache = ImageCache::load(root.clone());
        let (kept, gone) = (steam_game(), Game::new(Platform::Steam, "440", "TF2", Launch::Uri(String::new())));
        for g in [&kept, &gone] {
            let j = job(g, "cover", &format!("{CDN}/x.jpg"));
            write(&root, &j, JPEG, "jpg").unwrap();
            cache.record(&j);
        }
        let unknown = root.join("epic").join("fortnite");
        fs::create_dir_all(&unknown).unwrap();
        cache.touch(std::slice::from_ref(&kept));
        cache.index.seen.insert("steam/440".into(), now() - PURGE_AFTER - 1);

        assert_eq!(cache.purge(), 1);
        assert!(root.join("steam/730/cover.jpg").is_file());
        assert!(!root.join("steam/440").exists());
        assert!(unknown.is_dir() && cache.index.seen.contains_key("epic/fortnite")); // décompte commencé
        assert!(!cache.index.sources.contains_key("steam/440/cover"));
        assert_eq!(cache.usage(), (1, JPEG.len() as u64));
        let _ = fs::remove_dir_all(root);
    }

    /// Scan réel de ce PC et téléchargement des images manquantes dans un dossier temporaire :
    /// `cargo test images_this_pc -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn images_this_pc() {
        let started = std::time::Instant::now();
        let root = std::env::temp_dir().join("ludex-images-this-pc");
        let mut cache = ImageCache::load(root.clone());
        let mut games = crate::library::scan().games;
        // Jaquettes déjà retrouvées par l'app (lecture seule de son `art-cache.json`).
        let app_dir = PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join("com.ludex.app");
        let art = crate::art::ArtCache::load(&app_dir);
        games.iter_mut().for_each(|g| {
            art.apply(g);
        });
        cache.touch(&games);
        let before = games.iter_mut().map(|g| cache.apply(g)).filter(|&b| b).count();
        let jobs = cache.to_download(&games);
        println!("{} jeux, {before} déjà servis par le cache, {} images à télécharger", games.len(), jobs.len());

        let agent = agent();
        let queue = std::sync::Mutex::new(jobs.into_iter());
        let cache = std::sync::Mutex::new(cache);
        let (mut ok, mut missing, mut failed) = (0, 0, 0);
        let results = std::sync::Mutex::new(Vec::new());
        std::thread::scope(|s| {
            for _ in 0..WORKERS {
                s.spawn(|| {
                    while let Some(job) = queue.lock().unwrap().next() {
                        let outcome = fetch(&agent, &job.url);
                        if let Fetch::Image(bytes, ext) = &outcome {
                            write(&root, &job, bytes, ext).unwrap();
                            cache.lock().unwrap().record(&job);
                        } else if outcome == Fetch::Missing {
                            cache.lock().unwrap().note_missing(&job.url);
                        }
                        results.lock().unwrap().push((job, outcome));
                    }
                });
            }
        });
        for (job, outcome) in results.into_inner().unwrap() {
            match outcome {
                Fetch::Image(..) => ok += 1,
                Fetch::Missing => {
                    missing += 1;
                    println!("  absente : {} {}", job.game_id, job.url);
                }
                Fetch::Failed(e) => {
                    failed += 1;
                    println!("  échec : {} {} ({e})", job.game_id, job.url);
                }
            }
        }
        let cache = cache.into_inner().unwrap();
        cache.save().unwrap();
        let after = games.iter_mut().map(|g| cache.apply(g)).filter(|&b| b).count() + before;
        let (files, bytes) = cache.usage();
        println!("Téléchargées : {ok}, absentes (404) : {missing}, échecs : {failed}");
        println!("Jeux servis par le cache : {after}");
        println!("Cache : {files} images, {:.1} Mo, {} URL notées absentes, dans {}", bytes as f64 / 1e6, cache.missing_count(), root.display());
        println!("Temps : {:.1} s", started.elapsed().as_secs_f64());
    }
}
