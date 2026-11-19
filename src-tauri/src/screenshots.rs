//! Captures d'écran des jeux, en lecture seule :
//! - Steam : `userdata/<compte>/760/remote/<appid>/screenshots/*.jpg` et leurs miniatures
//!   (`screenshots/thumbnails/`), compte = celui de `localconfig.vdf` ;
//! - Xbox Game Bar : dossier « Captures » (`<Titre du jeu> <date>.png` ou `.mp4`), rattaché au jeu
//!   par son titre, sinon rangé dans « Autres captures ».
//!
//! Le front ne voit jamais de chemin : chaque capture a une empreinte (`key`) et des URL `ludex`
//! (`/capture/<empreinte>/<thumb|full>`). Un fichier n'est servi que s'il a été relevé ici et qu'il
//! est toujours dans un dossier de captures connu (vérifié à chaque requête, liens résolus).

use crate::images::fnv;
use crate::library::{Game, Platform, ART_ORIGIN};
use serde::Serialize;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Extensions affichables. Les captures HDR de la Game Bar (`.jxr`) ne le sont pas dans la WebView.
const IMAGE_EXTS: [&str; 3] = ["jpg", "jpeg", "png"];
const VIDEO_EXTS: [&str; 1] = ["mp4"];

/// Dossier connu « Captures » de Windows (celui de la Xbox Game Bar) dans `User Shell Folders`.
#[cfg(windows)]
const CAPTURES_FOLDER: &str = "{EDC0FE71-98D8-4F4A-B920-C8DC133CB165}";

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Steam,
    /// Xbox Game Bar (Win+Alt+Impr. écran).
    Gamebar,
}

/// Une capture. Les chemins restent côté Rust.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Shot {
    /// Empreinte du chemin : seule référence connue du front.
    pub key: String,
    /// Jeu de la bibliothèque, `None` : rattachement impossible (« Autres captures »).
    pub game_id: Option<String>,
    pub source: Source,
    /// Dossier Steam de la capture : seul indice pour un jeu désinstallé (« Autres captures »).
    pub steam_appid: Option<u32>,
    /// Date de la capture (modification du fichier, secondes Unix).
    pub taken: u64,
    /// Taille du fichier en octets.
    pub size: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Vidéo (`.mp4` de la Game Bar) : pas de miniature.
    pub video: bool,
    /// Miniature (Steam), sinon l'image elle-même.
    pub thumb_url: String,
    pub url: String,
    #[serde(skip)]
    pub path: PathBuf,
    #[serde(skip)]
    pub thumb: Option<PathBuf>,
}

/// Captures d'un jeu (ou « Autres captures »), pour la page Captures.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub game_id: Option<String>,
    pub title: String,
    pub shots: Vec<Shot>,
}

pub const OTHER_TITLE: &str = "Autres captures";

/// Dossiers de captures de ce PC.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Roots {
    /// `userdata/<compte>/760/remote` de Steam.
    pub steam: Option<PathBuf>,
    /// Dossier « Captures » de la Xbox Game Bar.
    pub gamebar: Option<PathBuf>,
}

impl Roots {
    pub fn this_pc() -> Self {
        let steam = crate::library::steam_account_dir().map(|d| d.join("760").join("remote")).filter(|d| d.is_dir());
        Roots { steam, gamebar: gamebar_dir() }
    }
}

/// Remplace les `%VARIABLE%` d'une valeur `REG_EXPAND_SZ` (lue telle quelle par `winreg`).
fn expand_env(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        let Some(len) = rest[start + 1..].find('%') else { break };
        let name = &rest[start + 1..start + 1 + len];
        out.push_str(&rest[..start]);
        match std::env::var(name) {
            Ok(v) if !name.is_empty() => out.push_str(&v),
            _ => out.push_str(&rest[start..start + len + 2]),
        }
        rest = &rest[start + len + 2..];
    }
    out.push_str(rest);
    out
}

