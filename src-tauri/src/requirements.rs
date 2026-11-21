//! Configuration requise d'un jeu (minimale et recommandée) comparée au matériel de ce PC.
//!
//! Source : l'API publique de la boutique Steam `store.steampowered.com/api/appdetails` (aucun
//! compte), champ `pc_requirements` (`minimum`, `recommended`), du HTML écrit à la main par chaque
//! éditeur. Demandée à la demande (ouverture de la fiche), une requête à la fois, redirections
//! refusées, dans la langue de Ludex (`l=french` ou `l=english`). Le HTML brut est gardé dans un
//! cache propre à chaque langue (dossier de données de l'app : `requirements-cache.json` en
//! français, `requirements-cache.en.json` en anglais) : un appid n'est redemandé qu'au bout de 30
//! jours, et une réponse vide n'efface pas ce qui est connu. Il est converti ici en lignes de texte (libellé + valeur) : jamais de HTML vers le front.
//!
//! Matériel (`Hardware`) lu une fois : processeur et cœurs (`sysinfo`), mémoire installée
//! (`GetPhysicallyInstalledSystemMemory`), cartes graphiques et mémoire dédiée (DXGI), niveau
//! DirectX le plus haut (`D3D11CreateDevice` sans créer de périphérique), version de Windows
//! (registre). Place libre sur le disque du jeu relue à chaque demande.
//!
//! Comparaison prudente, par ligne : `ok`, `below` ou `unknown`. Mémoire, stockage, Windows :
//! comparés aux nombres lus dans le texte. Processeur et carte graphique : seulement des
//! correspondances sûres (même famille, génération et gamme au moins égales, ou modèles très
//! anciens) qui donnent `ok`, jamais `below` ; seul un nombre de cœurs écrit sans modèle (« Quad
//! core ») peut donner `below`. Le reste : `unknown`, affiché côte à côte sans verdict.

use crate::library::Game;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const REFRESH_AFTER: u64 = 30 * 24 * 3600;
/// À augmenter quand le contenu du cache change : tout est alors redemandé.
const CACHE_VERSION: u32 = 1;
const APPDETAILS: &str = "https://store.steampowered.com/api/appdetails";
/// Une réponse plus lourde est refusée (une fiche complète fait ~20 Ko).
const MAX_BODY: u64 = 2 * 1024 * 1024;
/// HTML gardé au plus, par configuration.
const MAX_HTML: usize = 16 * 1024;
/// Lignes gardées par configuration, et caractères par ligne.
const MAX_LINES: usize = 12;
const MAX_TEXT: usize = 400;
const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

/// Ligne d'une configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    System,
    Cpu,
    Memory,
    Gpu,
    Directx,
    Storage,
    Notes,
}

/// Ce PC face à une ligne.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Ok,
    Below,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Line {
    pub kind: Kind,
    pub text: String,
    pub verdict: Verdict,
}

/// Carte graphique de ce PC.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Gpu {
    pub name: String,
    /// Mémoire dédiée, en octets.
    pub vram: u64,
}

/// Disque du jeu.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Disk {
    /// « E: », ou le point de montage.
    pub letter: String,
    pub free: u64,
    /// Place prise par le jeu (déjà installé), si connue.
    pub game: Option<u64>,
}

/// Matériel de ce PC, envoyé au front pour la colonne « Ce PC ».
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pc {
    pub windows: String,
    pub cpu: Option<String>,
    pub cores: Option<usize>,
    pub threads: Option<usize>,
    /// Mémoire installée, en octets.
    pub memory: Option<u64>,
    /// Cartes dédiées d'abord, la plus grosse mémoire d'abord.
    pub gpus: Vec<Gpu>,
    /// Version de DirectX la plus haute prise en charge par une carte (niveau de fonctionnalités).
    pub directx: Option<u32>,
    pub disk: Option<Disk>,
}

/// Configuration requise d'un jeu et matériel de ce PC.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Requirements {
    pub minimum: Vec<Line>,
    pub recommended: Vec<Line>,
    pub pc: Pc,
}

// ---------------------------------------------------------------------------------------------
// Lecture de la réponse et conversion du HTML
// ---------------------------------------------------------------------------------------------

/// HTML des deux configurations (`None` : absente).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Html {
    #[serde(default)]
    minimum: Option<String>,
    #[serde(default)]
    recommended: Option<String>,
    /// Langue de la requête (`request`) : une réponse n'entre jamais dans le cache d'une autre
    /// langue (langue changée pendant la requête). `None` : inconnue (tests, cache relu).
    #[serde(skip)]
    lang: Option<crate::i18n::Lang>,
}

impl Html {
    fn is_empty(&self) -> bool {
        self.minimum.is_none() && self.recommended.is_none()
    }
}

/// Lit la réponse de `appdetails` pour un appid : `Ok(None)` si la boutique ne le connaît pas ou
/// n'a aucune configuration PC (`pc_requirements` absent, vide ou en tableau vide).
fn parse_response(body: &str, appid: u32) -> Result<Option<Html>, String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| tr!("réponse illisible : {e}", "unreadable response: {e}"))?;
    let entry = value.get(appid.to_string()).ok_or_else(|| tr!("appid absent de la réponse", "appid missing from the response"))?;
    if entry.get("success").and_then(|s| s.as_bool()) != Some(true) {
        return Ok(None);
    }
    let Some(reqs) = entry.get("data").and_then(|d| d.get("pc_requirements")).and_then(|r| r.as_object()) else { return Ok(None) };
    let take = |key: &str| {
        reqs.get(key)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty() && s.len() <= MAX_HTML)
            .map(str::to_string)
    };
    let html = Html { minimum: take("minimum"), recommended: take("recommended"), lang: None };
    Ok(Some(html).filter(|h| !h.is_empty()))
}

/// Décode les entités HTML : nommées courantes et numériques.
fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let end = rest.bytes().take(12).position(|b| b == b';');
        let decoded = end.and_then(|e| {
            let name = &rest[1..e];
            let c = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                "reg" => Some('®'),
                "trade" => Some('™'),
                "copy" => Some('©'),
                _ => name
                    .strip_prefix("#x")
                    .or_else(|| name.strip_prefix("#X"))
                    .and_then(|h| u32::from_str_radix(h, 16).ok())
                    .or_else(|| name.strip_prefix('#').and_then(|d| d.parse().ok()))
                    .and_then(char::from_u32),
            };
            c.map(|c| (c, e))
        });
        match decoded {
            Some((c, e)) => {
                out.push(c);
                rest = &rest[e + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Texte d'un morceau de HTML : balises retirées, entités décodées, contrôles et caractères
/// invisibles retirés, espaces (insécables compris) regroupés.
fn plain(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                text.push(' ');
            }
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    let text = decode(&text);
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        let c = if c.is_whitespace() || c == '\u{a0}' || c == '\u{202f}' { ' ' } else { c };
        if c.is_control() || crate::library::manual::is_invisible(c) {
            continue;
        }
        if c == ' ' && (out.is_empty() || out.ends_with(' ')) {
            continue;
        }
        out.push(c);
    }
    out.trim().to_string()
}

/// Comparable : minuscules, sans accents courants, ®/™/© et « (r) », « (tm) » retirés.
fn fold(text: &str) -> String {
    let lower = text.to_lowercase().replace("(r)", " ").replace("(tm)", " ").replace('œ', "oe");
    lower
        .chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            '®' | '™' | '©' => ' ',
            c => c,
        })
        .collect()
}

/// Type d'une ligne d'après son libellé (avant les deux-points), en français ou en anglais.
fn kind_of(label: &str) -> Option<Kind> {
    let l = fold(label);
    let l = l.trim_matches(|c: char| c == '*' || c.is_whitespace());
    let has = |parts: &[&str]| parts.iter().any(|w| l.contains(w));
    // Mots courts comparés en entier (« os » est dans bien des mots).
    let ws: Vec<&str> = l.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let word = |list: &[&str]| list.iter().any(|w| ws.contains(w));
    if l.len() > 40 {
        None
    } else if (has(&["exploitation", "systeme", "system"]) || word(&["os"])) && !has(&["processeur", "processor"]) {
        Some(Kind::System)
    } else if has(&["processeur", "processor"]) || word(&["cpu"]) {
        Some(Kind::Cpu)
    } else if (has(&["memoire vive", "memory"]) || word(&["ram"])) && !has(&["video"]) {
        Some(Kind::Memory)
    } else if has(&["graphique", "graphics", "video"]) || word(&["gpu"]) {
        Some(Kind::Gpu)
    } else if has(&["directx"]) {
        Some(Kind::Directx)
    } else if has(&["disque", "stockage", "storage", "hard drive", "disk", "espace"]) {
        Some(Kind::Storage)
    } else if has(&["notes", "remarques"]) {
        Some(Kind::Notes)
    } else {
        None
    }
}

