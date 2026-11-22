//! Jeux gratuits de la semaine sur l'Epic Games Store, sans aucun compte.
//!
//! - Source : point d'entrée public de la boutique, celui que lit la page « Jeux gratuits »
//!   (`store-site-backend-static.ak.epicgames.com/freeGamesPromotions?locale=…&country=FR`), langue
//!   de Ludex, pays France. Réponse : `data.Catalog.searchStore.elements`, une offre par élément,
//!   avec `promotions.promotionalOffers` (en cours) et `promotions.upcomingPromotionalOffers` (à
//!   venir), chacune avec `startDate`, `endDate` (RFC 3339) et `discountSetting.discountPercentage`
//!   = part du prix payée (0 = gratuit, 70 = 30 % de remise).
//! - Gardé seulement ce qui est vraiment gratuit : promo à 0 % du prix d'un jeu payant
//!   (`originalPrice` > 0 ; un jeu toujours gratuit n'est pas une offre), prix payé à 0 pour une
//!   promo en cours, jeu de base, édition ou lot (`offerType`), jamais un contenu additionnel ; page
//!   de la boutique connue (`offerMappings` ou `catalogNs.mappings` de type `productHome`, sinon
//!   `productSlug`).
//! - Cache `epic-free-cache.json` (dossier de données) : offres vérifiées, date de lecture, langue,
//!   dernière fournée signalée. Relu au plus toutes les 6 h, ou 30 min après la fin d'une offre ou
//!   le début d'une offre à venir. En fond (après un scan, puis toutes les heures) seulement si la
//!   notification « Jeux gratuits sur Epic » est active ; à l'ouverture de la section Souhaits
//!   sinon.
//! - Images : seulement `https://cdn1.epicgames.com/<offer|spt-assets|…>/…` ou
//!   `https://cdn2.unrealengine.com/<fichier>`, sans requête ni `..` ; réduction du CDN ajoutée par
//!   Ludex (`?h=270&quality=medium&resize=1&w=480`, ~30 Ko).
//! - Page du jeu : `com.epicgames.launcher://store/p/<slug>` si le launcher Epic est installé
//!   (protocole déclaré dans le registre), sinon `https://store.epicgames.com/<langue>/p/<slug>`,
//!   reconstruite ici depuis l'offre du cache ; le front n'envoie que l'id de l'offre. Ludex ne
//!   récupère jamais un jeu à la place de l'utilisateur.

use crate::library::Game;
use crate::{diagnostic, lock};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_opener::OpenerExt;

const ENDPOINT: &str = "https://store-site-backend-static.ak.epicgames.com/freeGamesPromotions";
/// Réponse lue au plus (~50 Ko aujourd'hui).
const MAX_BODY: u64 = 4 * 1024 * 1024;
/// Relecture normale, et délai minimal après une échéance (fin ou début d'une offre).
const REFRESH_EVERY: u64 = 6 * 3600;
const AFTER_BOUNDARY: u64 = 30 * 60;
/// Vérification de fond (notification active) : toutes les heures, en plus des scans.
const TICK: Duration = Duration::from_secs(3600);
/// Offres gardées au plus.
const MAX_OFFERS: usize = 20;
const MAX_TITLE: usize = 120;
const MAX_SLUG: usize = 120;
/// À augmenter si le contenu du cache change : tout est relu.
const CACHE_VERSION: u32 = 1;
/// Réduction demandée au CDN d'Epic (vignette 480 × 270).
const IMAGE_SIZE: &str = "?h=270&quality=medium&resize=1&w=480";

/// Événement `epic-free-changed` : offres à jour après une relecture.
const CHANGED_EVENT: &str = "epic-free-changed";
/// Événement `epic-free-new` : nouvelle fournée, toast de l'appli quand aucune notification
/// Windows n'est partie.
const NEW_EVENT: &str = "epic-free-new";

pub fn now() -> u64 {
    crate::notify::now()
}

fn agent() -> ureq::Agent {
    crate::net::agent(Duration::from_secs(15))
}

/// Offre gratuite vérifiée, telle que gardée dans le cache.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    /// Id de l'offre dans la boutique (32 caractères hexadécimaux).
    pub id: String,
    pub title: String,
    /// Page de la boutique (`mark-of-the-deep-f7dc08`).
    pub slug: String,
    /// Début et fin de la gratuité, secondes Unix.
    pub start: u64,
    pub end: u64,
    /// Image large vérifiée, sans la réduction (ajoutée à l'affichage).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

impl Offer {
    pub fn current(&self, now: u64) -> bool {
        self.start <= now && now < self.end
    }

    pub fn upcoming(&self, now: u64) -> bool {
        self.start > now
    }

