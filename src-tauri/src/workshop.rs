//! Mods Steam Workshop installés pour un jeu Steam (lecture seule).
//!
//! Source locale : `steamapps/workshop/appworkshop_<appid>.acf` de la bibliothèque Steam du jeu
//! (`AppWorkshop > WorkshopItemsInstalled > <id> { size, timeupdated, manifest }` et
//! `WorkshopItemDetails > <id> { timeupdated, latest_timeupdated, manifest, latest_manifest }`), et
//! le dossier `steamapps/workshop/content/<appid>/<id>` de chaque mod. Rien n'est jamais écrit dans
//! le dossier de Steam.
//!
//! Titres et vignettes : API publique `ISteamRemoteStorage/GetPublishedFileDetails/v1` (POST, sans
//! clé ni compte), par lots, redirections refusées, arrêt à la première erreur réseau. Gardés dans
//! `workshop-cache.json` (dossier de données de l'app) : un mod n'est redemandé qu'au bout de
//! 30 jours, et une réponse vide n'efface pas ce qui est connu. De la vignette, seuls les deux
//! segments du chemin (`/ugc/<nombre>/<empreinte>/`) sont gardés : l'URL est reconstruite par Ludex
//! sur `images.steamusercontent.com` (seule origine ajoutée à `img-src`).

use crate::library::{Game, Platform};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const REFRESH_AFTER: u64 = 30 * 24 * 3600;
/// À augmenter quand le contenu du cache change : tout est alors redemandé.
const CACHE_VERSION: u32 = 1;
const DETAILS_URL: &str = "https://api.steampowered.com/ISteamRemoteStorage/GetPublishedFileDetails/v1/";
/// Mods demandés par requête.
pub const BATCH: usize = 50;
/// Une réponse plus lourde est refusée (descriptions comprises, un lot de 50 fait ~200 Ko).
const MAX_BODY: u64 = 8 * 1024 * 1024;
/// Manifeste plus lourd ignoré (177 mods font ~60 Ko).
const MAX_ACF: u64 = 16 * 1024 * 1024;
/// Mods gardés au plus par jeu.
const MAX_MODS: usize = 5000;
const MAX_TITLE: usize = 120;
/// Hôte des vignettes : seule origine d'image ajoutée à la CSP.
const PREVIEW_HOST: &str = "https://images.steamusercontent.com/ugc/";
/// Préfixes acceptés dans les réponses de l'API (l'ancien hôte sert les mêmes chemins).
const PREVIEW_PREFIXES: [&str; 2] = ["https://images.steamusercontent.com/ugc/", "https://steamuserimages-a.akamaihd.net/ugc/"];
/// Vignette réduite par le CDN (image complète parfois de plusieurs Mo).
const PREVIEW_QUERY: &str = "?imw=128&imh=128&ima=fit&impolicy=Letterbox&imcolor=%23000000&letterbox=false";

/// Mod installé, envoyé au front.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mod {
    /// Id Workshop (nombre en texte : il dépasse les entiers sûrs de JavaScript).
    pub id: String,
    /// Titre de l'API, `None` tant qu'il est inconnu.
    pub title: Option<String>,
    pub preview_url: Option<String>,
    /// Octets sur le disque (`size` du manifeste, sinon dossier mesuré).
    pub size: u64,
    /// Version installée (secondes Unix), si connue.
    pub updated: Option<u64>,
    /// Steam connaît une version plus récente qu'il n'a pas encore téléchargée.
    pub pending: bool,
    /// Le dossier du mod existe.
    pub present: bool,
}

/// Mods installés d'un jeu.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mods {
    pub mods: Vec<Mod>,
    pub total: u64,
    /// Le dossier des mods du jeu existe (« Ouvrir le dossier »).
    pub folder: bool,
    /// Des titres restent à demander (cache absent ou vieux de plus de 30 jours).
    pub stale: bool,
}