/// Dossier « Captures » : dossier connu de Windows s'il a été déplacé, sinon `Vidéos\Captures`.
#[cfg(windows)]
fn gamebar_dir() -> Option<PathBuf> {
    use crate::library::reg;
    let folders = reg::hkcu(r"Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders");
    let value = |name: &str| folders.as_ref().and_then(|k| reg::string(k, name)).map(|v| PathBuf::from(expand_env(&v)));
    let videos = value("My Video").or_else(|| std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("Videos")));
    let dir = value(CAPTURES_FOLDER).or_else(|| videos.map(|v| v.join("Captures")))?;
    dir.is_dir().then_some(dir)
}

#[cfg(not(windows))]
fn gamebar_dir() -> Option<PathBuf> {
    None
}

fn ext_of(path: &Path) -> Option<String> {
    path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase)
}

fn is_image(path: &Path) -> bool {
    ext_of(path).is_some_and(|e| IMAGE_EXTS.contains(&e.as_str()))
}

fn is_video(path: &Path) -> bool {
    ext_of(path).is_some_and(|e| VIDEO_EXTS.contains(&e.as_str()))
}

/// Empreinte d'une capture : 16 chiffres hexadécimaux, tirés du chemin (casse ignorée).
pub fn key_of(path: &Path) -> String {
    format!("{:016x}", fnv(&path.to_string_lossy().to_lowercase()))
}

/// Référence reçue du front : 16 chiffres hexadécimaux, rien d'autre.
pub fn valid_key(key: &str) -> bool {
    key.len() == 16 && key.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Adresse d'une capture servie par le protocole `ludex`.
fn capture_url(key: &str, kind: &str) -> String {
    format!("{ART_ORIGIN}/capture/{key}/{kind}")
}

/// Mots d'un titre, en minuscules, ponctuation et symboles (®, ™, :) retirés.
fn words(title: &str) -> Vec<String> {
    title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Titre du jeu dans le nom d'une capture de la Game Bar : la date est retirée en fin de nom
/// (« Hades 2024-01-15 14-23-33 », « Hades 1_15_2024 2_23_33 PM », « Hades 15_01_2024 14_23_33 »).
pub fn capture_title(stem: &str) -> Option<String> {
    let mut tokens: Vec<&str> = stem.split_whitespace().collect();
    // Date ou heure : chiffres avec au moins un séparateur (« 2 » de « Portal 2 » n'en est pas une).
    let date_like = |t: &str| {
        t.eq_ignore_ascii_case("am")
            || t.eq_ignore_ascii_case("pm")
            || (t.bytes().any(|b| b.is_ascii_digit())
                && t.bytes().any(|b| b"-_.:".contains(&b))
                && t.bytes().all(|b| b.is_ascii_digit() || b"-_.:".contains(&b)))
    };
    let before = tokens.len();
    // Au plus date, heure et AM/PM.
    while tokens.len() + 3 > before && tokens.last().is_some_and(|t| date_like(t)) {
        tokens.pop();
    }
    // Sans date reconnue, le nom ne suit pas le format de la Game Bar.
    if tokens.len() == before {
        return None;
    }
    let title = tokens.join(" ");
    let title = title.trim_end_matches(|c: char| c == '-' || c == '_' || c.is_whitespace());
    (!title.is_empty()).then(|| title.to_string())
}

/// Jeu dont le titre correspond à celui d'une capture : mêmes mots, ou les mots de l'un au début
/// de l'autre si le plus court en a au moins deux (« Cyberpunk 2077 » et « Cyberpunk 2077 (C)
/// 2020 by CD Projekt RED »). Le titre le plus long l'emporte : « Portal 2 » avant « Portal ».
pub fn match_game<'a>(title: &str, games: &'a [Game]) -> Option<&'a Game> {
    let capture = words(title);
    if capture.is_empty() {
        return None;
    }
    games
        .iter()
        .filter_map(|g| {
            let game = words(&g.title);
            if game.is_empty() {
                return None;
            }
            let (short, long) = if game.len() <= capture.len() { (&game, &capture) } else { (&capture, &game) };
            let fits = game == capture || (short.len() >= 2 && long.starts_with(short));
            // Égalité d'abord, puis le plus de mots en commun.
            fits.then(|| ((game == capture), short.len(), g))
        })
        .max_by_key(|(exact, n, _)| (*exact, *n))
        .map(|(_, _, g)| g)
}