    /// Offre relue du cache (fichier modifié à la main) : mêmes règles qu'à la lecture.
    fn checked(self) -> Option<Offer> {
        let id = valid_id(&self.id)?.to_string();
        let title = crate::library::manual::clean_text(&self.title, MAX_TITLE)?;
        let slug = valid_slug(&self.slug)?.to_string();
        (self.start < self.end).then_some(())?;
        Some(Offer { id, title, slug, start: self.start, end: self.end, image: self.image.as_deref().and_then(safe_image) })
    }
}

/// Id d'offre : 32 caractères hexadécimaux minuscules.
fn valid_id(id: &str) -> Option<&str> {
    (id.len() == 32 && id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))).then_some(id)
}

/// Page de la boutique : lettres, chiffres et `-`, ni au début ni à la fin.
fn valid_slug(slug: &str) -> Option<&str> {
    let ok = !slug.is_empty()
        && slug.len() <= MAX_SLUG
        && slug.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        && !slug.starts_with('-')
        && !slug.ends_with('-');
    ok.then_some(slug)
}

/// Image d'Epic gardée seulement sur ses deux CDN, sans requête, sans `..`, caractères sûrs.
pub fn safe_image(url: &str) -> Option<String> {
    let safe = |part: &str| !part.is_empty() && part != "." && part != ".." && part.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'));
    if url.len() > 400 {
        return None;
    }
    if let Some(path) = url.strip_prefix("https://cdn1.epicgames.com/") {
        let parts: Vec<&str> = path.split('/').collect();
        return ((2..=6).contains(&parts.len()) && parts.iter().all(|p| safe(p))).then(|| url.to_string());
    }
    if let Some(name) = url.strip_prefix("https://cdn2.unrealengine.com/") {
        return safe(name).then(|| url.to_string());
    }
    None
}

/// Adresse affichée : l'image vérifiée avec la réduction du CDN.
pub fn image_url(image: &str) -> Option<String> {
    safe_image(image).map(|url| format!("{url}{IMAGE_SIZE}"))
}

/// Date RFC 3339 en secondes Unix.
fn timestamp(text: &str) -> Option<u64> {
    let date = chrono::DateTime::parse_from_rfc3339(text).ok()?;
    u64::try_from(date.timestamp()).ok()
}

/// Réponse illisible (forme inattendue).
fn unreadable() -> String {
    tr!("Réponse illisible de la boutique Epic.", "Unreadable response from the Epic store.")
}

/// Fenêtres gratuites (début, fin) d'une liste `promotionalOffers` : promo à 0 % du prix.
fn free_windows(groups: Option<&serde_json::Value>) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    for group in groups.and_then(|g| g.as_array()).into_iter().flatten() {
        for promo in group.get("promotionalOffers").and_then(|p| p.as_array()).into_iter().flatten() {
            let setting = promo.get("discountSetting");
            let percentage = setting.and_then(|s| s.get("discountPercentage")).and_then(|p| p.as_u64());
            let kind = setting.and_then(|s| s.get("discountType")).and_then(|t| t.as_str());
            if percentage != Some(0) || kind != Some("PERCENTAGE") {
                continue;
            }
            let date = |key: &str| promo.get(key).and_then(|d| d.as_str()).and_then(timestamp);
            if let (Some(start), Some(end)) = (date("startDate"), date("endDate"))
                && start < end
            {
                out.push((start, end));
            }
        }
    }
    out
}

/// Page `productHome` d'une liste de correspondances.
fn home_slug(mappings: Option<&serde_json::Value>) -> Option<String> {
    mappings?
        .as_array()?
        .iter()
        .find(|m| m.get("pageType").and_then(|t| t.as_str()) == Some("productHome"))
        .and_then(|m| m.get("pageSlug")?.as_str())
        .and_then(valid_slug)
        .map(str::to_string)
}

