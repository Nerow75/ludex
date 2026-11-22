//! API de GOG : connexion OAuth2 du client Galaxy, jeux possédés, fiches produits, liens des
//! installeurs hors ligne. Aucune documentation officielle : formats relevés par la communauté
//! (gogapidocs, heroic-gogdl) et vérifiés sur des réponses publiques enregistrées dans `samples/`.
//!
//! Règles : aucune erreur ne recopie une URL ni une réponse (le code de connexion, les jetons et
//! les liens signés du CDN sont dans les URL) ; redirections jamais suivies sans vérification ;
//! réponses lues avec une taille bornée.

use serde_json::Value;
use std::io::Read;
use std::time::Duration;

use super::vault::Session;

/// Client OAuth2 public de GOG Galaxy. GOG ne propose aucune inscription d'application : Heroic,
/// gogdl, Lutris et Playnite utilisent tous ce couple, présent dans leurs dépôts publics (ce
/// « secret » n'en est pas un). Si GOG le change, la connexion échoue proprement.
pub const CLIENT_ID: &str = "46899977096215655";
const CLIENT_SECRET: &str = "9d85c43b1482497dbbce61f6e4aa173a433796eeae2ca8c5f6129f2dc4de46d9";
/// Page de redirection fixe de GOG, d'où Ludex lit le code (jamais le mot de passe).
pub const REDIRECT_URI: &str = "https://embed.gog.com/on_login_success?origin=client";

/// Réponses lues au plus (JSON de l'API).
const MAX_BODY: u64 = 4 * 1024 * 1024;
/// Produits demandés par requête (limite de `api.gog.com/products`).
pub const PRODUCTS_BATCH: usize = 50;
/// Jeux possédés gardés au plus.
pub const MAX_OWNED: usize = 20_000;

/// Points d'entrée (GOG en vrai, serveur local dans les tests).
#[derive(Clone, Debug)]
pub struct Endpoints {
    pub auth: String,
    pub embed: String,
    pub api: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Endpoints { auth: "https://auth.gog.com".into(), embed: "https://embed.gog.com".into(), api: "https://api.gog.com".into() }
    }
}

/// Page de connexion ouverte dans la fenêtre dédiée.
pub fn login_url() -> String {
    format!("https://auth.gog.com/auth?client_id={CLIENT_ID}&redirect_uri={}&response_type=code&layout=client2", encode(REDIRECT_URI))
}

/// Encodage d'un composant d'URL : tout sauf `A-Za-z0-9-_.~` en `%XX`.
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hex = s.get(i + 1..i + 3)?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// Code de connexion lu dans l'URL de redirection de GOG : exactement
/// `https://embed.gog.com/on_login_success?…code=…`, code de 1 à 512 caractères sûrs. Toute autre
/// adresse (autre hôte, `http`, port, identifiants, autre chemin) : `None`.
pub fn code_from_redirect(url: &url::Url) -> Option<String> {
    if url.scheme() != "https" || url.host_str() != Some("embed.gog.com") || url.port().is_some() || !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    if url.path() != "/on_login_success" {
        return None;
    }
    let code = url.query_pairs().find(|(k, _)| k == "code").map(|(_, v)| v.into_owned())?;
    let ok = !code.is_empty() && code.len() <= 512 && code.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~'));
    ok.then_some(code)
}

/// Navigation permise dans la fenêtre de connexion : `https` seulement (pages de GOG, CAPTCHA,
/// connexion par un tiers proposée par GOG) ; `about:blank` toléré. Jamais `http`, `file`,
/// `javascript`, ni un schéma d'application.
pub fn navigation_allowed(url: &url::Url) -> bool {
    url.scheme() == "https" || url.as_str() == "about:blank"
}

/// Agent HTTP : 20 s, aucune redirection suivie seule, User-Agent honnête.
pub fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(30))
        .redirects(0)
        .user_agent(concat!("Ludex/", env!("CARGO_PKG_VERSION"), " (lanceur de jeux)"))
        .build()
}