/// Jeu de la bibliothèque pour un dossier d'appid Steam : le jeu Steam, sinon un jeu dont l'appid
/// est connu (table Rockstar, jaquette retrouvée sur la boutique).
fn steam_owner(appid: u32, games: &[Game]) -> Option<&Game> {
    games
        .iter()
        .filter(|g| g.steam_appid == Some(appid))
        .max_by_key(|g| g.platform == Platform::Steam)
}

/// Capture trouvée, avant la lecture de ses métadonnées.
struct Found {
    path: PathBuf,
    thumb: Option<PathBuf>,
    source: Source,
    steam_appid: Option<u32>,
    game_id: Option<String>,
}

fn files(dir: &Path) -> impl Iterator<Item = PathBuf> {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
}

/// Captures Steam, dossier d'appid par dossier d'appid. `only` : un seul jeu (les autres dossiers
/// ne sont pas lus).
fn steam_found(remote: &Path, games: &[Game], only: Option<&str>, out: &mut Vec<Found>) {
    let Ok(entries) = fs::read_dir(remote) else { return };
    for entry in entries.flatten() {
        let Some(appid) = entry.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
        let owner = steam_owner(appid, games).map(|g| g.id.clone());
        if only.is_some_and(|id| owner.as_deref() != Some(id)) {
            continue;
        }
        let dir = entry.path().join("screenshots");
        let thumbs = dir.join("thumbnails");
        for path in files(&dir).filter(|p| is_image(p)) {
            let thumb = path.file_name().map(|n| thumbs.join(n)).filter(|t| t.is_file());
            out.push(Found { path, thumb, source: Source::Steam, steam_appid: Some(appid), game_id: owner.clone() });
        }
    }
}

/// Captures de la Game Bar, rattachées par leur titre.
fn gamebar_found(dir: &Path, games: &[Game], only: Option<&str>, out: &mut Vec<Found>) {
    for path in files(dir).filter(|p| is_image(p) || is_video(p)) {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        let owner = capture_title(stem).and_then(|t| match_game(&t, games)).map(|g| g.id.clone());
        if only.is_some_and(|id| owner.as_deref() != Some(id)) {
            continue;
        }
        out.push(Found { path, thumb: None, source: Source::Gamebar, steam_appid: None, game_id: owner });
    }
}

fn be16(b: &[u8]) -> u32 {
    u32::from(b[0]) << 8 | u32::from(b[1])
}

/// Dimensions d'une image PNG ou JPEG, lues dans son en-tête (quelques octets seulement).
pub fn image_size(path: &Path) -> Option<(u32, u32)> {
    let mut file = File::open(path).ok()?;
    let mut head = [0u8; 24];
    file.read_exact(&mut head).ok()?;
    if head.starts_with(b"\x89PNG\r\n\x1a\n") && &head[12..16] == b"IHDR" {
        let w = u32::from_be_bytes(head[16..20].try_into().ok()?);
        let h = u32::from_be_bytes(head[20..24].try_into().ok()?);
        return Some((w, h));
    }
    if !head.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    // JPEG : segments jusqu'au premier SOF (début de l'image), qui porte hauteur et largeur.
    let mut pos = 2u64;
    for _ in 0..256 {
        file.seek(SeekFrom::Start(pos)).ok()?;
        let mut seg = [0u8; 4];
        file.read_exact(&mut seg).ok()?;
        if seg[0] != 0xFF {
            return None;
        }
        let marker = seg[1];
        if marker == 0xFF {
            pos += 1; // octet de remplissage
            continue;
        }
        let len = u64::from(be16(&seg[2..4]));
        if (0xC0..=0xCF).contains(&marker) && ![0xC4, 0xC8, 0xCC].contains(&marker) {
            let mut sof = [0u8; 5];
            file.read_exact(&mut sof).ok()?;
            return Some((be16(&sof[3..5]), be16(&sof[1..3])));
        }
        if len < 2 {
            return None;
        }
        pos += 2 + len;
    }
    None
}