/// Coupe le texte à `MAX_TEXT` caractères (sur un caractère entier).
fn cut(text: &str) -> String {
    match text.char_indices().nth(MAX_TEXT) {
        Some((i, _)) => format!("{}…", text[..i].trim_end()),
        None => text.to_string(),
    }
}

/// Lignes (type, texte) d'une configuration. Le HTML est découpé aux `<li>`, `<br>`, `<p>`, `<h…>` ;
/// chaque morceau « Libellé : valeur » reconnu devient une ligne (libellé dans ou hors de `<strong>`).
/// Un morceau sans libellé qui suit une note la prolonge ; les autres (« Minimale : », « Système
/// d'exploitation et processeur 64 bits nécessaires ») sont écartés. Un type déjà vu est ignoré.
fn lines(html: &str) -> Vec<(Kind, String)> {
    let lower = html.to_ascii_lowercase();
    let mut pieces: Vec<&str> = Vec::new();
    let mut start = 0;
    let mut i = 0;
    let bytes = lower.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let tag = &lower[i + 1..];
            let tag = tag.trim_start_matches('/');
            let breaks = ["li", "br", "p", "h1", "h2", "h3", "h4", "ul", "div"]
                .iter()
                .any(|t| tag.starts_with(t) && tag[t.len()..].starts_with(['>', ' ', '/']));
            if breaks {
                pieces.push(&html[start..i]);
                let end = lower[i..].find('>').map_or(bytes.len(), |e| i + e + 1);
                start = end;
                i = end;
                continue;
            }
        }
        i += 1;
    }
    pieces.push(&html[start..]);

    let mut out: Vec<(Kind, String)> = Vec::new();
    let mut last: Option<Kind> = None;
    for piece in pieces {
        let text = plain(piece);
        if text.is_empty() {
            continue;
        }
        let labelled = text.split_once(':').and_then(|(label, value)| kind_of(label).map(|k| (k, value.trim().to_string())));
        match labelled {
            Some((kind, value)) if !value.is_empty() => {
                if out.iter().any(|(k, _)| *k == kind) || out.len() >= MAX_LINES {
                    last = None;
                    continue;
                }
                out.push((kind, cut(&value)));
                last = Some(kind);
            }
            Some(_) => last = None,
            None if last == Some(Kind::Notes) => {
                if let Some((_, note)) = out.last_mut() {
                    *note = cut(&format!("{note} {text}"));
                }
            }
            None => last = None,
        }
    }
    out.sort_by_key(|(k, _)| *k);
    out
}

// ---------------------------------------------------------------------------------------------
// Nombres lus dans le texte
// ---------------------------------------------------------------------------------------------

/// Première quantité « 12 GB », « 512 Mo », « 1,5 Go », « 2 To » du texte, en Gio. `None` sans unité.
fn gigabytes(text: &str) -> Option<f64> {
    let t = fold(text);
    let chars: Vec<char> = t.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() || (i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '.' || chars[i - 1] == ',')) {
            i += 1;
            continue;
        }
        let mut j = i;
        let mut number = String::new();
        while j < chars.len() && (chars[j].is_ascii_digit() || ((chars[j] == '.' || chars[j] == ',') && chars.get(j + 1).is_some_and(|c| c.is_ascii_digit()))) {
            number.push(if chars[j] == ',' { '.' } else { chars[j] });
            j += 1;
        }
        let mut k = j;
        while k < chars.len() && chars[k].is_whitespace() {
            k += 1;
        }
        let mut unit = String::new();
        while k < chars.len() && chars[k].is_ascii_alphabetic() && unit.len() < 4 {
            unit.push(chars[k]);
            k += 1;
        }
        let whole = k >= chars.len() || !chars[k].is_alphabetic();
        let factor = match unit.as_str() {
            "gb" | "go" | "gib" => Some(1.0),
            "mb" | "mo" | "mib" => Some(1.0 / 1024.0),
            "tb" | "to" | "tib" => Some(1024.0),
            _ => None,
        };
        if let (Some(f), true, Ok(n)) = (factor, whole, number.parse::<f64>()) {
            return Some(n * f);
        }
        i = j.max(i + 1);
    }
    None
}

/// Mots du texte (lettres et chiffres), comparables.
fn words(text: &str) -> Vec<String> {
    fold(text).split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_string).collect()
}

/// Version la plus basse de Windows citée (XP = 51, Vista = 60, 7 = 61, 8 = 62, 8.1 = 63, 10 = 100,
/// 11 = 110). `None` si le texte ne parle pas de Windows (« Windows », « Win », « Vista », « XP »).
fn windows_min(text: &str) -> Option<u32> {
    let t = fold(text);
    let tokens: Vec<&str> = t.split(|c: char| !(c.is_alphanumeric() || c == '.')).filter(|w| !w.is_empty()).collect();
    if !tokens.iter().any(|w| matches!(*w, "windows" | "win" | "vista" | "xp")) {
        return None;
    }
    tokens
        .iter()
        .filter_map(|w| match w.trim_end_matches('.') {
            "xp" => Some(51),
            "vista" => Some(60),
            "7" => Some(61),
            "8" => Some(62),
            "8.1" => Some(63),
            "10" => Some(100),
            "11" => Some(110),
            _ => None,
        })
        .min()
}

/// Version de DirectX demandée : premier nombre du texte (« Version 12 », « 9.0c »).
fn directx_version(text: &str) -> Option<u32> {
    let t = fold(text);
    let start = t.find(|c: char| c.is_ascii_digit())?;
    let digits: String = t[start..].chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok().filter(|&v| (7..=13).contains(&v))
}

/// Nombre de cœurs écrit sans modèle : « Dual core », « Quad-Core », « 4 cœurs », « 6-core ».
fn core_count(text: &str) -> Option<usize> {
    let w = words(text);
    let named = [("dual", 2), ("double", 2), ("triple", 3), ("quad", 4), ("quadri", 4), ("hexa", 6), ("six", 6), ("octa", 8), ("eight", 8)];
    for (i, word) in w.iter().enumerate() {
        let next = w.get(i + 1).map(String::as_str);
        let is_core = |s: Option<&str>| matches!(s, Some("core" | "cores" | "coeur" | "coeurs"));
        if let Some(&(_, n)) = named.iter().find(|(name, _)| word == name)
            && is_core(next)
        {
            return Some(n);
        }
        if let Ok(n) = word.parse::<usize>()
            && (1..=64).contains(&n)
            && is_core(next)
        {
            return Some(n);
        }
    }
    None
}