/// Offres gratuites en cours ou à venir à `now`, la fin la plus proche d'abord.
pub fn parse(body: &str, now: u64) -> Result<Vec<Offer>, String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|_| unreadable())?;
    let Some(elements) = value.pointer("/data/Catalog/searchStore/elements").and_then(|e| e.as_array()) else {
        return Err(unreadable());
    };
    let mut offers: Vec<Offer> = Vec::new();
    for element in elements {
        let text = |key: &str| element.get(key).and_then(|v| v.as_str());
        // Jeux, éditions et lots ; jamais un contenu additionnel (objets offerts d'un jeu gratuit).
        if !matches!(text("offerType"), Some("BASE_GAME" | "BUNDLE" | "EDITION")) {
            continue;
        }
        let Some(id) = text("id").and_then(valid_id) else { continue };
        let Some(title) = text("title").and_then(|t| crate::library::manual::clean_text(t, MAX_TITLE)) else { continue };
        let slug = home_slug(element.get("offerMappings"))
            .or_else(|| home_slug(element.pointer("/catalogNs/mappings")))
            .or_else(|| text("productSlug").map(|s| s.trim_end_matches("/home")).and_then(valid_slug).map(str::to_string));
        let Some(slug) = slug else { continue };
        let price = element.pointer("/price/totalPrice");
        let original = price.and_then(|p| p.get("originalPrice")).and_then(|p| p.as_u64()).unwrap_or(0);
        let paid = price.and_then(|p| p.get("discountPrice")).and_then(|p| p.as_u64());
        // Un jeu toujours gratuit (prix d'origine nul) n'est pas une offre de la semaine.
        if original == 0 {
            continue;
        }
        let image = element.get("keyImages").and_then(|k| k.as_array()).and_then(|images| {
            ["OfferImageWide", "DieselStoreFrontWide"].iter().find_map(|kind| {
                images
                    .iter()
                    .find(|i| i.get("type").and_then(|t| t.as_str()) == Some(kind))
                    .and_then(|i| i.get("url")?.as_str())
                    .and_then(safe_image)
            })
        });
        let promotions = element.get("promotions");
        let mut windows: Vec<(u64, u64)> = Vec::new();
        // En cours : le prix payé aujourd'hui doit aussi être nul.
        if paid == Some(0) {
            windows.extend(free_windows(promotions.and_then(|p| p.get("promotionalOffers"))).into_iter().filter(|&(s, e)| s <= now && now < e));
        }
        windows.extend(free_windows(promotions.and_then(|p| p.get("upcomingPromotionalOffers"))).into_iter().filter(|&(s, _)| s > now));
        for (start, end) in windows {
            if offers.iter().any(|o| o.id == id && o.start == start) {
                continue;
            }
            offers.push(Offer { id: id.to_string(), title: title.clone(), slug: slug.clone(), start, end, image: image.clone() });
        }
    }
    offers.sort_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)).then(a.title.cmp(&b.title)));
    offers.truncate(MAX_OFFERS);
    Ok(offers)
}

/// Langue de la boutique : celle de Ludex.
fn locale() -> &'static str {
    if crate::i18n::english() { "en-US" } else { "fr" }
}

/// Lit les offres gratuites de la boutique (aucun compte).
pub fn fetch(agent: &ureq::Agent, now: u64) -> Result<Vec<Offer>, String> {
    let response = agent
        .get(ENDPOINT)
        .query("locale", locale())
        .query("country", "FR")
        .query("allowCountries", "FR")
        .call()
        .map_err(|e| e.to_string())
        .and_then(|r| crate::net::checked(r, &[]))?;
    let mut body = String::new();
    response.into_reader().take(MAX_BODY).read_to_string(&mut body).map_err(|e| e.to_string())?;
    parse(&body, now)
}

// ------------------------------------------------------------------------------------------------
// Cache

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cache {
    #[serde(default)]
    version: u32,
    /// Dernière lecture réussie (secondes Unix), 0 = jamais.
    #[serde(default)]
    checked: u64,
    /// Langue de la dernière lecture.
    #[serde(default)]
    locale: String,
    #[serde(default)]
    offers: Vec<Offer>,
    /// Dernière fournée signalée (ids des jeux gratuits du moment, triés).
    #[serde(default)]
    notified: String,
    #[serde(skip)]
    path: PathBuf,
}

impl Cache {
    pub fn load(dir: &Path) -> Cache {
        let path = dir.join("epic-free-cache.json");
        let mut cache: Cache = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        if cache.version != CACHE_VERSION {
            cache = Cache { version: CACHE_VERSION, ..Cache::default() };
        }
        cache.offers = std::mem::take(&mut cache.offers).into_iter().filter_map(Offer::checked).take(MAX_OFFERS).collect();
        cache.path = path;
        cache
    }

    /// Écrit dans un fichier temporaire puis le renomme.
    fn save(&self) -> Result<(), String> {
        if self.path.as_os_str().is_empty() {
            return Ok(());
        }
        let text = serde_json::to_string(self).map_err(|e| e.to_string())?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }

    /// Faut-il relire ? Jamais lu, autre langue, plus de 6 h, ou une offre a commencé ou fini
    /// depuis la dernière lecture (30 min plus tôt au moins).
    pub fn due(&self, now: u64, locale: &str) -> bool {
        if self.checked == 0 || self.locale != locale {
            return true;
        }
        let age = now.saturating_sub(self.checked);
        let boundary = self.offers.iter().flat_map(|o| [o.start, o.end]).any(|t| t > self.checked && t <= now);
        age >= REFRESH_EVERY || (boundary && age >= AFTER_BOUNDARY)
    }

    fn store(&mut self, offers: Vec<Offer>, now: u64, locale: &str) {
        self.offers = offers;
        self.checked = now;
        self.locale = locale.to_string();
    }

    /// Jeux gratuits du moment.
    pub fn current(&self, now: u64) -> Vec<&Offer> {
        self.offers.iter().filter(|o| o.current(now)).collect()
    }