fn shot(found: Found) -> Option<Shot> {
    let meta = fs::metadata(&found.path).ok()?;
    let taken = meta.modified().ok()?.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let video = is_video(&found.path);
    let (width, height) = if video { (None, None) } else { image_size(&found.path).map_or((None, None), |(w, h)| (Some(w), Some(h))) };
    let key = key_of(&found.path);
    let url = capture_url(&key, "full");
    let thumb_url = if found.thumb.is_some() { capture_url(&key, "thumb") } else { url.clone() };
    Some(Shot {
        key,
        game_id: found.game_id,
        source: found.source,
        steam_appid: found.steam_appid,
        taken,
        size: meta.len(),
        width,
        height,
        video,
        thumb_url,
        url,
        path: found.path,
        thumb: found.thumb,
    })
}

/// Captures de ce PC, la plus récente d'abord. `only` : celles d'un jeu seulement.
pub fn collect(roots: &Roots, games: &[Game], only: Option<&str>) -> Vec<Shot> {
    let mut found = Vec::new();
    if let Some(remote) = &roots.steam {
        steam_found(remote, games, only, &mut found);
    }
    if let Some(dir) = &roots.gamebar {
        gamebar_found(dir, games, only, &mut found);
    }
    let mut shots: Vec<Shot> = found.into_iter().filter_map(shot).collect();
    shots.sort_by(|a, b| b.taken.cmp(&a.taken).then_with(|| a.key.cmp(&b.key)));
    shots
}

/// Captures groupées par jeu : le jeu à la capture la plus récente d'abord, « Autres captures »
/// en dernier. Les captures arrivent déjà triées (`collect`).
pub fn group(shots: Vec<Shot>, games: &[Game]) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    let mut index: HashMap<Option<String>, usize> = HashMap::new();
    for s in shots {
        let i = *index.entry(s.game_id.clone()).or_insert_with(|| {
            let title = s
                .game_id
                .as_deref()
                .and_then(|id| games.iter().find(|g| g.id == id))
                .map_or_else(|| OTHER_TITLE.to_string(), |g| g.title.clone());
            groups.push(Group { game_id: s.game_id.clone(), title, shots: Vec::new() });
            groups.len() - 1
        });
        groups[i].shots.push(s);
    }
    // Tri stable : l'ordre des captures les plus récentes est gardé, « Autres » passe à la fin.
    groups.sort_by_key(|g| g.game_id.is_none());
    groups
}

/// Captures déjà envoyées au front, retrouvées par leur empreinte pour être servies.
#[derive(Default)]
pub struct Index {
    files: HashMap<String, (PathBuf, Option<PathBuf>)>,
    roots: Roots,
}

impl Index {
    /// Note les captures d'un relevé. `reset` : relevé complet, les anciennes entrées partent.
    pub fn remember(&mut self, roots: &Roots, shots: &[Shot], reset: bool) {
        if reset || *roots != self.roots {
            self.files.clear();
            self.roots = roots.clone();
        }
        self.files.extend(shots.iter().map(|s| (s.key.clone(), (s.path.clone(), s.thumb.clone()))));
    }

    /// Fichier à servir (`thumb` : miniature si elle existe), seulement s'il est toujours dans un
    /// dossier de captures connu.
    pub fn file(&self, key: &str, thumb: bool) -> Option<PathBuf> {
        let (full, small) = self.files.get(key)?;
        let path = if thumb { small.as_ref().unwrap_or(full) } else { full };
        allowed(path, &self.roots).then(|| path.clone())
    }
}

fn canonical(path: &Path) -> Option<PathBuf> {
    fs::canonicalize(path).ok()
}

fn name_is(path: Option<&Path>, name: &str) -> bool {
    path.and_then(Path::file_name).and_then(|n| n.to_str()).is_some_and(|n| n.eq_ignore_ascii_case(name))
}

