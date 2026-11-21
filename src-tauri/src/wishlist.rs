//! Liste de souhaits locale, sans compte Steam : des jeux de la boutique Steam que l'utilisateur
//! veut suivre, avec leur prix et une alerte quand une promo commence.
//!
//! - Liste gardée dans `user-data.json` (`wishlist` : appid, titre, date d'ajout, prix vu à
//!   l'ajout), donc dans l'export des données.
//! - Ajout : le front envoie un texte de recherche borné, Ludex interroge la recherche publique de
//!   la boutique (`storesearch`, comme `art.rs`) et garde les résultats ; le front renvoie ensuite
//!   un appid choisi parmi eux (jamais un titre ni une adresse).
//! - Prix : API publique `appdetails` avec `filters=price_overview` (seul filtre accepté pour
//!   plusieurs appids à la fois), par lots, `cc=fr`. Réponse par appid : `price_overview`
//!   (`currency`, `initial`, `final` en centimes, `discount_percent`), `data: []` (gratuit ou sans
//!   prix : pas encore en vente) ou `success: false` (retiré, ou pas vendu en France).
//! - Cache `wishlist-cache.json` (dossier de données) : dernier prix, plus bas vu, promo déjà
//!   signalée ; relu en fond au plus une fois par jour (`sweep_due`).
//! - Promo nouvelle : remise qui apparaît, ou prix encore plus bas qu'à la dernière alerte. Une
//!   promo déjà là à l'ajout n'est jamais signalée (l'utilisateur la voit).
//!
//! Images : `header.jpg` du CDN de Steam, adresse reconstruite depuis l'appid seul (elle répond
//! aussi pour les jeux récents aux images hachées) ; aucune adresse venue du réseau n'est gardée.

use crate::library::Game;
use crate::{AppState, diagnostic, lock};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_opener::OpenerExt;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

const SEARCH: &str = "https://store.steampowered.com/api/storesearch/";
const DETAILS: &str = "https://store.steampowered.com/api/appdetails";
const IMAGES: &str = "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps";

/// Jeux suivis au plus.
pub const MAX_WISHES: usize = 500;
/// Recherche tapée par l'utilisateur, en caractères.
pub const MAX_QUERY: usize = 100;
/// Résultats de recherche montrés.
const MAX_RESULTS: usize = 10;
const MAX_TITLE: usize = 120;
/// Réponse lue au plus (une recherche fait ~4 Ko, un lot de prix ~5 Ko).
const MAX_BODY: u64 = 2 * 1024 * 1024;
/// Appids par requête de prix, et pause entre deux lots.
pub const BATCH: usize = 25;
pub const BATCH_GAP: Duration = Duration::from_millis(1500);
/// Prix relus au plus une fois par jour.
const SWEEP_EVERY: u64 = 24 * 3600;
/// Jeux retirés gardés en mémoire pour « Annuler ».
const KEEP_REMOVED: usize = 20;
/// À augmenter si le contenu du cache change : tout est relu.
const CACHE_VERSION: u32 = 1;

pub fn now() -> u64 {
    crate::notify::now()
}

pub fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(10)).redirects(0).user_agent("Ludex (lanceur de jeux)").build()
}

/// Prix en centimes, dans une monnaie (code ISO de trois lettres).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Price {
    pub currency: String,
    /// Prix sans remise.
    pub initial: u32,
    /// Prix payé aujourd'hui.
    #[serde(rename = "final")]
    pub now: u32,
    /// Remise en %, 0 sans promo.
    pub discount: u8,
}

impl Price {
    /// Prix relu d'un fichier ou du réseau : monnaie en trois majuscules, remise entre 0 et 99,
    /// prix payé jamais au-dessus du prix d'origine.
    fn checked(currency: &str, initial: u64, now: u64, discount: Option<u64>) -> Option<Price> {
        let currency = currency.trim();
        if currency.len() != 3 || !currency.bytes().all(|b| b.is_ascii_uppercase()) {
            return None;
        }
        let initial = u32::try_from(initial).ok()?;
        let now = u32::try_from(now).ok()?;
        if now > initial || initial == 0 {
            return None;
        }
        // La recherche ne donne pas la remise : calculée (arrondie comme Steam l'affiche).
        let discount = match discount {
            Some(d) if d < 100 => d as u8,
            Some(_) => return None,
            None => (((initial - now) as u64 * 100 + initial as u64 / 2) / initial as u64).min(99) as u8,
        };
        Some(Price { currency: currency.to_string(), initial, now, discount })
    }

    pub fn on_sale(&self) -> bool {
        self.discount > 0 && self.now < self.initial
    }
}

/// Jeu suivi, tel que gardé dans `user-data.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wish {
    pub appid: u32,
    pub title: String,
    /// Date d'ajout, secondes Unix.
    pub added: u64,
    /// Prix vu au moment de l'ajout (`None` : gratuit ou sans prix).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price: Option<Price>,
}

/// Titre d'un jeu de la boutique, sur une ligne, `MAX_TITLE` caractères au plus.
fn clean_title(raw: &str) -> Option<String> {
    crate::library::manual::clean_text(raw, MAX_TITLE)
}

