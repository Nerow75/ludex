//! Actualités des jeux : annonces officielles publiées par le studio sur Steam, pour tout jeu dont
//! on connaît l'appid (`Game.steam_appid`).
//!
//! API publique sans clé `ISteamNews/GetNewsForApp/v2` limitée au flux
//! `steam_community_announcements` (annonces du groupe officiel du jeu, pas la presse). Elle n'a
//! pas de paramètre de langue : les annonces arrivent dans la langue du studio (souvent l'anglais).
//! Le texte est du BBCode Steam (`[p]`, `[h2]`, `[list][*]`, `[url=…]`, `[img]{STEAM_CLAN_IMAGE}/…`) :
//! il est converti ici en blocs sûrs (titres, paragraphes, listes, images, séparateurs), jamais en
//! HTML. Les liens ne partent jamais vers le front : chaque lien cliquable est un numéro, et
//! `open_news` reprend l'adresse dans le cache, seulement en https vers un hôte Steam
//! (`LINK_HOSTS`). Images : seulement le CDN des annonces Steam (`IMAGE_HOSTS`, dans `img-src`).
//!
//! Cache `news-cache.json` (dossier de données de l'app) : annonces brutes par appid, relues après
//! 6 heures, oubliées après 60 jours sans lecture ; une réponse vide n'efface pas des annonces connues. Vérification de fond une fois par
//! jour pour les jeux « En cours » (`sweep_due`, `alerts`) : une annonce de moins de 3 jours jamais
//! signalée donne une notification dans l'app.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const ENDPOINT: &str = "https://api.steampowered.com/ISteamNews/GetNewsForApp/v2/";
const FEED: &str = "steam_community_announcements";
/// Annonces demandées par jeu.
const COUNT: usize = 10;
/// Relecture des annonces d'un jeu après ce temps.
const REFRESH_AFTER: u64 = 6 * 3600;
/// Annonces d'un jeu plus regardé depuis ce temps : oubliées au chargement.
const FORGET_AFTER: u64 = 60 * 24 * 3600;
/// À augmenter quand le contenu du cache change : tout est alors redemandé.
const CACHE_VERSION: u32 = 1;
/// Réponse lue au plus (les annonces les plus longues font une cinquantaine de Ko).
const MAX_BODY: u64 = 4 * 1024 * 1024;
/// Texte BBCode gardé par annonce.
const MAX_CONTENTS: usize = 200_000;
const MAX_TITLE: usize = 300;
/// Blocs et liens rendus par annonce.
const MAX_BLOCKS: usize = 400;
const MAX_LINKS: usize = 300;
/// Extrait de la liste.
const EXCERPT: usize = 220;
/// Vérification de fond des jeux « En cours » : une fois par jour.
pub const SWEEP_EVERY: u64 = 24 * 3600;
/// Pause entre deux requêtes de la vérification de fond.
pub const SWEEP_GAP: Duration = Duration::from_secs(2);
/// Jeux « En cours » vérifiés au plus par passage.
pub const SWEEP_MAX: usize = 20;
/// Une annonce plus ancienne n'est plus signalée.
const RECENT: u64 = 3 * 24 * 3600;
/// Préfixe des images hébergées par Steam pour les annonces.
const CLAN_IMAGE: &str = "{STEAM_CLAN_IMAGE}";
const CLAN_IMAGE_URL: &str = "https://clan.akamai.steamstatic.com/images";
/// Hôtes des images des annonces (aussi dans `img-src` de la CSP).
const IMAGE_HOSTS: &[&str] = &["clan.akamai.steamstatic.com", "clan.fastly.steamstatic.com"];
/// Hôtes des liens ouverts par Ludex : la boutique, la communauté et l'aide de Steam.
const LINK_HOSTS: &[&str] = &["store.steampowered.com", "steamcommunity.com", "help.steampowered.com"];

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

// ---------------------------------------------------------------------------------------------
// Blocs envoyés au front
// ---------------------------------------------------------------------------------------------

fn no(b: &bool) -> bool {
    !*b
}

/// Morceau de texte d'un bloc. `link` : numéro du lien (ouvert par `open_news`), seulement pour
/// une adresse permise.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Span {
    pub text: String,
    #[serde(skip_serializing_if = "no")]
    pub bold: bool,
    #[serde(skip_serializing_if = "no")]
    pub italic: bool,
    #[serde(skip_serializing_if = "no")]
    pub underline: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Block {
    /// Niveau 1 à 3.
    Heading { level: u8, spans: Vec<Span> },
    Paragraph { spans: Vec<Span> },
    List { ordered: bool, items: Vec<Vec<Span>> },
    /// Adresse déjà vérifiée (`safe_image`).
    Image { url: String },
    Rule,
}

/// Annonce envoyée au front.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct News {
    /// Identifiant Steam de l'annonce (chiffres).
    pub gid: String,
    pub title: String,
    /// Publication (secondes Unix).
    pub date: u64,
    pub author: Option<String>,
    /// Notes de mise à jour (tag `patchnotes` de Steam).
    pub update: bool,
    /// Première image de l'annonce.
    pub image: Option<String>,
    pub excerpt: String,
    pub blocks: Vec<Block>,
}

// ---------------------------------------------------------------------------------------------
// Adresses permises
// ---------------------------------------------------------------------------------------------

/// Caractères permis dans une adresse (RFC 3986, sans espace ni guillemet ni accolade).
fn url_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "-._~:/?#[]@!$&'()*+,;=%".contains(c)
}

/// Hôte d'une adresse `https://` propre (sans identifiants ni port), en minuscules.
fn https_host(url: &str) -> Option<(String, &str)> {
    if url.len() > 2000 || !url.chars().all(url_char) {
        return None;
    }
    let rest = url.get(..8).filter(|p| p.eq_ignore_ascii_case("https://")).map(|_| &url[8..])?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (host, path) = rest.split_at(end);
    if host.is_empty() || host.contains(['@', ':']) {
        return None;
    }
    Some((host.to_ascii_lowercase(), path))
}

/// Lien ouvert par Ludex : https vers un hôte Steam de `LINK_HOSTS`, sans `..`.
pub fn safe_link(url: &str) -> Option<String> {
    let url = url.trim();
    let (host, path) = https_host(url)?;
    (LINK_HOSTS.contains(&host.as_str()) && !path.contains("..")).then(|| url.to_string())
}

