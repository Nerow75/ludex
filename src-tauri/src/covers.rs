//! Jaquettes perso : jaquette et image de fond choisies par l'utilisateur pour n'importe quel jeu
//! (jeux Riot sans visuel, jaquette générée, image du launcher qui ne plaît pas).
//!
//! Le fichier vient toujours du sélecteur natif ouvert côté Rust (`choose_cover` dans `lib.rs`),
//! jamais du front. Il est vérifié (extension, signature, 16 Mo au plus, dimensions lues dans
//! l'en-tête) puis COPIÉ dans `<données>/images/custom/<id nettoyé>/<cover|hero>.<ext>` : Ludex ne
//! relit jamais le fichier d'origine. Le choix est gardé dans `user-data.json` (`covers`), avec la
//! date du choix qui versionne l'URL `ludex` (la WebView ne garde pas l'ancienne image en mémoire).
//!
//! Priorité : image perso, puis images locales du launcher, cache disque, CDN, jaquette générée.
//! Posée en dernier au scan ; l'image qu'elle remplace est gardée en mémoire (`Originals`) pour que
//! « Jaquette d'origine » la rende sans rescan. Une jaquette perso coupe la recherche en ligne de
//! `art.rs` (le jeu a une jaquette). Le dossier `custom/` échappe à la purge des 60 jours de
//! `images.rs` : un dossier n'est supprimé que si son jeu n'est ni dans `covers` ni dans la
//! bibliothèque (reste d'un choix annulé).

use crate::images::{self, clean_id};
use crate::library::{art_url, Game};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Sous-dossier du cache des images qui garde les jaquettes perso.
pub const DIR: &str = "custom";
/// Poids maximal d'une image choisie.
pub const MAX_BYTES: u64 = 16 * 1024 * 1024;
/// Extensions proposées par le sélecteur et acceptées à la vérification.
pub const EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];
const MIN_SIDE: u32 = 16;
const MAX_SIDE: u32 = 16_384;

/// Image remplaçable. Valeur fermée reçue du front (`"cover"` ou `"hero"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Cover,
    Hero,
}

impl Kind {
    pub const ALL: [Kind; 2] = [Kind::Cover, Kind::Hero];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Cover => "cover",
            Kind::Hero => "hero",
        }
    }

    /// Titre du sélecteur de fichier.
    pub fn title(self) -> &'static str {
        match self {
            Kind::Cover => tr_str!("Jaquette", "Cover"),
            Kind::Hero => tr_str!("Image de fond", "Background image"),
        }
    }
}

/// Image choisie : extension du fichier copié et date du choix (millisecondes Unix).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Picked {
    pub ext: String,
    pub at: u64,
}

/// Images perso d'un jeu, telles qu'enregistrées dans `user-data.json`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomArt {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover: Option<Picked>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hero: Option<Picked>,
}

impl CustomArt {
    pub fn get(&self, kind: Kind) -> Option<&Picked> {
        match kind {
            Kind::Cover => self.cover.as_ref(),
            Kind::Hero => self.hero.as_ref(),
        }
    }