/// Liste relue (fichier modifié à la main, archive importée) : appids non nuls et sans doublon,
/// titres renettoyés, prix revérifiés, `MAX_WISHES` au plus.
pub fn clean(list: Vec<Wish>) -> Vec<Wish> {
    let mut out: Vec<Wish> = Vec::new();
    for w in list {
        if w.appid == 0 || out.iter().any(|k| k.appid == w.appid) || out.len() >= MAX_WISHES {
            continue;
        }
        let Some(title) = clean_title(&w.title) else { continue };
        let price = w.price.and_then(|p| Price::checked(&p.currency, p.initial.into(), p.now.into(), Some(p.discount.into())));
        out.push(Wish { appid: w.appid, title, added: w.added, price });
    }
    out
}

/// Relecture de `wishlist` dans `user-data.json` : une entrée illisible est ignorée sans perdre le
/// reste du fichier.
pub fn read_list<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Wish>, D::Error> {
    let raw = Vec::<serde_json::Value>::deserialize(d)?;
    Ok(clean(raw.into_iter().filter_map(|v| serde_json::from_value(v).ok()).collect()))
}

/// Fusion d'un import : ce PC garde ses entrées, celles de l'archive dont l'appid manque sont ajoutées.
pub fn merge(mine: &mut Vec<Wish>, theirs: Vec<Wish>) {
    for w in theirs {
        if mine.len() >= MAX_WISHES {
            break;
        }
        if !mine.iter().any(|k| k.appid == w.appid) {
            mine.push(w);
        }
    }
}

/// Adresse de l'image d'un jeu de la boutique, reconstruite depuis son appid.
pub fn image_url(appid: u32) -> String {
    format!("{IMAGES}/{appid}/header.jpg")
}

/// Texte de recherche : `MAX_QUERY` caractères, sur une ligne. Erreur : rien à chercher.
pub fn clean_query(raw: &str) -> Result<String, String> {
    crate::collections::check_input(raw)?;
    crate::library::manual::clean_text(raw, MAX_QUERY).ok_or_else(|| "Tape le titre d'un jeu à chercher.".to_string())
}

// ------------------------------------------------------------------------------------------------
// Recherche

/// Jeu trouvé par la recherche de la boutique.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub appid: u32,
    pub title: String,
    pub price: Option<Price>,
}

/// Résultats de `storesearch` : jeux seulement (`type` = `app`), titre nettoyé, prix vérifié.
pub fn parse_search(body: &str) -> Result<Vec<Hit>, String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|_| "Réponse illisible de la boutique Steam.".to_string())?;
    let Some(items) = value.get("items").and_then(|i| i.as_array()) else {
        return Err("Réponse illisible de la boutique Steam.".into());
    };
    let mut hits: Vec<Hit> = Vec::new();
    for item in items {
        if item.get("type").and_then(|t| t.as_str()) != Some("app") {
            continue;
        }
        let Some(appid) = item.get("id").and_then(|i| i.as_u64()).and_then(|i| u32::try_from(i).ok()).filter(|&i| i > 0) else { continue };
        let Some(title) = item.get("name").and_then(|n| n.as_str()).and_then(clean_title) else { continue };
        if hits.iter().any(|h| h.appid == appid) {
            continue;
        }
        let price = item.get("price").and_then(|p| {
            Price::checked(p.get("currency")?.as_str()?, p.get("initial")?.as_u64()?, p.get("final")?.as_u64()?, None)
        });
        hits.push(Hit { appid, title, price });
        if hits.len() >= MAX_RESULTS {
            break;
        }
    }
    Ok(hits)
}

fn get(request: ureq::Request) -> Result<String, String> {
    let response = request.call().map_err(|e| e.to_string())?;
    if response.status() != 200 {
        return Err(format!("Réponse {} de la boutique Steam.", response.status()));
    }
    let mut body = String::new();
    response.into_reader().take(MAX_BODY).read_to_string(&mut body).map_err(|e| e.to_string())?;
    Ok(body)
}

/// Cherche un titre dans la boutique Steam (aucun compte).
pub fn search(agent: &ureq::Agent, query: &str) -> Result<Vec<Hit>, String> {
    let request = agent.get(SEARCH).query("term", query).query("l", "french").query("cc", "FR");
    parse_search(&get(request)?)
}

// ------------------------------------------------------------------------------------------------
// Prix

/// Ce que la boutique dit d'un jeu.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Offer {
    /// En vente, avec son prix.
    Price { price: Price },
    /// Gratuit, ou pas encore en vente (aucun prix).
    NoPrice,
    /// Retiré de la vente, ou pas vendu en France.
    Unavailable,
}

/// Réponse de `appdetails` pour un lot : seuls les appids demandés sont gardés. Un appid absent
/// de la réponse n'y est pas (rien de connu).
pub fn parse_prices(body: &str, asked: &[u32]) -> Result<BTreeMap<u32, Offer>, String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|_| "Réponse illisible de la boutique Steam.".to_string())?;
    let Some(map) = value.as_object() else { return Err("Réponse illisible de la boutique Steam.".into()) };
    let mut out = BTreeMap::new();
    for &appid in asked {
        let Some(entry) = map.get(&appid.to_string()) else { continue };
        let offer = if entry.get("success").and_then(|s| s.as_bool()) != Some(true) {
            Offer::Unavailable
        } else {
            match entry.get("data").and_then(|d| d.get("price_overview")) {
                Some(p) => {
                    let price = Price::checked(
                        p.get("currency").and_then(|c| c.as_str()).unwrap_or(""),
                        p.get("initial").and_then(|v| v.as_u64()).unwrap_or(0),
                        p.get("final").and_then(|v| v.as_u64()).unwrap_or(u64::MAX),
                        Some(p.get("discount_percent").and_then(|v| v.as_u64()).unwrap_or(0)),
                    );
                    // Prix présent mais incohérent : on ne note rien plutôt qu'un faux prix.
                    let Some(price) = price else { continue };
                    Offer::Price { price }
                }
                None => Offer::NoPrice,
            }
        };
        out.insert(appid, offer);
    }
    Ok(out)
}