/// Image d'une annonce : `{STEAM_CLAN_IMAGE}/<n>/<empreinte>.<ext>` ou la même adresse sur un hôte
/// de `IMAGE_HOSTS`, sans requête. Toute autre image (imgur, sites des studios) est écartée.
pub fn safe_image(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let url = match raw.strip_prefix(CLAN_IMAGE) {
        Some(rest) => format!("{CLAN_IMAGE_URL}{rest}"),
        None => raw.to_string(),
    };
    let (host, path) = https_host(&url)?;
    if !IMAGE_HOSTS.contains(&host.as_str()) {
        return None;
    }
    let parts: Vec<&str> = path.strip_prefix("/images/")?.split('/').collect();
    let [clan, file] = parts.as_slice() else { return None };
    let (name, ext) = file.rsplit_once('.')?;
    let ok = !clan.is_empty()
        && clan.bytes().all(|b| b.is_ascii_digit())
        && !name.is_empty()
        && name.bytes().all(|b| b.is_ascii_hexdigit())
        && ["png", "jpg", "jpeg", "gif", "webp"].contains(&ext.to_ascii_lowercase().as_str());
    ok.then(|| format!("https://{host}{path}"))
}

/// Page de l'annonce sur la boutique Steam.
pub fn news_page(appid: u32, gid: &str) -> Option<String> {
    (!gid.is_empty() && gid.len() <= 24 && gid.bytes().all(|b| b.is_ascii_digit()))
        .then(|| format!("https://store.steampowered.com/news/app/{appid}/view/{gid}"))
}

// ---------------------------------------------------------------------------------------------
// BBCode
// ---------------------------------------------------------------------------------------------

/// Balise reconnue. Un texte entre crochets inconnu (« [PC] », « [Fixed] ») reste du texte.
#[derive(Debug)]
struct Tag<'a> {
    name: String,
    closing: bool,
    /// Ce qui suit le nom, jusqu'au `]` (attributs).
    attrs: &'a str,
}

const KNOWN: &[&str] = &[
    "p", "h1", "h2", "h3", "h4", "h5", "h6", "b", "i", "u", "s", "strike", "list", "olist", "*", "url", "dynamiclink", "img",
    "previewyoutube", "video", "hr", "br", "quote", "code", "spoiler", "noparse", "table", "tr", "td", "th", "expand", "carousel",
    "emoticon", "sup", "sub", "c", "color", "size",
];

/// Lit une balise au début de `s` (qui commence par `[`) : la balise et sa longueur.
fn read_tag(s: &str) -> Option<(Tag<'_>, usize)> {
    let inner_end = s[1..].find([']', '[', '\n'])? + 1;
    if s.as_bytes()[inner_end] != b']' || inner_end > 4000 {
        return None;
    }
    let inner = &s[1..inner_end];
    let (closing, body) = match inner.strip_prefix('/') {
        Some(b) => (true, b),
        None => (false, inner),
    };
    let name_len = body.find(|c: char| !(c.is_ascii_alphanumeric() || c == '*')).unwrap_or(body.len());
    let name = body[..name_len].to_ascii_lowercase();
    if !KNOWN.contains(&name.as_str()) {
        return None;
    }
    Some((Tag { name, closing, attrs: &body[name_len..] }, inner_end + 1))
}

/// Valeur par défaut d'une balise : `[url=X]`, `[url="X"]`.
fn default_attr(attrs: &str) -> Option<String> {
    let v = attrs.trim_start().strip_prefix('=')?.trim();
    let v = v.strip_prefix('"').map(|v| v.split('"').next().unwrap_or("")).unwrap_or(v);
    Some(v.trim().to_string()).filter(|v| !v.is_empty())
}

/// Attribut nommé : `src="X"`, `href=X`.
fn named_attr(attrs: &str, key: &str) -> Option<String> {
    let lower = attrs.to_ascii_lowercase();
    let mut from = 0;
    while let Some(pos) = lower[from..].find(key).map(|p| p + from) {
        let before = lower[..pos].chars().last();
        let after = &attrs[pos + key.len()..];
        if before.is_none_or(|c| c.is_whitespace()) && let Some(v) = after.trim_start().strip_prefix('=') {
            let v = v.trim_start();
            let v = match v.strip_prefix('"') {
                Some(q) => q.split('"').next().unwrap_or(""),
                None => v.split(char::is_whitespace).next().unwrap_or(""),
            };
            return Some(v.trim().to_string()).filter(|v| !v.is_empty());
        }
        from = pos + key.len();
    }
    None
}

/// Entités HTML courantes (certaines annonces anciennes en contiennent).
fn decode(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    text.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", "\u{a0}")
        .replace("&amp;", "&")
}

/// Adresse courte d'un lien sans texte : hôte et chemin, 60 caractères au plus.
fn short_url(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let rest = rest.trim_end_matches('/');
    if rest.chars().count() <= 60 { rest.to_string() } else { format!("{}…", rest.chars().take(59).collect::<String>()) }
}

#[derive(Default)]
struct Builder {
    blocks: Vec<Block>,
    links: Vec<String>,
    spans: Vec<Span>,
    bold: u32,
    italic: u32,
    underline: u32,
    /// Lien de chaque `[url]` ouvert ; `Raw` : `[url]adresse[/url]`, l'adresse est le texte lu.
    link_stack: Vec<LinkOpen>,
    /// Liste ouverte : ordonnée ou non, éléments déjà finis.
    list: Option<(bool, Vec<Vec<Span>>)>,
    list_depth: u32,
    heading: Option<u8>,
    /// Dans `[img]…[/img]` : l'adresse est le texte.
    image: Option<String>,
    /// Dans `[previewyoutube]…` ou `[video]…` : texte ignoré.
    skip: u32,
    /// Cellule déjà écrite sur la ligne de tableau en cours.
    cell: bool,
}

enum LinkOpen {
    To(Option<u32>),
    Raw(String),
}

impl Builder {
    fn current_link(&self) -> Option<u32> {
        match self.link_stack.last() {
            Some(LinkOpen::To(l)) => *l,
            _ => None,
        }
    }

    fn add_link(&mut self, url: &str) -> Option<u32> {
        let url = safe_link(url)?;
        if let Some(i) = self.links.iter().position(|l| *l == url) {
            return Some(i as u32);
        }
        if self.links.len() >= MAX_LINKS {
            return None;
        }
        self.links.push(url);
        Some(self.links.len() as u32 - 1)
    }

