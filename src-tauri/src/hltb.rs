//! Durée de vie des jeux (HowLongToBeat) : temps moyen pour finir l'histoire, l'histoire avec les
//! extras, et tout à 100 %.
//!
//! HowLongToBeat n'a pas d'API publique : Ludex fait comme la recherche du site, avec un
//! User-Agent qui dit qui il est et sans compte. D'abord `GET <point>/init?t=<ms>`, qui donne un
//! jeton, puis `POST <point>` (JSON, en-tête `x-auth-token`). Le point d'entrée (aujourd'hui
//! `/api/search/site`) change de temps en temps : s'il répond 404, il est relu une fois dans les
//! scripts de la page d'accueil (`/api/…/init?t=`) et gardé dans le cache. Un jeton refusé (403)
//! est redemandé une fois. Toute autre erreur (réseau, 429, réponse illisible) arrête la lecture :
//! les jeux restants sont simplement « inconnus » jusqu'au scan suivant.
//!
//! La correspondance est stricte (titre égal au nom ou à un alias du site, aux symboles, accents
//! et mentions d'édition près, ou faute de frappe sans changer les nombres) : mieux vaut aucune
//! durée qu'une durée d'un autre jeu. L'année de sortie (`Game.details`) départage les homonymes
//! (« Resident Evil 4 » de 2005 et de 2023).
//!
//! Les réponses sont gardées dans `hltb-cache.json` (dossier de données de l'app) : un titre trouvé
//! est redemandé au bout de 30 jours (les moyennes bougent), un titre introuvable aussi.

use crate::art::without_edition;
use crate::library::Game;
use chrono::Datelike;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const REFRESH_AFTER: u64 = 30 * 24 * 3600;
/// À augmenter quand les règles de correspondance ou le contenu du cache changent : tous les
/// titres sont alors redemandés.
const CACHE_VERSION: u32 = 1;
const SITE: &str = "https://howlongtobeat.com";
/// Point d'entrée de la recherche connu à ce jour.
const DEFAULT_ENDPOINT: &str = "/api/search/site";
/// Temps minimal entre deux requêtes au site.
const PAUSE: Duration = Duration::from_millis(1200);
/// Écart maximal entre l'année de sortie connue et celle du site. Au-delà, c'est un autre jeu du
/// même nom (« Call of Duty® » de 2022 n'est pas « Call of Duty » de 2003) ; en deçà, une sortie
/// tardive sur Steam (« Resident Evil 4 » : 2005, sur Steam en 2014).
const MAX_YEAR_GAP: i32 = 10;
/// Résultats lus par recherche (le site trie par popularité).
const RESULTS: u32 = 20;
/// Scripts lus au plus pour retrouver le point d'entrée.
const MAX_SCRIPTS: usize = 20;

/// Durées moyennes en minutes (`None` : pas assez de joueurs l'ont noté), envoyées au front.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hltb {
    /// Nom du jeu sur HowLongToBeat (affiché au survol).
    pub name: String,
    /// Histoire principale.
    #[serde(default)]
    pub main: Option<u32>,
    /// Histoire et extras.
    #[serde(default)]
    pub plus: Option<u32>,
    /// Tout à 100 %.
    #[serde(default)]
    pub full: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Entry {
    /// Date de la requête (secondes Unix).
    checked: u64,
    /// `None` : introuvable, ou trouvé sans aucune durée.
    #[serde(default)]
    times: Option<Hltb>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct HltbCache {
    #[serde(default)]
    version: u32,
    /// Point d'entrée de la recherche retrouvé dans les scripts du site, s'il a changé.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    endpoint: Option<String>,
    /// Clé : titre comparable, suivi de « @<année> » quand l'année est connue.
    #[serde(default)]
    titles: BTreeMap<String, Entry>,
    #[serde(skip)]
    path: PathBuf,
}

/// Un titre à chercher.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    key: String,
    title: String,
    year: Option<i32>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Lettre sans accent (lettres latines courantes).
fn fold(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        _ => c,
    }
}