/// Erreur d'une requête, sans l'URL ni le corps (ils peuvent porter un code ou un jeton).
#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
    /// Jeton refusé (401) : rafraîchir, ou se reconnecter.
    Unauthorized,
    /// Autre statut HTTP.
    Status(u16),
    /// Réseau, délai, réponse illisible.
    Network(String),
}

impl Failure {
    pub fn message(&self) -> String {
        match self {
            Failure::Unauthorized => tr!("GOG a refusé la connexion : reconnecte ton compte.", "GOG refused the session: reconnect your account."),
            Failure::Status(code) => tr!("Réponse {} de GOG.", "Response {} from GOG.", code),
            Failure::Network(kind) => tr!("GOG ne répond pas ({}).", "GOG isn't responding ({}).", kind),
        }
    }
}

/// Transforme une erreur de `ureq` sans jamais recopier son texte (qui contient l'URL).
pub fn failure(e: ureq::Error) -> Failure {
    match e {
        ureq::Error::Status(401, _) => Failure::Unauthorized,
        ureq::Error::Status(code, _) => Failure::Status(code),
        ureq::Error::Transport(t) => Failure::Network(format!("{:?}", t.kind())),
    }
}

/// Corps texte d'une réponse 200, borné.
fn body(response: ureq::Response) -> Result<String, Failure> {
    if response.status() != 200 {
        return Err(Failure::Status(response.status()));
    }
    let mut text = String::new();
    response.into_reader().take(MAX_BODY).read_to_string(&mut text).map_err(|_| Failure::Network(tr!("réponse illisible", "unreadable response")))?;
    Ok(text)
}

// ---------------------------------------------------------------------------------------------
// Jetons
// ---------------------------------------------------------------------------------------------