// ---------------------------------------------------------------------------------------------
// Processeurs
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Brand {
    Intel,
    Ryzen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CpuModel {
    /// Core iN / Core Ultra N, Ryzen N : gamme (3, 5, 7, 9), génération si le numéro est écrit.
    Family { brand: Brand, tier: u32, generation: Option<u32>, low_power: bool },
    /// Core 2, Pentium, Celeron, Athlon, Phenom, Q6600… : dépassés par tout Core i récent ou Ryzen.
    Old,
    /// AMD FX.
    Fx,
    /// Modèle cité mais pas comparable (Xeon, Threadripper…).
    Other,
}

/// Génération d'un Core i d'après son numéro : « 750 » = 1, « 6700 » = 6, « 1165G7 » = 11, « 12400 » = 12.
fn intel_gen(number: &str) -> Option<u32> {
    let digits: String = number.chars().take_while(|c| c.is_ascii_digit()).collect();
    match digits.len() {
        3 => Some(1),
        4 if digits.starts_with('1') => digits[..2].parse().ok(),
        4 => digits[..1].parse().ok(),
        5 => digits[..2].parse().ok(),
        _ => None,
    }
}

/// Lettres qui suivent le numéro (« k », « u », « g7 », « x3d »).
fn suffix(number: &str) -> &str {
    number.trim_start_matches(|c: char| c.is_ascii_digit())
}

/// Modèles de processeurs cités dans le texte (chaque alternative).
fn cpu_models(text: &str) -> Vec<CpuModel> {
    let w = words(text);
    let mut out = Vec::new();
    let tier_of = |s: &str| match s {
        "3" | "5" | "7" | "9" => s.parse::<u32>().ok(),
        _ => None,
    };
    let number_at = |i: usize| w.get(i).filter(|n| n.starts_with(|c: char| c.is_ascii_digit())).map(String::as_str);
    for (i, word) in w.iter().enumerate() {
        let word = word.as_str();
        // Core i5-750, i7 6700K, i5 (sans numéro).
        if let Some(t) = word.strip_prefix('i').and_then(tier_of) {
            let number = number_at(i + 1);
            let low_power = number.is_some_and(|n| suffix(n).contains(['u', 'y', 'm']) || suffix(n).starts_with('g'));
            out.push(CpuModel::Family { brand: Brand::Intel, tier: t, generation: number.and_then(intel_gen), low_power });
        } else if word == "ultra"
            && let Some(t) = w.get(i + 1).and_then(|s| tier_of(s))
        {
            let number = number_at(i + 2);
            let generation = number.and_then(|n| n.chars().next()).and_then(|c| c.to_digit(10)).map(|d| 14 + d);
            let low_power = number.is_some_and(|n| suffix(n).contains(['u', 'v']));
            out.push(CpuModel::Family { brand: Brand::Intel, tier: t, generation, low_power });
        } else if word == "ryzen" {
            let mut j = i + 1;
            if w.get(j).is_some_and(|s| s == "ai" || s == "pro") {
                j += 1;
            }
            let Some(t) = w.get(j).and_then(|s| tier_of(s)) else {
                // « Ryzen » seul (« Ryzen CPU ») : la gamme la plus basse.
                out.push(CpuModel::Family { brand: Brand::Ryzen, tier: 3, generation: None, low_power: false });
                continue;
            };
            let mut k = j + 1;
            if w.get(k).is_some_and(|s| s == "pro") {
                k += 1;
            }
            let number = number_at(k).filter(|n| n.chars().take_while(|c| c.is_ascii_digit()).count() == 4);
            let generation = number.and_then(|n| n.chars().next()).and_then(|c| c.to_digit(10));
            let low_power = number.is_some_and(|n| suffix(n).starts_with('u'));
            out.push(CpuModel::Family { brand: Brand::Ryzen, tier: t, generation, low_power });
        } else if word == "fx" && number_at(i + 1).is_some() {
            out.push(CpuModel::Fx);
        } else if matches!(word, "pentium" | "celeron" | "athlon" | "phenom" | "core2" | "sempron" | "turion")
            || (word == "core" && w.get(i + 1).is_some_and(|s| s == "2") && w.get(i + 2).is_some_and(|s| matches!(s.as_str(), "duo" | "quad" | "extreme")))
            || (word.len() == 5 && word.starts_with(['q', 'e']) && word[1..].chars().all(|c| c.is_ascii_digit()))
        {
            out.push(CpuModel::Old);
        } else if matches!(word, "xeon" | "threadripper" | "opteron" | "snapdragon") {
            out.push(CpuModel::Other);
        }
    }
    out
}

/// Génération Intel à laquelle une génération de Ryzen est au moins égale (prudent), et l'inverse.
fn ryzen_as_intel(generation: u32) -> u32 {
    match generation {
        1 => 5,
        2 => 6,
        3 => 8,
        4 | 5 => 10,
        6..=8 => 12,
        _ => 13,
    }
}

fn intel_as_ryzen(generation: u32) -> u32 {
    match generation {
        0..=7 => 0,
        8 => 1,
        9 | 10 => 2,
        11 | 12 => 3,
        13 | 14 => 5,
        _ => 7,
    }
}

/// Ce processeur suffit-il face à ce modèle ? Seulement des cas sûrs : `false` = on ne sait pas.
fn cpu_beats(pc: CpuModel, req: CpuModel) -> bool {
    let CpuModel::Family { brand, tier, generation, low_power } = pc else { return false };
    match req {
        CpuModel::Old => brand == Brand::Ryzen || generation.is_some_and(|g| g >= 2),
        CpuModel::Fx => !low_power && (brand == Brand::Ryzen || generation.is_some_and(|g| g >= 6)),
        CpuModel::Other => false,
        CpuModel::Family { brand: rbrand, tier: rtier, generation: rgeneration, .. } => {
            let Some(rgeneration) = rgeneration else { return tier >= rtier };
            if low_power {
                return false;
            }
            let Some(generation) = generation else { return false };
            let generation = match (brand, rbrand) {
                (a, b) if a == b => generation,
                (Brand::Ryzen, Brand::Intel) => ryzen_as_intel(generation),
                _ => intel_as_ryzen(generation),
            };
            (generation >= rgeneration && tier >= rtier) || (generation >= rgeneration + 4 && tier + 2 >= rtier)
        }
    }
}

fn cpu_verdict(text: &str, hw: &Hardware) -> Verdict {
    let models = cpu_models(text);
    if models.is_empty() {
        // Ni modèle ni famille : seul un nombre de cœurs se compare.
        return match (core_count(text), hw.cores) {
            (Some(need), Some(have)) if have >= need => Verdict::Ok,
            (Some(_), Some(_)) => Verdict::Below,
            _ => Verdict::Unknown,
        };
    }
    let pc = hw.cpu.as_deref().map(cpu_models).and_then(|m| m.into_iter().next());
    match pc {
        Some(pc) if models.iter().any(|&req| cpu_beats(pc, req)) => Verdict::Ok,
        _ => Verdict::Unknown,
    }
}

// ---------------------------------------------------------------------------------------------
// Cartes graphiques
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GpuVendor {
    Nvidia,
    Amd,
    Arc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GpuModel {
    /// Carte dédiée : génération (rang dans la famille), gamme (« 60 » pour une x060), bonus
    /// (Ti, Super, XT : 1 ; Ti Super, XTX : 2).
    Card { vendor: GpuVendor, generation: u32, tier: u32, bonus: u32, low_power: bool },
    /// Carte dédiée trop ancienne pour être classée (GeForce 9800, Radeon HD 4870, X1300…).
    Old,
    /// Graphiques intégrés (Intel HD, UHD, Iris, GMA ; Radeon Graphics sans numéro).
    Integrated,
}

/// Générations NVIDIA dans l'ordre (premiers chiffres du numéro).
const NVIDIA_GENS: &[u32] = &[2, 3, 4, 5, 6, 7, 8, 9, 10, 16, 20, 30, 40, 50, 60];

fn nvidia(number: &str) -> Option<GpuModel> {
    let digits: String = number.chars().take_while(|c| c.is_ascii_digit()).collect();
    let (generation, tier) = match digits.len() {
        3 => (digits[..1].parse::<u32>().ok()?, digits[1..].parse::<u32>().ok()?),
        4 => {
            let head: u32 = digits[..2].parse().ok()?;
            if !NVIDIA_GENS.contains(&head) || head < 10 {
                return Some(GpuModel::Old);
            }
            (head, digits[2..].parse::<u32>().ok()?)
        }
        _ => return None,
    };
    let rank = NVIDIA_GENS.iter().position(|&g| g == generation)? as u32 + 1;
    Some(GpuModel::Card { vendor: GpuVendor::Nvidia, generation: rank, tier, bonus: 0, low_power: false })
}

/// Radeon : « HD 7870 », « R9 290 », « RX 580 », « RX 6700 ».
fn radeon(series: &str, number: &str) -> Option<GpuModel> {
    let digits: String = number.chars().take_while(|c| c.is_ascii_digit()).collect();
    let d = |i: usize| digits[i..=i].parse::<u32>().ok();
    let (generation, tier) = match (series, digits.len()) {
        ("hd", 4) => {
            let g = d(0)?;
            if !(5..=8).contains(&g) {
                return Some(GpuModel::Old);
            }
            (g - 3, d(1)? * 10 + d(2)?)
        }
        ("r5" | "r7" | "r9", 3) => (if d(0)? <= 2 { 6 } else { 7 }, digits[1..].parse().ok()?),
        ("rx", 3) => (if d(0)? <= 4 { 8 } else { 9 }, digits[1..].parse().ok()?),
        ("rx", 4) => {
            let g = match d(0)? {
                5 => 10,
                6 => 11,
                7 => 12,
                9 => 13,
                _ => return None,
            };
            (g, digits[1..3].parse().ok()?)
        }
        _ => return None,
    };
    Some(GpuModel::Card { vendor: GpuVendor::Amd, generation, tier, bonus: 0, low_power: false })
}

/// Coupe les mots collés « gtx1060 », « hd7870 », « rx580 » en deux.
fn split_gpu_words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for w in words(text) {
        let split = ["gtx", "rtx", "gts", "gt", "hd", "rx", "mx"].iter().find_map(|p| {
            w.strip_prefix(p).filter(|rest| rest.starts_with(|c: char| c.is_ascii_digit())).map(|rest| (p.to_string(), rest.to_string()))
        });
        match split {
            Some((p, rest)) => {
                out.push(p);
                out.push(rest);
            }
            None => out.push(w),
        }
    }
    out
}

/// Cartes graphiques citées dans le texte (chaque alternative).
fn gpu_models(text: &str) -> Vec<GpuModel> {
    let w = split_gpu_words(text);
    let mut out = Vec::new();
    let low_power = w.iter().any(|s| matches!(s.as_str(), "laptop" | "mobile" | "notebook" | "maxq"))
        || fold(text).contains("max-q");
    let is_num = |s: Option<&String>| s.is_some_and(|n| n.starts_with(|c: char| c.is_ascii_digit()));
    let bonus_at = |i: usize| {
        let a = w.get(i).map(String::as_str);
        let b = w.get(i + 1).map(String::as_str);
        match (a, b) {
            (Some("ti"), Some("super")) | (Some("xtx"), _) => 2,
            (Some("ti" | "super" | "xt"), _) => 1,
            _ => 0,
        }
    };
    let mut i = 0;
    while i < w.len() {
        let word = w[i].as_str();
        let next = w.get(i + 1);
        let found = match word {
            "gtx" | "rtx" | "gts" | "gt" if is_num(next) => nvidia(next.unwrap()),
            // « GeForce 760 », « NVIDIA 9800 GT » : numéro sans préfixe.
            "geforce" | "nvidia" if is_num(next) && !w.get(i + 2).is_some_and(|s| s == "gb" || s == "go") => nvidia(next.unwrap()),
            "hd" | "rx" | "r5" | "r7" | "r9" if is_num(next) => radeon(word, next.unwrap()),
            "arc" => next.and_then(|n| {
                let generation = match n.chars().next()? {
                    'a' => 1,
                    'b' => 2,
                    'c' => 3,
                    _ => return None,
                };
                let tier = n[1..].chars().next()?.to_digit(10)? * 10;
                Some(GpuModel::Card { vendor: GpuVendor::Arc, generation, tier, bonus: 0, low_power: false })
            }),
            _ if word.len() == 5 && word.starts_with('x') && word[1..].chars().all(|c| c.is_ascii_digit()) => Some(GpuModel::Old),
            "uhd" | "iris" | "gma" => Some(GpuModel::Integrated),
            "intel" if w.get(i + 1).is_some_and(|s| s == "hd") => Some(GpuModel::Integrated),
            "hd" if w.get(i + 1).is_some_and(|s| s == "graphics") => Some(GpuModel::Integrated),
            "radeon" if w.get(i + 1).is_some_and(|s| s == "graphics") => Some(GpuModel::Integrated),
            _ => None,
        };
        if let Some(mut model) = found {
            if let GpuModel::Card { bonus, low_power: lp, .. } = &mut model {
                // Le numéro suit le mot : Ti, Super, XT juste après.
                *bonus = bonus_at(i + 2);
                *lp = low_power;
            }
            out.push(model);
            // « X1300 », « UHD », « Iris » : un seul mot ; les autres avec le suivant (« GMA X4500 »).
            i += if matches!(word, "uhd" | "iris") || word.starts_with('x') { 1 } else { 2 };
            continue;
        }
        i += 1;
    }
    out
}

/// Cette carte suffit-elle face à ce modèle ? Seulement des cas sûrs.
fn gpu_beats(pc: GpuModel, req: GpuModel) -> bool {
    let GpuModel::Card { vendor, generation, tier, bonus, low_power } = pc else { return false };
    // Carte dédiée récente : GTX 900, RX 400, Arc et après.
    let modern = match vendor {
        GpuVendor::Nvidia => generation >= 8,
        GpuVendor::Amd => generation >= 8,
        GpuVendor::Arc => true,
    };
    match req {
        GpuModel::Old | GpuModel::Integrated => modern && tier >= 50,
        GpuModel::Card { vendor: rv, generation: rgeneration, tier: rtier, bonus: rbonus, .. } => {
            if rv != vendor {
                return false;
            }
            if generation == rgeneration {
                return !low_power && (tier > rtier || (tier == rtier && bonus >= rbonus));
            }
            if low_power {
                return generation > rgeneration && tier >= rtier + 10;
            }
            (generation > rgeneration && tier >= rtier) || (generation >= rgeneration + 3 && tier + 10 >= rtier)
        }
    }
}

fn gpu_verdict(text: &str, hw: &Hardware) -> Verdict {
    let models = gpu_models(text);
    let dedicated = hw.gpus.iter().filter(|g| g.vram >= 1 << 30);
    if models.is_empty() {
        // Sans modèle : la mémoire vidéo demandée (« 1 Go de mémoire vidéo ») se compare, à condition
        // que la version de DirectX citée soit prise en charge.
        let t = fold(text);
        let vram_asked = ["memoire", "memory", "vram"].iter().any(|k| t.contains(k)) || words(text).iter().any(|w| w == "ram");
        let dx_need = t.find("directx").and_then(|i| directx_version(&t[i..]));
        let dx_ok = match dx_need {
            Some(need) => hw.directx.is_some_and(|have| have >= need),
            None => true,
        };
        let best = dedicated.map(|g| g.vram).max().unwrap_or(0) as f64 / GIB;
        return match gigabytes(text) {
            Some(need) if vram_asked && dx_ok && best + 0.05 >= need => Verdict::Ok,
            _ => Verdict::Unknown,
        };
    }
    let mine: Vec<GpuModel> = hw.gpus.iter().flat_map(|g| gpu_models(&g.name)).collect();
    if mine.iter().any(|&pc| models.iter().any(|&req| gpu_beats(pc, req))) { Verdict::Ok } else { Verdict::Unknown }
}

// ---------------------------------------------------------------------------------------------
// Comparaison
// ---------------------------------------------------------------------------------------------

/// Matériel de ce PC (sans le disque).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hardware {
    pub windows: String,
    /// 10 ou 11 (versions de `windows_min`).
    pub windows_major: Option<u32>,
    pub cpu: Option<String>,
    pub cores: Option<usize>,
    pub threads: Option<usize>,
    pub memory: Option<u64>,
    pub gpus: Vec<Gpu>,
    pub directx: Option<u32>,
}