/// Titre comparable : minuscules sans accents, « & » lu « and », lettres et chiffres seulement
/// (« Pokémon™ Sword & Shield » → « pokemonswordandshield »), mention d'édition retirée.
fn compact(title: &str) -> String {
    let s: String = title
        .replace('&', " and ")
        .chars()
        .flat_map(char::to_lowercase)
        .map(fold)
        .filter(|c| c.is_alphanumeric())
        .collect();
    without_edition(&s).to_string()
}

/// Mots envoyés à la recherche : symboles retirés, ponctuation changée en espaces.
fn terms(title: &str) -> Vec<String> {
    let cleaned: String = title
        .chars()
        .filter(|c| !matches!(c, '™' | '®' | '©'))
        .map(|c| if c.is_alphanumeric() || matches!(c, '\'' | '&' | '-' | '.') { c } else { ' ' })
        .collect();
    cleaned.split_whitespace().map(str::to_string).collect()
}

/// Année de sortie connue du jeu (fiche de la boutique Steam).
fn release_year(game: &Game) -> Option<i32> {
    let secs = game.details.as_ref()?.release_date?;
    chrono::DateTime::from_timestamp(i64::try_from(secs).ok()?, 0).map(|d| d.year())
}

fn key(title: &str, year: Option<i32>) -> String {
    match year {
        Some(y) => format!("{}@{y}", compact(title)),
        None => compact(title),
    }
}

/// Le nom trouvé sur le site est-il bien ce titre ? Égalité seulement (symboles, accents, casse
/// et mention d'édition ignorés), sans tolérance aux fautes de frappe : « Crusader King » n'est
/// pas « Crusader Kings ». Jamais « le nom contient le titre » : « Hades » n'est pas « Saint
/// Seiya: The Hades ». Les autres noms du jeu passent par les alias du site.
fn same_name(title: &str, name: &str) -> bool {
    let (t, n) = (compact(title), compact(name));
    t.len() >= 2 && t == n
}

/// Un résultat de la recherche.
#[derive(Clone, Debug, PartialEq)]
struct Item {
    name: String,
    aliases: Vec<String>,
    kind: String,
    year: Option<i32>,
    main: u64,
    plus: u64,
    full: u64,
}

/// Nombre lu tel quel ou dans une chaîne (le site a déjà changé de format).
fn number(v: Option<&Value>) -> u64 {
    match v {
        Some(Value::Number(n)) => n.as_u64().or_else(|| n.as_f64().map(|f| f.max(0.0) as u64)).unwrap_or(0),
        Some(Value::String(s)) => s.trim().parse::<f64>().map(|f| f.max(0.0) as u64).unwrap_or(0),
        _ => 0,
    }
}

fn text(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).unwrap_or("").trim().to_string()
}

/// Lit la réponse de la recherche. Erreur si ce n'est pas la forme attendue (le site a changé).
fn parse(body: &str) -> Result<Vec<Item>, String> {
    let value: Value = serde_json::from_str(body).map_err(|e| format!("réponse illisible : {e}"))?;
    let data = value.get("data").and_then(Value::as_array).ok_or("réponse sans « data »")?;
    Ok(data
        .iter()
        .filter_map(|g| {
            let name = text(g.get("game_name"));
            if name.is_empty() {
                return None;
            }
            let aliases = text(g.get("game_alias")).split(',').map(str::trim).filter(|a| !a.is_empty()).map(str::to_string).collect();
            let year = i32::try_from(number(g.get("release_world"))).ok().filter(|&y| y > 1950);
            Some(Item {
                name,
                aliases,
                kind: text(g.get("game_type")),
                year,
                main: number(g.get("comp_main")),
                plus: number(g.get("comp_plus")),
                full: number(g.get("comp_100")),
            })
        })
        .collect())
}

/// Le site écrit parfois l'année dans le nom pour départager (« Resident Evil 4 (2005) »).
fn name_and_year(name: &str) -> (&str, Option<i32>) {
    let trimmed = name.trim_end();
    if let Some(open) = trimmed.strip_suffix(')').and_then(|s| s.rfind('(').map(|i| (i, s))) {
        let (i, s) = open;
        if let Ok(y) = s[i + 1..].trim().parse::<i32>()
            && (1950..=2100).contains(&y)
        {
            return (trimmed[..i].trim_end(), Some(y));
        }
    }
    (trimmed, None)
}