    pub fn set(&mut self, kind: Kind, value: Option<Picked>) {
        match kind {
            Kind::Cover => self.cover = value,
            Kind::Hero => self.hero = value,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.cover.is_none() && self.hero.is_none()
    }
}

/// Date d'un choix, en millisecondes : deux choix dans la même seconde changent quand même l'URL.
pub fn stamp() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn be16(b: &[u8]) -> u32 {
    u32::from(b[0]) << 8 | u32::from(b[1])
}

fn le16(b: &[u8]) -> u32 {
    u32::from(b[0]) | u32::from(b[1]) << 8
}

fn le24(b: &[u8]) -> u32 {
    u32::from(b[0]) | u32::from(b[1]) << 8 | u32::from(b[2]) << 16
}

/// Dimensions d'une image PNG, JPEG ou WebP, lues dans son en-tête. `None` : en-tête illisible.
pub fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    match images::sniff(bytes)? {
        "png" => {
            let ihdr = bytes.get(12..24).filter(|h| &h[..4] == b"IHDR")?;
            Some((u32::from_be_bytes(ihdr[4..8].try_into().ok()?), u32::from_be_bytes(ihdr[8..12].try_into().ok()?)))
        }
        "jpg" => {
            // Segments jusqu'au premier SOF (début de l'image), qui porte hauteur et largeur.
            let mut pos = 2usize;
            loop {
                let seg = bytes.get(pos..pos + 4)?;
                if seg[0] != 0xFF {
                    return None;
                }
                if seg[1] == 0xFF {
                    pos += 1; // octet de remplissage
                    continue;
                }
                if (0xC0..=0xCF).contains(&seg[1]) && ![0xC4, 0xC8, 0xCC].contains(&seg[1]) {
                    let sof = bytes.get(pos + 4..pos + 9)?;
                    return Some((be16(&sof[3..5]), be16(&sof[1..3])));
                }
                let len = be16(&seg[2..4]) as usize;
                if len < 2 {
                    return None;
                }
                pos += 2 + len;
            }
        }
        _ => {
            let chunk = bytes.get(12..16)?;
            match chunk {
                b"VP8 " => {
                    let frame = bytes.get(23..30).filter(|f| f[..3] == [0x9D, 0x01, 0x2A])?;
                    Some((le16(&frame[3..5]) & 0x3FFF, le16(&frame[5..7]) & 0x3FFF))
                }
                b"VP8L" => {
                    let b = bytes.get(20..25).filter(|b| b[0] == 0x2F)?;
                    let w = 1 + (u32::from(b[1]) | (u32::from(b[2]) & 0x3F) << 8);
                    let h = 1 + (u32::from(b[2]) >> 6 | u32::from(b[3]) << 2 | (u32::from(b[4]) & 0x0F) << 10);
                    Some((w, h))
                }
                b"VP8X" => {
                    let b = bytes.get(24..30)?;
                    Some((1 + le24(&b[..3]), 1 + le24(&b[3..])))
                }
                _ => None,
            }
        }
    }
}

/// Vérifie le contenu d'une image : signature PNG, JPEG ou WebP, poids et dimensions raisonnables.
/// Renvoie l'extension du fichier à copier (tirée de la signature, jamais du nom).
pub fn check_bytes(name: &str, bytes: &[u8]) -> Result<&'static str, String> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err(tr!("{name} est trop lourde : 16 Mo au plus.", "{name} is too large: 16 MB at most."));
    }
    let ext = images::sniff(bytes).ok_or_else(|| tr!("{name} n'est pas une image PNG, JPEG ou WebP.", "{name} isn't a PNG, JPEG or WebP image."))?;
    let (w, h) = dimensions(bytes).ok_or_else(|| tr!("{name} est illisible : en-tête d'image incomplet.", "{name} can't be read: incomplete image header."))?;
    if w < MIN_SIDE || h < MIN_SIDE {
        return Err(tr!("{name} est trop petite ({w} × {h}).", "{name} is too small ({w} × {h})."));
    }
    if w > MAX_SIDE || h > MAX_SIDE {
        return Err(tr!("{name} est trop grande ({w} × {h}) : {MAX_SIDE} pixels de côté au plus.", "{name} is too big ({w} × {h}): {MAX_SIDE} pixels per side at most."));
    }
    Ok(ext)
}

/// Valide un fichier choisi dans le sélecteur et le lit : chemin absolu, extension d'image, fichier
/// existant, 16 Mo au plus, contenu vérifié par `check_bytes`.
pub fn check_image(path: &Path) -> Result<(Vec<u8>, &'static str), String> {
    if !path.is_absolute() {
        return Err(tr!("Chemin invalide.", "Invalid path."));
    }
    let is_image = path.extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)));
    if !is_image {
        return Err(tr!("Choisis une image PNG, JPEG ou WebP.", "Pick a PNG, JPEG or WebP image."));
    }
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let meta = fs::metadata(path).map_err(|_| tr!("Fichier introuvable : {}", "File not found: {}", path.display()))?;
    if !meta.is_file() {
        return Err(tr!("Fichier introuvable : {}", "File not found: {}", path.display()));
    }
    if meta.len() > MAX_BYTES {
        return Err(tr!("{name} est trop lourde : 16 Mo au plus.", "{name} is too large: 16 MB at most."));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(MAX_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|e| tr!("Lecture de {name} impossible : {e}", "Couldn't read {name}: {e}"))?;
    let ext = check_bytes(&name, &bytes)?;
    Ok((bytes, ext))
}