fn verdict(kind: Kind, text: &str, hw: &Hardware, disk: Option<&Disk>) -> Verdict {
    match kind {
        Kind::Memory => match (gigabytes(text), hw.memory) {
            // Un quart de Gio de marge : mémoire réservée par le matériel quand elle est lue par `sysinfo`.
            (Some(need), Some(have)) if have as f64 / GIB + 0.25 >= need => Verdict::Ok,
            (Some(_), Some(_)) => Verdict::Below,
            _ => Verdict::Unknown,
        },
        Kind::Storage => match (gigabytes(text), disk) {
            (Some(need), Some(d)) if (d.free + d.game.unwrap_or(0)) as f64 / GIB >= need => Verdict::Ok,
            (Some(_), Some(_)) => Verdict::Below,
            _ => Verdict::Unknown,
        },
        Kind::System => match (windows_min(text), hw.windows_major) {
            (Some(need), Some(have)) if have >= need => Verdict::Ok,
            (Some(_), Some(_)) => Verdict::Below,
            _ => Verdict::Unknown,
        },
        Kind::Directx => match (directx_version(text), hw.directx) {
            (Some(need), Some(have)) if have >= need => Verdict::Ok,
            _ => Verdict::Unknown,
        },
        Kind::Cpu => cpu_verdict(text, hw),
        Kind::Gpu => gpu_verdict(text, hw),
        Kind::Notes => Verdict::Unknown,
    }
}

fn compare(html: Option<&str>, hw: &Hardware, disk: Option<&Disk>) -> Vec<Line> {
    html.map(lines)
        .unwrap_or_default()
        .into_iter()
        .map(|(kind, text)| Line { kind, verdict: verdict(kind, &text, hw, disk), text })
        .collect()
}

fn build(html: &Html, hw: &Hardware, disk: Option<Disk>) -> Option<Requirements> {
    let minimum = compare(html.minimum.as_deref(), hw, disk.as_ref());
    let recommended = compare(html.recommended.as_deref(), hw, disk.as_ref());
    if minimum.is_empty() && recommended.is_empty() {
        return None;
    }
    let mut gpus = hw.gpus.clone();
    gpus.sort_by_key(|g| std::cmp::Reverse(g.vram));
    let pc = Pc {
        windows: hw.windows.clone(),
        cpu: hw.cpu.clone(),
        cores: hw.cores,
        threads: hw.threads,
        memory: hw.memory,
        gpus,
        directx: hw.directx,
        disk,
    };
    Some(Requirements { minimum, recommended, pc })
}

// ---------------------------------------------------------------------------------------------
// Matériel de ce PC
// ---------------------------------------------------------------------------------------------

/// Nom lisible d'un processeur : « AMD Ryzen 7 9800X3D 8-Core Processor » → « AMD Ryzen 7 9800X3D ».
fn clean_cpu(name: &str) -> String {
    let mut s = name.replace("(R)", "").replace("(TM)", "").replace("(tm)", "").replace(['®', '™'], "");
    if let Some(i) = s.find(" @ ") {
        s.truncate(i);
    }
    let words: Vec<&str> = s.split_whitespace().collect();
    let keep: Vec<&str> = words
        .iter()
        .copied()
        .filter(|w| {
            let l = w.to_ascii_lowercase();
            !(l == "processor" || l == "cpu" || (l.ends_with("-core") && l[..l.len() - 5].chars().all(|c| c.is_ascii_digit())))
        })
        .collect();
    keep.join(" ")
}