fn minutes(secs: u64) -> Option<u32> {
    (secs >= 30).then(|| u32::try_from((secs + 30) / 60).unwrap_or(u32::MAX))
}

/// Le bon résultat pour ce titre : nom ou alias identique, jeu de base plutôt que DLC, année la
/// plus proche, puis le plus populaire (ordre du site). Mods et hacks écartés, ainsi que les
/// homonymes sortis plus de `MAX_YEAR_GAP` ans avant ou après.
fn pick(items: &[Item], title: &str, year: Option<i32>) -> Option<Hltb> {
    let mut best: Option<((bool, i32, usize), &Item)> = None;
    for (i, item) in items.iter().enumerate() {
        if matches!(item.kind.as_str(), "mod" | "hack") {
            continue;
        }
        let (base, named_year) = name_and_year(&item.name);
        let matches = same_name(title, &item.name) || same_name(title, base) || item.aliases.iter().any(|a| same_name(title, a));
        if !matches {
            continue;
        }
        let item_year = named_year.or(item.year);
        let gap = match (year, item_year) {
            (Some(a), Some(b)) => (a - b).abs(),
            _ => 0,
        };
        if gap > MAX_YEAR_GAP {
            continue;
        }
        let score = (item.kind != "game", gap, i);
        if best.as_ref().is_none_or(|(s, _)| score < *s) {
            best = Some((score, item));
        }
    }
    let item = best?.1;
    let times = Hltb { name: item.name.clone(), main: minutes(item.main), plus: minutes(item.plus), full: minutes(item.full) };
    (times.main.is_some() || times.plus.is_some() || times.full.is_some()).then_some(times)
}

/// Pourquoi une requête n'a pas abouti.
#[derive(Debug, PartialEq)]
enum Failure {
    /// Jeton refusé ou expiré (403) : en redemander un.
    Token,
    /// Point d'entrée disparu (404) : le relire dans les scripts du site.
    Moved,
    /// Tout le reste : arrêt de la lecture.
    Stop(String),
}

fn failure(e: ureq::Error) -> Failure {
    match e {
        ureq::Error::Status(403, _) => Failure::Token,
        ureq::Error::Status(404, _) => Failure::Moved,
        ureq::Error::Status(code, _) => Failure::Stop(format!("HTTP {code}")),
        other => Failure::Stop(other.to_string()),
    }
}

/// Jeton de la recherche, et la paire clé-valeur que le site demande parfois en plus.
#[derive(Debug, Default, PartialEq)]
struct Token {
    token: String,
    extra: Option<(String, String)>,
}

fn parse_token(body: &str) -> Result<Token, String> {
    let value: Value = serde_json::from_str(body).map_err(|e| format!("jeton illisible : {e}"))?;
    let obj = value.as_object().ok_or("jeton illisible")?;
    let token = obj.get("token").and_then(Value::as_str).filter(|t| !t.is_empty()).ok_or("réponse sans jeton")?;
    let find = |part: &str| {
        obj.iter()
            .find(|(k, v)| k.as_str() != "token" && k.to_lowercase().contains(part) && v.is_string())
            .and_then(|(_, v)| v.as_str())
            .map(str::to_string)
    };
    let extra = find("key").zip(find("val")).filter(|(k, _)| !k.is_empty());
    Ok(Token { token: token.to_string(), extra })
}

/// Chemin « /api/…/init?t= » dans un script de la page : le point d'entrée de la recherche.
fn endpoint_in_script(script: &str) -> Option<String> {
    // En octets : les scripts minifiés contiennent de l'UTF-8, le chemin cherché est en ASCII.
    let bytes = script.as_bytes();
    let (marker, api) = (b"/init?t=", b"/api/");
    for pos in (0..bytes.len().saturating_sub(marker.len() - 1)).filter(|&i| bytes[i..].starts_with(marker)) {
        let before = &bytes[pos.saturating_sub(80)..pos];
        let Some(start) = (0..before.len()).rev().find(|&i| before[i..].starts_with(api)) else { continue };
        let path = &before[start..];
        if path.len() > api.len() && path.iter().all(|&c| c.is_ascii_alphanumeric() || matches!(c, b'/' | b'_' | b'-')) {
            return std::str::from_utf8(path).ok().map(str::to_string);
        }
    }
    None
}