/// Prix d'un lot d'appids (`BATCH` au plus).
pub fn fetch_prices(agent: &ureq::Agent, appids: &[u32]) -> Result<BTreeMap<u32, Offer>, String> {
    let ids: Vec<String> = appids.iter().map(u32::to_string).collect();
    let request = agent.get(DETAILS).query("appids", &ids.join(",")).query("cc", "fr").query("filters", "price_overview");
    parse_prices(&get(request)?, appids)
}

// ------------------------------------------------------------------------------------------------
// Cache

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    /// Date du dernier prix connu.
    checked: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    offer: Option<Offer>,
    /// Plus bas prix vu par Ludex (même monnaie que le prix actuel).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lowest: Option<Price>,
    /// Prix payé de la dernière promo signalée (ou déjà là à l'ajout) ; `None` hors promo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    alerted: Option<u32>,
}

/// Promo à signaler.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sale {
    pub appid: u32,
    pub title: String,
    pub price: Price,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WishCache {
    #[serde(default)]
    version: u32,
    /// Dernière relecture complète des prix (secondes Unix).
    #[serde(default)]
    last_sweep: u64,
    #[serde(default)]
    items: BTreeMap<u32, Entry>,
    #[serde(skip)]
    path: PathBuf,
}

impl WishCache {
    pub fn load(dir: &Path) -> WishCache {
        let path = dir.join("wishlist-cache.json");
        let mut cache: WishCache = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        if cache.version != CACHE_VERSION {
            cache = WishCache { version: CACHE_VERSION, ..WishCache::default() };
        }
        cache.path = path;
        cache
    }

    /// Écrit dans un fichier temporaire puis le renomme.
    pub fn save(&self) -> Result<(), String> {
        if self.path.as_os_str().is_empty() {
            return Ok(());
        }
        let text = serde_json::to_string(self).map_err(|e| e.to_string())?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }

    /// Les prix n'ont pas été relus depuis un jour.
    pub fn sweep_due(&self, now: u64) -> bool {
        now.saturating_sub(self.last_sweep) >= SWEEP_EVERY
    }

    pub fn swept(&mut self, now: u64) {
        self.last_sweep = now;
    }

    pub fn last_sweep(&self) -> Option<u64> {
        (self.last_sweep > 0).then_some(self.last_sweep)
    }

    /// Note le prix d'un jeu ; renvoie la promo si elle est nouvelle et `alert` est vrai (relecture
    /// de fond). À l'ajout (`alert` faux), une promo en cours est notée comme déjà vue.
    pub fn record(&mut self, appid: u32, offer: Offer, at: u64, alert: bool) -> Option<Price> {
        let entry = self.items.entry(appid).or_default();
        entry.checked = at;
        let mut fresh = None;
        match &offer {
            Offer::Price { price } => {
                let lower = entry.lowest.as_ref().is_none_or(|l| l.currency != price.currency || price.now < l.now);
                if lower {
                    entry.lowest = Some(price.clone());
                }
                if price.on_sale() {
                    if entry.alerted.is_none_or(|a| price.now < a) {
                        entry.alerted = Some(price.now);
                        if alert {
                            fresh = Some(price.clone());
                        }
                    }
                } else {
                    entry.alerted = None;
                }
            }
            Offer::NoPrice | Offer::Unavailable => entry.alerted = None,
        }
        entry.offer = Some(offer);
        fresh
    }

    /// Oublie les jeux qui ne sont plus dans la liste.
    pub fn retain(&mut self, list: &[Wish]) {
        self.items.retain(|appid, _| list.iter().any(|w| w.appid == *appid));
    }
}