/// Nom lisible d'une carte graphique : marques déposées retirées, espaces regroupés.
fn clean_gpu(name: &str) -> String {
    name.replace("(R)", "").replace("(TM)", "").replace(['®', '™'], "").split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(windows)]
fn windows_major() -> Option<u32> {
    use crate::library::reg;
    let key = reg::hklm(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")?;
    let build: u32 = reg::string(&key, "CurrentBuild")?.parse().ok()?;
    Some(if build >= 22_000 { 110 } else if build >= 10_240 { 100 } else { 63 })
}

#[cfg(windows)]
fn installed_memory() -> Option<u64> {
    let mut kb = 0u64;
    // SAFETY : écrit un entier dans `kb`.
    unsafe { windows::Win32::System::SystemInformation::GetPhysicallyInstalledSystemMemory(&mut kb) }.ok()?;
    Some(kb * 1024).filter(|&b| b > 0)
}

/// Cartes graphiques matérielles (DXGI) et niveau DirectX le plus haut (majeur).
#[cfg(windows)]
fn graphics() -> (Vec<Gpu>, Option<u32>) {
    use windows::Win32::Foundation::HMODULE;
    use windows::Win32::Graphics::Direct3D::*;
    use windows::Win32::Graphics::Direct3D11::{D3D11_CREATE_DEVICE_FLAG, D3D11_SDK_VERSION, D3D11CreateDevice};
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIAdapter, IDXGIFactory1};
    use windows::core::Interface;

    let mut gpus = Vec::new();
    let mut best: Option<u32> = None;
    // SAFETY : appels COM documentés, interfaces libérées à la fin de la portée.
    let Ok(factory) = (unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }) else { return (gpus, best) };
    let levels = [
        D3D_FEATURE_LEVEL_12_1,
        D3D_FEATURE_LEVEL_12_0,
        D3D_FEATURE_LEVEL_11_1,
        D3D_FEATURE_LEVEL_11_0,
        D3D_FEATURE_LEVEL_10_1,
        D3D_FEATURE_LEVEL_10_0,
        D3D_FEATURE_LEVEL_9_3,
    ];
    for i in 0..16 {
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(i) }) else { break };
        let Ok(desc) = (unsafe { adapter.GetDesc1() }) else { continue };
        if (desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0 {
            continue;
        }
        let len = desc.Description.iter().position(|&c| c == 0).unwrap_or(desc.Description.len());
        let name = clean_gpu(&String::from_utf16_lossy(&desc.Description[..len]));
        // Pilote « Microsoft Basic Render » : pas une carte.
        if name.is_empty() || name.contains("Microsoft Basic") {
            continue;
        }
        // Niveau de fonctionnalités seulement (aucun périphérique créé).
        let mut level = D3D_FEATURE_LEVEL::default();
        let base: Option<IDXGIAdapter> = adapter.cast().ok();
        let made = unsafe {
            D3D11CreateDevice(
                base.as_ref(),
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_FLAG(0),
                Some(&levels),
                D3D11_SDK_VERSION,
                None,
                Some(&mut level),
                None,
            )
        };
        if made.is_ok() {
            let major = ((level.0 as u32) >> 12) & 0xf;
            best = best.max(Some(major));
        }
        if !gpus.iter().any(|g: &Gpu| g.name == name) {
            gpus.push(Gpu { name, vram: desc.DedicatedVideoMemory as u64 });
        }
    }
    (gpus, best)
}

fn read_hardware() -> Hardware {
    let mut hw = Hardware { windows: crate::diagnostic::windows_version(), ..Hardware::default() };
    if let Ok(mut sys) = sysinfo::System::new() {
        sys.refresh_cpu_list(sysinfo::CpuRefreshKind::nothing());
        sys.refresh_memory();
        hw.cpu = sys.cpus().first().map(|c| clean_cpu(c.brand())).filter(|n| !n.is_empty());
        hw.threads = Some(sys.cpus().len()).filter(|&n| n > 0);
        hw.memory = Some(sys.total_memory()).filter(|&m| m > 0);
    }
    hw.cores = sysinfo::System::physical_core_count().ok().filter(|&n| n > 0);
    #[cfg(windows)]
    {
        hw.windows_major = windows_major();
        if let Some(m) = installed_memory() {
            hw.memory = Some(m);
        }
        (hw.gpus, hw.directx) = graphics();
    }
    hw
}

/// Matériel de ce PC, lu au premier appel puis gardé.
pub fn hardware() -> &'static Hardware {
    static HW: OnceLock<Hardware> = OnceLock::new();
    HW.get_or_init(read_hardware)
}

/// Disque du dossier d'installation du jeu (place libre relue maintenant).
pub fn disk_of(game: &Game) -> Option<Disk> {
    let dir = game.install_dir.as_deref()?;
    let volumes = crate::storage::volumes();
    let v = crate::storage::volume_of(dir, &volumes)?;
    Some(Disk { letter: v.mount.trim_end_matches(['\\', '/']).to_string(), free: v.free, game: game.size_on_disk.filter(|&s| s > 0) })
}