/// Le fichier (liens résolus) est-il une image ou une vidéo d'un dossier de captures connu ?
/// Steam : `<racine>\<appid>\screenshots\` ou sa sous-dossier `thumbnails\` ; Game Bar : à la
/// racine du dossier « Captures ».
pub fn allowed(path: &Path, roots: &Roots) -> bool {
    let Some(file) = canonical(path) else { return false };
    if !file.is_file() || !(is_image(&file) || is_video(&file)) {
        return false;
    }
    let parent = file.parent();
    let in_gamebar = roots.gamebar.as_deref().and_then(canonical).is_some_and(|root| parent == Some(root.as_path()));
    let in_steam = roots.steam.as_deref().and_then(canonical).is_some_and(|root| {
        let screenshots = if name_is(parent, "thumbnails") { parent.and_then(Path::parent) } else { parent };
        is_image(&file)
            && name_is(screenshots, "screenshots")
            && screenshots.and_then(Path::parent).and_then(Path::parent) == Some(root.as_path())
    });
    in_gamebar || in_steam
}

/// Type MIME d'une capture, d'après sa signature (l'extension pour les vidéos).
pub fn mime(path: &Path, head: &[u8]) -> &'static str {
    if is_video(path) {
        return "video/mp4";
    }
    crate::images::mime(crate::images::sniff(head).unwrap_or("jpg"))
}

/// Taille d'un morceau de vidéo servi par requête (`Range`).
pub const VIDEO_CHUNK: u64 = 2 * 1024 * 1024;

/// Plage d'octets demandée (`Range: bytes=<début>-<fin>`), bornée à `VIDEO_CHUNK` et au fichier.
/// `None` : plage invalide (réponse 416).
pub fn byte_range(header: Option<&str>, len: u64) -> Option<(u64, u64)> {
    if len == 0 {
        return None;
    }
    let (start, end) = match header.and_then(|h| h.trim().strip_prefix("bytes=")) {
        None => (0, None),
        Some(spec) => {
            // Une seule plage : la première si le navigateur en demande plusieurs.
            let (a, b) = spec.split(',').next()?.trim().split_once('-')?;
            if a.is_empty() {
                // `bytes=-500` : les 500 derniers octets.
                let n: u64 = b.parse().ok()?;
                (len.saturating_sub(n), None)
            } else {
                (a.parse().ok()?, if b.is_empty() { None } else { Some(b.parse::<u64>().ok()?) })
            }
        }
    };
    if start >= len {
        return None;
    }
    let end = end.unwrap_or(u64::MAX).min(len - 1).min(start + VIDEO_CHUNK - 1);
    (end >= start).then_some((start, end))
}