// ------------------------------------------------------------------------------------------------
// Vue pour le front

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WishView {
    pub appid: u32,
    pub title: String,
    pub added: u64,
    /// Prix vu à l'ajout.
    pub added_price: Option<Price>,
    /// Dernier prix connu (`None` : inconnu, gratuit ou indisponible, voir `offer`).
    pub price: Option<Price>,
    /// `price`, `noPrice`, `unavailable`, ou `unknown` (jamais relu).
    pub offer: &'static str,
    /// Plus bas prix vu, s'il est plus bas que le prix actuel.
    pub lowest: Option<Price>,
    pub checked: Option<u64>,
    pub image_url: String,
    /// Id du jeu dans la bibliothèque : « Déjà à toi ».
    pub owned: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Wishlist {
    pub items: Vec<WishView>,
    /// Dernière relecture complète des prix.
    pub checked: Option<u64>,
}

/// Jeu de la bibliothèque qui est cet appid Steam.
pub fn owned(games: &[Game], appid: u32) -> Option<String> {
    games.iter().find(|g| g.steam_appid == Some(appid) || g.id == format!("steam:{appid}")).map(|g| g.id.clone())
}

/// Liste affichée : promos d'abord (plus grosse remise), puis les plus récemment ajoutés.
pub fn view(list: &[Wish], cache: &WishCache, games: &[Game]) -> Wishlist {
    let mut items: Vec<WishView> = list
        .iter()
        .map(|w| {
            let entry = cache.items.get(&w.appid);
            let (price, offer) = match entry.and_then(|e| e.offer.as_ref()) {
                Some(Offer::Price { price }) => (Some(price.clone()), "price"),
                Some(Offer::NoPrice) => (None, "noPrice"),
                Some(Offer::Unavailable) => (None, "unavailable"),
                None => (None, "unknown"),
            };
            let lowest = entry
                .and_then(|e| e.lowest.clone())
                .filter(|l| price.as_ref().is_some_and(|p| p.currency == l.currency && l.now < p.now));
            WishView {
                appid: w.appid,
                title: w.title.clone(),
                added: w.added,
                added_price: w.price.clone(),
                price,
                offer,
                lowest,
                checked: entry.map(|e| e.checked).filter(|&c| c > 0),
                image_url: image_url(w.appid),
                owned: owned(games, w.appid),
            }
        })
        .collect();
    items.sort_by(|a, b| {
        let discount = |v: &WishView| v.price.as_ref().map_or(0, |p| if p.on_sale() { p.discount } else { 0 });
        discount(b).cmp(&discount(a)).then(b.added.cmp(&a.added)).then(a.title.cmp(&b.title))
    });
    Wishlist { items, checked: cache.last_sweep() }
}

/// Résultat de recherche envoyé au front.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HitView {
    pub appid: u32,
    pub title: String,
    pub price: Option<Price>,
    pub image_url: String,
    pub owned: Option<String>,
    /// Déjà dans la liste.
    pub wished: bool,
}

pub fn hit_views(hits: &[Hit], list: &[Wish], games: &[Game]) -> Vec<HitView> {
    hits.iter()
        .map(|h| HitView {
            appid: h.appid,
            title: h.title.clone(),
            price: h.price.clone(),
            image_url: image_url(h.appid),
            owned: owned(games, h.appid),
            wished: list.iter().any(|w| w.appid == h.appid),
        })
        .collect()
}

/// Ajout et retrait, sous le verrou de l'état : résultats de la dernière recherche et jeux retirés
/// récemment (pour « Annuler »), les seules sources d'un appid ajouté.
#[derive(Debug, Default)]
pub struct Picks {
    hits: Vec<Hit>,
    removed: Vec<Wish>,
}

impl Picks {
    pub fn set_hits(&mut self, hits: Vec<Hit>) {
        self.hits = hits;
    }

    /// Appid connu de la dernière recherche ou de la liste : sa page peut s'ouvrir.
    pub fn knows(&self, appid: u32) -> bool {
        self.hits.iter().any(|h| h.appid == appid)
    }

    /// Ajoute un jeu de la dernière recherche, ou remet un jeu retiré (même date d'ajout). Renvoie
    /// le prix de la recherche à noter dans le cache (`None` : jeu remis ou sans prix).
    pub fn add(&mut self, list: &mut Vec<Wish>, appid: u32, at: u64) -> Result<Option<Offer>, String> {
        if list.iter().any(|w| w.appid == appid) {
            return Ok(None);
        }
        if list.len() >= MAX_WISHES {
            return Err(format!("Ta liste de souhaits compte déjà {MAX_WISHES} jeux."));
        }
        if let Some(i) = self.removed.iter().position(|w| w.appid == appid) {
            list.push(self.removed.remove(i));
            return Ok(None);
        }
        let hit = self.hits.iter().find(|h| h.appid == appid).ok_or("Jeu introuvable : relance la recherche.")?;
        list.push(Wish { appid, title: hit.title.clone(), added: at, price: hit.price.clone() });
        Ok(Some(match &hit.price {
            Some(price) => Offer::Price { price: price.clone() },
            None => Offer::NoPrice,
        }))
    }

    /// Retire un jeu de la liste ; il reste en mémoire pour « Annuler ».
    pub fn remove(&mut self, list: &mut Vec<Wish>, appid: u32) -> bool {
        let Some(i) = list.iter().position(|w| w.appid == appid) else { return false };
        let wish = list.remove(i);
        self.removed.retain(|w| w.appid != appid);
        self.removed.push(wish);
        if self.removed.len() > KEEP_REMOVED {
            self.removed.remove(0);
        }
        true
    }
}

/// « 12,25 € », « 9,99 USD ».
pub fn money(cents: u32, currency: &str) -> String {
    let units = cents / 100;
    let digits = units.to_string();
    let mut grouped = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push('\u{a0}');
        }
        grouped.push(c);
    }
    let sign = if currency == "EUR" { "€" } else { currency };
    format!("{grouped},{:02}\u{a0}{sign}", cents % 100)
}

// ------------------------------------------------------------------------------------------------
// Commandes et relecture de fond

/// Événement `wishlist-changed` : liste à jour après une relecture des prix.
const CHANGED_EVENT: &str = "wishlist-changed";
/// Événement `wishlist-sale` : promos nouvelles, toast de l'appli quand aucune notification Windows
/// n'est partie.
const SALE_EVENT: &str = "wishlist-sale";