// ---------------------------------------------------------------------------------------------
// Cache et requêtes
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Entry {
    /// Date de la requête (secondes Unix).
    checked: u64,
    /// `None` : aucune configuration connue.
    #[serde(default)]
    html: Option<Html>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ReqCache {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    apps: BTreeMap<u32, Entry>,
    #[serde(skip)]
    path: PathBuf,
    /// Langue de ce cache (celle de Ludex au chargement), `None` avant `load`.
    #[serde(skip)]
    lang: Option<crate::i18n::Lang>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Demande la fiche d'un appid à la boutique (une requête, sans redirection), dans la langue de
/// Ludex. Toujours `Some` en cas de réponse : configuration vide si la boutique n'en a pas, marquée
/// de la langue de la requête pour `ReqCache::record`.
pub fn request(appid: u32) -> Result<Option<Html>, String> {
    let lang = crate::i18n::current();
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .redirects(0)
        .user_agent("Ludex (lanceur de jeux)")
        .build();
    let response = agent
        .get(APPDETAILS)
        .query("appids", &appid.to_string())
        .query("l", lang.steam())
        .query("filters", "basic")
        .call()
        .map_err(|e| e.to_string())?;
    if response.status() != 200 {
        let status = response.status();
        return Err(tr!("réponse {status}", "response {status}"));
    }
    let mut body = String::new();
    std::io::Read::read_to_string(&mut std::io::Read::take(response.into_reader(), MAX_BODY), &mut body).map_err(|e| e.to_string())?;
    let html = parse_response(&body, appid)?.unwrap_or_default();
    Ok(Some(Html { lang: Some(lang), ..html }))
}

impl ReqCache {
    /// Cache de la langue actuelle de Ludex.
    pub fn load(dir: &Path) -> Self {
        Self::load_for(dir, crate::i18n::current())
    }

    fn load_for(dir: &Path, lang: crate::i18n::Lang) -> Self {
        let path = crate::metadata::language_file(dir, "requirements-cache", lang);
        let mut cache: ReqCache = fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        if cache.version != CACHE_VERSION {
            cache.apps.clear();
            cache.version = CACHE_VERSION;
        }
        cache.path = path;
        cache.lang = Some(lang);
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

    /// Appid jamais demandé, ou demandé il y a plus de 30 jours.
    pub fn stale(&self, appid: u32) -> bool {
        self.apps.get(&appid).is_none_or(|e| now().saturating_sub(e.checked) > REFRESH_AFTER)
    }

    /// Note la réponse. Une réponse vide n'efface pas une configuration connue. Une réponse lue
    /// dans une autre langue que celle du cache (langue changée pendant la requête) est jetée :
    /// l'appid reste à demander dans la langue du cache.
    pub fn record(&mut self, appid: u32, html: Option<Html>) {
        if html.as_ref().and_then(|h| h.lang).is_some_and(|l| self.lang.is_some_and(|mine| mine != l)) {
            return;
        }
        let html = html.map(|h| Html { lang: None, ..h }).filter(|h| !h.is_empty());
        let html = html.or_else(|| self.apps.get(&appid).and_then(|e| e.html.clone()));
        self.apps.insert(appid, Entry { checked: now(), html });
    }

    /// Configuration connue du jeu, comparée au matériel.
    pub fn requirements(&self, appid: u32, hw: &Hardware, disk: Option<Disk>) -> Option<Requirements> {
        build(self.apps.get(&appid)?.html.as_ref()?, hw, disk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(name: &str) -> &'static str {
        match name {
            "cyberpunk" => include_str!("requirements_samples/cyberpunk.json"),
            "terraria" => include_str!("requirements_samples/terraria.json"),
            "cs2" => include_str!("requirements_samples/cs2.json"),
            "sims4" => include_str!("requirements_samples/sims4.json"),
            "rocket-league" => include_str!("requirements_samples/rocket-league.json"),
            "wallpaper-engine" => include_str!("requirements_samples/wallpaper-engine.json"),
            "gtav" => include_str!("requirements_samples/gtav.json"),
            _ => unreachable!(),
        }
    }

    fn html(name: &str, appid: u32) -> Html {
        parse_response(sample(name), appid).unwrap().unwrap()
    }

    fn text(lines: &[(Kind, String)], kind: Kind) -> &str {
        lines.iter().find(|(k, _)| *k == kind).map(|(_, t)| t.as_str()).unwrap_or("")
    }

    /// PC de test : Ryzen 7 9800X3D, 32 Go, RTX 3070 et graphiques intégrés, Windows 11.
    fn pc() -> Hardware {
        Hardware {
            windows: "Windows 11 Home".into(),
            windows_major: Some(110),
            cpu: Some(clean_cpu("AMD Ryzen 7 9800X3D 8-Core Processor            ")),
            cores: Some(8),
            threads: Some(16),
            memory: Some(32 << 30),
            gpus: vec![
                Gpu { name: "NVIDIA GeForce RTX 3070".into(), vram: 8 << 30 },
                Gpu { name: clean_gpu("AMD Radeon(TM) Graphics"), vram: 512 << 20 },
            ],
            directx: Some(12),
        }
    }

    fn disk(free_gb: u64) -> Disk {
        Disk { letter: "E:".into(), free: free_gb << 30, game: None }
    }

    #[test]
    fn parses_responses() {
        let cp = html("cyberpunk", 1091500);
        assert!(cp.minimum.as_deref().unwrap().contains("Core i7-6700"));
        assert!(cp.recommended.is_some());
        let cs2 = html("cs2", 730);
        assert!(cs2.minimum.is_some() && cs2.recommended.is_none());
        // Inconnu, sans configuration, tableau vide, appid absent, illisible.
        assert_eq!(parse_response(r#"{"1":{"success":false}}"#, 1), Ok(None));
        assert_eq!(parse_response(r#"{"5":{"success":true,"data":{"pc_requirements":[]}}}"#, 5), Ok(None));
        assert_eq!(parse_response(r#"{"5":{"success":true,"data":{"pc_requirements":{"minimum":"  "}}}}"#, 5), Ok(None));
        assert_eq!(parse_response(r#"{"5":{"success":true,"data":{}}}"#, 5), Ok(None));
        assert!(parse_response(r#"{"6":{"success":true}}"#, 5).is_err());
        assert!(parse_response("<html>", 5).is_err());
        // HTML démesuré refusé.
        let huge = format!(r#"{{"5":{{"success":true,"data":{{"pc_requirements":{{"minimum":"{}"}}}}}}}}"#, "a".repeat(MAX_HTML + 1));
        assert_eq!(parse_response(&huge, 5), Ok(None));
    }

    #[test]
    fn converts_html_to_lines() {
        let cp = lines(html("cyberpunk", 1091500).minimum.as_deref().unwrap());
        let kinds: Vec<Kind> = cp.iter().map(|(k, _)| *k).collect();
        assert_eq!(kinds, vec![Kind::System, Kind::Cpu, Kind::Memory, Kind::Gpu, Kind::Directx, Kind::Storage, Kind::Notes]);
        assert_eq!(text(&cp, Kind::System), "64-bit Windows 10");
        assert_eq!(text(&cp, Kind::Cpu), "Core i7-6700 or Ryzen 5 1600");
        assert_eq!(text(&cp, Kind::Memory), "12 GB de mémoire");
        assert_eq!(text(&cp, Kind::Gpu), "GeForce GTX 1060 6GB or Radeon RX 580 8GB or Arc A380");
        assert_eq!(text(&cp, Kind::Directx), "Version 12");
        assert_eq!(text(&cp, Kind::Storage), "70 GB d'espace disque disponible");
        assert!(text(&cp, Kind::Notes).starts_with("SSD required."));
        assert!(cp.iter().all(|(_, t)| !t.contains('<') && !t.contains("&nbsp;")));

        // Ancien format : libellé et valeur dans le même `<strong>`, en anglais avec des tabulations.
        let terraria = html("terraria", 105600);
        let min = lines(terraria.minimum.as_deref().unwrap());
        assert_eq!(text(&min, Kind::System), "Windows XP, Vista, 7");
        assert_eq!(text(&min, Kind::Memory), "512 Mo de RAM");
        assert_eq!(text(&min, Kind::Storage), "200 Mo d'espace disque disponible");
        assert_eq!(text(&min, Kind::Directx), "9.0c ou ultérieure");
        let rec = lines(terraria.recommended.as_deref().unwrap());
        assert_eq!(text(&rec, Kind::System), "Windows 7, 8/8.1, 10");
        assert_eq!(text(&rec, Kind::Cpu), "Dual Core 3.0 Ghz");
        assert_eq!(text(&rec, Kind::Memory), "4GB");
        assert_eq!(text(&rec, Kind::Storage), "200MB");
        assert_eq!(text(&rec, Kind::Gpu), "256mb Video Memory, capable of Shader Model 2.0+");

        // Libellé avec astérisque, ligne « Réseau » écartée.
        let rl = lines(html("rocket-league", 252950).minimum.as_deref().unwrap());
        assert_eq!(text(&rl, Kind::System), "Windows 7 (64 bit) or Newer (64 bit) Windows OS");
        assert_eq!(rl.len(), 6);
        // « Carte son » écartée.
        let gta = lines(html("gtav", 271590).minimum.as_deref().unwrap());
        assert!(!gta.iter().any(|(_, t)| t.contains("compatible avec DirectX 10")));

        // Entités, libellé hors `<strong>`, morceau sans libellé qui prolonge une note.
        let made = lines("<p>OS: Windows&nbsp;10 &amp; 11</p><p>Additional Notes: first<br>second part</p><p>RAM: 8&#160;GB</p>");
        assert_eq!(text(&made, Kind::System), "Windows 10 & 11");
        assert_eq!(text(&made, Kind::Notes), "first second part");
        assert_eq!(text(&made, Kind::Memory), "8 GB");
        assert!(lines("").is_empty());
        assert!(lines("<strong>Minimum:</strong><br>Rien ici").is_empty());
        // Texte trop long coupé.
        let long = lines(&format!("<li>Processor: {}</li>", "x".repeat(1000)));
        assert_eq!(long[0].1.chars().count(), MAX_TEXT + 1);
    }

    #[test]
    fn reads_numbers() {
        assert_eq!(gigabytes("12 GB de mémoire"), Some(12.0));
        assert_eq!(gigabytes("512 Mo de RAM"), Some(0.5));
        assert_eq!(gigabytes("4GB"), Some(4.0));
        assert_eq!(gigabytes("1,5 Go"), Some(1.5));
        assert_eq!(gigabytes("2 To"), Some(2048.0));
        assert_eq!(gigabytes("1024\u{a0}MB de mémoire"), Some(1.0));
        assert_eq!(gigabytes("Version 12"), None);
        assert_eq!(gigabytes("5 Mods"), None);
        assert_eq!(gigabytes("DDR4 8 GB"), Some(8.0));
        assert_eq!(windows_min("Windows XP, Vista, 7"), Some(51));
        assert_eq!(windows_min("Windows 7, 8/8.1, 10"), Some(61));
        assert_eq!(windows_min("64-bit Windows 10"), Some(100));
        assert_eq!(windows_min("Windows® 11 64-bit"), Some(110));
        assert_eq!(windows_min("64 bits requis"), None);
        assert_eq!(windows_min("32-bit: Vista, Win 7, Win 8"), Some(60));
        assert_eq!(core_count("Quadri-cœur 2.3 GHz"), Some(4));
        assert_eq!(directx_version("Version 12"), Some(12));
        assert_eq!(directx_version("9.0c ou ultérieure"), Some(9));
        assert_eq!(directx_version("Version 2024"), None);
        assert_eq!(core_count("2.5 GHz Dual core"), Some(2));
        assert_eq!(core_count("3.0+ GHz Quad core"), Some(4));
        assert_eq!(core_count("processeur 4 cœurs"), Some(4));
        assert_eq!(core_count("1.6 Ghz"), None);
    }

    #[test]
    fn recognizes_processors() {
        let fam = |brand, tier, generation| CpuModel::Family { brand, tier, generation, low_power: false };
        assert_eq!(cpu_models("Core i7-6700 or Ryzen 5 1600"), vec![fam(Brand::Intel, 7, Some(6)), fam(Brand::Ryzen, 5, Some(1))]);
        assert_eq!(cpu_models("Intel Core i5-750"), vec![fam(Brand::Intel, 5, Some(1))]);
        assert_eq!(cpu_models("Intel Core i5-12400F"), vec![fam(Brand::Intel, 5, Some(12))]);
        assert_eq!(cpu_models("Intel Core i7 (4 coeurs), AMD Ryzen 5"), vec![fam(Brand::Intel, 7, None), fam(Brand::Ryzen, 5, None)]);
        assert_eq!(cpu_models("AMD Ryzen 7 9800X3D"), vec![fam(Brand::Ryzen, 7, Some(9))]);
        assert_eq!(
            cpu_models("Intel Core i7-1165G7"),
            vec![CpuModel::Family { brand: Brand::Intel, tier: 7, generation: Some(11), low_power: true }]
        );
        assert_eq!(cpu_models("Intel Core Ultra 7 155H"), vec![fam(Brand::Intel, 7, Some(15))]);
        assert_eq!(cpu_models("Intel Core 2 Quad CPU Q6600 / AMD Phenom 9850 Quad-Core"), vec![CpuModel::Old, CpuModel::Old, CpuModel::Old]);
        assert_eq!(cpu_models("Intel Core i3-4170 ou AMD FX-8120"), vec![fam(Brand::Intel, 3, Some(4)), CpuModel::Fx]);
        assert!(cpu_models("2.5 GHz Dual core").is_empty());
        assert_eq!(clean_cpu("Intel(R) Core(TM) i7-6700K CPU @ 4.00GHz"), "Intel Core i7-6700K");
        assert_eq!(clean_cpu("AMD Ryzen 7 9800X3D 8-Core Processor   "), "AMD Ryzen 7 9800X3D");
    }

    #[test]
    fn compares_processors() {
        let hw = pc();
        assert_eq!(cpu_verdict("Core i7-6700 or Ryzen 5 1600", &hw), Verdict::Ok);
        assert_eq!(cpu_verdict("Core i7-12700 or Ryzen 7 7800X3D", &hw), Verdict::Ok);
        assert_eq!(cpu_verdict("processeur Intel Core i5-750 à 4 threads (ou mieux)", &hw), Verdict::Ok);
        assert_eq!(cpu_verdict("1.66 GHz Intel i5 or equivalent", &hw), Verdict::Ok);
        assert_eq!(cpu_verdict("Intel Core 2 Quad CPU Q6600 @ 2,40 GHz", &hw), Verdict::Ok);
        assert_eq!(cpu_verdict("2.5 GHz Dual core", &hw), Verdict::Ok);
        // Plus haut de gamme qu'une génération proche : on ne sait pas.
        assert_eq!(cpu_verdict("Ryzen 9 9950X", &hw), Verdict::Unknown);
        assert_eq!(cpu_verdict("Intel Core i9-14900K", &hw), Verdict::Unknown);
        assert_eq!(cpu_verdict("1.6 Ghz", &hw), Verdict::Unknown);
        assert_eq!(cpu_verdict("Intel Xeon E5", &hw), Verdict::Unknown);
        // Moins de cœurs qu'écrit : en dessous.
        let small = Hardware { cores: Some(2), ..pc() };
        assert_eq!(cpu_verdict("3.0+ GHz Quad core", &small), Verdict::Below);
        // Portable basse consommation : seulement face à une famille sans numéro.
        let laptop = Hardware { cpu: Some("Intel Core i7-1165G7".into()), ..pc() };
        assert_eq!(cpu_verdict("Intel Core i7-8700", &laptop), Verdict::Unknown);
        assert_eq!(cpu_verdict("Intel Core i5", &laptop), Verdict::Ok);
        // Bien plus récent, gamme un peu plus basse.
        let i5 = Hardware { cpu: Some("Intel(R) Core(TM) i5-12400F".into()), ..pc() };
        assert_eq!(cpu_verdict("Core i7-6700 or Ryzen 5 1600", &i5), Verdict::Ok);
        assert_eq!(cpu_verdict("Core i7-12700", &i5), Verdict::Unknown);
        // Processeur inconnu de ce PC : rien.
        let none = Hardware { cpu: None, ..pc() };
        assert_eq!(cpu_verdict("Core i7-6700", &none), Verdict::Unknown);
    }

    #[test]
    fn recognizes_graphics_cards() {
        let card = |vendor, generation, tier, bonus| GpuModel::Card { vendor, generation, tier, bonus, low_power: false };
        let rank = |g: u32| NVIDIA_GENS.iter().position(|&x| x == g).unwrap() as u32 + 1;
        assert_eq!(
            gpu_models("GeForce GTX 1060 6GB or Radeon RX 580 8GB or Arc A380"),
            vec![card(GpuVendor::Nvidia, rank(10), 60, 0), card(GpuVendor::Amd, 9, 80, 0), card(GpuVendor::Arc, 1, 30, 0)]
        );
        assert_eq!(
            gpu_models("GeForce RTX 2060 SUPER or Radeon RX 5700 XT"),
            vec![card(GpuVendor::Nvidia, rank(20), 60, 1), card(GpuVendor::Amd, 10, 70, 1)]
        );
        assert_eq!(gpu_models("NVIDIA GeForce 760, AMD Radeon R7 270X"), vec![card(GpuVendor::Nvidia, rank(7), 60, 0), card(GpuVendor::Amd, 6, 70, 0)]);
        assert_eq!(gpu_models("NVIDIA GeForce GTX 660, AMD HD7870, 2 GB VRAM"), vec![card(GpuVendor::Nvidia, rank(6), 60, 0), card(GpuVendor::Amd, 4, 87, 0)]);
        assert_eq!(gpu_models("NVIDIA 9800 GT 1 Go / AMD HD 4870 1 Go"), vec![GpuModel::Old, GpuModel::Old]);
        assert_eq!(gpu_models("NVIDIA GeForce 6600 ou supérieure, ATI Radeon X1300, Intel GMA X4500"), vec![GpuModel::Old, GpuModel::Old, GpuModel::Integrated]);
        assert_eq!(gpu_models("HD Graphics 4000 or above"), vec![GpuModel::Integrated]);
        assert_eq!(gpu_models("AMD Radeon Graphics"), vec![GpuModel::Integrated]);
        assert_eq!(gpu_models(&clean_gpu("AMD Radeon(TM) Graphics")), vec![GpuModel::Integrated]);
        assert!(gpu_models("256mb Video Memory, capable of Shader Model 2.0+").is_empty());
        assert_eq!(
            gpu_models("NVIDIA GeForce RTX 3070 Laptop GPU"),
            vec![GpuModel::Card { vendor: GpuVendor::Nvidia, generation: rank(30), tier: 70, bonus: 0, low_power: true }]
        );
    }

    #[test]
    fn compares_graphics_cards() {
        let hw = pc();
        assert_eq!(gpu_verdict("GeForce GTX 1060 6GB or Radeon RX 580 8GB or Arc A380", &hw), Verdict::Ok);
        assert_eq!(gpu_verdict("GeForce RTX 2060 SUPER or Radeon RX 5700 XT or Arc A770", &hw), Verdict::Ok);
        assert_eq!(gpu_verdict("NVIDIA GeForce 6600 ou supérieure, ATI Radeon X1300", &hw), Verdict::Ok);
        assert_eq!(gpu_verdict("HD Graphics 4000 or above", &hw), Verdict::Ok);
        // Sans modèle : la mémoire vidéo et DirectX.
        assert_eq!(gpu_verdict("la carte graphique doit avoir au moins 1 Go de mémoire et être compatible avec DirectX 11", &hw), Verdict::Ok);
        assert_eq!(gpu_verdict("256mb Video Memory, capable of Shader Model 2.0+", &hw), Verdict::Ok);
        assert_eq!(gpu_verdict("512 Mo de RAM", &hw), Verdict::Ok);
        assert_eq!(gpu_verdict("12 Go de mémoire vidéo", &hw), Verdict::Unknown);
        assert_eq!(gpu_verdict("Carte compatible DirectX 11", &hw), Verdict::Unknown);
        // Gamme au-dessus, autre marque seule : on ne sait pas.
        assert_eq!(gpu_verdict("GeForce RTX 4080", &hw), Verdict::Unknown);
        assert_eq!(gpu_verdict("GeForce RTX 3070 Ti", &hw), Verdict::Unknown);
        assert_eq!(gpu_verdict("Radeon RX 6800", &hw), Verdict::Unknown);
        let rtx3050 = Hardware { gpus: vec![Gpu { name: "NVIDIA GeForce RTX 3050".into(), vram: 8 << 30 }], ..pc() };
        assert_eq!(gpu_verdict("GeForce GTX 1060", &rtx3050), Verdict::Ok);
        assert_eq!(gpu_verdict("GeForce GTX 1080", &rtx3050), Verdict::Unknown);
        let gtx1050 = Hardware { gpus: vec![Gpu { name: "NVIDIA GeForce GTX 1050".into(), vram: 2 << 30 }], ..pc() };
        assert_eq!(gpu_verdict("GeForce GTX 770", &gtx1050), Verdict::Unknown);
        // Seulement des graphiques intégrés : rien.
        let igpu = Hardware { gpus: vec![Gpu { name: "Intel UHD Graphics 770".into(), vram: 128 << 20 }], ..pc() };
        assert_eq!(gpu_verdict("HD Graphics 4000 or above", &igpu), Verdict::Unknown);
        assert_eq!(gpu_verdict("1 Go de mémoire vidéo", &igpu), Verdict::Unknown);
    }

    #[test]
    fn compares_whole_configurations() {
        let hw = pc();
        let cp = html("cyberpunk", 1091500);
        let req = build(&cp, &hw, Some(disk(500))).unwrap();
        let v = |lines: &[Line], kind| lines.iter().find(|l| l.kind == kind).unwrap().verdict;
        for kind in [Kind::System, Kind::Cpu, Kind::Memory, Kind::Gpu, Kind::Directx, Kind::Storage] {
            assert_eq!(v(&req.minimum, kind), Verdict::Ok, "{kind:?}");
        }
        assert_eq!(v(&req.minimum, Kind::Notes), Verdict::Unknown);
        assert_eq!(v(&req.recommended, Kind::Cpu), Verdict::Ok); // Ryzen 7 7800X3D
        assert_eq!(v(&req.recommended, Kind::Gpu), Verdict::Ok); // RTX 2060 SUPER
        assert_eq!(req.pc.gpus[0].name, "NVIDIA GeForce RTX 3070");

        // Petit PC : mémoire et disque en dessous, Windows 10.
        let small = Hardware { memory: Some(8 << 30), windows_major: Some(100), ..pc() };
        let req = build(&cp, &small, Some(disk(40))).unwrap();
        assert_eq!(v(&req.minimum, Kind::Memory), Verdict::Below);
        assert_eq!(v(&req.minimum, Kind::Storage), Verdict::Below);
        assert_eq!(v(&req.minimum, Kind::System), Verdict::Ok);
        // Jeu installé : sa propre place compte comme disponible.
        let installed = Disk { game: Some(65 << 30), ..disk(10) };
        assert_eq!(v(&build(&cp, &small, Some(installed)).unwrap().minimum, Kind::Storage), Verdict::Ok);
        // Sans disque connu : stockage inconnu.
        assert_eq!(v(&build(&cp, &small, None).unwrap().minimum, Kind::Storage), Verdict::Unknown);
        // Windows trop ancien.
        let win10 = Hardware { windows_major: Some(100), ..pc() };
        assert_eq!(verdict(Kind::System, "Windows 11 64-bit", &win10, None), Verdict::Below);
        // Marge de la mémoire lue par sysinfo.
        let almost = Hardware { memory: Some((16 << 30) - (200 << 20)), ..pc() };
        assert_eq!(verdict(Kind::Memory, "16 GB RAM", &almost, None), Verdict::Ok);
        // DirectX trop récent pour la carte : on ne sait pas.
        let dx11 = Hardware { directx: Some(11), ..pc() };
        assert_eq!(verdict(Kind::Directx, "Version 12", &dx11, None), Verdict::Unknown);

        // Configuration vide : rien.
        assert!(build(&Html { minimum: Some("<strong>Minimum:</strong>".into()), recommended: None, lang: None }, &hw, None).is_none());

        // Toutes les réponses enregistrées se lisent sans perte : chaque ligne a un texte.
        for (name, id) in [("terraria", 105600), ("cs2", 730), ("sims4", 1222670), ("rocket-league", 252950), ("wallpaper-engine", 431960), ("gtav", 271590)] {
            let req = build(&html(name, id), &hw, Some(disk(500))).unwrap();
            assert!(!req.minimum.is_empty(), "{name}");
            assert!(req.minimum.iter().chain(&req.recommended).all(|l| !l.text.is_empty()), "{name}");
            // Ce PC récent n'est jamais « en dessous » de ces jeux.
            assert!(req.minimum.iter().chain(&req.recommended).all(|l| l.verdict != Verdict::Below), "{name}");
        }
    }

    #[test]
    fn caches_answers() {
        let dir = std::env::temp_dir().join(format!("ludex-req-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut cache = ReqCache::load(&dir);
        assert!(cache.stale(1091500));
        cache.record(1091500, Some(html("cyberpunk", 1091500)));
        cache.record(1, None);
        assert!(!cache.stale(1091500) && !cache.stale(1));
        // Une réponse vide n'efface pas ce qui est connu.
        cache.record(1091500, None);
        assert!(cache.requirements(1091500, &pc(), None).is_some());
        assert!(cache.requirements(1, &pc(), None).is_none());
        assert!(cache.requirements(42, &pc(), None).is_none());
        cache.save().unwrap();
        assert!(!dir.join("requirements-cache.json.tmp").exists());

        let mut reloaded = ReqCache::load(&dir);
        assert_eq!(reloaded.apps.len(), 2);
        assert!(reloaded.requirements(1091500, &pc(), None).is_some());
        // Redemandé après 30 jours.
        reloaded.apps.get_mut(&1091500).unwrap().checked = now() - REFRESH_AFTER - 1;
        assert!(reloaded.stale(1091500));
        // Ancienne version : tout est relu.
        reloaded.version = CACHE_VERSION + 1;
        reloaded.save().unwrap();
        assert!(ReqCache::load(&dir).apps.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn one_cache_per_language() {
        use crate::i18n::{with, Lang};
        let dir = std::env::temp_dir().join(format!("ludex-req-lang-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let tagged = |lang: Lang| Some(Html { lang: Some(lang), ..html("cyberpunk", 1091500) });

        let mut french = ReqCache::load(&dir);
        french.record(1091500, tagged(Lang::Fr));
        // Réponse anglaise arrivée après un changement de langue : jetée.
        french.record(10, tagged(Lang::En));
        assert!(!french.stale(1091500) && french.stale(10));
        french.save().unwrap();
        assert!(dir.join("requirements-cache.json").is_file());

        with(Lang::En, || {
            let mut english = ReqCache::load(&dir);
            assert!(english.stale(1091500), "le cache anglais ne reprend pas le français");
            english.record(1091500, tagged(Lang::En));
            english.record(10, tagged(Lang::Fr));
            // Réponse sans configuration : rien de connu, mais la date est notée.
            english.record(20, Some(Html { lang: Some(Lang::En), ..Html::default() }));
            assert!(english.stale(10) && !english.stale(20));
            assert!(english.requirements(20, &pc(), None).is_none());
            english.save().unwrap();
        });
        assert!(dir.join("requirements-cache.en.json").is_file());
        assert_eq!(ReqCache::load(&dir).apps.len(), 1);
        assert_eq!(ReqCache::load_for(&dir, Lang::En).apps.len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    /// Vraies configurations de quelques jeux de ce PC et matériel lu :
    /// `cargo test requirements_this_pc -- --ignored --nocapture`. Cache dans un dossier temporaire
    /// (jamais celui de l'app), supprimé à la fin.
    #[test]
    #[ignore]
    fn requirements_this_pc() {
        let started = std::time::Instant::now();
        let hw = hardware();
        println!("Matériel lu en {} ms :", started.elapsed().as_millis());
        println!("  {}", hw.windows);
        println!("  {:?} · {:?} cœurs · {:?} threads", hw.cpu, hw.cores, hw.threads);
        println!("  mémoire installée {:.1} Gio", hw.memory.unwrap_or(0) as f64 / GIB);
        for g in &hw.gpus {
            println!("  carte : {} ({:.1} Gio dédiés)", g.name, g.vram as f64 / GIB);
        }
        println!("  DirectX {:?}", hw.directx);

        let games: Vec<Game> = crate::library::scan().games.into_iter().filter(|g| g.steam_appid.is_some() && !g.is_tool).take(8).collect();
        let tmp = std::env::temp_dir().join(format!("ludex-req-pc-{}", std::process::id()));
        let mut cache = ReqCache::load(&tmp);
        for (i, g) in games.iter().enumerate() {
            let appid = g.steam_appid.unwrap();
            if i > 0 {
                std::thread::sleep(Duration::from_millis(500));
            }
            let t = std::time::Instant::now();
            match request(appid) {
                Ok(html) => cache.record(appid, html),
                Err(e) => {
                    println!("{} ({appid}) : erreur {e}", g.title);
                    continue;
                }
            }
            let ms = t.elapsed().as_millis();
            let Some(req) = cache.requirements(appid, hw, disk_of(g)) else {
                println!("\n{} ({appid}, {ms} ms) : aucune configuration", g.title);
                continue;
            };
            println!("\n{} ({appid}, {ms} ms), disque {:?}", g.title, req.pc.disk.as_ref().map(|d| (&d.letter, d.free >> 30)));
            for (name, list) in [("min", &req.minimum), ("rec", &req.recommended)] {
                for l in list.iter() {
                    println!("  {name} {:?} [{:?}] {}", l.kind, l.verdict, l.text);
                }
            }
        }
        cache.save().unwrap();
        let _ = fs::remove_dir_all(&tmp);
    }
}