    fn text(&mut self, text: &str) {
        if self.skip > 0 {
            return;
        }
        if let Some(url) = &mut self.image {
            url.push_str(text);
            return;
        }
        if let Some(LinkOpen::Raw(url)) = self.link_stack.last_mut() {
            url.push_str(text);
            return;
        }
        // Deux retours à la ligne séparent deux paragraphes (annonces sans `[p]`).
        let mut parts = text.split("\n\n").peekable();
        while let Some(part) = parts.next() {
            self.push_span(&decode(part));
            if parts.peek().is_some() {
                if self.list.is_some() || self.heading.is_some() {
                    self.push_span("\n");
                } else {
                    self.end_paragraph();
                }
            }
        }
    }

    fn push_span(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let span = Span {
            text: text.to_string(),
            bold: self.bold > 0 || self.heading.is_some(),
            italic: self.italic > 0,
            underline: self.underline > 0,
            link: self.current_link(),
        };
        match self.spans.last_mut() {
            Some(last) if last.bold == span.bold && last.italic == span.italic && last.underline == span.underline && last.link == span.link => {
                last.text.push_str(&span.text)
            }
            _ => self.spans.push(span),
        }
    }

    fn push_block(&mut self, block: Block) {
        if self.blocks.len() < MAX_BLOCKS {
            self.blocks.push(block);
        }
    }

    /// Morceaux en cours, sans blancs au début ni à la fin ; vide s'il n'y a que des blancs.
    fn take_spans(&mut self) -> Vec<Span> {
        let mut spans = std::mem::take(&mut self.spans);
        spans.retain(|s| !s.text.is_empty());
        while spans.first().is_some_and(|s| s.text.trim().is_empty()) {
            spans.remove(0);
        }
        while spans.last().is_some_and(|s| s.text.trim().is_empty()) {
            spans.pop();
        }
        if let Some(first) = spans.first_mut() {
            first.text = first.text.trim_start().to_string();
        }
        if let Some(last) = spans.last_mut() {
            last.text = last.text.trim_end().to_string();
        }
        for s in &mut spans {
            while s.text.contains("\n\n\n") {
                s.text = s.text.replace("\n\n\n", "\n\n");
            }
        }
        spans
    }

    /// Fin d'un paragraphe (ou d'un élément de liste, dans une liste).
    fn end_paragraph(&mut self) {
        if self.list.is_some() {
            return self.end_item();
        }
        let spans = self.take_spans();
        if spans.is_empty() {
            return;
        }
        match self.heading {
            Some(level) => self.push_block(Block::Heading { level, spans }),
            None => self.push_block(Block::Paragraph { spans }),
        }
    }

    fn end_item(&mut self) {
        let spans = self.take_spans();
        if let Some((_, items)) = &mut self.list
            && !spans.is_empty()
        {
            items.push(spans);
        }
    }

    /// Ferme la liste en cours (une image au milieu la coupe en deux).
    fn end_list(&mut self) {
        self.end_item();
        if let Some((ordered, items)) = self.list.take()
            && !items.is_empty()
        {
            self.push_block(Block::List { ordered, items });
        }
    }

    fn image_block(&mut self, raw: &str) {
        let Some(url) = safe_image(raw) else { return };
        let reopen = self.list.as_ref().map(|(ordered, _)| *ordered);
        if reopen.is_some() {
            self.end_list();
        } else {
            self.end_paragraph();
        }
        self.push_block(Block::Image { url });
        if let Some(ordered) = reopen {
            self.list = Some((ordered, Vec::new()));
        }
    }

    fn tag(&mut self, tag: &Tag) {
        let name = tag.name.as_str();
        if self.image.is_some() && !(name == "img" && tag.closing) {
            return;
        }
        if self.skip > 0 && !(tag.closing && matches!(name, "previewyoutube" | "video")) {
            return;
        }
        match (name, tag.closing) {
            ("p", _) => {
                if self.list.is_none() {
                    self.end_paragraph();
                }
            }
            ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", false) => {
                if self.list.is_some() {
                    self.bold += 1;
                } else {
                    self.end_paragraph();
                    self.heading = Some(name[1..].parse::<u8>().unwrap_or(3).min(3));
                }
            }
            ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", true) => {
                if self.heading.is_some() {
                    self.end_paragraph();
                    self.heading = None;
                } else {
                    self.bold = self.bold.saturating_sub(1);
                }
            }
            ("b", false) => self.bold += 1,
            ("b", true) => self.bold = self.bold.saturating_sub(1),
            ("i", false) => self.italic += 1,
            ("i", true) => self.italic = self.italic.saturating_sub(1),
            ("u", false) => self.underline += 1,
            ("u", true) => self.underline = self.underline.saturating_sub(1),
            ("list" | "olist", false) => {
                self.list_depth += 1;
                if self.list.is_none() {
                    self.end_paragraph();
                    self.list = Some((name == "olist", Vec::new()));
                } else {
                    // Liste dans une liste : mise à plat.
                    self.end_item();
                }
            }
            ("list" | "olist", true) => {
                self.list_depth = self.list_depth.saturating_sub(1);
                if self.list_depth == 0 {
                    self.end_list();
                } else {
                    self.end_item();
                }
            }
            ("*", false) => {
                if self.list.is_some() {
                    self.end_item();
                } else {
                    self.end_paragraph();
                    self.push_span("• ");
                }
            }
            ("url", false) => {
                let open = match default_attr(tag.attrs) {
                    Some(url) => LinkOpen::To(self.add_link(&url)),
                    None => LinkOpen::Raw(String::new()),
                };
                self.link_stack.push(open);
            }
            ("url", true) => {
                if let Some(LinkOpen::Raw(url)) = self.link_stack.pop() {
                    // `[url]adresse[/url]` : l'adresse est le texte, gardé tel quel si elle est écartée.
                    match self.add_link(url.trim()) {
                        Some(link) => {
                            self.link_stack.push(LinkOpen::To(Some(link)));
                            self.push_span(&short_url(url.trim()));
                            self.link_stack.pop();
                        }
                        None => self.push_span(&decode(&url)),
                    }
                }
            }
            ("dynamiclink", false) => {
                // Aperçu d'un lien : rien à montrer s'il n'est pas permis.
                if let Some(url) = named_attr(tag.attrs, "href").or_else(|| default_attr(tag.attrs))
                    && let Some(link) = self.add_link(&url)
                {
                    self.end_paragraph();
                    self.link_stack.push(LinkOpen::To(Some(link)));
                    self.push_span(&short_url(&url));
                    self.link_stack.pop();
                    self.end_paragraph();
                }
            }
            ("img", false) => match named_attr(tag.attrs, "src") {
                Some(src) => self.image_block(&src),
                None => self.image = Some(String::new()),
            },
            ("img", true) => {
                if let Some(raw) = self.image.take() {
                    self.image_block(&raw);
                }
            }
            ("video", false) => {
                if let Some(poster) = named_attr(tag.attrs, "poster") {
                    self.image_block(&poster);
                }
                self.skip += 1;
            }
            ("previewyoutube", false) => self.skip += 1,
            ("previewyoutube" | "video", true) => self.skip = self.skip.saturating_sub(1),
            ("hr", false) => {
                if self.list.is_none() {
                    self.end_paragraph();
                    self.push_block(Block::Rule);
                }
            }
            ("br", false) => self.push_span("\n"),
            ("tr", _) => {
                self.end_paragraph();
                self.cell = false;
            }
            ("td" | "th", false) => {
                if self.cell {
                    self.push_span(" · ");
                }
                self.cell = true;
            }
            ("quote" | "code" | "table", _) if self.list.is_none() => self.end_paragraph(),
            // Autres balises connues (`s`, `spoiler`, `expand`…) : seul le texte reste.
            _ => {}
        }
    }

    fn finish(mut self) -> (Vec<Block>, Vec<String>) {
        if let Some(raw) = self.image.take() {
            self.image_block(&raw);
        }
        self.end_paragraph();
        self.heading = None;
        self.end_list();
        (self.blocks, self.links)
    }
}