/// Demande de jeton : premier échange du code, ou rafraîchissement.
pub enum Grant<'a> {
    Code(&'a str),
    Refresh(&'a str),
}

/// Adresse de `auth.gog.com/token` (GOG attend les paramètres dans la requête, en GET).
pub fn token_url(base: &str, grant: &Grant) -> String {
    let common = format!("{base}/token?client_id={CLIENT_ID}&client_secret={CLIENT_SECRET}");
    match grant {
        Grant::Code(code) => format!("{common}&grant_type=authorization_code&code={}&redirect_uri={}", encode(code), encode(REDIRECT_URI)),
        Grant::Refresh(token) => format!("{common}&grant_type=refresh_token&refresh_token={}", encode(token)),
    }
}

/// Lit la réponse de `token` : jetons non vides, durée bornée (5 min à 1 jour, 1 h par défaut).
/// `previous` : pseudo déjà connu, gardé au rafraîchissement.
pub fn parse_token(text: &str, now: u64, previous: Option<&Session>) -> Result<Session, Failure> {
    let unreadable = || Failure::Network(tr!("réponse de connexion illisible", "unreadable sign-in response"));
    let v: Value = serde_json::from_str(text).map_err(|_| unreadable())?;
    let token = |name: &str| v.get(name).and_then(Value::as_str).filter(|s| !s.is_empty() && s.len() <= 8192).map(String::from);
    let access_token = token("access_token").ok_or_else(unreadable)?;
    // Un rafraîchissement peut ne pas renvoyer de nouveau jeton de rafraîchissement : l'ancien sert.
    let refresh_token = token("refresh_token").or_else(|| previous.map(|p| p.refresh_token.clone())).ok_or_else(unreadable)?;
    let user_id = v
        .get("user_id")
        .and_then(|u| u.as_str().map(String::from).or_else(|| u.as_u64().map(|n| n.to_string())))
        .or_else(|| previous.map(|p| p.user_id.clone()))
        .filter(|u| !u.is_empty() && u.len() <= 32 && u.bytes().all(|b| b.is_ascii_digit()))
        .ok_or_else(unreadable)?;
    let life = v.get("expires_in").and_then(Value::as_u64).unwrap_or(3600).clamp(300, 86_400);
    Ok(Session { access_token, refresh_token, expires_at: now + life, user_id, username: previous.and_then(|p| p.username.clone()) })
}

/// Échange un code ou rafraîchit le jeton.
pub fn token(agent: &ureq::Agent, ends: &Endpoints, grant: Grant, now: u64, previous: Option<&Session>) -> Result<Session, Failure> {
    let response = agent.get(&token_url(&ends.auth, &grant)).call().map_err(|e| match failure(e) {
        // Code ou jeton de rafraîchissement refusés : 400 ou 401 selon les cas.
        Failure::Status(400) => Failure::Unauthorized,
        other => other,
    })?;
    parse_token(&body(response)?, now, previous)
}

/// Le jeton d'accès expire dans moins d'une minute.
pub fn needs_refresh(session: &Session, now: u64) -> bool {
    session.expires_at <= now + 60
}

/// Pseudo GOG (`userData.json`), nettoyé : 1 à 40 caractères imprimables.
pub fn parse_username(text: &str) -> Option<String> {
    let v: Value = serde_json::from_str(text).ok()?;
    let name = v.get("username")?.as_str()?;
    let clean: String = name.chars().filter(|c| !c.is_control()).take(40).collect();
    let clean = clean.trim().to_string();
    (!clean.is_empty()).then_some(clean)
}

pub fn username(agent: &ureq::Agent, ends: &Endpoints, session: &Session) -> Result<Option<String>, Failure> {
    let response = agent.get(&format!("{}/userData.json", ends.embed)).set("Authorization", &format!("Bearer {}", session.access_token)).call().map_err(failure)?;
    Ok(parse_username(&body(response)?))
}

// ---------------------------------------------------------------------------------------------
// Bibliothèque
// ---------------------------------------------------------------------------------------------

/// Ids possédés (`user/data/games` : `{ "owned": [ids] }`), non nuls, sans doublon, dans l'ordre.
pub fn parse_owned(text: &str) -> Result<Vec<u64>, Failure> {
    let v: Value = serde_json::from_str(text).map_err(|_| Failure::Network(tr!("liste des jeux illisible", "unreadable game list")))?;
    let list = v.get("owned").and_then(Value::as_array).ok_or_else(|| Failure::Network(tr!("liste des jeux illisible", "unreadable game list")))?;
    let mut seen = std::collections::HashSet::new();
    Ok(list.iter().filter_map(Value::as_u64).filter(|&id| id > 0 && seen.insert(id)).take(MAX_OWNED).collect())
}

pub fn owned(agent: &ureq::Agent, ends: &Endpoints, session: &Session) -> Result<Vec<u64>, Failure> {
    let response = agent.get(&format!("{}/user/data/games", ends.embed)).set("Authorization", &format!("Bearer {}", session.access_token)).call().map_err(failure)?;
    parse_owned(&body(response)?)
}

/// Sorte de produit : seuls les jeux s'installent seuls (DLC et lots à part).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Game,
    Dlc,
    Pack,
    Other,
}

/// Fiche d'un produit, telle que gardée dans le cache.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Product {
    pub id: u64,
    pub title: String,
    pub kind: Kind,
    pub windows: bool,
    pub installable: bool,
    /// Empreinte de l'image « logo » de GOG (64 hexadécimaux), seule partie gardée : l'adresse est
    /// reconstruite par `image_url` sur `images.gog-statics.com`.
    #[serde(default)]
    pub image: Option<String>,
}

/// Empreinte d'une image de `*.gog-statics.com` : `//images-N.gog-statics.com/<64 hex>[_suffixe].<ext>`.
/// Tout autre hôte ou forme : `None`.
pub fn image_hash(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https:").unwrap_or(url).strip_prefix("//")?;
    let (host, path) = rest.split_once('/')?;
    let host_ok = host == "images.gog-statics.com"
        || host.strip_prefix("images-").and_then(|h| h.strip_suffix(".gog-statics.com")).is_some_and(|n| !n.is_empty() && n.len() <= 2 && n.bytes().all(|b| b.is_ascii_digit()));
    if !host_ok || path.contains('?') || path.contains('/') {
        return None;
    }
    let hash = path.get(..64)?;
    (hash.bytes().all(|b| b.is_ascii_hexdigit()) && path[64..].starts_with(['_', '.'])).then(|| hash.to_ascii_lowercase())
}