/// Dossier des images perso d'un jeu : `<racine du cache>/custom/<id nettoyé>`.
pub fn folder(root: &Path, id: &str) -> PathBuf {
    root.join(DIR).join(clean_id(id))
}

/// Fichier d'une image perso enregistrée. `None` : extension inconnue (fichier `user-data.json`
/// modifié à la main).
pub fn file(root: &Path, id: &str, kind: Kind, picked: &Picked) -> Option<PathBuf> {
    images::EXTENSIONS.contains(&picked.ext.as_str()).then(|| folder(root, id).join(format!("{}.{}", kind.as_str(), picked.ext)))
}

/// Copie une image vérifiée dans le dossier du jeu (tmp puis renommage, anciennes versions retirées).
pub fn save(root: &Path, id: &str, kind: Kind, bytes: &[u8], ext: &str) -> Result<PathBuf, String> {
    images::store(&folder(root, id), kind.as_str(), bytes, ext).map_err(|e| tr!("Copie de l'image impossible : {e}", "Couldn't copy the image: {e}"))
}

/// Retire une image perso (toutes extensions), puis le dossier du jeu s'il est vide.
pub fn remove(root: &Path, id: &str, kind: Kind) {
    let dir = folder(root, id);
    for ext in images::EXTENSIONS.iter().chain(["tmp"].iter()) {
        let _ = fs::remove_file(dir.join(format!("{}.{ext}", kind.as_str())));
    }
    let _ = fs::remove_dir(&dir); // échoue s'il reste une image : voulu
}

/// Image d'un jeu avant sa jaquette perso : fichier local et adresse donnée au front.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Original {
    local: Option<PathBuf>,
    url: Option<String>,
}

/// Images d'origine des jeux qui ont une image perso, par id et type. Refaites à chaque scan.
pub type Originals = BTreeMap<(String, Kind), Original>;

fn slots(game: &mut Game, kind: Kind) -> (&mut Option<PathBuf>, &mut Option<String>) {
    let local = match kind {
        Kind::Cover => &mut game.local_art.cover,
        Kind::Hero => &mut game.local_art.hero,
    };
    let url = match kind {
        Kind::Cover => &mut game.cover_url,
        Kind::Hero => &mut game.hero_url,
    };
    (local, url)
}

/// Pose une image perso sur le jeu, en gardant l'image qu'elle remplace (sauf si une image perso
/// était déjà là : l'origine reste la première).
pub fn set(game: &mut Game, kind: Kind, file: PathBuf, picked: &Picked, originals: &mut Originals) {
    let url = format!("{}?v={}", art_url(game, kind.as_str()), picked.at);
    let already = game.custom_art.contains(&kind);
    let key = (game.id.clone(), kind);
    let (local, shown) = slots(game, kind);
    if !already || !originals.contains_key(&key) {
        originals.insert(key, Original { local: local.take(), url: shown.take() });
    }
    *local = Some(file);
    *shown = Some(url);
    if !already {
        game.custom_art.push(kind);
        game.custom_art.sort();
    }
}

/// Rend au jeu son image d'origine. Sans origine connue : plus d'image (jaquette générée, ou
/// recherche en ligne relancée par l'appelant).
pub fn restore(game: &mut Game, kind: Kind, originals: &mut Originals) {
    if !game.custom_art.contains(&kind) {
        return;
    }
    let original = originals.remove(&(game.id.clone(), kind)).unwrap_or_default();
    let (local, shown) = slots(game, kind);
    *local = original.local;
    *shown = original.url;
    game.custom_art.retain(|&k| k != kind);
}

/// Pose les images perso enregistrées d'un jeu scanné (en dernier : elles priment sur tout). Une
/// image dont le fichier a disparu est ignorée. Renvoie vrai si le jeu a changé.
pub fn apply(game: &mut Game, custom: Option<&CustomArt>, root: &Path, originals: &mut Originals) -> bool {
    let Some(custom) = custom else { return false };
    let mut changed = false;
    for kind in Kind::ALL {
        let Some(picked) = custom.get(kind) else { continue };
        let Some(path) = file(root, &game.id, kind, picked).filter(|p| p.is_file()) else { continue };
        set(game, kind, path, picked, originals);
        changed = true;
    }
    changed
}