/// État de la liste de souhaits dans `AppState`.
#[derive(Default)]
pub struct State {
    cache: Mutex<WishCache>,
    picks: Mutex<Picks>,
    /// Une relecture des prix tourne.
    busy: AtomicBool,
    /// Promos à signaler quand la fenêtre principale sera de nouveau visible.
    pending: Mutex<Vec<Sale>>,
}

impl State {
    pub fn load(dir: &Path) -> State {
        State { cache: Mutex::new(WishCache::load(dir)), ..State::default() }
    }
}

/// Liste affichée (liste, cache, bibliothèque ; verrous pris l'un après l'autre).
fn current(state: &AppState) -> Result<Wishlist, String> {
    let list = lock(&state.data)?.wishlist.clone();
    let cache = lock(&state.wishlist.cache)?;
    let games = lock(&state.games)?;
    Ok(view(&list, &cache, &games))
}

/// Remplace la liste et l'enregistre ; rien de changé en mémoire si l'écriture échoue.
fn store_list(state: &AppState, change: impl FnOnce(&mut Vec<Wish>, &mut Picks) -> Result<Option<Offer>, String>) -> Result<Option<Offer>, String> {
    let mut data = lock(&state.data)?;
    let mut list = data.wishlist.clone();
    let offer = change(&mut list, &mut *lock(&state.wishlist.picks)?)?;
    if list != data.wishlist {
        let before = std::mem::replace(&mut data.wishlist, list);
        if let Err(e) = data.save(&state.data_path) {
            data.wishlist = before;
            return Err(e);
        }
    }
    Ok(offer)
}

/// Liste de souhaits, sans requête.
#[tauri::command]
pub fn wishlist(state: tauri::State<'_, AppState>) -> Result<Wishlist, String> {
    current(&state)
}

/// Cherche un titre dans la boutique Steam ; les résultats sont gardés côté Rust, seule source
/// des ajouts.
#[tauri::command]
pub async fn wishlist_search(app: AppHandle, text: String) -> Result<Vec<HitView>, String> {
    let query = clean_query(&text)?;
    crate::blocking(move || {
        let state = app.state::<AppState>();
        let hits = match search(&agent(), &query) {
            Ok(hits) => {
                diagnostic::ok(diagnostic::Service::Wishlist);
                hits
            }
            Err(e) => {
                diagnostic::failed(diagnostic::Service::Wishlist, &e);
                return Err("La boutique Steam ne répond pas. Réessaie dans un moment.".into());
            }
        };
        let list = lock(&state.data)?.wishlist.clone();
        let views = hit_views(&hits, &list, &lock(&state.games)?);
        lock(&state.wishlist.picks)?.set_hits(hits);
        Ok(views)
    })
    .await
}

/// Ajoute un jeu de la dernière recherche (ou remet un jeu retiré : « Annuler »).
#[tauri::command]
pub fn wishlist_add(state: tauri::State<'_, AppState>, appid: u32) -> Result<Wishlist, String> {
    let offer = store_list(&state, |list, picks| picks.add(list, appid, now()))?;
    // Prix de la recherche noté tout de suite ; une promo déjà là n'est jamais signalée.
    if let Some(offer) = offer {
        let mut cache = lock(&state.wishlist.cache)?;
        cache.record(appid, offer, now(), false);
        if let Err(e) = cache.save() {
            eprintln!("Cache de la liste de souhaits non enregistré : {e}");
        }
    }
    current(&state)
}

/// Retire un jeu de la liste (gardé en mémoire pour « Annuler »).
#[tauri::command]
pub fn wishlist_remove(state: tauri::State<'_, AppState>, appid: u32) -> Result<Wishlist, String> {
    store_list(&state, |list, picks| {
        picks.remove(list, appid);
        Ok(None)
    })?;
    current(&state)
}

/// Ouvre dans Steam la page d'un jeu de la liste ou de la dernière recherche : adresse
/// `steam://store/<appid>` reconstruite ici.
#[tauri::command]
pub fn open_store_page(app: AppHandle, state: tauri::State<'_, AppState>, appid: u32) -> Result<(), String> {
    let known = lock(&state.data)?.wishlist.iter().any(|w| w.appid == appid) || lock(&state.wishlist.picks)?.knows(appid);
    if !known {
        return Err("Jeu inconnu : relance la recherche.".into());
    }
    app.opener().open_url(format!("steam://store/{appid}"), None::<&str>).map_err(|e| format!("Impossible d'ouvrir Steam : {e}"))
}

/// Relecture de fond des prix, au plus une fois par jour (après un scan) : lots espacés, puis
/// `wishlist-changed`, et pour les promos nouvelles une notification Windows ou `wishlist-sale`.
pub fn resolve(app: AppHandle) {
    let Some(state) = app.try_state::<AppState>() else { return };
    let due = lock(&state.wishlist.cache).is_ok_and(|c| c.sweep_due(now()));
    let empty = lock(&state.data).is_ok_and(|d| d.wishlist.is_empty());
    if !due || empty || state.wishlist.busy.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        let Some(state) = app.try_state::<AppState>() else { return };
        sweep(&app, &state);
        state.wishlist.busy.store(false, Ordering::SeqCst);
    });
}