/// Vignette 16:9 du jeu (432 × 243, ~30 Ko), reconstruite par Ludex sur un seul hôte (`img-src`).
pub fn image_url(hash: &str) -> Option<String> {
    (hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())).then(|| format!("https://images.gog-statics.com/{hash}_product_tile_extended_432x243.webp"))
}

/// Titre nettoyé : contrôles et invisibles retirés, espaces regroupés, 120 caractères.
fn clean_title(s: &str) -> Option<String> {
    crate::library::manual::clean_text(s, 120)
}

/// Une fiche de `api.gog.com/products` ; produit sans id ou sans titre : `None`.
pub fn parse_product(v: &Value) -> Option<Product> {
    let id = v.get("id")?.as_u64().filter(|&id| id > 0)?;
    let title = clean_title(v.get("title")?.as_str()?)?;
    let kind = match v.get("game_type").and_then(Value::as_str) {
        Some("game") => Kind::Game,
        Some("dlc") => Kind::Dlc,
        Some("pack") => Kind::Pack,
        _ => Kind::Other,
    };
    let windows = v.pointer("/content_system_compatibility/windows").and_then(Value::as_bool).unwrap_or(false);
    let installable = v.get("is_installable").and_then(Value::as_bool).unwrap_or(false);
    let image = ["/images/logo2x", "/images/logo"].iter().find_map(|p| v.pointer(p).and_then(Value::as_str).and_then(image_hash));
    Some(Product { id, title, kind, windows, installable, image })
}

pub fn parse_products(text: &str) -> Result<Vec<Product>, Failure> {
    let v: Value = serde_json::from_str(text).map_err(|_| Failure::Network(tr!("fiches illisibles", "unreadable product details")))?;
    let list = v.as_array().ok_or_else(|| Failure::Network(tr!("fiches illisibles", "unreadable product details")))?;
    Ok(list.iter().filter_map(parse_product).collect())
}

/// Fiches de 50 produits au plus (requête publique, sans jeton).
pub fn products(agent: &ureq::Agent, ends: &Endpoints, ids: &[u64]) -> Result<Vec<Product>, Failure> {
    let list: Vec<String> = ids.iter().take(PRODUCTS_BATCH).map(u64::to_string).collect();
    let response = agent.get(&format!("{}/products?ids={}&locale=en-US", ends.api, list.join(","))).call().map_err(failure)?;
    parse_products(&body(response)?)
}

// ---------------------------------------------------------------------------------------------
// Installeurs hors ligne
// ---------------------------------------------------------------------------------------------

/// Une partie de l'installeur (le `.exe`, puis les `.bin`), taille annoncée par GOG.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    /// Identifiant de la partie (`en1installer0`), validé.
    pub id: String,
    pub size: u64,
}

/// Installeur Windows retenu pour un jeu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installer {
    pub product: u64,
    pub title: String,
    pub language: String,
    pub version: String,
    pub parts: Vec<Part>,
}

impl Installer {
    pub fn total(&self) -> u64 {
        self.parts.iter().map(|p| p.size).sum()
    }
}

/// Plus gros installeur accepté (200 Go), plus grand nombre de parties.
const MAX_TOTAL: u64 = 200 * 1024 * 1024 * 1024;
const MAX_PARTS: usize = 64;

