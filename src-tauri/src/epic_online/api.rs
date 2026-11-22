//! API d'Epic Games : connexion OAuth2 du client public du launcher, bibliothèque du compte,
//! fiches du catalogue. Aucune documentation officielle : formats relevés dans legendary et Heroic
//! (projets libres qui utilisent ce client) et vérifiés sur des réponses publiques du catalogue
//! enregistrées dans `samples/` (jeton d'application, aucun compte).
//!
//! Règles (comme GOG) : aucune erreur ne recopie une URL ni une réponse ; aucune redirection
//! suivie ; réponses lues avec une taille bornée ; seules les données utiles sont gardées.

use serde_json::Value;
use std::io::Read;
use std::time::Duration;

use crate::gog_online::vault::Session;

/// Client OAuth2 public du launcher Epic (`launcherAppClient2`). Epic ne propose aucune
/// inscription d'application pour lire sa propre bibliothèque : legendary, Heroic et Playnite
/// utilisent tous ce couple, présent dans leurs dépôts publics (ce « secret » n'en est pas un).
/// Si Epic le change, la connexion échoue proprement.
pub const CLIENT_ID: &str = "34a02cf8f4414e29b15921876da36f9a";
const CLIENT_SECRET: &str = "daafbccc737745039dffe53d94fc76cf";

/// Page d'Epic qui renvoie, une fois connecté, un JSON avec le code d'autorisation à usage unique.
pub const REDIRECT_HOST: &str = "www.epicgames.com";
pub const REDIRECT_PATH: &str = "/id/api/redirect";
/// Adresse fictive (domaine `.invalid` réservé : jamais résolu) par laquelle le script de la page
/// de redirection remet le code à Ludex ; la navigation est toujours annulée avant de partir.
pub const HANDOFF_HOST: &str = "ludex-epic.invalid";

/// Réponses lues au plus (JSON de l'API).
const MAX_BODY: u64 = 8 * 1024 * 1024;
/// Fiches demandées par requête (ids d'un même espace de noms).
pub const CATALOG_BATCH: usize = 50;
/// Jeux de la bibliothèque gardés au plus, pages lues au plus.
pub const MAX_RECORDS: usize = 20_000;
pub const MAX_PAGES: usize = 100;

/// Points d'entrée (Epic en vrai, serveur local dans les tests).
#[derive(Clone, Debug)]
pub struct Endpoints {
    pub account: String,
    pub library: String,
    pub catalog: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Endpoints {
            account: "https://account-public-service-prod03.ol.epicgames.com".into(),
            library: "https://library-service.live.use1a.on.epicgames.com".into(),
            catalog: "https://catalog-public-service-prod06.ol.epicgames.com".into(),
        }
    }
}

/// Page de connexion ouverte dans la fenêtre dédiée : connexion chez Epic, puis la page qui
/// donne le code d'autorisation du launcher.
pub fn login_url() -> String {
    let redirect = format!("https://{REDIRECT_HOST}{REDIRECT_PATH}?clientId={CLIENT_ID}&responseType=code");
    format!("https://{REDIRECT_HOST}/id/login?redirectUrl={}", crate::gog_online::api::encode(&redirect))
}

/// La page chargée est celle qui porte le code (https, hôte exact, chemin exact, sans port).
pub fn is_redirect_page(url: &url::Url) -> bool {
    url.scheme() == "https" && url.host_str() == Some(REDIRECT_HOST) && url.port().is_none() && url.path() == REDIRECT_PATH
}

/// Script lancé seulement sur la page de redirection (JSON affiché en texte) : il lit
/// `authorizationCode` et le remet à Ludex par une navigation vers `HANDOFF_HOST`, que Ludex
/// annule. Aucune autre donnée de la page n'en sort ; pas de code = `error=1`.
pub fn handoff_script() -> String {
    format!(
        "(()=>{{let c=null;try{{const j=JSON.parse(document.body.innerText);c=j&&j.authorizationCode}}catch(e){{}}\
location.replace('https://{HANDOFF_HOST}/code?'+(typeof c==='string'&&/^[A-Za-z0-9]{{8,128}}$/.test(c)?'code='+c:'error=1'))}})()"
    )
}

/// Ce que la navigation vers `HANDOFF_HOST` apporte.
#[derive(Debug, PartialEq, Eq)]
pub enum Handoff {
    Code(String),
    /// La page n'avait pas de code (session non ouverte, format changé).
    Missing,
}