fn sweep(app: &AppHandle, state: &AppState) {
    let Ok(list) = lock(&state.data).map(|d| d.wishlist.clone()) else { return };
    let agent = agent();
    let mut sales: Vec<Sale> = Vec::new();
    let mut complete = true;
    for (n, chunk) in list.chunks(BATCH).enumerate() {
        if n > 0 {
            std::thread::sleep(BATCH_GAP);
        }
        let appids: Vec<u32> = chunk.iter().map(|w| w.appid).collect();
        match fetch_prices(&agent, &appids) {
            Ok(offers) => {
                diagnostic::ok(diagnostic::Service::Wishlist);
                let Ok(mut cache) = lock(&state.wishlist.cache) else { return };
                for (appid, offer) in offers {
                    if let Some(price) = cache.record(appid, offer, now(), true)
                        && let Some(w) = chunk.iter().find(|w| w.appid == appid)
                    {
                        sales.push(Sale { appid, title: w.title.clone(), price });
                    }
                }
            }
            Err(e) => {
                // Réessayée au prochain scan : la journée n'est pas comptée comme relue. Les
                // promos des lots déjà lus restent notées et signalées.
                diagnostic::failed(diagnostic::Service::Wishlist, &e);
                eprintln!("Relecture des prix de la liste de souhaits interrompue : {e}");
                complete = false;
                break;
            }
        }
    }
    if let Ok(mut cache) = lock(&state.wishlist.cache) {
        if complete {
            cache.swept(now());
            cache.retain(&list);
        }
        if let Err(e) = cache.save() {
            eprintln!("Cache de la liste de souhaits non enregistré : {e}");
        }
    }
    if let Ok(view) = current(state) {
        let _ = app.emit_to(crate::MAIN_WINDOW, CHANGED_EVENT, view);
    }
    if sales.is_empty() {
        return;
    }
    sales.sort_by_key(|a| std::cmp::Reverse(a.price.discount));
    if crate::notify::sale_note(&sales).is_some_and(|note| crate::notify_user(app, note)) {
        return;
    }
    if let Ok(mut pending) = lock(&state.wishlist.pending) {
        pending.extend(sales);
    }
    flush(app);
}