fn part_id_ok(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Installeur Windows de `products/<id>?expand=downloads` : langue de Ludex d'abord, puis anglais,
/// puis le premier. Chaque partie doit avoir une taille et un lien `api.gog.com/products/<id>/downlink/installer/<partie>`
/// (vérifié : Ludex reconstruit ce lien lui-même et n'en suit jamais un autre).
pub fn parse_installer(text: &str, product: u64, language: &str) -> Result<Option<Installer>, Failure> {
    let v: Value = serde_json::from_str(text).map_err(|_| Failure::Network(tr!("fiche illisible", "unreadable product details")))?;
    if v.get("id").and_then(Value::as_u64) != Some(product) {
        return Err(Failure::Network(tr!("fiche d'un autre jeu", "details of another game")));
    }
    let title = v.get("title").and_then(Value::as_str).and_then(clean_title).unwrap_or_else(|| product.to_string());
    let installers: Vec<&Value> = v
        .pointer("/downloads/installers")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter(|i| i.get("os").and_then(Value::as_str) == Some("windows")).collect())
        .unwrap_or_default();
    let lang_of = |i: &Value| i.get("language").and_then(Value::as_str).unwrap_or("").to_string();
    let chosen = installers.iter().find(|i| lang_of(i) == language).or_else(|| installers.iter().find(|i| lang_of(i) == "en")).or_else(|| installers.first());
    let Some(chosen) = chosen else { return Ok(None) };
    let mut parts = Vec::new();
    for file in chosen.get("files").and_then(Value::as_array).into_iter().flatten() {
        let id = file.get("id").and_then(Value::as_str).unwrap_or("");
        let size = file.get("size").and_then(Value::as_u64).unwrap_or(0);
        let expected = format!("https://api.gog.com/products/{product}/downlink/installer/{id}");
        if !part_id_ok(id) || size == 0 || file.get("downlink").and_then(Value::as_str) != Some(expected.as_str()) {
            return Err(Failure::Network(tr!("liste des fichiers de l'installeur inattendue", "unexpected installer file list")));
        }
        parts.push(Part { id: id.to_string(), size });
    }
    let installer = Installer {
        product,
        title,
        language: lang_of(chosen),
        version: chosen.get("version").and_then(Value::as_str).and_then(|s| crate::library::manual::clean_text(s, 60)).unwrap_or_default(),
        parts,
    };
    if installer.parts.is_empty() || installer.parts.len() > MAX_PARTS || installer.total() > MAX_TOTAL {
        return Err(Failure::Network(tr!("installeur inattendu", "unexpected installer")));
    }
    Ok(Some(installer))
}

pub fn installer(agent: &ureq::Agent, ends: &Endpoints, product: u64, language: &str) -> Result<Option<Installer>, Failure> {
    let response = agent.get(&format!("{}/products/{product}?expand=downloads&locale=en-US", ends.api)).call().map_err(failure)?;
    parse_installer(&body(response)?, product, language)
}