/// Lit la remise du code : exactement `https://ludex-epic.invalid/code?code=…`, code de 8 à 128
/// lettres et chiffres. Toute autre adresse : `None` (navigation normale).
pub fn handoff(url: &url::Url) -> Option<Handoff> {
    if url.host_str() != Some(HANDOFF_HOST) {
        return None;
    }
    let code = (url.scheme() == "https" && url.path() == "/code" && url.port().is_none())
        .then(|| url.query_pairs().find(|(k, _)| k == "code").map(|(_, v)| v.into_owned()))
        .flatten()
        .filter(|c| (8..=128).contains(&c.len()) && c.bytes().all(|b| b.is_ascii_alphanumeric()));
    Some(code.map_or(Handoff::Missing, Handoff::Code))
}

/// Agent HTTP : 30 s, aucune redirection suivie, User-Agent honnête (accepté par Epic).
pub fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(30))
        .redirects(0)
        .user_agent(concat!("Ludex/", env!("CARGO_PKG_VERSION"), " (lanceur de jeux)"))
        .build()
}

/// Erreur d'une requête, sans l'URL ni le corps.
#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
    Unauthorized,
    Status(u16),
    Network(String),
}

impl Failure {
    pub fn message(&self) -> String {
        match self {
            Failure::Unauthorized => tr!("Epic a refusé la connexion : reconnecte ton compte.", "Epic refused the session: reconnect your account."),
            Failure::Status(code) => tr!("Réponse {} d'Epic.", "Response {} from Epic.", code),
            Failure::Network(kind) => tr!("Epic ne répond pas ({}).", "Epic isn't responding ({}).", kind),
        }
    }
}

fn failure(e: ureq::Error) -> Failure {
    match e {
        ureq::Error::Status(401, _) => Failure::Unauthorized,
        ureq::Error::Status(code, _) => Failure::Status(code),
        ureq::Error::Transport(t) => Failure::Network(format!("{:?}", t.kind())),
    }
}

fn body(response: ureq::Response) -> Result<String, Failure> {
    if response.status() != 200 {
        return Err(Failure::Status(response.status()));
    }
    let mut text = String::new();
    response.into_reader().take(MAX_BODY).read_to_string(&mut text).map_err(|_| Failure::Network(tr!("réponse illisible", "unreadable response")))?;
    Ok(text)
}