/// Scripts Next.js de la page d'accueil (« /_next/static/chunks/….js »).
fn scripts_in_page(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in html.split('"').skip(1) {
        if part.starts_with("/_next/static/chunks/")
            && part.ends_with(".js")
            && part.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '.'))
            && !part.contains("..")
            && !out.iter().any(|o| o == part)
        {
            out.push(part.to_string());
        }
    }
    out
}

/// Accès au site : une requête à la fois, au moins `PAUSE` entre deux.
struct Client {
    agent: ureq::Agent,
    endpoint: String,
    token: Option<Token>,
    last: Option<Instant>,
    /// Requêtes faites (pour le plafond du relevé réel).
    requests: usize,
}

impl Client {
    fn new(endpoint: String) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(10))
            .redirects(0)
            .user_agent(concat!("Ludex/", env!("CARGO_PKG_VERSION"), " (lanceur de jeux)"))
            .build();
        Client { agent, endpoint, token: None, last: None, requests: 0 }
    }

    fn wait(&mut self) {
        if let Some(last) = self.last {
            let since = last.elapsed();
            if since < PAUSE {
                std::thread::sleep(PAUSE - since);
            }
        }
        self.last = Some(Instant::now());
        self.requests += 1;
    }

    fn get(&mut self, path: &str) -> Result<String, Failure> {
        self.wait();
        self.agent
            .get(&format!("{SITE}{path}"))
            .set("Referer", &format!("{SITE}/"))
            .call()
            .map_err(failure)?
            .into_string()
            .map_err(|e| Failure::Stop(e.to_string()))
    }

    fn init(&mut self) -> Result<(), Failure> {
        let millis = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        let body = self.get(&format!("{}/init?t={millis}", self.endpoint))?;
        self.token = Some(parse_token(&body).map_err(Failure::Stop)?);
        Ok(())
    }

    /// Relit le point d'entrée dans les scripts de la page d'accueil.
    fn discover(&mut self) -> Result<String, String> {
        let html = self.get("/").map_err(|e| format!("page d'accueil illisible : {e:?}"))?;
        for script in scripts_in_page(&html).into_iter().take(MAX_SCRIPTS) {
            let Ok(js) = self.get(&script) else { continue };
            if let Some(found) = endpoint_in_script(&js) {
                return Ok(found);
            }
        }
        Err("point d'entrée de la recherche introuvable dans les scripts du site".into())
    }

    fn post(&mut self, title: &str) -> Result<Vec<Item>, Failure> {
        let token = self.token.as_ref().ok_or(Failure::Token)?;
        let mut body = json!({
            "searchType": "games",
            "searchTerms": terms(title),
            "searchPage": 1,
            "size": RESULTS,
            "searchOptions": {
                "games": {
                    "userId": 0, "platform": "", "sortCategory": "popular", "rangeCategory": "main",
                    "rangeTime": { "min": 0, "max": 0 },
                    "gameplay": { "perspective": "", "flow": "", "genre": "" },
                    "year": "", "modifier": ""
                },
                "users": { "sortCategory": "postcount" },
                "lists": { "sortCategory": "follows" },
                "filter": "", "sort": 0, "randomizer": 0
            },
            "useCache": true
        });
        let mut request = self
            .agent
            .post(&format!("{SITE}{}", self.endpoint))
            .set("Referer", &format!("{SITE}/"))
            .set("Origin", SITE)
            .set("Content-Type", "application/json")
            .set("x-auth-token", &token.token);
        if let Some((k, v)) = &token.extra {
            request = request.set("x-hp-key", k).set("x-hp-val", v);
            body[k.as_str()] = Value::String(v.clone());
        }
        self.wait();
        let text = request.send_string(&body.to_string()).map_err(failure)?.into_string().map_err(|e| Failure::Stop(e.to_string()))?;
        parse(&text).map_err(Failure::Stop)
    }

    /// Cherche un titre : jeton demandé au besoin, redemandé une fois s'il est refusé, point
    /// d'entrée relu une fois s'il a disparu. `Err` : arrêter la lecture.
    fn search(&mut self, title: &str, moved: &mut bool) -> Result<Vec<Item>, String> {
        let mut token_retry = false;
        loop {
            let result = if self.token.is_some() { self.post(title) } else { self.init().and_then(|()| self.post(title)) };
            match result {
                Ok(items) => return Ok(items),
                Err(Failure::Token) if !token_retry => {
                    token_retry = true;
                    self.token = None;
                }
                Err(Failure::Moved) if !*moved => {
                    *moved = true;
                    let found = self.discover()?;
                    if found == self.endpoint {
                        return Err(format!("{found} ne répond plus"));
                    }
                    self.endpoint = found;
                    self.token = None;
                }
                Err(Failure::Token) => return Err("jeton refusé par le site".into()),
                Err(Failure::Moved) => return Err(format!("{} introuvable (404)", self.endpoint)),
                Err(Failure::Stop(e)) => return Err(e),
            }
        }
    }
}