/// Hôte permis pour télécharger un installeur : `api.gog.com` et les CDN de GOG (`*.gog.com`),
/// en `https` sur le port par défaut, sans identifiants. Rien d'autre, même par redirection.
pub fn download_allowed(url: &url::Url) -> bool {
    if url.scheme() != "https" || url.port().is_some() || !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    let Some(url::Host::Domain(host)) = url.host() else { return false };
    let host = host.to_ascii_lowercase();
    host == "gog.com" || host.strip_suffix(".gog.com").is_some_and(|sub| !sub.is_empty() && sub.split('.').all(|l| !l.is_empty() && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')))
}

/// Réponse de `downlink` (`{ "downlink": "<lien signé du CDN>", … }`) : lien vérifié par `download_allowed`.
pub fn parse_downlink(text: &str) -> Result<url::Url, Failure> {
    let v: Value = serde_json::from_str(text).map_err(|_| Failure::Network(tr!("lien de téléchargement illisible", "unreadable download link")))?;
    let link = v.get("downlink").and_then(Value::as_str).ok_or_else(|| Failure::Network(tr!("lien de téléchargement absent", "download link missing")))?;
    let url = url::Url::parse(link).map_err(|_| Failure::Network(tr!("lien de téléchargement illisible", "unreadable download link")))?;
    if !download_allowed(&url) {
        return Err(Failure::Network(tr!("lien de téléchargement hors de GOG refusé", "download link outside GOG refused")));
    }
    Ok(url)
}

/// Nom du fichier sur le CDN (dernier segment du chemin, décodé) : lettres, chiffres, espaces,
/// `._-()+,'&!` seulement, sans `..`, terminé par `.exe` (première partie) ou `.bin` (les autres) ;
/// Inno Setup retrouve ses `.bin` par leur nom, à côté de l'exe.
pub fn file_name(url: &url::Url, first: bool) -> Option<String> {
    let last = url.path_segments()?.next_back()?;
    let name = decode(last)?;
    let safe = !name.is_empty()
        && name.len() <= 200
        && !name.contains("..")
        && !name.starts_with([' ', '.'])
        && name.chars().all(|c| c.is_ascii_alphanumeric() || " ._-()+,'&!".contains(c));
    let lower = name.to_ascii_lowercase();
    let ext_ok = if first { lower.ends_with(".exe") } else { lower.ends_with(".bin") };
    (safe && ext_ok).then_some(name)
}

/// Lien signé d'une partie : `downlink` (jeton requis) répond en JSON, ou par une redirection
/// directe vers le CDN ; dans les deux cas l'adresse finale est vérifiée.
pub fn part_link(agent: &ureq::Agent, ends: &Endpoints, session: &Session, product: u64, part: &Part) -> Result<url::Url, Failure> {
    let response = agent
        .get(&format!("{}/products/{product}/downlink/installer/{}", ends.api, part.id))
        .set("Authorization", &format!("Bearer {}", session.access_token))
        .call()
        .map_err(failure)?;
    if (300..400).contains(&response.status()) {
        let location = response.header("location").unwrap_or("");
        let url = url::Url::parse(location).map_err(|_| Failure::Network(tr!("redirection illisible", "unreadable redirect")))?;
        return if download_allowed(&url) { Ok(url) } else { Err(Failure::Network(tr!("redirection hors de GOG refusée", "redirect outside GOG refused"))) };
    }
    parse_downlink(&body(response)?)
}

/// Taille totale d'un fichier d'après une réponse : `Content-Range: bytes a-b/total`, sinon
/// `Content-Length` (+ début demandé).
pub fn total_of(content_range: Option<&str>, content_length: Option<&str>, start: u64) -> Option<u64> {
    if let Some(range) = content_range {
        return range.rsplit_once('/').and_then(|(_, t)| t.trim().parse().ok());
    }
    content_length.and_then(|l| l.trim().parse::<u64>().ok()).map(|l| l + start)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRODUCTS: &str = include_str!("samples/products.json");
    const DOWNLOADS: &str = include_str!("samples/product_downloads.json");
    const TOKEN: &str = include_str!("samples/token.json");
    const OWNED: &str = include_str!("samples/owned.json");
    const USER: &str = include_str!("samples/user_data.json");
    const DOWNLINK: &str = include_str!("samples/downlink.json");

    fn url(s: &str) -> url::Url {
        url::Url::parse(s).unwrap()
    }

    #[test]
    fn code_is_read_only_from_gog_redirect() {
        let ok = url("https://embed.gog.com/on_login_success?origin=client&code=AbC-123_x.y~z");
        assert_eq!(code_from_redirect(&ok).as_deref(), Some("AbC-123_x.y~z"));
        for bad in [
            "http://embed.gog.com/on_login_success?code=abc",
            "https://embed.gog.com.evil.io/on_login_success?code=abc",
            "https://evil.io/on_login_success?code=abc",
            "https://embed.gog.com:8443/on_login_success?code=abc",
            "https://user@embed.gog.com/on_login_success?code=abc",
            "https://embed.gog.com/on_login_success/x?code=abc",
            "https://embed.gog.com/on_login_success?origin=client",
            "https://embed.gog.com/on_login_success?code=",
            "https://embed.gog.com/on_login_success?code=a%22b",
        ] {
            assert_eq!(code_from_redirect(&url(bad)), None, "{bad}");
        }
        assert!(navigation_allowed(&url("https://login.gog.com/x")));
        assert!(!navigation_allowed(&url("http://login.gog.com/x")));
        assert!(!navigation_allowed(&url("file:///C:/Windows")));
        assert!(!navigation_allowed(&url("javascript:alert(1)")));
        assert!(login_url().contains("redirect_uri=https%3A%2F%2Fembed.gog.com%2Fon_login_success%3Forigin%3Dclient"));
    }

    #[test]
    fn token_url_encodes_code_and_refresh() {
        let u = token_url("https://auth.gog.com", &Grant::Code("a b&c"));
        assert!(u.starts_with("https://auth.gog.com/token?client_id=46899977096215655&client_secret="));
        assert!(u.contains("grant_type=authorization_code&code=a%20b%26c&redirect_uri=https%3A%2F%2Fembed.gog.com"));
        let r = token_url("https://auth.gog.com", &Grant::Refresh("r/t"));
        assert!(r.ends_with("grant_type=refresh_token&refresh_token=r%2Ft"));
    }

    #[test]
    fn token_response_is_parsed_and_refresh_keeps_known_fields() {
        let s = parse_token(TOKEN, 1_000, None).unwrap();
        assert_eq!(s.access_token, "faux-jeton-d-acces-0123456789abcdef");
        assert_eq!(s.refresh_token, "faux-jeton-de-rafraichissement-fedcba9876543210");
        assert_eq!(s.expires_at, 4_600);
        assert_eq!(s.user_id, "48628349957132247");
        let mut known = s.clone();
        known.username = Some("JoueurEssai".into());
        let renewed = parse_token(r#"{"access_token":"neuf","expires_in":10}"#, 2_000, Some(&known)).unwrap();
        assert_eq!(renewed.refresh_token, known.refresh_token);
        assert_eq!(renewed.user_id, known.user_id);
        assert_eq!(renewed.username.as_deref(), Some("JoueurEssai"));
        assert_eq!(renewed.expires_at, 2_300, "durée bornée à 5 min au moins");
        for bad in ["", "{}", r#"{"access_token":""}"#, r#"{"access_token":"a","refresh_token":"b","user_id":"x1"}"#, "<html>"] {
            assert!(parse_token(bad, 0, None).is_err(), "{bad}");
        }
        assert!(needs_refresh(&s, 4_550));
        assert!(!needs_refresh(&s, 4_000));
    }

    #[test]
    fn errors_never_copy_the_url() {
        let err = Failure::Network("Dns".into()).message();
        assert!(!err.contains("http"));
        assert_eq!(parse_username(USER).as_deref(), Some("JoueurEssai"));
        assert_eq!(parse_username(r#"{"username":"  \u0007 "}"#), None);
    }

    #[test]
    fn owned_ids_and_products_from_samples() {
        assert_eq!(parse_owned(OWNED).unwrap(), vec![1207658924, 1207664663, 1207658930, 1207666413, 1]);
        assert!(parse_owned("{}").is_err());
        let products = parse_products(PRODUCTS).unwrap();
        assert_eq!(products.len(), 5);
        let witcher = &products[0];
        assert_eq!(witcher.title, "The Witcher: Enhanced Edition");
        assert_eq!((witcher.kind, witcher.windows, witcher.installable), (Kind::Game, true, true));
        assert_eq!(witcher.image.as_deref(), Some("37d4a208d1f5bb0e163da540ac894ba46a7d566ede31aaaefc74bbcd46ebd190"));
        assert_eq!(
            image_url(witcher.image.as_deref().unwrap()).as_deref(),
            Some("https://images.gog-statics.com/37d4a208d1f5bb0e163da540ac894ba46a7d566ede31aaaefc74bbcd46ebd190_product_tile_extended_432x243.webp")
        );
        let pack = products.iter().find(|p| p.id == 1207664663).unwrap();
        assert_eq!((pack.kind, pack.installable), (Kind::Pack, false));
        let tie = products.iter().find(|p| p.id == 1207666413).unwrap();
        assert!(tie.title.starts_with("STAR WARS"));
    }

    #[test]
    fn image_hosts_are_whitelisted() {
        let h = "37d4a208d1f5bb0e163da540ac894ba46a7d566ede31aaaefc74bbcd46ebd190";
        assert_eq!(image_hash(&format!("//images-1.gog-statics.com/{h}_glx_logo.jpg")).as_deref(), Some(h));
        assert_eq!(image_hash(&format!("https://images.gog-statics.com/{h}.png")).as_deref(), Some(h));
        for bad in [
            format!("//images-1.evil.com/{h}.jpg"),
            format!("//images-1.gog-statics.com.evil.com/{h}.jpg"),
            format!("//images-x.gog-statics.com/{h}.jpg"),
            format!("//images-1.gog-statics.com/a/{h}.jpg"),
            format!("//images-1.gog-statics.com/{h}.jpg?x=1"),
            "//images-1.gog-statics.com/court.jpg".to_string(),
        ] {
            assert_eq!(image_hash(&bad), None, "{bad}");
        }
        assert_eq!(image_url("pas-une-empreinte"), None);
    }

    #[test]
    fn installer_choice_and_checks() {
        let i = parse_installer(DOWNLOADS, 1207658924, "fr").unwrap().unwrap();
        assert_eq!(i.language, "en", "pas de français : anglais");
        assert_eq!(i.parts.len(), 4);
        assert_eq!(i.parts[0], Part { id: "en1installer0".into(), size: 1_048_576 });
        assert_eq!(i.total(), 10_177_478_656);
        assert!(parse_installer(DOWNLOADS, 42, "fr").is_err(), "fiche d'un autre jeu");
        // Lien d'une partie qui pointe ailleurs : refusé en bloc.
        let tampered = DOWNLOADS.replace(r"https:\/\/api.gog.com\/products\/1207658924\/downlink\/installer\/en1installer1", "https://evil.io/x");
        assert_ne!(tampered, DOWNLOADS);
        assert!(parse_installer(&tampered, 1207658924, "fr").is_err());
        let no_windows = DOWNLOADS.replace("\"os\":\"windows\"", "\"os\":\"linux\"");
        assert_ne!(no_windows, DOWNLOADS);
        assert_eq!(parse_installer(&no_windows, 1207658924, "fr").unwrap(), None);
    }

    #[test]
    fn download_hosts_and_names_are_checked() {
        let link = parse_downlink(DOWNLINK).unwrap();
        assert_eq!(link.host_str(), Some("gog-cdn-fastly.gog.com"));
        assert_eq!(file_name(&link, true).as_deref(), Some("setup_the_witcher_enhanced_edition_1.5_(10712).exe"));
        assert_eq!(file_name(&link, false), None, "une partie suivante doit être un .bin");
        for ok in ["https://cdn.gog.com/a", "https://gog-cdn-lumen.gog.com/a", "https://api.gog.com/products/1"] {
            assert!(download_allowed(&url(ok)), "{ok}");
        }
        for bad in [
            "http://cdn.gog.com/a",
            "https://cdn.gog.com:444/a",
            "https://user:pass@cdn.gog.com/a",
            "https://gog.com.evil.io/a",
            "https://evilgog.com/a",
            "https://cdn.gog.com.evil.io/a",
            "https://127.0.0.1/a",
            "ftp://cdn.gog.com/a",
        ] {
            assert!(!download_allowed(&url(bad)), "{bad}");
        }
        assert!(parse_downlink(r#"{"downlink":"https://evil.io/setup.exe"}"#).is_err());
        assert!(parse_downlink(r#"{"downlink":"http://cdn.gog.com/setup.exe"}"#).is_err());
        assert!(parse_downlink("{}").is_err());
        assert_eq!(file_name(&url("https://cdn.gog.com/x/setup%20a-1.bin"), false).as_deref(), Some("setup a-1.bin"));
        for bad in ["https://cdn.gog.com/x/..%5Csetup.exe", "https://cdn.gog.com/x/setup.exe%00.bin", "https://cdn.gog.com/x/", "https://cdn.gog.com/x/a%2F..%2Fb.exe", "https://cdn.gog.com/x/setup.msi"] {
            assert_eq!(file_name(&url(bad), true), None, "{bad}");
        }
        assert_eq!(total_of(Some("bytes 100-199/5000"), Some("100"), 100), Some(5000));
        assert_eq!(total_of(None, Some("900"), 100), Some(1000));
        assert_eq!(total_of(None, None, 0), None);
    }
}