    /// Nouvelle fournée à signaler : jeux gratuits du moment pas encore à soi, dont la liste
    /// diffère de la dernière signalée. La fournée est notée tout de suite (jamais deux fois).
    pub fn fresh_batch(&mut self, now: u64, games: &[Game]) -> Option<Vec<Offer>> {
        let current: Vec<Offer> = self.current(now).into_iter().cloned().collect();
        let mut ids: Vec<&str> = current.iter().map(|o| o.id.as_str()).collect();
        ids.sort_unstable();
        let key = ids.join(",");
        if current.is_empty() || key == self.notified {
            return None;
        }
        self.notified = key;
        let new: Vec<Offer> = current.into_iter().filter(|o| owned(games, &o.title).is_none()).collect();
        (!new.is_empty()).then_some(new)
    }
}

// ------------------------------------------------------------------------------------------------
// Vue pour le front

/// Titre comparable : minuscules, lettres et chiffres, mention d'édition retirée.
fn comparable(title: &str) -> String {
    let compact: String = title.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect();
    crate::art::without_edition(&compact).to_string()
}

/// Jeu de la bibliothèque qui porte ce titre (aux symboles, casse et mention d'édition près).
pub fn owned(games: &[Game], title: &str) -> Option<String> {
    let wanted = comparable(title);
    if wanted.len() < 3 {
        return None;
    }
    games.iter().find(|g| !g.is_tool && comparable(&g.title) == wanted).map(|g| g.id.clone())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfferView {
    pub id: String,
    pub title: String,
    pub start: u64,
    pub end: u64,
    /// Gratuit maintenant (sinon : bientôt).
    pub current: bool,
    pub image_url: Option<String>,
    /// Id du jeu dans la bibliothèque : « Déjà à toi ».
    pub owned: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FreeGames {
    /// Gratuits maintenant (fin la plus proche d'abord), puis à venir.
    pub items: Vec<OfferView>,
    /// Dernière lecture réussie.
    pub checked: Option<u64>,
    /// Une lecture tourne.
    pub loading: bool,
}

pub fn view(cache: &Cache, games: &[Game], now: u64, loading: bool) -> FreeGames {
    let mut items: Vec<OfferView> = cache
        .offers
        .iter()
        .filter(|o| o.current(now) || o.upcoming(now))
        .map(|o| OfferView {
            id: o.id.clone(),
            title: o.title.clone(),
            start: o.start,
            end: o.end,
            current: o.current(now),
            image_url: o.image.as_deref().and_then(image_url),
            owned: owned(games, &o.title),
        })
        .collect();
    items.sort_by(|a, b| b.current.cmp(&a.current).then(if a.current { a.end.cmp(&b.end) } else { a.start.cmp(&b.start) }).then(a.title.cmp(&b.title)));
    FreeGames { items, checked: (cache.checked > 0).then_some(cache.checked), loading }
}

/// Fin de la gratuité en toutes lettres, date locale : « 26 novembre », « November 26 ».
pub fn until_text(end: u64) -> String {
    use chrono::{Datelike, TimeZone};
    let Some(date) = chrono::Local.timestamp_opt(end as i64, 0).single() else { return String::new() };
    day_month(date.day(), date.month())
}

fn day_month(day: u32, month: u32) -> String {
    const FR: [&str; 12] = ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"];
    const EN: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    let i = (month.clamp(1, 12) - 1) as usize;
    if crate::i18n::english() { format!("{} {day}", EN[i]) } else { format!("{day}{} {}", if day == 1 { "er" } else { "" }, FR[i]) }
}

// ------------------------------------------------------------------------------------------------
// État, commandes et relecture de fond

/// État géré par Tauri (`app.manage`).
#[derive(Default)]
pub struct State {
    cache: Mutex<Cache>,
    /// Une lecture tourne.
    busy: AtomicBool,
    /// Titres de la fournée à montrer quand la fenêtre principale sera de nouveau visible.
    pending: Mutex<Vec<String>>,
}

impl State {
    pub fn load(dir: &Path) -> State {
        State { cache: Mutex::new(Cache::load(dir)), ..State::default() }
    }
}

fn games_of<R: Runtime>(app: &AppHandle<R>) -> Vec<Game> {
    app.try_state::<crate::AppState>().and_then(|s| lock(&s.games).ok().map(|g| g.clone())).unwrap_or_default()
}

fn current_view<R: Runtime>(app: &AppHandle<R>, state: &State) -> Result<FreeGames, String> {
    let games = games_of(app);
    let cache = lock(&state.cache)?;
    Ok(view(&cache, &games, now(), state.busy.load(Ordering::SeqCst)))
}

fn notifications_on<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.try_state::<crate::AppState>().is_some_and(|s| lock(&s.data).is_ok_and(|d| d.settings.notifications.free_games))
}

/// Jeux gratuits d'Epic, depuis le cache ; une relecture part en fond si elle est due
/// (`epic-free-changed` à la fin).
#[tauri::command]
pub fn epic_free_games(app: AppHandle, state: tauri::State<'_, State>) -> Result<FreeGames, String> {
    refresh(&app);
    current_view(&app, &state)
}

/// Ouvre la page d'une offre du cache dans le launcher Epic (ou la boutique web) : adresse
/// reconstruite ici depuis l'id de l'offre.
#[tauri::command]
pub fn open_epic_free(app: AppHandle, state: tauri::State<'_, State>, id: String) -> Result<(), String> {
    let id = valid_id(&id).ok_or_else(|| tr!("Offre inconnue.", "Unknown offer."))?;
    let slug = lock(&state.cache)?
        .offers
        .iter()
        .find(|o| o.id == id)
        .and_then(|o| valid_slug(&o.slug).map(str::to_string))
        .ok_or_else(|| tr!("Offre introuvable : elle a peut-être pris fin.", "Offer not found: it may have ended."))?;
    let url = store_url(&slug, launcher_installed());
    app.opener().open_url(url, None::<&str>).map_err(|e| tr!("Impossible d'ouvrir la boutique Epic : {e}", "Can't open the Epic store: {e}"))
}

/// Page d'un jeu : dans le launcher s'il est là, sinon sur le site.
pub fn store_url(slug: &str, launcher: bool) -> String {
    if launcher {
        format!("com.epicgames.launcher://store/p/{slug}")
    } else {
        format!("https://store.epicgames.com/{}/p/{slug}", locale())
    }
}

/// Le launcher Epic déclare son protocole dans le registre.
#[cfg(windows)]
fn launcher_installed() -> bool {
    winreg::RegKey::predef(winreg::enums::HKEY_CLASSES_ROOT).open_subkey("com.epicgames.launcher").is_ok()
}

#[cfg(not(windows))]
fn launcher_installed() -> bool {
    false
}

/// Vérification de fond : toutes les heures (les scans en déclenchent aussi une), seulement si la
/// notification est active.
pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(TICK);
            resolve(app.clone());
        }
    });
}