// ---------------------------------------------------------------------------------------------
// Manifeste local
// ---------------------------------------------------------------------------------------------

/// Mod lu dans `appworkshop_<appid>.acf`.
#[derive(Clone, Debug, PartialEq)]
pub struct Installed {
    pub id: u64,
    pub size: Option<u64>,
    pub updated: Option<u64>,
    pub pending: bool,
}

/// Id Workshop valide : chiffres seulement, 20 au plus, différent de zéro.
pub fn parse_id(text: &str) -> Option<u64> {
    if text.is_empty() || text.len() > 20 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok().filter(|&n| n > 0)
}

/// Appid d'un jeu Steam (chiffres seulement).
pub fn appid(game: &Game) -> Option<u32> {
    if game.platform != Platform::Steam {
        return None;
    }
    let id = &game.platform_id;
    (!id.is_empty() && id.len() <= 10 && id.bytes().all(|b| b.is_ascii_digit())).then(|| id.parse().ok()).flatten()
}

/// Dossier `steamapps/workshop` de la bibliothèque du jeu : le dossier d'installation d'un jeu
/// Steam est `<bibliothèque>/steamapps/common/<dossier>`.
pub fn workshop_dir(game: &Game) -> Option<PathBuf> {
    appid(game)?;
    let dir = Path::new(game.install_dir.as_deref()?);
    let common = dir.parent()?;
    let steamapps = common.parent()?;
    let named = |p: &Path, name: &str| p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name));
    (named(common, "common") && named(steamapps, "steamapps")).then(|| steamapps.join("workshop"))
}

fn number(v: &crate::library::vdf::Vdf, key: &str) -> Option<u64> {
    v.str(key).and_then(|s| s.parse().ok())
}

/// Lit le manifeste Workshop d'un jeu. `None` : illisible, ou d'un autre appid.
pub fn parse_acf(text: &str, appid: u32) -> Option<Vec<Installed>> {
    let doc = crate::library::vdf::parse(text);
    let root = doc.get("AppWorkshop")?;
    if root.str("appid")?.parse::<u32>().ok()? != appid {
        return None;
    }
    let details = root.get("WorkshopItemDetails");
    let mut out = Vec::new();
    for (key, item) in root.get("WorkshopItemsInstalled").map(|v| v.entries()).unwrap_or_default() {
        let Some(id) = parse_id(key) else { continue };
        if out.iter().any(|m: &Installed| m.id == id) {
            continue;
        }
        let updated = number(item, "timeupdated").filter(|&t| t > 0);
        // Mise à jour en attente : Steam a noté une version plus récente que celle installée.
        let pending = details.and_then(|d| d.get(key)).is_some_and(|d| {
            let latest = number(d, "latest_timeupdated").unwrap_or(0);
            let newer = updated.is_some_and(|u| latest > u);
            let manifest = item.str("manifest").or_else(|| d.str("manifest")).unwrap_or("");
            let latest_manifest = d.str("latest_manifest").unwrap_or("");
            let other = !latest_manifest.is_empty() && latest_manifest != "0" && !manifest.is_empty() && latest_manifest != manifest;
            newer || other
        });
        out.push(Installed { id, size: number(item, "size"), updated, pending });
        if out.len() >= MAX_MODS {
            break;
        }
    }
    Some(out)
}

/// Mods installés d'un jeu Steam, lus sur le disque. `None` : pas un jeu Steam, ou aucun manifeste.
pub fn installed(game: &Game) -> Option<(PathBuf, Vec<Installed>)> {
    let appid = appid(game)?;
    let dir = workshop_dir(game)?;
    let file = fs::File::open(dir.join(format!("appworkshop_{appid}.acf"))).ok()?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut std::io::Read::take(file, MAX_ACF), &mut text).ok()?;
    let list = parse_acf(&text, appid)?;
    Some((dir.join("content").join(appid.to_string()), list))
}