/// Convertit le BBCode d'une annonce en blocs, avec les adresses des liens permis (numérotés).
pub fn parse_bbcode(source: &str) -> (Vec<Block>, Vec<String>) {
    const BACKSLASH: char = '\u{5C}';
    let source = source.replace("\r\n", "\n");
    let mut b = Builder::default();
    let mut rest = source.as_str();
    while let Some(at) = rest.find(['[', BACKSLASH]) {
        b.text(&rest[..at]);
        rest = &rest[at..];
        // Barre oblique inverse devant `[`, `]` ou une autre barre : caractère échappé (notes de mise à
        // jour de CS2, appid 286160), écrit tel quel et jamais lu comme une balise ; devant autre chose,
        // la barre reste.
        if let Some(after) = rest.strip_prefix(BACKSLASH) {
            if after.starts_with(['[', ']', BACKSLASH]) {
                b.text(&after[..1]);
                rest = &after[1..];
            } else {
                b.text(&rest[..1]);
                rest = after;
            }
            continue;
        }
        match read_tag(rest) {
            Some((tag, len)) => {
                b.tag(&tag);
                rest = &rest[len..];
            }
            None => {
                b.text("[");
                rest = &rest[1..];
            }
        }
    }
    b.text(rest);
    b.finish()
}

/// Texte de la liste : paragraphes et éléments de liste, espaces regroupés, coupé à un mot.
fn excerpt(blocks: &[Block], title: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    let mut len = 0;
    let joined = |spans: &[Span]| spans.iter().map(|s| s.text.as_str()).collect::<String>();
    for block in blocks {
        let text = match block {
            Block::Paragraph { spans } => joined(spans),
            Block::List { items, .. } => items.iter().map(|i| joined(i)).collect::<Vec<_>>().join(" "),
            // Un titre qui répète celui de l'annonce n'apporte rien.
            Block::Heading { spans, .. } if joined(spans).trim() != title.trim() => joined(spans),
            _ => continue,
        };
        for w in text.split_whitespace() {
            if len + w.len() > EXCERPT {
                let mut out = words.join(" ");
                out.push('…');
                return out;
            }
            len += w.len() + 1;
            words.push(w.to_string());
        }
    }
    words.join(" ")
}

fn clip(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

// ---------------------------------------------------------------------------------------------
// Réponse de Steam et cache
// ---------------------------------------------------------------------------------------------

/// Annonce telle que Steam la donne, gardée dans le cache.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Raw {
    gid: String,
    title: String,
    date: u64,
    #[serde(default)]
    author: String,
    contents: String,
    #[serde(default)]
    update: bool,
}

impl Raw {
    fn news(&self) -> News {
        let (blocks, _) = parse_bbcode(&self.contents);
        let image = blocks.iter().find_map(|b| match b {
            Block::Image { url } => Some(url.clone()),
            _ => None,
        });
        News {
            gid: self.gid.clone(),
            title: self.title.clone(),
            date: self.date,
            author: Some(self.author.trim().to_string()).filter(|a| !a.is_empty()),
            update: self.update,
            image,
            excerpt: excerpt(&blocks, &self.title),
            blocks,
        }
    }

    /// Adresse du lien `n` de l'annonce, déjà vérifiée.
    fn link(&self, n: u32) -> Option<String> {
        parse_bbcode(&self.contents).1.get(n as usize).cloned()
    }
}

#[derive(Deserialize)]
struct Response {
    appnews: Option<AppNews>,
}

#[derive(Deserialize)]
struct AppNews {
    #[serde(default)]
    newsitems: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    #[serde(default)]
    gid: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    contents: String,
    #[serde(default)]
    feedname: String,
    #[serde(default)]
    date: u64,
    #[serde(default)]
    appid: u32,
    #[serde(default)]
    tags: Vec<String>,
}

/// Lit la réponse de `GetNewsForApp` : annonces officielles de cet appid, la plus récente d'abord.
pub fn parse_response(body: &str, appid: u32) -> Result<Vec<Raw>, String> {
    let response: Response = serde_json::from_str(body).map_err(|e| tr!("réponse illisible : {e}", "unreadable response: {e}"))?;
    let Some(news) = response.appnews else { return Err(tr!("réponse sans annonces", "response without announcements")) };
    let mut items: Vec<Raw> = news
        .newsitems
        .into_iter()
        .filter(|i| i.feedname == FEED && (i.appid == 0 || i.appid == appid))
        .filter(|i| !i.gid.is_empty() && i.gid.len() <= 24 && i.gid.bytes().all(|b| b.is_ascii_digit()))
        .filter(|i| !i.title.trim().is_empty() && i.date > 0)
        .map(|i| Raw {
            update: i.tags.iter().any(|t| t == "patchnotes"),
            gid: i.gid,
            title: clip(&decode(i.title.trim()), MAX_TITLE),
            date: i.date,
            author: clip(i.author.trim(), 100),
            contents: clip(&i.contents, MAX_CONTENTS),
        })
        .collect();
    items.sort_by_key(|r| std::cmp::Reverse(r.date));
    items.dedup_by(|a, b| a.gid == b.gid);
    items.truncate(COUNT);
    Ok(items)
}