impl HltbCache {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("hltb-cache.json");
        let mut cache: HltbCache = fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        if cache.version != CACHE_VERSION {
            cache.titles.clear();
            cache.version = CACHE_VERSION;
        }
        cache.path = path;
        cache
    }

    fn save(&self) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }

    /// Reporte la durée connue du jeu (titre et année actuels). Renvoie vrai si elle a changé.
    /// Une durée déjà affichée n'est jamais effacée ici.
    pub fn apply(&self, game: &mut Game) -> bool {
        let times = self.titles.get(&key(&game.title, release_year(game))).and_then(|e| e.times.clone());
        if times.is_none() || times == game.hltb {
            return false;
        }
        game.hltb = times;
        true
    }

    /// Titres jamais cherchés, ou cherchés il y a plus de 30 jours (outils exceptés).
    pub fn to_fetch(&self, games: &[Game]) -> Vec<Query> {
        let mut seen = BTreeSet::new();
        games
            .iter()
            .filter(|g| !g.is_tool)
            .map(|g| {
                let year = release_year(g);
                Query { key: key(&g.title, year), title: g.title.clone(), year }
            })
            .filter(|q| compact(&q.title).len() >= 2)
            .filter(|q| self.titles.get(&q.key).is_none_or(|e| now().saturating_sub(e.checked) > REFRESH_AFTER))
            .filter(|q| seen.insert(q.key.clone()))
            .collect()
    }

    fn record(&mut self, key: String, times: Option<Hltb>) {
        self.titles.insert(key, Entry { checked: now(), times });
    }

    /// Cherche les titres un par un (au moins `PAUSE` entre deux requêtes) et enregistre après
    /// chacun ; `on_update` reçoit le cache à jour quand une durée arrive. Au plus `limit`
    /// requêtes si donné. Une erreur arrête tout sans rien noter pour les titres restants : ils
    /// seront cherchés au prochain scan. Renvoie faux dans ce cas.
    pub fn fetch(&mut self, queries: Vec<Query>, limit: Option<usize>, mut on_update: impl FnMut(&HltbCache)) -> bool {
        let mut client = Client::new(self.endpoint.clone().unwrap_or_else(|| DEFAULT_ENDPOINT.to_string()));
        let mut moved = false;
        for q in queries {
            if limit.is_some_and(|l| client.requests >= l) {
                break;
            }
            match client.search(&q.title, &mut moved) {
                Ok(items) => {
                    let times = pick(&items, &q.title, q.year);
                    let found = times.is_some();
                    self.record(q.key, times);
                    if moved {
                        self.endpoint = Some(client.endpoint.clone()).filter(|e| e != DEFAULT_ENDPOINT);
                    }
                    if let Err(e) = self.save() {
                        eprintln!("Cache HowLongToBeat non enregistré : {e}");
                    }
                    if found {
                        on_update(self);
                    }
                }
                Err(e) => {
                    eprintln!("Durées HowLongToBeat interrompues : {e}");
                    return false;
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{Launch, Platform};
    use crate::metadata::Details;

    /// Réponse réelle du site (novembre 2026), réduite aux champs utiles et à quelques résultats.
    const HADES: &str = r#"{"color":"blue","title":"","category":"games","count":20,"pageCurrent":1,"pageTotal":1,"pageSize":20,"data":[
        {"game_id":62941,"game_name":"Hades","game_name_date":0,"game_alias":"","game_type":"game","comp_main":84950,"comp_plus":175350,"comp_100":343179,"comp_all":160269,"release_world":2020,"profile_platform":"PC"},
        {"game_id":118218,"game_name":"Hades II","game_alias":"Hades 2","game_type":"game","comp_main":115117,"comp_plus":193223,"comp_100":379828,"release_world":2025},
        {"game_id":8069,"game_name":"Saint Seiya: The Hades","game_alias":"Saint Seiya: The Hades Chapter Sanctuary","game_type":"game","comp_main":19424,"comp_plus":25350,"comp_100":189000,"release_world":2007},
        {"game_id":73884,"game_name":"Shades of Black","game_alias":"","game_type":"game","comp_main":0,"comp_plus":0,"comp_100":12240,"release_world":2015}
    ]}"#;

    const RE4: &str = r#"{"data":[
        {"game_id":108881,"game_name":"Resident Evil 4","game_alias":"Resident Evil 4 Remake","game_type":"game","comp_main":58322,"comp_plus":78545,"comp_100":233090,"release_world":2023},
        {"game_id":7720,"game_name":"Resident Evil 4","game_alias":"Resident Evil 4 HD, Biohazard 4, Resident Evil 4: Wii Edition","game_type":"game","comp_main":56212,"comp_plus":70267,"comp_100":117205,"release_world":2005},
        {"game_id":137868,"game_name":"Resident Evil 4 - Separate Ways","game_alias":"","game_type":"dlc","comp_main":17640,"comp_plus":21887,"comp_100":35305,"release_world":2023},
        {"game_id":91167,"game_name":"Resident Evil 4 VR","game_alias":"","game_type":"game","comp_main":"44962","comp_plus":51591,"comp_100":58623,"release_world":2021}
    ]}"#;

    #[test]
    fn reads_a_recorded_response() {
        let items = parse(HADES).unwrap();
        assert_eq!(items.len(), 4);
        assert_eq!(items[0].name, "Hades");
        assert_eq!((items[0].main, items[0].plus, items[0].full, items[0].year), (84950, 175350, 343179, Some(2020)));
        assert_eq!(items[1].aliases, vec!["Hades 2"]);
        let re4 = parse(RE4).unwrap();
        assert_eq!(re4[1].aliases, vec!["Resident Evil 4 HD", "Biohazard 4", "Resident Evil 4: Wii Edition"]);
        assert_eq!(re4[3].main, 44962); // nombre envoyé en chaîne
        assert!(parse("<html>Erreur</html>").is_err());
        assert!(parse(r#"{"games":[]}"#).is_err()); // forme inconnue : « inconnu », pas de plantage
        assert!(parse(r#"{"data":[]}"#).unwrap().is_empty());
    }

    #[test]
    fn picks_only_the_same_game() {
        let hades = pick(&parse(HADES).unwrap(), "Hades", None).unwrap();
        // Secondes arrondies à la minute.
        assert_eq!(hades, Hltb { name: "Hades".into(), main: Some(1416), plus: Some(2923), full: Some(5720) });
        assert_eq!(pick(&parse(HADES).unwrap(), "HADES™", None).unwrap().name, "Hades");
        assert_eq!(pick(&parse(HADES).unwrap(), "Hades 2", None).unwrap().name, "Hades II"); // alias
        // Jamais un nom qui contient le titre.
        assert_eq!(pick(&parse(HADES).unwrap(), "The Hades", None), None);
        assert_eq!(pick(&parse(HADES).unwrap(), "Saint Seiya", None), None);
        assert_eq!(pick(&parse(HADES).unwrap(), "Portal", None), None);
        // Trouvé, mais seulement le 100 % : gardé.
        assert_eq!(pick(&parse(HADES).unwrap(), "Shades of Black", None).unwrap().main, None);

        // L'année départage les homonymes ; sans année, le plus populaire.
        let re4 = parse(RE4).unwrap();
        assert_eq!(pick(&re4, "Resident Evil 4", Some(2005)).unwrap().full, Some(1953));
        assert_eq!(pick(&re4, "Resident Evil 4", Some(2023)).unwrap().full, Some(3885));
        assert_eq!(pick(&re4, "Resident Evil 4", None).unwrap().full, Some(3885));
        assert_eq!(pick(&re4, "Biohazard 4", None).unwrap().full, Some(1953)); // alias du seul jeu de 2005
        assert_eq!(pick(&re4, "Resident Evil 4 - Separate Ways", None).unwrap().main, Some(294)); // DLC demandé par son nom
        assert_eq!(pick(&re4, "Resident Evil", None), None);
        // Homonyme trop ancien : un autre jeu du même nom.
        assert_eq!(pick(&re4, "Resident Evil 4", Some(2034)), None);
        assert_eq!(pick(&re4, "Resident Evil 4", Some(2012)).unwrap().full, Some(1953)); // remake à 11 ans : écarté
    }

    #[test]
    fn compares_titles_strictly() {
        assert!(same_name("Pokémon™ Sword & Shield", "Pokemon Sword and Shield"));
        assert!(same_name("The Witcher 3: Wild Hunt Complete Edition", "The Witcher 3: Wild Hunt"));
        assert!(!same_name("Crusader King", "Crusader Kings")); // aucune faute de frappe tolérée
        assert!(!same_name("Football Manager 2022", "Football Manager 2023"));
        assert!(!same_name("Portal", "Portal 2"));
        assert!(!same_name("Hades", "Saint Seiya: The Hades"));
        assert!(!same_name("X", "X")); // trop court pour être sûr
        assert_eq!(name_and_year("Resident Evil 4 (2005)"), ("Resident Evil 4", Some(2005)));
        assert_eq!(name_and_year("Doom (Classic)"), ("Doom (Classic)", None));
        assert_eq!(terms("Tom Clancy's Rainbow Six® Siege: X"), vec!["Tom", "Clancy's", "Rainbow", "Six", "Siege", "X"]);
    }

    #[test]
    fn reads_the_token_and_the_endpoint() {
        let t = parse_token(r#"{"token":"abc"}"#).unwrap();
        assert_eq!(t, Token { token: "abc".into(), extra: None });
        let t = parse_token(r#"{"token":"abc","hpKey":"k1","hpVal":"v1"}"#).unwrap();
        assert_eq!(t.extra, Some(("k1".into(), "v1".into())));
        assert!(parse_token(r#"{"error":"x"}"#).is_err());
        assert!(parse_token("<!DOCTYPE html>").is_err());

        let js = r#"var é="ü";er=async()=>{try{let e=await fetch(`/api/search/site/init?t=${Date.now()}`);if(e.ok)"#;
        assert_eq!(endpoint_in_script(js).as_deref(), Some("/api/search/site"));
        assert_eq!(endpoint_in_script(r#"fetch("/api/find/" + x + "/init?t=1")"#), None);
        assert_eq!(endpoint_in_script("rien ici"), None);
        let html = r#"<script src="/_next/static/chunks/25ey50q5brmqu.js" defer=""></script><script src="/_next/static/chunks/25ey50q5brmqu.js"></script><link href="/_next/static/chunks/a.css"/><script src="https://cdn.example.com/x.js"></script>"#;
        assert_eq!(scripts_in_page(html), vec!["/_next/static/chunks/25ey50q5brmqu.js"]);
    }

    fn game(title: &str, release: Option<u64>) -> Game {
        let mut g = Game::new(Platform::Other, title, title, Launch::Uri(String::new()));
        g.details = release.map(|d| Details { release_date: Some(d), ..Details::default() });
        g
    }

    #[test]
    fn applies_cache_and_lists_titles_to_fetch() {
        let mut cache = HltbCache::default();
        let hades = pick(&parse(HADES).unwrap(), "Hades", None);
        // Hades sorti le 17 septembre 2020.
        cache.record(key("Hades", Some(2020)), hades.clone());
        cache.record(key("Portal", None), None);
        cache.titles.insert(key("Celeste", None), Entry { checked: 0, times: hades.clone() });

        let mut g = game("Hades", Some(1600300800));
        assert!(cache.apply(&mut g));
        assert_eq!(g.hltb, hades);
        assert!(!cache.apply(&mut g)); // déjà à jour
        assert!(!cache.apply(&mut game("Hades", None))); // autre clé (année inconnue)
        let mut portal = game("Portal", None);
        assert!(!cache.apply(&mut portal) && portal.hltb.is_none());

        let mut tool = game("Wallpaper Engine", None);
        tool.is_tool = true;
        let games = [g, game("Hades", None), portal, game("Celeste", None), game("Celeste", None), game("Inside", Some(1467331200)), tool];
        let titles: Vec<(String, Option<i32>)> = cache.to_fetch(&games).into_iter().map(|q| (q.title, q.year)).collect();
        // Hades sans année : nouvelle clé ; Celeste : plus de 30 jours ; Inside : jamais ; outil écarté.
        assert_eq!(titles, vec![("Hades".into(), None), ("Celeste".into(), None), ("Inside".into(), Some(2016))]);
    }

    #[test]
    fn saves_and_reloads_the_cache() {
        let dir = std::env::temp_dir().join(format!("ludex-hltb-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut cache = HltbCache::load(&dir);
        cache.record(key("Hades", None), pick(&parse(HADES).unwrap(), "Hades", None));
        cache.record(key("Portal", None), None);
        cache.endpoint = Some("/api/find".into());
        cache.save().unwrap();
        let reloaded = HltbCache::load(&dir);
        assert_eq!(reloaded.titles.len(), 2);
        assert_eq!(reloaded.titles["hades"].times.as_ref().unwrap().main, Some(1416));
        assert!(reloaded.titles["portal"].times.is_none());
        assert_eq!(reloaded.endpoint.as_deref(), Some("/api/find"));
        assert!(!dir.join("hltb-cache.json.tmp").exists());

        // Version différente : tout est redemandé.
        fs::write(dir.join("hltb-cache.json"), r#"{"version":0,"titles":{"hades":{"checked":1}}}"#).unwrap();
        assert!(HltbCache::load(&dir).titles.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    /// Durées de toute la bibliothèque de ce PC (jaquettes et fiches déjà en cache comprises, pour
    /// les titres et les années), sans toucher aux caches de l'app, 150 requêtes au plus :
    /// `cargo test hltb_this_pc -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn hltb_this_pc() {
        let mut games = crate::library::scan().games;
        if let Some(dir) = std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("com.ludex.app")) {
            let (art, meta) = (crate::art::ArtCache::load(&dir), crate::metadata::MetaCache::load(&dir));
            games.iter_mut().for_each(|g| {
                art.apply(g);
                meta.apply(g);
            });
        }
        let tmp = std::env::temp_dir().join(format!("ludex-hltb-pc-{}", std::process::id()));
        let mut cache = HltbCache::load(&tmp);
        let queries = cache.to_fetch(&games);
        let started = Instant::now();
        let finished = cache.fetch(queries.clone(), Some(150), |_| {});
        let elapsed = started.elapsed();
        let (mut found, mut with_main) = (0, 0);
        for g in games.iter_mut().filter(|g| !g.is_tool) {
            cache.apply(g);
            match &g.hltb {
                Some(h) => {
                    found += 1;
                    with_main += usize::from(h.main.is_some());
                    let year = release_year(g).map(|y| y.to_string()).unwrap_or_default();
                    let fmt = |m: Option<u32>| m.map(|m| format!("{:.1} h", f64::from(m) / 60.0)).unwrap_or_else(|| "-".into());
                    println!("{} ({year}) → « {} » : {} / {} / {}", g.title, h.name, fmt(h.main), fmt(h.plus), fmt(h.full));
                }
                None => println!("{} : inconnu", g.title),
            }
        }
        let total = games.iter().filter(|g| !g.is_tool).count();
        println!(
            "{found} jeux sur {total} ont une durée ({with_main} avec l'histoire), {} titres demandés en {:.1} s, lecture {}",
            queries.len(),
            elapsed.as_secs_f64(),
            if finished { "complète" } else { "interrompue" }
        );
        let _ = fs::remove_dir_all(&tmp);
    }
}