/// Envoie les promos en attente si la fenêtre principale est à l'écran ; sinon elles attendent le
/// retour sur la fenêtre.
pub fn flush<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<AppState>() else { return };
    let shown = app
        .get_webview_window(crate::MAIN_WINDOW)
        .is_some_and(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false));
    if !shown {
        return;
    }
    let sales = match lock(&state.wishlist.pending) {
        Ok(mut pending) if !pending.is_empty() => std::mem::take(&mut *pending),
        _ => return,
    };
    let _ = app.emit_to(crate::MAIN_WINDOW, SALE_EVENT, sales);
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRICES: &str = include_str!("wishlist_samples/prix.json");
    const SEARCH_BODY: &str = include_str!("wishlist_samples/recherche.json");
    const SEARCH_EMPTY: &str = include_str!("wishlist_samples/recherche-vide.json");

    fn price(currency: &str, initial: u32, now: u32, discount: u8) -> Price {
        Price { currency: currency.into(), initial, now, discount }
    }

    fn wish(appid: u32, title: &str, added: u64) -> Wish {
        Wish { appid, title: title.into(), added, price: None }
    }

    #[test]
    fn lit_les_prix_d_un_lot_enregistre() {
        let asked = [1426210, 2215430, 1145360, 730, 999999999, 42];
        let offers = parse_prices(PRICES, &asked).unwrap();
        assert_eq!(offers[&1426210], Offer::Price { price: price("EUR", 3999, 799, 80) });
        assert_eq!(offers[&2215430], Offer::Price { price: price("EUR", 5999, 2999, 50) });
        assert_eq!(offers[&1145360], Offer::Price { price: price("EUR", 2450, 2450, 0) });
        assert_eq!(offers[&730], Offer::NoPrice);
        assert_eq!(offers[&999999999], Offer::Unavailable);
        // Absent de la réponse : rien de connu.
        assert!(!offers.contains_key(&42));
        // Seuls les appids demandés sont gardés.
        assert_eq!(parse_prices(PRICES, &[730]).unwrap().len(), 1);
    }

    #[test]
    fn refuse_les_prix_incoherents() {
        let body = r#"{"1":{"success":true,"data":{"price_overview":{"currency":"eur","initial":100,"final":50,"discount_percent":50}}},
            "2":{"success":true,"data":{"price_overview":{"currency":"EUR","initial":100,"final":150,"discount_percent":0}}},
            "3":{"success":true,"data":{"price_overview":{"currency":"EUR","initial":100,"final":50,"discount_percent":100}}},
            "4":{"success":true,"data":{"price_overview":{"currency":"EUR","initial":0,"final":0,"discount_percent":0}}}}"#;
        assert!(parse_prices(body, &[1, 2, 3, 4]).unwrap().is_empty());
        assert!(parse_prices("<html>", &[1]).is_err());
        assert!(parse_prices("[]", &[1]).is_err());
    }

    #[test]
    fn lit_la_recherche_enregistree() {
        let hits = parse_search(SEARCH_BODY).unwrap();
        assert_eq!(hits[0].appid, 1426210);
        assert_eq!(hits[0].title, "It Takes Two");
        // Remise calculée depuis les deux prix (la recherche ne la donne pas).
        assert_eq!(hits[0].price, Some(price("EUR", 3999, 799, 80)));
        assert!(hits.len() <= MAX_RESULTS);
        assert!(parse_search(SEARCH_EMPTY).unwrap().is_empty());
        assert!(parse_search("pas du json").is_err());
    }

    #[test]
    fn recherche_jeux_seulement_et_titres_nettoyes() {
        let body = r#"{"items":[
            {"type":"sub","name":"Pack","id":5},
            {"type":"app","name":"  Jeu\u0000  avec\n espaces ","id":7},
            {"type":"app","name":"Doublon","id":7},
            {"type":"app","name":"Zéro","id":0},
            {"type":"app","name":"   ","id":8},
            {"type":"app","name":"Gratuit","id":9}]}"#;
        let hits = parse_search(body).unwrap();
        assert_eq!(hits.iter().map(|h| h.appid).collect::<Vec<_>>(), [7, 9]);
        assert_eq!(hits[0].title, "Jeu avec espaces");
        assert_eq!(hits[1].price, None);
    }

    #[test]
    fn texte_de_recherche_borne() {
        assert_eq!(clean_query("  hades \n ").unwrap(), "hades");
        assert!(clean_query("   ").is_err());
        assert_eq!(clean_query(&"a".repeat(300)).unwrap().chars().count(), MAX_QUERY);
        assert!(clean_query(&"a".repeat(5000)).is_err());
    }

    #[test]
    fn promo_signalee_une_fois() {
        let mut cache = WishCache::default();
        let full = Offer::Price { price: price("EUR", 2000, 2000, 0) };
        let half = Offer::Price { price: price("EUR", 2000, 1000, 50) };
        let quarter = Offer::Price { price: price("EUR", 2000, 500, 75) };
        assert_eq!(cache.record(1, full.clone(), 10, true), None);
        assert_eq!(cache.record(1, half.clone(), 20, true), Some(price("EUR", 2000, 1000, 50)));
        // Même promo le lendemain : rien.
        assert_eq!(cache.record(1, half.clone(), 30, true), None);
        // Encore plus bas : signalée.
        assert_eq!(cache.record(1, quarter.clone(), 40, true), Some(price("EUR", 2000, 500, 75)));
        // Fin de la promo, puis la même revient : signalée de nouveau.
        assert_eq!(cache.record(1, full, 50, true), None);
        assert_eq!(cache.record(1, half.clone(), 60, true), Some(price("EUR", 2000, 1000, 50)));
        // Plus bas vu gardé.
        assert_eq!(cache.items[&1].lowest, Some(price("EUR", 2000, 500, 75)));
        // Promo déjà là à l'ajout : jamais signalée.
        assert_eq!(cache.record(2, half.clone(), 10, false), None);
        assert_eq!(cache.record(2, half, 20, true), None);
        assert_eq!(cache.record(2, quarter, 30, true), Some(price("EUR", 2000, 500, 75)));
    }

    #[test]
    fn cache_relu_et_relecture_quotidienne() {
        let dir = std::env::temp_dir().join(format!("ludex-wishlist-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut cache = WishCache::load(&dir);
        assert!(cache.sweep_due(now()));
        cache.record(5, Offer::Price { price: price("EUR", 1000, 500, 50) }, 100, true);
        cache.swept(1_000_000);
        cache.save().unwrap();
        let back = WishCache::load(&dir);
        assert_eq!(back.items[&5].alerted, Some(500));
        assert!(!back.sweep_due(1_000_000 + 3600));
        assert!(back.sweep_due(1_000_000 + 24 * 3600));
        // Autre version : tout est relu.
        std::fs::write(dir.join("wishlist-cache.json"), r#"{"version":99,"lastSweep":5,"items":{}}"#).unwrap();
        assert_eq!(WishCache::load(&dir).last_sweep(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn liste_nettoyee_et_fusionnee() {
        let raw = r#"[{"appid":10,"title":" A ","added":5,"price":{"currency":"EUR","initial":100,"final":50,"discount":50}},
            {"appid":10,"title":"Doublon","added":6},
            {"appid":0,"title":"Zéro","added":1},
            {"appid":11,"title":"\u0000","added":1},
            {"appid":12,"title":"Prix faux","added":1,"price":{"currency":"€","initial":1,"final":2,"discount":0}},
            {"appid":"x"}]"#;
        let list = read_list(&mut serde_json::Deserializer::from_str(raw)).unwrap();
        assert_eq!(list.iter().map(|w| w.appid).collect::<Vec<_>>(), [10, 12]);
        assert_eq!(list[0].title, "A");
        assert_eq!(list[0].price, Some(price("EUR", 100, 50, 50)));
        assert_eq!(list[1].price, None);
        let mut mine = vec![wish(10, "Mien", 1)];
        merge(&mut mine, vec![wish(10, "Autre", 2), wish(20, "Nouveau", 3)]);
        assert_eq!(mine.iter().map(|w| (w.appid, w.title.as_str())).collect::<Vec<_>>(), [(10, "Mien"), (20, "Nouveau")]);
    }

    #[test]
    fn ajout_seulement_depuis_la_recherche_et_annuler() {
        let mut picks = Picks::default();
        let mut list = Vec::new();
        assert!(picks.add(&mut list, 1426210, 100).is_err());
        picks.set_hits(parse_search(SEARCH_BODY).unwrap());
        let offer = picks.add(&mut list, 1426210, 100).unwrap();
        assert_eq!(offer, Some(Offer::Price { price: price("EUR", 3999, 799, 80) }));
        assert_eq!(list[0].title, "It Takes Two");
        // Déjà là : rien.
        assert_eq!(picks.add(&mut list, 1426210, 200).unwrap(), None);
        assert_eq!(list.len(), 1);
        assert!(picks.remove(&mut list, 1426210));
        assert!(list.is_empty());
        // Nouvelle recherche sans ce jeu : « Annuler » le remet quand même, avec sa date d'ajout.
        picks.set_hits(Vec::new());
        assert_eq!(picks.add(&mut list, 1426210, 300).unwrap(), None);
        assert_eq!(list[0].added, 100);
        assert!(!picks.remove(&mut list, 42));
    }

    #[test]
    fn vue_promos_d_abord_et_deja_a_toi() {
        let list = vec![wish(1, "Ancien", 10), wish(2, "Récent", 30), wish(3, "En promo", 20), wish(4, "Inconnu", 5)];
        let mut cache = WishCache::default();
        cache.record(1, Offer::Price { price: price("EUR", 1000, 400, 60) }, 1, true);
        cache.record(1, Offer::Price { price: price("EUR", 1000, 1000, 0) }, 2, true);
        cache.record(2, Offer::NoPrice, 2, true);
        cache.record(3, Offer::Price { price: price("EUR", 2000, 1000, 50) }, 2, true);
        let mut owned_game = Game::new(crate::library::Platform::Steam, "2", "Récent", crate::library::Launch::Uri(String::new()));
        owned_game.steam_appid = Some(2);
        let games = vec![owned_game];
        let v = view(&list, &cache, &games);
        assert_eq!(v.items.iter().map(|i| i.appid).collect::<Vec<_>>(), [3, 2, 1, 4]);
        assert_eq!(v.items[1].owned.as_deref(), Some("steam:2"));
        assert_eq!(v.items[1].offer, "noPrice");
        // Plus bas vu montré seulement s'il est sous le prix actuel.
        assert_eq!(v.items[2].lowest, Some(price("EUR", 1000, 400, 60)));
        assert_eq!(v.items[0].lowest, None);
        assert_eq!(v.items[3].offer, "unknown");
        assert_eq!(v.items[0].image_url, "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/3/header.jpg");
        let hits = hit_views(&[Hit { appid: 3, title: "En promo".into(), price: None }], &list, &games);
        assert!(hits[0].wished && hits[0].owned.is_none());
    }

    #[test]
    fn montants_en_francais() {
        assert_eq!(money(799, "EUR"), "7,99\u{a0}€");
        assert_eq!(money(123456, "EUR"), "1\u{a0}234,56\u{a0}€");
        assert_eq!(money(5, "USD"), "0,05\u{a0}USD");
    }

    /// Relevé réel : recherche puis prix de quelques jeux (`--ignored`, réseau).
    #[test]
    #[ignore]
    fn wishlist_this_pc() {
        let agent = agent();
        let t = std::time::Instant::now();
        for query in ["hades", "it takes two", "cyberpunk", "zzqqxxrien"] {
            let hits = search(&agent, query).unwrap();
            println!("Recherche « {query} » : {} résultats en {:?}", hits.len(), t.elapsed());
            for h in hits.iter().take(4) {
                let p = h.price.as_ref().map_or("sans prix".to_string(), |p| format!("{} (-{} %)", money(p.now, &p.currency), p.discount));
                println!("  {} {} : {p}", h.appid, h.title);
            }
        }
        let featured: serde_json::Value = agent
            .get("https://store.steampowered.com/api/featuredcategories")
            .query("cc", "fr")
            .call()
            .ok()
            .and_then(|r| r.into_string().ok())
            .and_then(|b| serde_json::from_str(&b).ok())
            .unwrap_or_default();
        let mut appids: Vec<u32> = featured["specials"]["items"]
            .as_array()
            .map(|a| a.iter().filter_map(|i| i["id"].as_u64().map(|i| i as u32)).take(30).collect())
            .unwrap_or_default();
        appids.extend([1145360, 105600, 730, 1091500, 999999999]);
        appids.dedup();
        let mut cache = WishCache::default();
        for (n, chunk) in appids.chunks(BATCH).enumerate() {
            if n > 0 {
                std::thread::sleep(BATCH_GAP);
            }
            let t = std::time::Instant::now();
            let offers = fetch_prices(&agent, chunk).unwrap();
            println!("Lot de {} appids : {} réponses en {:?}", chunk.len(), offers.len(), t.elapsed());
            for (appid, offer) in offers {
                let sale = cache.record(appid, offer.clone(), now(), true);
                let text = match offer {
                    Offer::Price { price } => format!("{} au lieu de {} (-{} %)", money(price.now, &price.currency), money(price.initial, &price.currency), price.discount),
                    Offer::NoPrice => "gratuit ou sans prix".into(),
                    Offer::Unavailable => "indisponible".into(),
                };
                println!("  {appid} : {text}{}", if sale.is_some() { " : promo signalée" } else { "" });
            }
        }
    }
}