/// Liste envoyée au front : manifeste, dossiers sur le disque et titres du cache.
pub fn list(content: &Path, installed: &[Installed], cache: &WorkshopCache) -> Mods {
    let mut mods: Vec<Mod> = installed
        .iter()
        .map(|m| {
            let dir = content.join(m.id.to_string());
            let present = dir.is_dir();
            // Taille absente du manifeste : dossier mesuré (liens et jonctions non suivis).
            let size = match m.size.filter(|&s| s > 0) {
                Some(s) => s,
                None if present => crate::storage::dir_size(&dir).unwrap_or(0),
                None => 0,
            };
            let known = cache.items.get(&m.id);
            Mod {
                id: m.id.to_string(),
                title: known.and_then(|e| e.title.clone()),
                preview_url: known.and_then(|e| e.preview.as_ref()).map(Preview::url),
                size,
                updated: m.updated,
                pending: m.pending,
                present,
            }
        })
        .collect();
    // Les plus récemment mis à jour d'abord (une mise à jour en attente se voit en tête), puis par id.
    mods.sort_by(|a, b| b.updated.cmp(&a.updated).then_with(|| a.id.cmp(&b.id)));
    let ids: Vec<u64> = installed.iter().map(|m| m.id).collect();
    Mods {
        total: mods.iter().map(|m| m.size).fold(0u64, u64::saturating_add),
        folder: content.is_dir(),
        stale: !cache.to_fetch(&ids).is_empty(),
        mods,
    }
}

// ---------------------------------------------------------------------------------------------
// API publique et cache
// ---------------------------------------------------------------------------------------------

/// Vignette : segments du chemin `/ugc/<nombre>/<empreinte>/`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preview {
    ugc: String,
    hash: String,
}

impl Preview {
    /// Vignette d'une réponse de l'API, seulement sur un hôte connu et sous la forme attendue
    /// (aucune requête, aucun autre chemin).
    fn parse(url: &str) -> Option<Preview> {
        let rest = PREVIEW_PREFIXES.iter().find_map(|p| url.strip_prefix(p))?;
        let rest = rest.strip_suffix('/').unwrap_or(rest);
        let (ugc, hash) = rest.split_once('/')?;
        let ugc_ok = !ugc.is_empty() && ugc.len() <= 20 && ugc.bytes().all(|b| b.is_ascii_digit());
        let hash_ok = (16..=64).contains(&hash.len()) && hash.bytes().all(|b| b.is_ascii_hexdigit());
        (ugc_ok && hash_ok).then(|| Preview { ugc: ugc.to_string(), hash: hash.to_string() })
    }

    /// Vignette reconstruite par Ludex.
    fn url(&self) -> String {
        format!("{PREVIEW_HOST}{}/{}/{PREVIEW_QUERY}", self.ugc, self.hash)
    }

    fn valid(&self) -> bool {
        Preview::parse(&format!("{PREVIEW_HOST}{}/{}/", self.ugc, self.hash)).as_ref() == Some(self)
    }
}

/// Ce que l'API dit d'un mod publié.
#[derive(Clone, Debug, PartialEq)]
pub struct Info {
    pub title: Option<String>,
    preview: Option<Preview>,
}

/// Lit la réponse de `GetPublishedFileDetails`. Seuls les ids demandés sont gardés ; `None` pour un
/// mod que l'API ne décrit pas (retiré, privé : `result` différent de 1).
pub fn parse_details(body: &str, asked: &[u64]) -> Result<HashMap<u64, Option<Info>>, String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| format!("réponse illisible : {e}"))?;
    let list = value
        .get("response")
        .and_then(|r| r.get("publishedfiledetails"))
        .and_then(|d| d.as_array())
        .ok_or("réponse sans publishedfiledetails")?;
    let mut out = HashMap::new();
    for item in list {
        let Some(id) = item.get("publishedfileid").and_then(|v| v.as_str()).and_then(parse_id) else { continue };
        if !asked.contains(&id) || out.contains_key(&id) {
            continue;
        }
        if item.get("result").and_then(|r| r.as_u64()) != Some(1) {
            out.insert(id, None);
            continue;
        }
        let title = item.get("title").and_then(|t| t.as_str()).and_then(|t| crate::library::manual::clean_text(t, MAX_TITLE));
        let preview = item.get("preview_url").and_then(|p| p.as_str()).and_then(Preview::parse);
        out.insert(id, Some(Info { title, preview }));
    }
    Ok(out)
}