/// Demande les annonces d'un jeu (une requête, aucune redirection suivie).
pub fn fetch(agent: &ureq::Agent, appid: u32) -> Result<Vec<Raw>, String> {
    let response = agent
        .get(ENDPOINT)
        .query("appid", &appid.to_string())
        .query("count", &COUNT.to_string())
        .query("maxlength", "0")
        .query("feeds", FEED)
        .call()
        .map_err(|e| e.to_string())?;
    let mut body = String::new();
    response.into_reader().take(MAX_BODY).read_to_string(&mut body).map_err(|e| e.to_string())?;
    parse_response(&body, appid)
}

pub fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(10)).redirects(0).user_agent("Ludex (lanceur de jeux)").build()
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Entry {
    /// Date de la requête (secondes Unix).
    checked: u64,
    #[serde(default)]
    items: Vec<Raw>,
}

/// Annonce à signaler pour un jeu « En cours ».
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Alert {
    pub id: String,
    pub game: String,
    pub title: String,
    pub gid: String,
    pub update: bool,
}

/// Annonces gardées par appid, et suivi des notifications.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsCache {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    apps: BTreeMap<u32, Entry>,
    /// Dernière vérification de fond (secondes Unix).
    #[serde(default)]
    last_sweep: u64,
    /// Date de la plus récente annonce déjà signalée (ou vue), par appid.
    #[serde(default)]
    seen: BTreeMap<u32, u64>,
    #[serde(skip)]
    path: PathBuf,
}