/// Lit les octets `start..=end` d'un fichier.
pub fn read_range(path: &Path, start: u64, end: u64) -> Option<Vec<u8>> {
    let mut file = File::open(path).ok()?;
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = vec![0u8; usize::try_from(end - start + 1).ok()?];
    file.read_exact(&mut buf).ok()?;
    Some(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Launch;

    fn game(platform: Platform, id: &str, title: &str, appid: Option<u32>) -> Game {
        let mut g = Game::new(platform, id, title, Launch::Uri(String::new()));
        g.steam_appid = appid;
        g
    }

    fn library() -> Vec<Game> {
        vec![
            game(Platform::Steam, "620", "Portal 2", Some(620)),
            game(Platform::Steam, "400", "Portal", Some(400)),
            game(Platform::Rockstar, "gtav", "Grand Theft Auto V", Some(271_590)),
            game(Platform::Other, "cyberpunk", "Cyberpunk 2077®", None),
            game(Platform::Epic, "hades", "Hades", None),
        ]
    }

    /// Dossier temporaire propre à un test.
    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ludex-captures-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        b.extend_from_slice(&w.to_be_bytes());
        b.extend_from_slice(&h.to_be_bytes());
        b.extend_from_slice(&[8, 6, 0, 0, 0]);
        b
    }

    fn jpeg(w: u16, h: u16) -> Vec<u8> {
        let mut b = vec![0xFF, 0xD8];
        // APP0 de 16 octets, puis un octet de remplissage avant le SOF2 (progressif).
        b.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x10]);
        b.extend_from_slice(&[0u8; 14]);
        b.extend_from_slice(&[0xFF, 0xFF, 0xC2, 0x00, 0x11, 0x08]);
        b.extend_from_slice(&h.to_be_bytes());
        b.extend_from_slice(&w.to_be_bytes());
        b.extend_from_slice(&[0u8; 12]);
        b
    }

    fn write(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn strips_the_date_from_game_bar_names() {
        assert_eq!(capture_title("Hades 2024-01-15 14-23-33").as_deref(), Some("Hades"));
        assert_eq!(capture_title("Portal 2 1_15_2024 2_23_33 PM").as_deref(), Some("Portal 2"));
        assert_eq!(capture_title("Cyberpunk 2077 15_01_2024 14_23_33").as_deref(), Some("Cyberpunk 2077"));
        assert_eq!(capture_title("Grand Theft Auto V - 2024-01-15 14-23-33").as_deref(), Some("Grand Theft Auto V"));
        assert_eq!(capture_title("2026-10-12 14-23-33"), None);
        assert_eq!(capture_title("vacances"), None);
    }

    #[test]
    fn matches_captures_to_games_by_title() {
        let games = library();
        let id = |t: &str| match_game(t, &games).map(|g| g.id.clone());
        assert_eq!(id("Hades").as_deref(), Some("epic:hades"));
        assert_eq!(id("HADES").as_deref(), Some("epic:hades"));
        assert_eq!(id("Portal 2").as_deref(), Some("steam:620"));
        assert_eq!(id("Portal").as_deref(), Some("steam:400"));
        assert_eq!(id("Cyberpunk 2077").as_deref(), Some("other:cyberpunk"));
        assert_eq!(id("Cyberpunk 2077 (C) 2020 by CD Projekt RED").as_deref(), Some("other:cyberpunk"));
        // Un seul mot en commun ne suffit pas : « Portal Knights » n'est pas « Portal ».
        assert_eq!(id("Portal Knights"), None);
        assert_eq!(id("Minecraft"), None);
    }

    #[test]
    fn reads_image_dimensions() {
        let dir = temp("size");
        write(&dir.join("a.png"), &png(2560, 1440));
        write(&dir.join("b.jpg"), &jpeg(1920, 1080));
        write(&dir.join("c.jpg"), b"pas une image du tout");
        assert_eq!(image_size(&dir.join("a.png")), Some((2560, 1440)));
        assert_eq!(image_size(&dir.join("b.jpg")), Some((1920, 1080)));
        assert_eq!(image_size(&dir.join("c.jpg")), None);
        let _ = fs::remove_dir_all(dir);
    }

    /// Faux dossiers : Steam (jeu installé, jeu Rockstar par appid, appid inconnu) et Game Bar.
    fn fake_pc(name: &str) -> (PathBuf, Roots) {
        let dir = temp(name);
        let remote = dir.join("userdata").join("123").join("760").join("remote");
        let shots = |appid: &str| remote.join(appid).join("screenshots");
        write(&shots("620").join("20240101120000_1.jpg"), &jpeg(1920, 1080));
        write(&shots("620").join("thumbnails").join("20240101120000_1.jpg"), &jpeg(200, 112));
        write(&shots("620").join("20240102120000_1.jpg"), &jpeg(1920, 1080));
        write(&shots("620").join("notes.txt"), b"ignore");
        write(&shots("271590").join("20240103120000_1.jpg"), &jpeg(3840, 2160));
        write(&shots("999").join("20240104120000_1.jpg"), &jpeg(800, 600));
        let captures = dir.join("Captures");
        write(&captures.join("Hades 2024-01-15 14-23-33.png"), &png(1280, 720));
        write(&captures.join("Hades 2024-01-15 14-25-00.mp4"), b"....ftypisom");
        write(&captures.join("Inconnu 2024-01-15 14-23-33.png"), &png(640, 480));
        write(&captures.join("hdr 2024-01-15 14-23-33.jxr"), b"jxr");
        (dir, Roots { steam: Some(remote), gamebar: Some(captures) })
    }

    #[test]
    fn collects_and_groups_screenshots() {
        let (dir, roots) = fake_pc("collect");
        let games = library();
        let all = collect(&roots, &games, None);
        assert_eq!(all.len(), 7);
        let portal: Vec<_> = all.iter().filter(|s| s.game_id.as_deref() == Some("steam:620")).collect();
        assert_eq!(portal.len(), 2);
        let with_thumb = portal.iter().find(|s| s.thumb.is_some()).unwrap();
        assert!(with_thumb.thumb_url.ends_with("/thumb") && with_thumb.url.ends_with("/full"));
        assert_eq!((with_thumb.width, with_thumb.height), (Some(1920), Some(1080)));
        assert!(all.iter().any(|s| s.game_id.as_deref() == Some("rockstar:gtav")));
        let hades: Vec<_> = all.iter().filter(|s| s.game_id.as_deref() == Some("epic:hades")).collect();
        assert_eq!(hades.len(), 2);
        assert!(hades.iter().any(|s| s.video && s.width.is_none()));
        assert_eq!(all.iter().filter(|s| s.game_id.is_none()).count(), 2); // appid 999 et « Inconnu »

        // Un seul jeu : les autres dossiers ne comptent pas.
        let only = collect(&roots, &games, Some("epic:hades"));
        assert_eq!(only.len(), 2);
        assert!(only.windows(2).all(|w| w[0].taken >= w[1].taken));

        let groups = group(all, &games);
        assert_eq!(groups.last().map(|g| g.title.as_str()), Some(OTHER_TITLE));
        assert_eq!(groups.iter().map(|g| g.shots.len()).sum::<usize>(), 7);
        assert!(groups.iter().any(|g| g.title == "Grand Theft Auto V"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn serves_only_files_from_known_folders() {
        let (dir, roots) = fake_pc("serve");
        let games = library();
        let shots = collect(&roots, &games, None);
        let mut index = Index::default();
        index.remember(&roots, &shots, true);
        let steam = shots.iter().find(|s| s.thumb.is_some()).unwrap();
        assert_eq!(index.file(&steam.key, true).as_ref(), steam.thumb.as_ref());
        assert_eq!(index.file(&steam.key, false).as_ref(), Some(&steam.path));
        assert!(index.file("0000000000000000", false).is_none());

        // Fichiers hors des dossiers de captures, ou d'un type non servi : refusés.
        let outside = dir.join("secret.png");
        write(&outside, &png(1, 1));
        assert!(!allowed(&outside, &roots));
        assert!(!allowed(&roots.gamebar.as_ref().unwrap().join("hdr 2024-01-15 14-23-33.jxr"), &roots));
        let nested = roots.gamebar.as_ref().unwrap().join("sous-dossier").join("a.png");
        write(&nested, &png(1, 1));
        assert!(!allowed(&nested, &roots));
        let steam_root = roots.steam.as_ref().unwrap();
        let stray = steam_root.join("620").join("a.jpg");
        write(&stray, &jpeg(1, 1));
        assert!(!allowed(&stray, &roots));
        assert!(!allowed(&steam_root.join("620").join("screenshots").join("..").join("..").join("..").join("..").join("..").join("..").join("secret.png"), &roots));

        // Fichier supprimé après le relevé : plus servi.
        fs::remove_file(&steam.path).unwrap();
        assert!(index.file(&steam.key, false).is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn keys_are_stable_and_checked() {
        let a = key_of(Path::new(r"C:\Captures\Hades 1.png"));
        assert_eq!(a, key_of(Path::new(r"c:\captures\hades 1.PNG")));
        assert!(valid_key(&a));
        assert!(!valid_key("../../secret"));
        assert!(!valid_key("0123456789abcdeg"));
    }

    #[test]
    fn parses_byte_ranges() {
        let mb = 1024 * 1024;
        assert_eq!(byte_range(None, 10), Some((0, 9)));
        assert_eq!(byte_range(Some("bytes=0-"), 10 * mb), Some((0, VIDEO_CHUNK - 1)));
        assert_eq!(byte_range(Some("bytes=5-7"), 10), Some((5, 7)));
        assert_eq!(byte_range(Some("bytes=5-100"), 10), Some((5, 9)));
        assert_eq!(byte_range(Some("bytes=-4"), 10), Some((6, 9)));
        assert_eq!(byte_range(Some("bytes=10-"), 10), None);
        assert_eq!(byte_range(Some("bytes=7-5"), 10), None);
        assert_eq!(byte_range(Some("bytes=a-b"), 10), None);
        assert_eq!(byte_range(None, 0), None);
    }

    #[test]
    fn expands_environment_variables() {
        let profile = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap_or_default();
        let var = if std::env::var("USERPROFILE").is_ok() { "USERPROFILE" } else { "HOME" };
        assert_eq!(expand_env(&format!("%{var}%\\Videos")), format!("{profile}\\Videos"));
        assert_eq!(expand_env("%LUDEX_INCONNUE%\\x"), "%LUDEX_INCONNUE%\\x");
        assert_eq!(expand_env("C:\\sans variable"), "C:\\sans variable");
    }

    /// Relevé réel : `cargo test screenshots_this_pc -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn screenshots_this_pc() {
        let started = std::time::Instant::now();
        let games = crate::library::scan().games;
        let scanned = started.elapsed();
        let roots = Roots::this_pc();
        println!("Steam : {:?}", roots.steam);
        println!("Game Bar : {:?}", roots.gamebar);
        let started = std::time::Instant::now();
        let shots = collect(&roots, &games, None);
        let listed = started.elapsed();
        let started = std::time::Instant::now();
        let again = collect(&roots, &games, None).len();
        println!("Second relevé : {again} captures en {:?}", started.elapsed());
        let started = std::time::Instant::now();
        let one = shots.iter().find_map(|s| s.game_id.clone()).map(|id| collect(&roots, &games, Some(&id)).len());
        println!("Un seul jeu : {one:?} captures en {:?}", started.elapsed());
        let steam = shots.iter().filter(|s| s.source == Source::Steam).count();
        let thumbs = shots.iter().filter(|s| s.thumb.is_some()).count();
        let sized = shots.iter().filter(|s| s.width.is_some()).count();
        let bytes: u64 = shots.iter().map(|s| s.size).sum();
        println!(
            "{} captures ({steam} Steam, {} Game Bar), {thumbs} miniatures, {sized} avec dimensions, {:.1} Mo, relevé en {listed:?} (scan {scanned:?})",
            shots.len(),
            shots.len() - steam,
            bytes as f64 / 1_048_576.0
        );
        let mut index = Index::default();
        index.remember(&roots, &shots, true);
        let refused = shots.iter().filter(|s| index.file(&s.key, true).is_none()).count();
        println!("Refusées au service : {refused}");
        for g in group(shots, &games) {
            let first = &g.shots[0];
            println!(
                "  {} ({}) : {} capture(s), dernière {} {}x{}",
                g.title,
                g.game_id.as_deref().unwrap_or("-"),
                g.shots.len(),
                first.taken,
                first.width.unwrap_or(0),
                first.height.unwrap_or(0)
            );
        }
        for appid in fs::read_dir(roots.steam.as_deref().unwrap_or(Path::new(""))).into_iter().flatten().flatten() {
            let name = appid.file_name().to_string_lossy().into_owned();
            let n = files(&appid.path().join("screenshots")).filter(|p| is_image(p)).count();
            if n > 0 && name.parse().ok().and_then(|id| steam_owner(id, &games)).is_none() {
                println!("  appid sans jeu : {name} ({n})");
            }
        }
    }
}