/// Base64 standard (en-tête `Authorization: Basic`).
fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ABC[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn basic_auth() -> String {
    format!("basic {}", base64(format!("{CLIENT_ID}:{CLIENT_SECRET}").as_bytes()))
}

fn bearer(session: &Session) -> String {
    format!("bearer {}", session.access_token)
}

// ---------------------------------------------------------------------------------------------
// Jetons
// ---------------------------------------------------------------------------------------------

pub enum Grant<'a> {
    Code(&'a str),
    Refresh(&'a str),
}

/// Identifiant de compte Epic : 32 hexadécimaux.
fn account_id_ok(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Lit la réponse de `oauth/token` : jetons non vides, compte en 32 hexadécimaux, durée bornée
/// (5 min à 1 jour, 2 h par défaut), pseudo (`displayName`) nettoyé.
pub fn parse_token(text: &str, now: u64, previous: Option<&Session>) -> Result<Session, Failure> {
    let unreadable = || Failure::Network(tr!("réponse de connexion illisible", "unreadable sign-in response"));
    let v: Value = serde_json::from_str(text).map_err(|_| unreadable())?;
    let token = |name: &str| v.get(name).and_then(Value::as_str).filter(|s| !s.is_empty() && s.len() <= 8192).map(String::from);
    let access_token = token("access_token").ok_or_else(unreadable)?;
    let refresh_token = token("refresh_token").or_else(|| previous.map(|p| p.refresh_token.clone())).ok_or_else(unreadable)?;
    let user_id = v
        .get("account_id")
        .and_then(Value::as_str)
        .map(|s| s.to_ascii_lowercase())
        .or_else(|| previous.map(|p| p.user_id.clone()))
        .filter(|id| account_id_ok(id))
        .ok_or_else(unreadable)?;
    let life = v.get("expires_in").and_then(Value::as_u64).unwrap_or(7200).clamp(300, 86_400);
    let username = v
        .get("displayName")
        .and_then(Value::as_str)
        .and_then(|n| crate::library::manual::clean_text(n, 40))
        .or_else(|| previous.and_then(|p| p.username.clone()));
    Ok(Session { access_token, refresh_token, expires_at: now + life, user_id, username })
}

/// Échange un code ou rafraîchit le jeton (`POST oauth/token`, client du launcher, jeton `eg1`).
pub fn token(agent: &ureq::Agent, ends: &Endpoints, grant: Grant, now: u64, previous: Option<&Session>) -> Result<Session, Failure> {
    let form: [(&str, &str); 3] = match grant {
        Grant::Code(code) => [("grant_type", "authorization_code"), ("code", code), ("token_type", "eg1")],
        Grant::Refresh(token) => [("grant_type", "refresh_token"), ("refresh_token", token), ("token_type", "eg1")],
    };
    let response = agent
        .post(&format!("{}/account/api/oauth/token", ends.account))
        .set("Authorization", &basic_auth())
        .send_form(&form)
        .map_err(|e| match failure(e) {
            // Code ou jeton de rafraîchissement refusés : 400 selon les cas.
            Failure::Status(400) => Failure::Unauthorized,
            other => other,
        })?;
    parse_token(&body(response)?, now, previous)
}

pub fn needs_refresh(session: &Session, now: u64) -> bool {
    session.expires_at <= now + 60
}

/// Ferme la session chez Epic (déconnexion) ; échec ignoré par l'appelant.
pub fn kill_session(agent: &ureq::Agent, ends: &Endpoints, session: &Session) -> Result<(), Failure> {
    agent
        .delete(&format!("{}/account/api/oauth/sessions/kill/{}", ends.account, crate::gog_online::api::encode(&session.access_token)))
        .set("Authorization", &bearer(session))
        .call()
        .map(|_| ())
        .map_err(failure)
}

// ---------------------------------------------------------------------------------------------
// Bibliothèque
// ---------------------------------------------------------------------------------------------

/// Identifiant d'Epic utilisable dans une URI : lettres, chiffres, `.`, `_`, `-`, 1 à 128.
pub fn id_ok(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Une entrée de la bibliothèque (ce qui est gardé).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    /// Nom d'application d'Epic (`AppName` des manifestes : id du jeu dans Ludex).
    pub app_name: String,
    pub namespace: String,
    pub catalog_item_id: String,
}

/// Une page de `library/api/public/items` : entrées gardées (sans doublon) et curseur suivant. Écartées : espace
/// `ue` (contenus de l'Unreal Engine), bacs à sable privés, entrées sans `appName`, ids invalides.
pub fn parse_library(text: &str) -> Result<(Vec<Record>, Option<String>), Failure> {
    let unreadable = || Failure::Network(tr!("bibliothèque illisible", "unreadable library"));
    let v: Value = serde_json::from_str(text).map_err(|_| unreadable())?;
    let records = v.get("records").and_then(Value::as_array).ok_or_else(unreadable)?;
    let s = |r: &Value, k: &str| r.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let mut seen = std::collections::HashSet::new();
    let list = records
        .iter()
        .filter(|r| r.get("sandboxType").and_then(Value::as_str) != Some("PRIVATE"))
        .map(|r| Record { app_name: s(r, "appName"), namespace: s(r, "namespace"), catalog_item_id: s(r, "catalogItemId") })
        .filter(|r| r.namespace != "ue" && id_ok(&r.app_name) && id_ok(&r.namespace) && id_ok(&r.catalog_item_id))
        .filter(|r| seen.insert(r.app_name.clone()))
        .collect();
    let next = v
        .pointer("/responseMetadata/nextCursor")
        .and_then(Value::as_str)
        .filter(|c| !c.is_empty() && c.len() <= 512 && c.bytes().all(|b| b.is_ascii_graphic()))
        .map(String::from);
    Ok((list, next))
}

/// Toute la bibliothèque, page après page, sans doublon d'`appName`.
pub fn library(agent: &ureq::Agent, ends: &Endpoints, session: &Session) -> Result<Vec<Record>, Failure> {
    let mut all: Vec<Record> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let mut url = format!("{}/library/api/public/items?includeMetadata=true", ends.library);
        if let Some(c) = &cursor {
            url.push_str(&format!("&cursor={}", crate::gog_online::api::encode(c)));
        }
        let response = agent.get(&url).set("Authorization", &bearer(session)).call().map_err(failure)?;
        let (page, next) = parse_library(&body(response)?)?;
        all.extend(page.into_iter().filter(|r| seen.insert(r.app_name.clone())));
        if all.len() >= MAX_RECORDS {
            all.truncate(MAX_RECORDS);
            break;
        }
        match next {
            Some(n) if Some(&n) != cursor.as_ref() => cursor = Some(n),
            _ => break,
        }
    }
    Ok(all)
}

/// Fiche du catalogue, telle que gardée dans le cache.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub title: String,
    /// Jeu jouable seul (pas un DLC, un contenu additionnel, un bonus ni un module).
    pub game: bool,
    pub windows: bool,
    /// Chemin de l'image large (16:9) sur le CDN d'Epic, seule partie gardée : l'adresse est
    /// reconstruite par `image_url`.
    #[serde(default)]
    pub image: Option<Image>,
}

/// Hôtes d'images d'Epic acceptés (liste blanche stricte, aussi dans `img-src`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageHost {
    /// `cdn1.epicgames.com`
    Epic,
    /// `cdn2.unrealengine.com`
    Unreal,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Image {
    pub host: ImageHost,
    pub path: String,
}

/// Chemin d'image sûr : segments de lettres, chiffres, `.`, `_`, `-` (sans `..`), 400
/// caractères au plus.
fn image_path_ok(path: &str) -> bool {
    path.starts_with('/')
        && path.len() <= 400
        && path[1..].split('/').all(|seg| !seg.is_empty() && seg != "." && seg != ".." && seg.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-')))
}

/// Image d'une fiche : `https://cdn1.epicgames.com/…` ou `https://cdn2.unrealengine.com/…`, sans
/// port, identifiants, requête ni fragment. Tout le reste : `None`.
pub fn parse_image(raw: &str) -> Option<Image> {
    let url = url::Url::parse(raw).ok()?;
    if url.scheme() != "https" || url.port().is_some() || !url.username().is_empty() || url.password().is_some() || url.query().is_some() || url.fragment().is_some() {
        return None;
    }
    let host = match url.host_str()? {
        "cdn1.epicgames.com" => ImageHost::Epic,
        "cdn2.unrealengine.com" => ImageHost::Unreal,
        _ => return None,
    };
    image_path_ok(url.path()).then(|| Image { host, path: url.path().to_string() })
}

/// Vignette 16:9 reconstruite par Ludex (réduite par le CDN d'Epic à 432 × 243, ~25 Ko).
pub fn image_url(image: &Image) -> Option<String> {
    if !image_path_ok(&image.path) {
        return None;
    }
    let host = match image.host {
        ImageHost::Epic => "cdn1.epicgames.com",
        ImageHost::Unreal => "cdn2.unrealengine.com",
    };
    Some(format!("https://{host}{}?h=243&quality=medium&resize=1&w=432", image.path))
}

/// Une fiche de `bulk/items`. Jeu = catégorie `games`, sans jeu principal (`mainGameItem`) ni
/// catégorie `addons`, `digitalextras`, `plugins` ; Windows = `releaseInfo[].platform`.
pub fn parse_item(v: &Value) -> Option<Item> {
    let title = crate::library::manual::clean_text(v.get("title")?.as_str()?, 120)?;
    let categories: Vec<&str> = v.get("categories").and_then(Value::as_array).map(|a| a.iter().filter_map(|c| c.get("path").and_then(Value::as_str)).collect()).unwrap_or_default();
    let extra = categories.iter().any(|c| matches!(*c, "addons" | "digitalextras" | "plugins" | "engines"));
    let game = categories.contains(&"games") && !extra && v.get("mainGameItem").is_none();
    let windows = v
        .get("releaseInfo")
        .and_then(Value::as_array)
        .is_some_and(|r| r.iter().any(|i| i.get("platform").and_then(Value::as_array).is_some_and(|p| p.iter().any(|x| x.as_str() == Some("Windows")))));
    let images: Vec<(&str, &str)> = v
        .get("keyImages")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|i| Some((i.get("type")?.as_str()?, i.get("url")?.as_str()?))).collect())
        .unwrap_or_default();
    let image = ["DieselGameBox", "OfferImageWide", "DieselStoreFrontWide", "Thumbnail"]
        .iter()
        .find_map(|kind| images.iter().filter(|(t, _)| t == kind).find_map(|(_, u)| parse_image(u)));
    Some(Item { title, game, windows, image })
}