/// Après un scan ou au réveil du minuteur : relecture si la notification est active.
pub fn resolve(app: AppHandle) {
    if notifications_on(&app) {
        refresh(&app);
    }
}

/// Relit les offres si c'est dû, dans un fil.
fn refresh(app: &AppHandle) {
    let Some(state) = app.try_state::<State>() else { return };
    let due = lock(&state.cache).is_ok_and(|c| c.due(now(), locale()));
    if !due || state.busy.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let Some(state) = app.try_state::<State>() else { return };
        read(&state);
        state.busy.store(false, Ordering::SeqCst);
        if let Ok(view) = current_view(&app, &state) {
            let _ = app.emit_to(crate::MAIN_WINDOW, CHANGED_EVENT, view);
        }
        announce(&app, &state);
    });
}

fn read(state: &State) {
    let locale = locale();
    match fetch(&agent(), now()) {
        Ok(offers) => {
            diagnostic::ok(diagnostic::Service::EpicFree);
            if let Ok(mut cache) = lock(&state.cache) {
                cache.store(offers, now(), locale);
                if let Err(e) = cache.save() {
                    log::warn!("Cache des jeux gratuits d'Epic non enregistré : {e}");
                }
            }
        }
        Err(e) => {
            diagnostic::failed(diagnostic::Service::EpicFree, &e);
            log::warn!("Jeux gratuits d'Epic non relus : {e}");
        }
    }
}

/// Nouvelle fournée : notification Windows, sinon toast de l'appli (fenêtre à l'écran), sinon en
/// attente du retour sur la fenêtre. Rien si la notification est coupée.
fn announce(app: &AppHandle, state: &State) {
    if !notifications_on(app) {
        return;
    }
    let games = games_of(app);
    let batch = {
        let Ok(mut cache) = lock(&state.cache) else { return };
        let batch = cache.fresh_batch(now(), &games);
        if batch.is_some()
            && let Err(e) = cache.save()
        {
            log::warn!("Cache des jeux gratuits d'Epic non enregistré : {e}");
        }
        batch
    };
    let Some(batch) = batch else { return };
    let until = batch.iter().map(|o| o.end).min().map(until_text).unwrap_or_default();
    if crate::notify::free_games_note(&batch, &until).is_some_and(|note| crate::notify_user(app, note)) {
        return;
    }
    if let Ok(mut pending) = lock(&state.pending) {
        *pending = batch.into_iter().map(|o| o.title).collect();
    }
    flush(app);
}