/// Demande les titres et vignettes d'un lot de mods (une requête POST, sans redirection).
pub fn request(ids: &[u64]) -> Result<HashMap<u64, Option<Info>>, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .redirects(0)
        .user_agent("Ludex (lanceur de jeux)")
        .build();
    let count = ids.len().to_string();
    let keys: Vec<String> = (0..ids.len()).map(|i| format!("publishedfileids[{i}]")).collect();
    let values: Vec<String> = ids.iter().map(u64::to_string).collect();
    let mut form: Vec<(&str, &str)> = vec![("itemcount", count.as_str())];
    form.extend(keys.iter().map(String::as_str).zip(values.iter().map(String::as_str)));
    let response = agent.post(DETAILS_URL).send_form(&form).map_err(|e| e.to_string())?;
    if response.status() != 200 {
        return Err(format!("réponse {}", response.status()));
    }
    let mut body = String::new();
    std::io::Read::read_to_string(&mut std::io::Read::take(response.into_reader(), MAX_BODY), &mut body).map_err(|e| e.to_string())?;
    parse_details(&body, ids)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Entry {
    /// Date de la requête (secondes Unix).
    checked: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    preview: Option<Preview>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WorkshopCache {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    items: BTreeMap<u64, Entry>,
    #[serde(skip)]
    path: PathBuf,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl WorkshopCache {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("workshop-cache.json");
        let mut cache: WorkshopCache = fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        if cache.version != CACHE_VERSION {
            cache.items.clear();
            cache.version = CACHE_VERSION;
        }
        // Fichier modifié à la main : titre renettoyé, vignette revérifiée.
        for entry in cache.items.values_mut() {
            entry.title = entry.title.as_deref().and_then(|t| crate::library::manual::clean_text(t, MAX_TITLE));
            if entry.preview.as_ref().is_some_and(|p| !p.valid()) {
                entry.preview = None;
            }
        }
        cache.path = path;
        cache
    }

    pub fn save(&self) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }

    /// Mods jamais demandés, ou demandés il y a plus de 30 jours, dans l'ordre donné.
    pub fn to_fetch(&self, ids: &[u64]) -> Vec<u64> {
        let now = now();
        ids.iter().copied().filter(|id| self.items.get(id).is_none_or(|e| now.saturating_sub(e.checked) > REFRESH_AFTER)).collect()
    }

    /// Note une réponse. Ce qui manque à la réponse (mod retiré, titre ou vignette absents)
    /// n'efface pas ce qui est connu.
    pub fn record(&mut self, id: u64, info: Option<Info>) {
        let old = self.items.get(&id);
        let (title, preview) = match info {
            Some(i) => (i.title.or_else(|| old.and_then(|e| e.title.clone())), i.preview.or_else(|| old.and_then(|e| e.preview.clone()))),
            None => (old.and_then(|e| e.title.clone()), old.and_then(|e| e.preview.clone())),
        };
        self.items.insert(id, Entry { checked: now(), title, preview });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TMODLOADER: &str = include_str!("workshop_samples/appworkshop_1281930.acf");
    const ONE_MOD: &str = include_str!("workshop_samples/appworkshop_2749950.acf");
    const DETAILS: &str = include_str!("workshop_samples/details.json");

    fn steam_game(appid: &str, dir: &str) -> Game {
        let mut g = Game::new(Platform::Steam, appid, "Jeu", crate::library::Launch::Uri(format!("steam://rungameid/{appid}")));
        g.install_dir = Some(dir.to_string());
        g
    }

    #[test]
    fn reads_real_manifests() {
        let mods = parse_acf(TMODLOADER, 1281930).unwrap();
        assert_eq!(mods.len(), 3);
        assert_eq!(mods[0], Installed { id: 2824688072, size: Some(404391973), updated: Some(1786328541), pending: false });
        assert!(mods.iter().all(|m| !m.pending));
        let one = parse_acf(ONE_MOD, 2749950).unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].id, 3788254489);
        // Manifeste d'un autre jeu, ou illisible.
        assert!(parse_acf(TMODLOADER, 2749950).is_none());
        assert!(parse_acf("rien", 1281930).is_none());
    }

    #[test]
    fn notices_pending_updates() {
        // Échantillon réel modifié : Steam connaît une version plus récente du premier mod.
        let newer = TMODLOADER.replacen("\"latest_timeupdated\"\t\t\"1786328541\"", "\"latest_timeupdated\"\t\t\"1790000000\"", 1);
        assert_ne!(newer, TMODLOADER);
        let mods = parse_acf(&newer, 1281930).unwrap();
        assert!(mods[0].pending && !mods[1].pending && !mods[2].pending);
        // Autre manifeste attendu, même date.
        let other = TMODLOADER.replacen("\"latest_manifest\"\t\t\"7020942510722799195\"", "\"latest_manifest\"\t\t\"1\"", 1);
        let mods = parse_acf(&other, 1281930).unwrap();
        assert!(!mods[0].pending && mods[1].pending);
    }

    #[test]
    fn refuses_bad_ids() {
        for bad in ["", "0", "-1", "12a", "1 2", "../1", "123456789012345678901", "+5", "１２"] {
            assert_eq!(parse_id(bad), None, "{bad}");
        }
        assert_eq!(parse_id("2824688072"), Some(2824688072));
        let text = "\"AppWorkshop\" { \"appid\" \"10\" \"WorkshopItemsInstalled\" { \"..\\\\x\" { \"size\" \"1\" } \"0\" { } \"42\" { \"size\" \"5\" } \"42\" { \"size\" \"9\" } } }";
        let mods = parse_acf(text, 10).unwrap();
        assert_eq!(mods, vec![Installed { id: 42, size: Some(5), updated: None, pending: false }]);
    }

    #[test]
    fn finds_the_workshop_folder() {
        let g = steam_game("1281930", r"E:\steam\steamapps\common\tModLoader");
        assert_eq!(workshop_dir(&g), Some(PathBuf::from(r"E:\steam\steamapps\workshop")));
        assert_eq!(appid(&g), Some(1281930));
        // Dossier inattendu, appid illisible ou autre launcher : rien.
        assert_eq!(workshop_dir(&steam_game("1281930", r"E:\Jeux\tModLoader")), None);
        assert_eq!(workshop_dir(&steam_game("abc", r"E:\steam\steamapps\common\x")), None);
        let mut epic = steam_game("1281930", r"E:\steam\steamapps\common\x");
        epic.platform = Platform::Epic;
        assert_eq!(workshop_dir(&epic), None);
    }

    #[test]
    fn reads_api_answers() {
        let asked = [3788254489, 2824688072, 2915318664, 1];
        let out = parse_details(DETAILS, &asked).unwrap();
        assert_eq!(out.len(), 4);
        let calamity = out[&2824688072].as_ref().unwrap();
        assert_eq!(calamity.title.as_deref(), Some("Calamity Mod"));
        assert_eq!(
            calamity.preview.as_ref().unwrap().url(),
            format!("https://images.steamusercontent.com/ugc/18354810480491550448/8159A1EAB947FEB7CB222CE4AC6075AD5020B25E/{PREVIEW_QUERY}")
        );
        assert_eq!(out[&1], None);
        // Un id non demandé est ignoré.
        let partial = parse_details(DETAILS, &[2915318664]).unwrap();
        assert_eq!(partial.len(), 1);
        assert!(parse_details("{}", &asked).is_err());
        assert!(parse_details("pas du json", &asked).is_err());
    }

    #[test]
    fn checks_preview_urls() {
        let ok = "https://images.steamusercontent.com/ugc/2039600735598372411/4AF82931EAFF508C298CB58684B5A2FE55A6ECC6/";
        assert!(Preview::parse(ok).is_some());
        assert!(Preview::parse("https://steamuserimages-a.akamaihd.net/ugc/1/4AF82931EAFF508C298C/").is_some());
        for bad in [
            "http://images.steamusercontent.com/ugc/1/4AF82931EAFF508C298C/",
            "https://evil.example/ugc/1/4AF82931EAFF508C298C/",
            "https://images.steamusercontent.com.evil.example/ugc/1/4AF82931EAFF508C298C/",
            "https://images.steamusercontent.com/ugc/1/4AF82931EAFF508C298C/?x=1",
            "https://images.steamusercontent.com/ugc/../4AF82931EAFF508C298C/",
            "https://images.steamusercontent.com/ugc/1/../../x/",
            "https://images.steamusercontent.com/ugc/1/4AF8/",
            "https://images.steamusercontent.com/ugc/1/4AF82931EAFF508C298C/autre",
            "",
        ] {
            assert!(Preview::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn caches_answers() {
        let dir = std::env::temp_dir().join(format!("ludex-workshop-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut cache = WorkshopCache::load(&dir);
        let asked = [3788254489, 2824688072, 1];
        assert_eq!(cache.to_fetch(&asked), asked.to_vec());
        for (id, info) in parse_details(DETAILS, &asked).unwrap() {
            cache.record(id, info);
        }
        assert!(cache.to_fetch(&asked).is_empty());
        // Une réponse vide (mod retiré ou devenu privé) n'efface pas ce qui est connu.
        cache.record(2824688072, None);
        cache.record(3788254489, Some(Info { title: None, preview: None }));
        assert_eq!(cache.items[&2824688072].title.as_deref(), Some("Calamity Mod"));
        assert_eq!(cache.items[&3788254489].title.as_deref(), Some("Custom Images"));
        assert!(cache.items[&3788254489].preview.is_some());
        cache.save().unwrap();
        assert!(!dir.join("workshop-cache.json.tmp").exists());

        // Relu ; une vignette modifiée à la main est écartée, un titre renettoyé.
        let text = fs::read_to_string(dir.join("workshop-cache.json")).unwrap();
        let text = text.replace("18354810480491550448", "../../x").replace("Custom Images", "Custom\\u0007 Images");
        fs::write(dir.join("workshop-cache.json"), text).unwrap();
        let mut reloaded = WorkshopCache::load(&dir);
        assert_eq!(reloaded.items.len(), 3);
        assert!(reloaded.items[&2824688072].preview.is_none());
        assert_eq!(reloaded.items[&3788254489].title.as_deref(), Some("Custom Images"));
        // Redemandé après 30 jours.
        reloaded.items.get_mut(&1).unwrap().checked = now() - REFRESH_AFTER - 1;
        assert_eq!(reloaded.to_fetch(&asked), vec![1]);
        // Ancienne version : tout est redemandé.
        reloaded.version = CACHE_VERSION + 1;
        reloaded.save().unwrap();
        assert!(WorkshopCache::load(&dir).items.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn lists_mods_with_folders_and_titles() {
        let root = std::env::temp_dir().join(format!("ludex-workshop-list-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let content = root.join("content").join("1281930");
        fs::create_dir_all(content.join("2824688072")).unwrap();
        // Taille absente du manifeste : dossier mesuré.
        fs::create_dir_all(content.join("77")).unwrap();
        fs::write(content.join("77").join("mod.dat"), [0u8; 300]).unwrap();
        let mut cache = WorkshopCache::load(&root);
        cache.record(2824688072, parse_details(DETAILS, &[2824688072]).unwrap().remove(&2824688072).unwrap());
        let mut installed = parse_acf(TMODLOADER, 1281930).unwrap();
        installed.push(Installed { id: 77, size: None, updated: None, pending: false });
        let mods = list(&content, &installed, &cache);
        assert!(mods.folder && mods.stale);
        assert_eq!(mods.mods.len(), 4);
        // Le plus récemment mis à jour d'abord, mod sans date à la fin.
        assert_eq!(mods.mods[0].id, "2824688072");
        assert_eq!(mods.mods[0].title.as_deref(), Some("Calamity Mod"));
        assert!(mods.mods[0].present && mods.mods[0].preview_url.is_some());
        assert!(!mods.mods[1].present && mods.mods[1].title.is_none());
        assert_eq!(mods.mods[3].id, "77");
        assert_eq!(mods.mods[3].size, 300);
        assert_eq!(mods.total, 404391973 + 696605416 + 46150 + 300);
        let _ = fs::remove_dir_all(&root);
    }

    /// Mods Workshop installés des jeux Steam de ce PC, puis titres demandés à l'API :
    /// `cargo test workshop_this_pc -- --ignored --nocapture`. Cache dans un dossier temporaire
    /// (jamais celui de l'app), supprimé à la fin.
    #[test]
    #[ignore]
    fn workshop_this_pc() {
        let started = std::time::Instant::now();
        let games: Vec<Game> = crate::library::scan().games.into_iter().filter(|g| appid(g).is_some()).collect();
        println!("{} jeux Steam relevés en {} ms", games.len(), started.elapsed().as_millis());
        let tmp = std::env::temp_dir().join(format!("ludex-workshop-pc-{}", std::process::id()));
        let mut cache = WorkshopCache::load(&tmp);
        let mut found = Vec::new();
        let t = std::time::Instant::now();
        for g in &games {
            if let Some((content, mods)) = installed(g)
                && !mods.is_empty()
            {
                found.push((g, content, mods));
            }
        }
        println!("Manifestes lus en {} ms : {} jeux avec des mods", t.elapsed().as_millis(), found.len());
        let ids: Vec<u64> = found.iter().flat_map(|(_, _, m)| m.iter().map(|m| m.id)).collect();
        let t = std::time::Instant::now();
        let mut requests = 0;
        for chunk in cache.to_fetch(&ids).chunks(BATCH) {
            if requests > 0 {
                std::thread::sleep(Duration::from_millis(300));
            }
            requests += 1;
            match request(chunk) {
                Ok(answers) => {
                    for (id, info) in answers {
                        cache.record(id, info);
                    }
                }
                Err(e) => {
                    println!("Erreur de l'API : {e}");
                    break;
                }
            }
        }
        println!("{} mods, {requests} requêtes en {} ms", ids.len(), t.elapsed().as_millis());
        for (g, content, mods) in &found {
            let t = std::time::Instant::now();
            let list = list(content, mods, &cache);
            let titled = list.mods.iter().filter(|m| m.title.is_some()).count();
            let previews = list.mods.iter().filter(|m| m.preview_url.is_some()).count();
            let pending = list.mods.iter().filter(|m| m.pending).count();
            let missing = list.mods.iter().filter(|m| !m.present).count();
            println!(
                "\n{} ({}) : {} mods, {:.1} Go, {titled} titres, {previews} vignettes, {pending} en attente, {missing} dossiers absents ({} ms)",
                g.title,
                g.platform_id,
                list.mods.len(),
                list.total as f64 / 1e9,
                t.elapsed().as_millis()
            );
            for m in list.mods.iter().take(3) {
                println!("  {} « {} » {:.1} Mo", m.id, m.title.as_deref().unwrap_or("?"), m.size as f64 / 1e6);
            }
        }
        cache.save().unwrap();
        let _ = fs::remove_dir_all(&tmp);
    }
}