/// Réponse de `bulk/items` : `{ <catalogItemId>: fiche }`, seulement les ids demandés.
pub fn parse_catalog(text: &str, asked: &[String]) -> Result<Vec<(String, Item)>, Failure> {
    let v: Value = serde_json::from_str(text).map_err(|_| Failure::Network(tr!("fiches illisibles", "unreadable product details")))?;
    let map = v.as_object().ok_or_else(|| Failure::Network(tr!("fiches illisibles", "unreadable product details")))?;
    Ok(map.iter().filter(|(id, _)| asked.contains(id)).filter_map(|(id, item)| parse_item(item).map(|i| (id.clone(), i))).collect())
}

/// Fiches de 50 ids au plus d'un même espace de noms.
pub fn catalog(agent: &ureq::Agent, ends: &Endpoints, session: &Session, namespace: &str, ids: &[String]) -> Result<Vec<(String, Item)>, Failure> {
    if !id_ok(namespace) || ids.iter().any(|i| !id_ok(i)) {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = ids.iter().take(CATALOG_BATCH).cloned().collect();
    let query: Vec<String> = ids.iter().map(|i| format!("id={i}")).collect();
    let url = format!(
        "{}/catalog/api/shared/namespace/{namespace}/bulk/items?{}&includeDLCDetails=false&includeMainGameDetails=false&country=US&locale=en",
        ends.catalog,
        query.join("&")
    );
    let response = agent.get(&url).set("Authorization", &bearer(session)).call().map_err(failure)?;
    parse_catalog(&body(response)?, &ids)
}

/// URI d'installation par le launcher Epic, reconstruite depuis une entrée de la bibliothèque
/// (même forme que le lancement des jeux Epic de Ludex, `action=install`). Ids invalides : `None`.
pub fn install_uri(record: &Record) -> Option<String> {
    (id_ok(&record.namespace) && id_ok(&record.catalog_item_id) && id_ok(&record.app_name))
        .then(|| format!("com.epicgames.launcher://apps/{}%3A{}%3A{}?action=install", record.namespace, record.catalog_item_id, record.app_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        // En-tête du client du launcher, tel que legendary l'envoie.
        assert_eq!(basic_auth(), "basic MzRhMDJjZjhmNDQxNGUyOWIxNTkyMTg3NmRhMzZmOWE6ZGFhZmJjY2M3Mzc3NDUwMzlkZmZlNTNkOTRmYzc2Y2Y=");
    }

    #[test]
    fn login_and_handoff_addresses() {
        let login = url::Url::parse(&login_url()).unwrap();
        assert_eq!(login.host_str(), Some("www.epicgames.com"));
        let redirect = login.query_pairs().find(|(k, _)| k == "redirectUrl").unwrap().1.into_owned();
        assert!(is_redirect_page(&url::Url::parse(&redirect).unwrap()));
        assert!(redirect.contains(CLIENT_ID));
        for bad in ["http://www.epicgames.com/id/api/redirect", "https://www.epicgames.com:444/id/api/redirect", "https://epicgames.com/id/api/redirect", "https://www.epicgames.com/id/api/redirect/x"] {
            assert!(!is_redirect_page(&url::Url::parse(bad).unwrap()), "{bad}");
        }
        let h = |s: &str| handoff(&url::Url::parse(s).unwrap());
        assert_eq!(h("https://ludex-epic.invalid/code?code=0123456789abcdef0123456789abcdef"), Some(Handoff::Code("0123456789abcdef0123456789abcdef".into())));
        assert_eq!(h("https://ludex-epic.invalid/code?error=1"), Some(Handoff::Missing));
        assert_eq!(h("https://ludex-epic.invalid/code?code=abc%27%3B"), Some(Handoff::Missing));
        assert_eq!(h("https://ludex-epic.invalid/autre?code=0123456789abcdef"), Some(Handoff::Missing));
        assert_eq!(h("http://ludex-epic.invalid/code?code=0123456789abcdef"), Some(Handoff::Missing));
        assert_eq!(h("https://www.epicgames.com/code?code=0123456789abcdef"), None);
        let script = handoff_script();
        assert!(script.contains("https://ludex-epic.invalid/code?"));
        assert!(script.contains("authorizationCode"));
    }

    #[test]
    fn token_response_is_checked() {
        let text = r#"{"access_token":"eg1~faux-jeton","expires_in":28800,"refresh_token":"eg1~faux-rafraichissement","account_id":"0123456789ABCDEF0123456789abcdef","displayName":"Joueur\u0007 Essai"}"#;
        let s = parse_token(text, 100, None).unwrap();
        assert_eq!(s.user_id, "0123456789abcdef0123456789abcdef");
        assert_eq!(s.expires_at, 100 + 28_800);
        assert_eq!(s.username.as_deref(), Some("Joueur Essai"));
        // Rafraîchissement sans nouveau jeton de rafraîchissement ni compte : les anciens servent.
        let again = parse_token(r#"{"access_token":"eg1~nouveau","expires_in":99999999}"#, 0, Some(&s)).unwrap();
        assert_eq!(again.refresh_token, "eg1~faux-rafraichissement");
        assert_eq!(again.expires_at, 86_400);
        assert_eq!(again.username.as_deref(), Some("Joueur Essai"));
        assert!(parse_token(r#"{"access_token":"x","refresh_token":"y","account_id":"pas-un-compte"}"#, 0, None).is_err());
        assert!(parse_token("pas du json", 0, None).is_err());
    }

    #[test]
    fn library_skips_engine_private_and_bad_ids() {
        let (list, next) = parse_library(include_str!("samples/library_page1.json")).unwrap();
        let names: Vec<&str> = list.iter().map(|r| r.app_name.as_str()).collect();
        assert_eq!(names, ["Sugar", "Fortnite", "9d2d0eb64d5c44529cece33fe2a46482"]);
        assert_eq!(next.as_deref(), Some("curseur-page-2"));
        let (list, next) = parse_library(include_str!("samples/library_page2.json")).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(next, None);
        assert!(parse_library("{}").is_err());
    }

    #[test]
    fn catalog_reads_real_responses() {
        let rl = parse_catalog(include_str!("samples/catalog_rocket_league.json"), &["530145df28a24424923f5828cc9031a1".into(), "8eeee1bbc27442fd914a0b97b940869d".into()]).unwrap();
        let game = rl.iter().find(|(id, _)| id == "530145df28a24424923f5828cc9031a1").unwrap();
        assert_eq!(game.1.title, "Rocket League®");
        assert!(game.1.game && game.1.windows);
        let url = image_url(game.1.image.as_ref().unwrap()).unwrap();
        assert!(url.starts_with("https://cdn1.epicgames.com/item/9773aa1aa54f4f7b80e44bef04986cea/Rocket_Wide_2560x1440-"));
        assert!(url.ends_with("?h=243&quality=medium&resize=1&w=432"));
        let dlc = rl.iter().find(|(id, _)| id == "8eeee1bbc27442fd914a0b97b940869d").unwrap();
        assert!(!dlc.1.game, "DLC jamais proposé seul");
        // Id non demandé : ignoré.
        assert!(parse_catalog(include_str!("samples/catalog_rocket_league.json"), &["530145df28a24424923f5828cc9031a1".into()]).unwrap().len() == 1);
        let fortnite = parse_catalog(include_str!("samples/catalog_fortnite.json"), &["4fe75bbc5a674f4f9b356b5c90567da5".into()]).unwrap();
        assert!(fortnite[0].1.game && fortnite[0].1.windows);
        assert!(fortnite[0].1.image.is_some());
    }

    #[test]
    fn images_outside_epic_cdn_are_refused() {
        for bad in [
            "https://evil.example.com/item/x.png",
            "http://cdn1.epicgames.com/item/x.png",
            "https://cdn1.epicgames.com/item/x.png?track=1",
            "https://cdn1.epicgames.com:8443/item/x.png",
            "https://user@cdn1.epicgames.com/item/x.png",
            "https://cdn1.epicgames.com/item/x\"y.png",
            "https://cdn1.epicgames.com.evil.com/x.png",
        ] {
            assert_eq!(parse_image(bad), None, "{bad}");
        }
        assert!(parse_image("https://cdn2.unrealengine.com/egs-hades-supergiantgames-s1-2560x1440-123.jpg").is_some());
        assert_eq!(image_url(&Image { host: ImageHost::Epic, path: "/../x".into() }), None);
    }

    #[test]
    fn install_uri_is_rebuilt_from_ids() {
        let r = Record { app_name: "Sugar".into(), namespace: "9773aa1aa54f4f7b80e44bef04986cea".into(), catalog_item_id: "530145df28a24424923f5828cc9031a1".into() };
        assert_eq!(install_uri(&r).unwrap(), "com.epicgames.launcher://apps/9773aa1aa54f4f7b80e44bef04986cea%3A530145df28a24424923f5828cc9031a1%3ASugar?action=install");
        let bad = Record { app_name: "Sugar?action=uninstall&x".into(), ..r };
        assert_eq!(install_uri(&bad), None);
    }
}