/// Envoie la fournée en attente si la fenêtre principale est à l'écran.
pub fn flush<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<State>() else { return };
    let shown = app
        .get_webview_window(crate::MAIN_WINDOW)
        .is_some_and(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false));
    if !shown {
        return;
    }
    let titles = match lock(&state.pending) {
        Ok(mut pending) if !pending.is_empty() => std::mem::take(&mut *pending),
        _ => return,
    };
    let _ = app.emit_to(crate::MAIN_WINDOW, NEW_EVENT, titles);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{Launch, Platform};

    const FR: &str = include_str!("epic_free_samples/gratuits-fr.json");
    const EN: &str = include_str!("epic_free_samples/gratuits-en.json");

    /// 22 novembre 2026, 16 h 47 UTC : heure du relevé enregistré.
    const NOW: u64 = 1_795_366_020;

    fn at(text: &str) -> u64 {
        timestamp(text).unwrap()
    }

    fn offer(id: char, title: &str, start: u64, end: u64) -> Offer {
        Offer { id: id.to_string().repeat(32), title: title.into(), slug: "jeu".into(), start, end, image: None }
    }

    fn game(platform: Platform, id: &str, title: &str) -> Game {
        Game::new(platform, id, title, Launch::Uri(String::new()))
    }

    #[test]
    fn lit_la_reponse_enregistree() {
        let offers = parse(FR, NOW).unwrap();
        let titles: Vec<&str> = offers.iter().map(|o| o.title.as_str()).collect();
        // Mark of the Deep gratuit maintenant ; Bread and Fred à venir. Firestone (toujours
        // gratuit), les promos à -30 % ou -80 % et le contenu additionnel sont écartés.
        assert_eq!(titles, ["Mark of the Deep", "Bread and Fred"]);
        let mark = &offers[0];
        assert_eq!(mark.id, "1f9a12b06b4f437790b6913ad136a08c");
        assert_eq!(mark.slug, "mark-of-the-deep-f7dc08");
        assert_eq!((mark.start, mark.end), (at("2026-11-19T16:00:00.000Z"), at("2026-11-26T16:00:00.000Z")));
        assert!(mark.current(NOW));
        assert_eq!(mark.image.as_deref(), Some("https://cdn1.epicgames.com/spt-assets/83a240506a884c10ad8fb99694e57a0d/mark-of-the-deep-w1ygr.png"));
        let bread = &offers[1];
        assert_eq!(bread.slug, "bread-and-fred-1db362");
        assert!(bread.upcoming(NOW));
        assert_eq!((bread.start, bread.end), (at("2026-11-26T16:00:00.000Z"), at("2026-12-03T16:00:00.000Z")));
        // Même chose dans la réponse en anglais.
        let en: Vec<String> = parse(EN, NOW).unwrap().into_iter().map(|o| o.title).collect();
        assert_eq!(en, ["Mark of the Deep", "Bread and Fred"]);
    }

    #[test]
    fn le_temps_qui_passe() {
        // Après la fin de Mark of the Deep, l'ancienne réponse ne dit pas que Bread and Fred est
        // gratuit (promo seulement « à venir », prix payé non nul) : rien, d'où la relecture.
        let later = at("2026-11-27T10:00:00.000Z");
        assert!(parse(FR, later).unwrap().is_empty());
        // Avant le début de la promo de Mark of the Deep, elle n'est pas encore « en cours ».
        let before = at("2026-11-19T15:00:00.000Z");
        let titles: Vec<String> = parse(FR, before).unwrap().into_iter().map(|o| o.title).collect();
        assert_eq!(titles, ["Bread and Fred"]);
    }

    #[test]
    fn refuse_ce_qui_n_est_pas_vraiment_gratuit() {
        let element = |kind: &str, original: u64, paid: u64, percent: u64, slug: &str, id: &str| {
            format!(
                r#"{{"title":"Jeu","id":"{id}","offerType":"{kind}","productSlug":"{slug}","price":{{"totalPrice":{{"discountPrice":{paid},"originalPrice":{original}}}}},
                "promotions":{{"promotionalOffers":[{{"promotionalOffers":[{{"startDate":"2026-11-19T16:00:00.000Z","endDate":"2026-11-26T16:00:00.000Z","discountSetting":{{"discountType":"PERCENTAGE","discountPercentage":{percent}}}}}]}}],"upcomingPromotionalOffers":[]}}}}"#
            )
        };
        let body = |e: String| format!(r#"{{"data":{{"Catalog":{{"searchStore":{{"elements":[{e}]}}}}}}}}"#);
        let id = "a".repeat(32);
        assert_eq!(parse(&body(element("BASE_GAME", 1999, 0, 0, "jeu", &id)), NOW).unwrap().len(), 1, "le cas qui passe");
        assert_eq!(parse(&body(element("BUNDLE", 1999, 0, 0, "lot/home", &id)), NOW).unwrap()[0].slug, "lot");
        assert!(parse(&body(element("ADD_ON", 1999, 0, 0, "jeu", &id)), NOW).unwrap().is_empty(), "contenu additionnel");
        assert!(parse(&body(element("BASE_GAME", 0, 0, 0, "jeu", &id)), NOW).unwrap().is_empty(), "toujours gratuit");
        assert!(parse(&body(element("BASE_GAME", 1999, 599, 30, "jeu", &id)), NOW).unwrap().is_empty(), "simple promo");
        assert!(parse(&body(element("BASE_GAME", 1999, 599, 0, "jeu", &id)), NOW).unwrap().is_empty(), "prix payé non nul");
        assert!(parse(&body(element("BASE_GAME", 1999, 0, 0, "../piege", &id)), NOW).unwrap().is_empty(), "page piégée");
        assert!(parse(&body(element("BASE_GAME", 1999, 0, 0, "jeu", "pas-un-id")), NOW).unwrap().is_empty(), "id invalide");
        assert!(parse("<html>", NOW).is_err());
        assert!(parse(r#"{"data":{}}"#, NOW).is_err());
    }

    #[test]
    fn images_d_epic_seulement() {
        let ok = "https://cdn1.epicgames.com/offer/708f57aaa04b42ef885be16c8288f0ac/EGS_Ghostrunner2_S1_2560x1440-1584";
        assert_eq!(safe_image(ok).as_deref(), Some(ok));
        assert_eq!(image_url(ok).unwrap(), format!("{ok}?h=270&quality=medium&resize=1&w=480"));
        assert!(safe_image("https://cdn2.unrealengine.com/egs-lisa-s1-2560x1440-5be18c558828.jpg").is_some());
        for bad in [
            "http://cdn1.epicgames.com/offer/a/b.png",
            "https://cdn1.epicgames.com/offer/a/b.png?x=1",
            "https://cdn1.epicgames.com/offer/../b.png",
            "https://cdn1.epicgames.com/b.png",
            "https://cdn1.epicgames.com.evil.example/offer/a/b.png",
            "https://cdn2.unrealengine.com/a/b.png",
            "https://media-cdn.epicgames.com/a/b.png",
            "https://cdn1.epicgames.com/offer//b.png",
        ] {
            assert!(safe_image(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn adresse_de_la_page() {
        assert_eq!(store_url("mark-of-the-deep-f7dc08", true), "com.epicgames.launcher://store/p/mark-of-the-deep-f7dc08");
        crate::i18n::with(crate::i18n::Lang::Fr, || assert_eq!(store_url("jeu", false), "https://store.epicgames.com/fr/p/jeu"));
        crate::i18n::with(crate::i18n::Lang::En, || assert_eq!(store_url("jeu", false), "https://store.epicgames.com/en-US/p/jeu"));
        for bad in ["", "-a", "a-", "a/b", "a b", "a?b", "é"] {
            assert!(valid_slug(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn relecture_quelques_fois_par_jour() {
        let mut cache = Cache::default();
        assert!(cache.due(NOW, "fr"), "jamais lu");
        let mark = offer('a', "Mark of the Deep", at("2026-11-19T16:00:00Z"), at("2026-11-26T16:00:00Z"));
        cache.store(vec![mark.clone()], NOW, "fr");
        assert!(!cache.due(NOW + 3600, "fr"));
        assert!(cache.due(NOW + 3600, "en-US"), "autre langue");
        assert!(cache.due(NOW + REFRESH_EVERY, "fr"));
        // Fin de l'offre : relue 30 min après la lecture au plus tôt.
        cache.store(vec![mark.clone()], mark.end - 600, "fr");
        assert!(!cache.due(mark.end + 60, "fr"), "trop tôt après la lecture");
        assert!(cache.due(mark.end - 600 + AFTER_BOUNDARY, "fr"));
    }

    #[test]
    fn une_notification_par_fournee() {
        let mut cache = Cache::default();
        let a = offer('a', "Mark of the Deep", NOW - 100, NOW + 100);
        let b = offer('b', "Bread and Fred", NOW + 100, NOW + 900);
        cache.store(vec![a.clone(), b.clone()], NOW, "fr");
        let batch = cache.fresh_batch(NOW, &[]).unwrap();
        assert_eq!(batch, vec![a.clone()]);
        assert!(cache.fresh_batch(NOW + 50, &[]).is_none(), "même fournée");
        // La semaine suivante : nouvelle fournée.
        assert_eq!(cache.fresh_batch(NOW + 200, &[]).unwrap(), vec![b.clone()]);
        // Rien de gratuit : rien.
        assert!(cache.fresh_batch(NOW + 1000, &[]).is_none());
        // Jeu déjà à soi : fournée notée mais rien à signaler.
        let mut cache = Cache::default();
        cache.store(vec![a.clone()], NOW, "fr");
        let games = [game(Platform::Epic, "Sugar", "Mark of the Deep™")];
        assert!(cache.fresh_batch(NOW, &games).is_none());
        assert!(cache.fresh_batch(NOW, &[]).is_none(), "fournée déjà notée");
    }

    #[test]
    fn deja_a_toi_et_ordre() {
        let games = [game(Platform::Steam, "1", "Bread & Fred"), game(Platform::Epic, "x", "Ghostrunner 2 Deluxe Edition")];
        assert_eq!(owned(&games, "Bread and Fred"), None, "« & » et « and » différents : prudence");
        assert_eq!(owned(&games, "Ghostrunner 2").as_deref(), Some("epic:x"));
        assert_eq!(owned(&games, "Ghostrunner"), None);
        let mut cache = Cache::default();
        cache.store(
            vec![
                offer('c', "Plus tard", NOW + 500, NOW + 900),
                offer('a', "Finit tard", NOW - 10, NOW + 400),
                offer('b', "Finit tôt", NOW - 10, NOW + 200),
                offer('d', "Fini", NOW - 900, NOW - 10),
            ],
            NOW,
            "fr",
        );
        let v = view(&cache, &games, NOW, false);
        let titles: Vec<&str> = v.items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles, ["Finit tôt", "Finit tard", "Plus tard"]);
        assert!(v.items[0].current && !v.items[2].current);
        assert_eq!(v.checked, Some(NOW));
    }

    #[test]
    fn cache_relu_et_reverifie() {
        let dir = std::env::temp_dir().join(format!("ludex-epic-free-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut cache = Cache::load(&dir);
        let mut good = offer('a', "Mark of the Deep", NOW, NOW + 10);
        good.image = Some("https://cdn1.epicgames.com/spt-assets/83a2/mark.png".into());
        let mut bad = offer('b', "Piège", NOW, NOW + 10);
        bad.slug = "../x".into();
        let mut odd_image = offer('c', "Image", NOW, NOW + 10);
        odd_image.image = Some("https://evil.example/a.png".into());
        cache.store(vec![good.clone(), bad, odd_image], NOW, "fr");
        cache.save().unwrap();
        let again = Cache::load(&dir);
        assert_eq!(again.offers.len(), 2);
        assert_eq!(again.offers[0], good);
        assert_eq!(again.offers[1].image, None);
        assert_eq!(again.checked, NOW);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn textes_de_la_notification() {
        crate::i18n::with(crate::i18n::Lang::Fr, || {
            assert_eq!(day_month(26, 11), "26 novembre");
            assert_eq!(day_month(1, 12), "1er décembre");
            let one = crate::notify::free_games_note(&[offer('a', "Mark of the Deep", 0, 1)], "26 novembre").unwrap();
            assert_eq!(one.title, "Gratuit sur Epic : Mark of the Deep");
            assert_eq!(one.body, "À récupérer jusqu'au 26 novembre.");
            assert_eq!(one.target, crate::notify::Target::Wishlist);
            assert_eq!(one.kind, crate::notify::Kind::FreeGames);
            let many = crate::notify::free_games_note(
                &[offer('b', "B", 0, 1), offer('a', "A", 0, 1), offer('c', "C", 0, 1), offer('d', "D", 0, 1)],
                "26 novembre",
            )
            .unwrap();
            assert_eq!(many.title, "4 jeux gratuits sur Epic");
            assert_eq!(many.body, "B, A, C et 1 autre. À récupérer jusqu'au 26 novembre.");
            assert!(many.key.starts_with(&format!("freeGames:{}", "a".repeat(32))), "ids triés");
            assert!(crate::notify::free_games_note(&[], "").is_none());
        });
        crate::i18n::with(crate::i18n::Lang::En, || {
            assert_eq!(day_month(26, 11), "November 26");
            let one = crate::notify::free_games_note(&[offer('a', "Mark of the Deep", 0, 1)], "November 26").unwrap();
            assert_eq!((one.title.as_str(), one.body.as_str()), ("Free on Epic: Mark of the Deep", "Claim it until November 26."));
        });
    }

    /// Relevé réel : `cargo test epic_free_this_pc -- --ignored --nocapture` (rien d'écrit).
    #[test]
    #[ignore]
    fn epic_free_this_pc() {
        let started = std::time::Instant::now();
        let offers = fetch(&agent(), now()).expect("boutique Epic");
        println!("{} offres en {:?} (launcher Epic : {})", offers.len(), started.elapsed(), launcher_installed());
        for o in &offers {
            let state = if o.current(now()) { "gratuit jusqu'au" } else { "bientôt, jusqu'au" };
            println!("- {} ({}) {state} {} : {}", o.title, o.slug, until_text(o.end), o.image.as_deref().and_then(image_url).unwrap_or_default());
        }
    }
}