/// Supprime les dossiers d'images perso dont le jeu n'est plus connu : ni dans `covers` de
/// `user-data.json`, ni dans la bibliothèque (`keep` : ids des deux). Renvoie le nombre supprimé.
pub fn purge<'a>(root: &Path, keep: impl IntoIterator<Item = &'a str>) -> usize {
    let keep: BTreeSet<String> = keep.into_iter().map(clean_id).collect();
    images::read_dirs(&root.join(DIR))
        .into_iter()
        .filter(|dir| images::name(dir).is_some_and(|n| !keep.contains(n)))
        .filter(|dir| fs::remove_dir_all(dir).is_ok())
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{Launch, LocalArt, Platform, ART_ORIGIN};

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ludex-test-covers-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    /// PNG minimal : signature puis en-tête IHDR.
    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        b.extend_from_slice(&w.to_be_bytes());
        b.extend_from_slice(&h.to_be_bytes());
        b.extend_from_slice(&[8, 6, 0, 0, 0]);
        b
    }

    /// JPEG minimal : APP0 de 16 octets, puis SOF0 (progressif : SOF2).
    fn jpeg(w: u16, h: u16) -> Vec<u8> {
        let mut b = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        b.extend_from_slice(&[0; 14]);
        b.extend_from_slice(&[0xFF, 0xFF, 0xC2, 0x00, 0x11, 0x08]); // remplissage puis SOF2
        b.extend_from_slice(&h.to_be_bytes());
        b.extend_from_slice(&w.to_be_bytes());
        b
    }

    #[test]
    fn refusals_in_english() {
        crate::i18n::with(crate::i18n::Lang::En, || {
            assert_eq!(check_bytes("a.png", &png(4, 4)).unwrap_err(), "a.png is too small (4 × 4).");
            assert_eq!(check_bytes("a.png", b"pas une image").unwrap_err(), "a.png isn't a PNG, JPEG or WebP image.");
            assert_eq!(check_image(Path::new(r"C:\x\a.gif")).unwrap_err(), "Pick a PNG, JPEG or WebP image.");
            assert_eq!(Kind::Hero.title(), "Background image");
        });
        assert_eq!(Kind::Hero.title(), "Image de fond");
    }

    fn webp(chunk: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut b = b"RIFF\0\0\0\0WEBP".to_vec();
        b.extend_from_slice(chunk);
        b.extend_from_slice(&[0; 4]);
        b.extend_from_slice(payload);
        b
    }

    fn riot() -> Game {
        Game::new(Platform::Riot, "valorant", "VALORANT", Launch::Uri(String::new()))
    }

    #[test]
    fn reads_dimensions_of_each_format() {
        assert_eq!(dimensions(&png(600, 900)), Some((600, 900)));
        assert_eq!(dimensions(&jpeg(1920, 620)), Some((1920, 620)));
        // VP8 : 3 octets d'en-tête de trame, code de début, largeur et hauteur sur 14 bits.
        let vp8 = webp(b"VP8 ", &[0, 0, 0, 0x9D, 0x01, 0x2A, 0x58, 0x02, 0x84, 0x03]);
        assert_eq!(dimensions(&vp8), Some((600, 900)));
        // VP8L : 0x2F puis largeur − 1 et hauteur − 1 sur 14 bits chacune.
        let (w, h) = (600u32 - 1, 900u32 - 1);
        let bits = w | h << 14;
        let vp8l = webp(b"VP8L", &[0x2F, bits as u8, (bits >> 8) as u8, (bits >> 16) as u8, (bits >> 24) as u8]);
        assert_eq!(dimensions(&vp8l), Some((600, 900)));
        let vp8x = webp(b"VP8X", &[0, 0, 0, 0, 0x57, 0x02, 0, 0x83, 0x03, 0]);
        assert_eq!(dimensions(&vp8x), Some((600, 900)));
        assert_eq!(dimensions(&webp(b"VP8 ", &[0; 10])), None); // code de début absent
        assert_eq!(dimensions(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10]), None); // tronqué
        assert_eq!(dimensions(b"GIF89a"), None);
    }

    #[test]
    fn checks_the_picked_image() {
        assert_eq!(check_bytes("a.png", &png(600, 900)), Ok("png"));
        assert_eq!(check_bytes("a.jpeg", &jpeg(1920, 620)), Ok("jpg"));
        assert!(check_bytes("a.png", b"<html>").unwrap_err().contains("n'est pas une image"));
        assert!(check_bytes("a.png", &png(8, 900)).unwrap_err().contains("trop petite"));
        assert!(check_bytes("a.png", &png(20_000, 900)).unwrap_err().contains("trop grande"));
        assert!(check_bytes("a.jpg", &[0xFF, 0xD8, 0xFF, 0xE0]).unwrap_err().contains("illisible"));
        let mut heavy = png(600, 900);
        heavy.resize(MAX_BYTES as usize + 1, 0);
        assert!(check_bytes("a.png", &heavy).unwrap_err().contains("16 Mo"));

        let root = temp_root("check");
        fs::create_dir_all(&root).unwrap();
        let good = root.join("Jaquette.PNG");
        fs::write(&good, png(600, 900)).unwrap();
        let disguised = root.join("faux.jpg");
        fs::write(&disguised, b"MZ pas une image").unwrap();
        let text = root.join("image.txt");
        fs::write(&text, png(600, 900)).unwrap();
        let (bytes, ext) = check_image(&good).unwrap();
        assert_eq!((bytes, ext), (png(600, 900), "png"));
        assert!(check_image(&disguised).is_err());
        assert_eq!(check_image(&text).unwrap_err(), "Choisis une image PNG, JPEG ou WebP.");
        assert!(check_image(&root.join("absente.png")).unwrap_err().starts_with("Fichier introuvable"));
        assert_eq!(check_image(Path::new(r"images\a.png")).unwrap_err(), "Chemin invalide.");
        // Signature JPEG dans un fichier nommé .png : copié en .jpg.
        let misnamed = root.join("vraie.png");
        fs::write(&misnamed, jpeg(600, 900)).unwrap();
        assert_eq!(check_image(&misnamed).unwrap().1, "jpg");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn copies_the_image_into_the_data_folder() {
        let root = temp_root("copy");
        let source = root.join("source.png");
        fs::create_dir_all(&root).unwrap();
        fs::write(&source, png(600, 900)).unwrap();
        let (bytes, ext) = check_image(&source).unwrap();
        let file = save(&root, "riot:valorant", Kind::Cover, &bytes, ext).unwrap();
        assert!(file.starts_with(root.join(DIR)));
        assert_eq!(file.file_name().unwrap(), "cover.png");
        // Dossier nettoyé : jamais l'id brut (« : » interdit sous Windows).
        assert_eq!(file.parent().unwrap().file_name().unwrap().to_str().unwrap(), clean_id("riot:valorant"));
        // Le fichier d'origine peut disparaître : Ludex garde sa copie.
        fs::remove_file(&source).unwrap();
        assert_eq!(fs::read(&file).unwrap(), png(600, 900));

        // Une nouvelle image d'un autre type remplace l'ancienne version.
        let jpg = save(&root, "riot:valorant", Kind::Cover, &jpeg(600, 900), "jpg").unwrap();
        assert!(jpg.is_file() && !file.exists());
        let picked = Picked { ext: "jpg".into(), at: 1 };
        assert_eq!(super::file(&root, "riot:valorant", Kind::Cover, &picked), Some(jpg.clone()));
        assert_eq!(super::file(&root, "riot:valorant", Kind::Cover, &Picked { ext: "../x".into(), at: 1 }), None);
        remove(&root, "riot:valorant", Kind::Cover);
        assert!(!jpg.exists() && !folder(&root, "riot:valorant").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn custom_art_wins_and_resets_to_the_original() {
        let root = temp_root("priority");
        let mut game = Game::new(Platform::Steam, "730", "Counter-Strike 2", Launch::Uri(String::new()));
        // Jaquette locale du launcher, image de fond venue du CDN.
        game.local_art = LocalArt { cover: Some(PathBuf::from(r"C:\Steam\librarycache\730\cover.jpg")), ..LocalArt::default() };
        game.cover_url = Some(art_url(&game, "cover"));
        let cdn_hero = "https://cdn.cloudflare.steamstatic.com/steam/apps/730/library_hero.jpg".to_string();
        game.hero_url = Some(cdn_hero.clone());
        let launcher = game.clone();

        let picked = Picked { ext: "png".into(), at: 42 };
        let cover = save(&root, &game.id, Kind::Cover, &png(600, 900), "png").unwrap();
        let custom = CustomArt { cover: Some(picked.clone()), hero: None };
        let mut originals = Originals::new();
        assert!(apply(&mut game, Some(&custom), &root, &mut originals));
        assert_eq!(game.local_art.cover.as_ref(), Some(&cover));
        assert_eq!(game.cover_url.as_deref(), Some(format!("{ART_ORIGIN}/steam/730/cover?v=42").as_str()));
        assert_eq!(game.hero_url.as_deref(), Some(cdn_hero.as_str())); // image de fond inchangée
        assert_eq!(game.custom_art, vec![Kind::Cover]);
        // Une jaquette perso coupe la recherche en ligne et le téléchargement de la jaquette.
        let mut other = Game::new(Platform::Riot, "lol", "League of Legends", Launch::Uri(String::new()));
        save(&root, &other.id, Kind::Cover, &png(600, 900), "png").unwrap();
        apply(&mut other, Some(&custom), &root, &mut originals);
        assert!(!crate::art::needs_art(&other));
        let images = crate::images::ImageCache::load(root.join("cache"));
        assert!(images.to_download(std::slice::from_ref(&game)).iter().all(|j| j.kind != "cover"));

        // Nouveau choix : l'origine reste celle du launcher.
        let again = Picked { ext: "png".into(), at: 43 };
        set(&mut game, Kind::Cover, cover.clone(), &again, &mut originals);
        assert!(game.cover_url.as_deref().unwrap().ends_with("?v=43"));
        assert_eq!(game.custom_art, vec![Kind::Cover]);

        restore(&mut game, Kind::Cover, &mut originals);
        assert_eq!(game.local_art.cover, launcher.local_art.cover);
        assert_eq!(game.cover_url, launcher.cover_url);
        assert!(game.custom_art.is_empty());
        restore(&mut game, Kind::Cover, &mut originals); // sans effet
        assert_eq!(game.cover_url, launcher.cover_url);

        // Jeu sans image : retour à la jaquette générée.
        let mut riot = riot();
        let hero = save(&root, &riot.id, Kind::Hero, &png(1920, 620), "png").unwrap();
        set(&mut riot, Kind::Hero, hero, &picked, &mut originals);
        restore(&mut riot, Kind::Hero, &mut Originals::new());
        assert_eq!((riot.hero_url, riot.local_art.hero), (None, None));

        // Fichier perso disparu : le jeu garde ses images.
        let mut lost = launcher.clone();
        let gone = CustomArt { cover: None, hero: Some(Picked { ext: "png".into(), at: 1 }) };
        assert!(!apply(&mut lost, Some(&gone), &root, &mut Originals::new()));
        assert_eq!(lost.hero_url.as_deref(), Some(cdn_hero.as_str()));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn purges_only_unknown_custom_folders() {
        let root = temp_root("purge");
        for id in ["riot:valorant", "steam:730", "other:manual-0123456789abcdef"] {
            save(&root, id, Kind::Cover, &png(600, 900), "png").unwrap();
        }
        // Valorant : dans `covers` (même désinstallé) ; CS2 : dans la bibliothèque ; le jeu manuel : ni l'un ni l'autre.
        assert_eq!(purge(&root, ["riot:valorant", "steam:730"]), 1);
        assert!(folder(&root, "riot:valorant").is_dir() && folder(&root, "steam:730").is_dir());
        assert!(!folder(&root, "other:manual-0123456789abcdef").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn kind_is_a_closed_value() {
        assert_eq!(serde_json::from_str::<Kind>("\"hero\"").unwrap(), Kind::Hero);
        assert!(serde_json::from_str::<Kind>("\"logo\"").is_err());
        assert!(serde_json::from_str::<Kind>("\"../cover\"").is_err());
        let stored: CustomArt = serde_json::from_str(r#"{"cover":{"ext":"png","at":5}}"#).unwrap();
        assert_eq!(stored.get(Kind::Cover).map(|p| p.at), Some(5));
        assert_eq!(serde_json::to_string(&CustomArt::default()).unwrap(), "{}");
    }
}