impl NewsCache {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("news-cache.json");
        let mut cache: NewsCache = fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        if cache.version != CACHE_VERSION {
            cache.apps.clear();
            cache.version = CACHE_VERSION;
        }
        let at = now();
        cache.apps.retain(|_, e| at.saturating_sub(e.checked) < FORGET_AFTER);
        cache.path = path;
        cache
    }

    pub fn save(&self) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }

    /// Annonces jamais demandées, ou demandées il y a plus de 6 heures.
    pub fn stale(&self, appid: u32, at: u64) -> bool {
        self.apps.get(&appid).is_none_or(|e| at.saturating_sub(e.checked) > REFRESH_AFTER)
    }

    /// Enregistre une réponse. Vide : les annonces déjà connues restent.
    pub fn record(&mut self, appid: u32, items: Vec<Raw>, at: u64) {
        let items = if items.is_empty() { self.apps.get(&appid).map(|e| e.items.clone()).unwrap_or_default() } else { items };
        self.apps.insert(appid, Entry { checked: at, items });
    }

    /// Des annonces de ce jeu sont déjà connues (affichées même hors ligne).
    pub fn known(&self, appid: u32) -> bool {
        self.apps.get(&appid).is_some_and(|e| !e.items.is_empty())
    }

    pub fn news(&self, appid: u32) -> Vec<News> {
        self.apps.get(&appid).map(|e| e.items.iter().map(Raw::news).collect()).unwrap_or_default()
    }

    /// Adresse à ouvrir pour une annonce : son lien `link`, ou sa page sur la boutique Steam.
    pub fn target(&self, appid: u32, gid: &str, link: Option<u32>) -> Option<String> {
        let raw = self.apps.get(&appid)?.items.iter().find(|r| r.gid == gid)?;
        match link {
            Some(n) => raw.link(n),
            None => news_page(appid, &raw.gid),
        }
    }

    /// Les annonces de ce jeu ont été montrées : elles ne seront plus signalées.
    pub fn mark_seen(&mut self, appid: u32) -> bool {
        let Some(newest) = self.apps.get(&appid).and_then(|e| e.items.iter().map(|r| r.date).max()) else { return false };
        let seen = self.seen.entry(appid).or_default();
        let changed = newest > *seen;
        *seen = (*seen).max(newest);
        changed
    }

    /// La vérification de fond est due (plus d'un jour depuis la dernière).
    pub fn sweep_due(&self, at: u64) -> bool {
        at.saturating_sub(self.last_sweep) >= SWEEP_EVERY
    }

    pub fn swept(&mut self, at: u64) {
        self.last_sweep = at;
    }

    /// Annonces à signaler : pour chaque jeu (id, titre, appid), la plus récente si elle date de
    /// moins de 3 jours et n'a jamais été signalée ni vue. Toutes ses annonces sont ensuite notées.
    pub fn alerts(&mut self, games: &[(String, String, u32)], at: u64) -> Vec<Alert> {
        let mut out = Vec::new();
        for (id, title, appid) in games {
            let Some(newest) = self.apps.get(appid).and_then(|e| e.items.iter().max_by_key(|r| r.date)) else { continue };
            let seen = self.seen.get(appid).copied().unwrap_or(0);
            if newest.date > seen && at.saturating_sub(newest.date) <= RECENT && !out.iter().any(|a: &Alert| a.gid == newest.gid) {
                out.push(Alert { id: id.clone(), game: title.clone(), title: newest.title.clone(), gid: newest.gid.clone(), update: newest.update });
            }
            self.mark_seen(*appid);
        }
        out
    }

    #[cfg(test)]
    fn at(path: PathBuf) -> Self {
        NewsCache { version: CACHE_VERSION, path, ..Default::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(appid: u32) -> Vec<Raw> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures").join("news").join(format!("{appid}.json"));
        parse_response(&fs::read_to_string(path).unwrap(), appid).unwrap()
    }

    fn text(spans: &[Span]) -> String {
        spans.iter().map(|s| s.text.as_str()).collect()
    }

    fn all_text(blocks: &[Block]) -> String {
        blocks
            .iter()
            .map(|b| match b {
                Block::Heading { spans, .. } | Block::Paragraph { spans } => text(spans),
                Block::List { items, .. } => items.iter().map(|i| text(i)).collect::<Vec<_>>().join("\n"),
                Block::Image { url } => format!("<{url}>"),
                Block::Rule => "---".into(),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn images(blocks: &[Block]) -> Vec<&str> {
        blocks
            .iter()
            .filter_map(|b| match b {
                Block::Image { url } => Some(url.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn lit_les_reponses_enregistrees() {
        for appid in [730, 105600, 1091500, 2073850] {
            let items = fixture(appid);
            assert!(!items.is_empty(), "{appid}");
            assert!(items.windows(2).all(|w| w[0].date >= w[1].date));
            for raw in &items {
                let news = raw.news();
                assert!(!news.blocks.is_empty(), "{} vide", raw.title);
                assert!(!news.excerpt.is_empty(), "{} sans extrait", raw.title);
                let all = all_text(&news.blocks);
                // Aucune balise ni marque de remplacement ne reste dans le texte.
                for tag in ["[p", "[/p]", "[img", "[url", "[list", "[*]", "[h1", "[h2", "[b]", "{STEAM_CLAN_IMAGE}", "[dynamiclink", "[video"] {
                    assert!(!all.contains(tag), "{tag} dans {}", raw.title);
                }
            }
        }
    }

    #[test]
    fn notes_de_mise_a_jour_reconnues() {
        let items = fixture(730);
        let update = items.iter().find(|r| r.title == "Counter-Strike 2 Update").unwrap();
        assert!(update.update);
        assert!(!items.iter().find(|r| r.title.contains("Singapore")).unwrap().update);
        let news = update.news();
        assert!(news.blocks.iter().any(|b| matches!(b, Block::List { .. })));
        // Crochets échappés de la vraie réponse : rendus sans la barre oblique inverse.
        assert!(update.contents.contains(r"\["));
        assert!(!all_text(&news.blocks).contains('\\'), "{}", news.excerpt);
    }

    #[test]
    fn images_du_cdn_de_steam_seulement() {
        let items = fixture(105600);
        let state = items.iter().find(|r| r.title.contains("State of the Game")).unwrap();
        assert!(state.contents.contains("i.imgur.com"));
        let news = state.news();
        let imgs = images(&news.blocks);
        assert!(!imgs.is_empty());
        assert!(imgs.iter().all(|u| u.starts_with("https://clan.akamai.steamstatic.com/images/")), "{imgs:?}");
        assert_eq!(news.image.as_deref(), imgs.first().copied());
        // Vidéo de CS2 : l'affiche seulement.
        let cs = fixture(730);
        let road = cs.iter().find(|r| r.title.contains("Singapore")).unwrap().news();
        assert!(images(&road.blocks).iter().any(|u| u.starts_with("https://clan.fastly.steamstatic.com/images/3381077/")));
    }

    #[test]
    fn liens_hors_steam_non_cliquables() {
        let items = fixture(105600);
        let state = items.iter().find(|r| r.title.contains("State of the Game")).unwrap();
        let (blocks, links) = parse_bbcode(&state.contents);
        assert!(links.iter().all(|l| safe_link(l).is_some()));
        assert!(!links.iter().any(|l| l.contains("github") || l.contains("discord") || l.contains("forums.terraria")));
        // Le texte des liens écartés reste.
        assert!(all_text(&blocks).contains("Discord") || all_text(&blocks).contains("FAQ"));
        // Liens dynamiques de Cyberpunk : Twitch et YouTube écartés, Steam gardé.
        let cp = fixture(1091500);
        for raw in &cp {
            let (_, links) = parse_bbcode(&raw.contents);
            assert!(links.iter().all(|l| l.starts_with("https://steamcommunity.com/") || l.starts_with("https://store.steampowered.com/")), "{links:?}");
        }
    }

    #[test]
    fn titres_listes_et_separateurs() {
        let items = fixture(2073850);
        let news = items[0].news();
        assert!(news.blocks.iter().any(|b| matches!(b, Block::Heading { .. })));
        assert!(news.blocks.iter().any(|b| matches!(b, Block::Rule)));
        assert!(news.blocks.iter().any(|b| matches!(b, Block::List { items, .. } if items.len() > 1)));
    }

    #[test]
    fn bbcode_de_base() {
        let (blocks, links) = parse_bbcode(
            "[h2]Nouveautés[/h2][p]Texte [b]gras[/b] et [i]italique[/i] [url=https://store.steampowered.com/app/1]ici[/url] [url=https://evil.example/]là[/url].[/p][list][*]Un[*]Deux [list][*]Trois[/list][/list][hr][/hr][p][PC] Correctif[/p]",
        );
        assert_eq!(links, vec!["https://store.steampowered.com/app/1".to_string()]);
        assert!(matches!(&blocks[0], Block::Heading { level: 2, spans } if text(spans) == "Nouveautés"));
        let Block::Paragraph { spans } = &blocks[1] else { panic!("{blocks:?}") };
        assert_eq!(text(spans), "Texte gras et italique ici là.");
        assert!(spans.iter().any(|s| s.bold && s.text == "gras"));
        assert!(spans.iter().any(|s| s.italic && s.text == "italique"));
        assert!(spans.iter().any(|s| s.link == Some(0) && s.text == "ici"));
        assert!(spans.iter().all(|s| s.text != "là" || s.link.is_none()));
        let Block::List { ordered: false, items } = &blocks[2] else { panic!("{blocks:?}") };
        assert_eq!(items.iter().map(|i| text(i)).collect::<Vec<_>>(), ["Un", "Deux", "Trois"]);
        assert_eq!(blocks[3], Block::Rule);
        // Crochets échappés : du texte, jamais une balise.
        let (escaped, _) = parse_bbcode(r"[p]\[ MAPS \] \[b]non[/b][/p]");
        assert!(matches!(&escaped[0], Block::Paragraph { spans } if text(spans) == "[ MAPS ] [b]non"), "{escaped:?}");
        // Barre oblique inverse échappée (appid 286160) : une seule affichée, même devant une balise ;
        // une barre seule devant autre chose reste telle quelle (`~` = barre oblique inverse ici).
        let slash = |s: &str| s.replace('~', "\u{5C}");
        let (slashes, _) = parse_bbcode(&slash("[p]C:~~Jeux~~Save ~~[b]gras[/b] a~b fin~[/p]"));
        let Block::Paragraph { spans } = &slashes[0] else { panic!("{slashes:?}") };
        assert_eq!(text(spans), slash("C:~Jeux~Save ~gras a~b fin[/p]"));
        assert!(spans.iter().any(|s| s.bold && s.text == "gras"), "{spans:?}");
        // Crochets inconnus : du texte.
        assert!(matches!(&blocks[4], Block::Paragraph { spans } if text(spans) == "[PC] Correctif"));
    }

    #[test]
    fn annonce_sans_balise_de_paragraphe() {
        let (blocks, _) = parse_bbcode("Premier paragraphe\nsuite.\n\nSecond [url]https://steamcommunity.com/app/1[/url]\n\n\n");
        assert_eq!(blocks.len(), 2);
        assert!(matches!(&blocks[0], Block::Paragraph { spans } if text(spans) == "Premier paragraphe\nsuite."));
        let Block::Paragraph { spans } = &blocks[1] else { panic!() };
        assert_eq!(spans.last().unwrap().link, Some(0));
        assert_eq!(spans.last().unwrap().text, "steamcommunity.com/app/1");
    }

    #[test]
    fn balises_cassees_toleres() {
        for src in ["[b]gras sans fin", "[url=https://store.steampowered.com/]ouvert", "[img]{STEAM_CLAN_IMAGE}/1/ab.png", "[list][*]a", "[/list][/b][/url]", "[", "[[[", "[h1]Titre", "a [img src=\"x\"] b"] {
            let (blocks, _) = parse_bbcode(src);
            let all = all_text(&blocks);
            assert!(!all.contains("[b]") && !all.contains("[url"), "{src} → {all}");
        }
        let (blocks, _) = parse_bbcode("[img]{STEAM_CLAN_IMAGE}/1/ab.png");
        assert_eq!(images(&blocks), ["https://clan.akamai.steamstatic.com/images/1/ab.png"]);
    }

    #[test]
    fn image_au_milieu_d_une_liste() {
        let (blocks, _) = parse_bbcode("[list][*]a[img]{STEAM_CLAN_IMAGE}/1/ab.png[/img][*]b[/list]");
        assert!(matches!(&blocks[0], Block::List { items, .. } if items.len() == 1));
        assert!(matches!(&blocks[1], Block::Image { .. }));
        assert!(matches!(&blocks[2], Block::List { items, .. } if items.len() == 1));
    }

    #[test]
    fn refus_des_images() {
        for bad in [
            "https://i.imgur.com/oLHogOc.png",
            "http://clan.akamai.steamstatic.com/images/1/ab.png",
            "https://clan.akamai.steamstatic.com/images/1/ab.png?x=1",
            "https://clan.akamai.steamstatic.com/images/1/../ab.png",
            "https://clan.akamai.steamstatic.com/images/1/ab.svg",
            "https://clan.akamai.steamstatic.com/images/a/ab.png",
            "https://clan.akamai.steamstatic.com/images/1/2/ab.png",
            "https://clan.akamai.steamstatic.com.evil.com/images/1/ab.png",
            "https://user@clan.akamai.steamstatic.com/images/1/ab.png",
            "https://clan.akamai.steamstatic.com:444/images/1/ab.png",
            "https://clan.akamai.steamstatic.com/images/1/zz.png",
            "javascript:alert(1)",
            "{STEAM_CLAN_IMAGE}/1/ab.png\" onerror=\"x",
        ] {
            assert_eq!(safe_image(bad), None, "{bad}");
        }
        assert_eq!(safe_image("{STEAM_CLAN_IMAGE}/3949769/98759BAB.png").as_deref(), Some("https://clan.akamai.steamstatic.com/images/3949769/98759BAB.png"));
        assert!(safe_image("https://clan.fastly.steamstatic.com/images/3381077/8e96d2f4.png").is_some());
    }

    #[test]
    fn refus_des_liens() {
        for bad in [
            "http://store.steampowered.com/app/1",
            "https://store.steampowered.com.evil.com/",
            "https://evil.com/?https://store.steampowered.com",
            "https://user@steamcommunity.com/",
            "https://steamcommunity.com:8080/",
            "https://steamcommunity.com/a b",
            "https://steamcommunity.com/\"x",
            "https://steamcommunity.com/../x",
            "steam://run/1",
            "file:///C:/Windows",
            "https://www.youtube.com/watch?v=1",
        ] {
            assert_eq!(safe_link(bad), None, "{bad}");
        }
        assert!(safe_link("https://store.steampowered.com/news/app/1/view/2").is_some());
        assert!(safe_link("HTTPS://SteamCommunity.com/games/1").is_some());
        assert_eq!(news_page(252950, "1844751498216898").as_deref(), Some("https://store.steampowered.com/news/app/252950/view/1844751498216898"));
        assert_eq!(news_page(1, "12/../3"), None);
    }

    #[test]
    fn reponse_filtree() {
        let body = r#"{"appnews":{"appid":5,"newsitems":[
            {"gid":"2","title":"B","contents":"[p]b[/p]","feedname":"steam_community_announcements","date":20,"appid":5,"tags":["patchnotes"]},
            {"gid":"1","title":"A","contents":"a","feedname":"steam_community_announcements","date":10,"appid":5},
            {"gid":"3","title":"Presse","contents":"x","feedname":"pcgamer","date":30,"appid":5},
            {"gid":"4","title":"Autre jeu","contents":"x","feedname":"steam_community_announcements","date":40,"appid":6},
            {"gid":"x5","title":"Id invalide","contents":"x","feedname":"steam_community_announcements","date":50,"appid":5},
            {"gid":"6","title":"  ","contents":"x","feedname":"steam_community_announcements","date":50,"appid":5}
        ]}}"#;
        let items = parse_response(body, 5).unwrap();
        assert_eq!(items.iter().map(|r| r.gid.as_str()).collect::<Vec<_>>(), ["2", "1"]);
        assert!(items[0].update && !items[1].update);
        assert!(parse_response("{}", 5).is_err());
        assert!(parse_response("pas du json", 5).is_err());
        assert!(parse_response(r#"{"appnews":{"appid":5,"newsitems":[]}}"#, 5).unwrap().is_empty());
    }

    fn raw(gid: &str, date: u64, contents: &str) -> Raw {
        Raw { gid: gid.into(), title: format!("Annonce {gid}"), date, author: String::new(), contents: contents.into(), update: false }
    }

    #[test]
    fn cache_garde_les_annonces_et_relit_apres_six_heures() {
        let dir = std::env::temp_dir().join(format!("ludex-news-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut cache = NewsCache::at(dir.join("news-cache.json"));
        let t = now();
        assert!(cache.stale(5, t));
        cache.record(5, vec![raw("1", t - 100, "[url=https://steamcommunity.com/x]lien[/url]")], t);
        assert!(!cache.stale(5, t + REFRESH_AFTER));
        assert!(cache.stale(5, t + 1 + REFRESH_AFTER));
        // Réponse vide : rien d'effacé.
        cache.record(5, Vec::new(), t);
        assert_eq!(cache.news(5).len(), 1);
        assert!(cache.known(5));
        cache.save().unwrap();
        let back = NewsCache::load(&dir);
        assert_eq!(back.news(5), cache.news(5));
        // Liens : seulement par numéro, depuis le cache.
        assert_eq!(back.target(5, "1", Some(0)).as_deref(), Some("https://steamcommunity.com/x"));
        assert_eq!(back.target(5, "1", Some(1)), None);
        assert_eq!(back.target(5, "9", None), None);
        assert_eq!(back.target(5, "1", None).as_deref(), Some("https://store.steampowered.com/news/app/5/view/1"));
        // Version changée : annonces oubliées.
        let mut old: serde_json::Value = serde_json::from_str(&fs::read_to_string(dir.join("news-cache.json")).unwrap()).unwrap();
        old["version"] = serde_json::json!(0);
        fs::write(dir.join("news-cache.json"), old.to_string()).unwrap();
        assert!(NewsCache::load(&dir).news(5).is_empty());
        // Jeu plus regardé depuis 60 jours : oublié.
        let mut old = NewsCache::at(dir.join("news-cache.json"));
        old.record(5, vec![raw("1", 1, "a")], t - FORGET_AFTER);
        old.save().unwrap();
        assert!(NewsCache::load(&dir).news(5).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn alertes_une_fois_et_seulement_recentes() {
        let day = 24 * 3600;
        let at = 100 * day;
        let mut cache = NewsCache::at(PathBuf::new());
        cache.record(1, vec![raw("11", at - day, "a"), raw("10", at - 10 * day, "b")], at);
        cache.record(2, vec![raw("20", at - 5 * day, "c")], at);
        let games = vec![("steam:1".to_string(), "Un".to_string(), 1), ("steam:2".to_string(), "Deux".to_string(), 2), ("steam:3".to_string(), "Trois".to_string(), 3)];
        let alerts = cache.alerts(&games, at);
        assert_eq!(alerts.len(), 1);
        assert_eq!((alerts[0].id.as_str(), alerts[0].gid.as_str()), ("steam:1", "11"));
        // Déjà signalée : plus rien.
        assert!(cache.alerts(&games, at).is_empty());
        // Nouvelle annonce : signalée.
        cache.record(1, vec![raw("12", at, "x"), raw("11", at - day, "a")], at);
        assert_eq!(cache.alerts(&games, at)[0].gid, "12");
        // Vue dans la fiche avant la vérification : pas signalée.
        cache.record(2, vec![raw("21", at, "y")], at);
        assert!(cache.mark_seen(2));
        assert!(cache.alerts(&games, at).is_empty());
        // Vérification de fond une fois par jour.
        assert!(cache.sweep_due(at));
        cache.swept(at);
        assert!(!cache.sweep_due(at + day - 1));
        assert!(cache.sweep_due(at + day));
    }

    #[test]
    fn extrait_coupe_a_un_mot() {
        let long = "mot ".repeat(200);
        let (blocks, _) = parse_bbcode(&format!("[h1]Titre[/h1][p]{long}[/p]"));
        let e = excerpt(&blocks, "Titre");
        assert!(e.ends_with('…') && e.len() <= EXCERPT + 4 && !e.contains("Titre"));
    }

    /// Annonces réelles des jeux de ce PC qui ont un appid (cache dans un dossier temporaire,
    /// jamais celui de l'app) : `cargo test news_this_pc -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn news_this_pc() {
        let started = std::time::Instant::now();
        let scan = crate::library::scan();
        let mut appids: Vec<(u32, String)> = scan.games.iter().filter_map(|g| g.steam_appid.map(|a| (a, g.title.clone()))).collect();
        appids.sort();
        appids.dedup_by_key(|(a, _)| *a);
        println!("{} jeux avec un appid (scan {:?})", appids.len(), started.elapsed());
        let dir = std::env::temp_dir().join("ludex-news-this-pc");
        let mut cache = NewsCache::load(&dir);
        let agent = agent();
        let (mut with, mut images, mut links, mut updates, mut blocks) = (0, 0, 0, 0, 0);
        let mut newest: Vec<(u64, String, String)> = Vec::new();
        for (i, (appid, title)) in appids.iter().take(40).enumerate() {
            if cache.stale(*appid, now()) {
                if i > 0 {
                    std::thread::sleep(Duration::from_millis(500));
                }
                let t = std::time::Instant::now();
                match fetch(&agent, *appid) {
                    Ok(items) => {
                        println!("{title} ({appid}) : {} annonces en {:?}", items.len(), t.elapsed());
                        cache.record(*appid, items, now());
                    }
                    Err(e) => {
                        println!("{title} ({appid}) : erreur {e}, arrêt");
                        break;
                    }
                }
            }
            let news = cache.news(*appid);
            if !news.is_empty() {
                with += 1;
            }
            for n in &news {
                images += images_count(&n.blocks);
                links += cache.apps[appid].items.iter().find(|r| r.gid == n.gid).map(|r| parse_bbcode(&r.contents).1.len()).unwrap_or(0);
                updates += usize::from(n.update);
                blocks += n.blocks.len();
            }
            if let Some(n) = news.first() {
                newest.push((n.date, title.clone(), n.title.clone()));
            }
        }
        cache.save().unwrap();
        // Annonces converties, pour les relire à la main.
        let parsed: BTreeMap<u32, Vec<News>> = appids.iter().map(|(a, _)| (*a, cache.news(*a))).filter(|(_, n)| !n.is_empty()).collect();
        fs::write(dir.join("annonces.json"), serde_json::to_vec_pretty(&parsed).unwrap()).unwrap();
        newest.sort_by_key(|n| std::cmp::Reverse(n.0));
        println!("{with} jeux avec des annonces ; {blocks} blocs, {images} images, {links} liens Steam, {updates} notes de mise à jour");
        println!("Annonces les plus récentes :");
        for (date, game, title) in newest.iter().take(10) {
            println!("  {} jours : {game} : {title}", now().saturating_sub(*date) / 86_400);
        }
        println!("Cache : {}, annonces converties : {} ({:?} au total)", dir.join("news-cache.json").display(), dir.join("annonces.json").display(), started.elapsed());
    }

    fn images_count(blocks: &[Block]) -> usize {
        images(blocks).len()
    }
}
