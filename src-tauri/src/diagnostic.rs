//! Page de diagnostic des launchers (Paramètres) : pour chaque source, état, emplacements lus,
//! jeux écartés et durée de lecture, d'après le dernier scan (aucun rescan) ; état des services en
//! ligne depuis le démarrage ; versions de Ludex, de Windows et de WebView2. Le rapport texte à
//! copier est anonymisé : nom d'utilisateur Windows et numéro de compte Steam remplacés, aucun
//! contenu de `user-data.json` hormis le nombre de jeux ajoutés à la main. Le rapport se termine par
//! les 50 dernières lignes du journal d'erreurs (`logfile.rs`).

use crate::library::{Game, Location, Platform, Skipped, SourceReport};
use crate::{AppState, blocking, lock};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use tauri::Manager;

/// Services en ligne suivis : dernière réussite et dernière erreur depuis le démarrage.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Service {
    Images,
    Art,
    Details,
    Hltb,
    Saves,
    Requirements,
    News,
    Workshop,
    Wishlist,
    Gog,
    Epic,
    Steam,
}

impl Service {
    pub const ALL: [Service; 12] = [
        Service::Images,
        Service::Art,
        Service::Details,
        Service::Hltb,
        Service::Saves,
        Service::Requirements,
        Service::News,
        Service::Workshop,
        Service::Wishlist,
        Service::Gog,
        Service::Epic,
        Service::Steam,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Service::Images => tr_str!("Cache des images", "Image cache"),
            Service::Art => tr_str!("Recherche des jaquettes", "Cover search"),
            Service::Details => tr_str!("Fiches des jeux", "Game details"),
            Service::Hltb => tr_str!("Durées de vie", "Time to beat"),
            Service::Saves => tr_str!("Base des sauvegardes", "Save locations database"),
            Service::Requirements => tr_str!("Configurations requises", "System requirements"),
            Service::News => tr_str!("Actualités des jeux", "Game news"),
            Service::Workshop => tr_str!("Mods Steam Workshop", "Steam Workshop mods"),
            Service::Wishlist => tr_str!("Prix de la liste de souhaits", "Wishlist prices"),
            Service::Gog => tr_str!("Compte GOG", "GOG account"),
            Service::Epic => tr_str!("Compte Epic Games", "Epic Games account"),
            Service::Steam => tr_str!("Jeux Steam possédés", "Owned Steam games"),
        }
    }

    /// Ce que le service contacte.
    pub fn host(self) -> &'static str {
        match self {
            Service::Images => tr_str!("CDN de Steam et du Microsoft Store", "Steam and Microsoft Store CDN"),
            Service::Art => tr_str!("boutique Steam, Microsoft Store", "Steam store, Microsoft Store"),
            Service::Details => tr_str!("boutique Steam", "Steam store"),
            Service::Hltb => "HowLongToBeat",
            Service::Saves => tr_str!("manifeste Ludusavi sur GitHub", "Ludusavi manifest on GitHub"),
            Service::Requirements => tr_str!("boutique Steam", "Steam store"),
            Service::News => tr_str!("annonces Steam", "Steam announcements"),
            Service::Workshop => tr_str!("API publique de Steam", "Steam public API"),
            Service::Wishlist => tr_str!("boutique Steam", "Steam store"),
            Service::Gog => tr_str!("API de GOG", "GOG API"),
            Service::Epic => tr_str!("API d'Epic Games", "Epic Games API"),
            Service::Steam => tr_str!("API Web de Steam (clé personnelle)", "Steam Web API (personal key)"),
        }
    }

    fn index(self) -> usize {
        Service::ALL.iter().position(|&s| s == self).unwrap_or(0)
    }
}

/// Suivi d'un service depuis le démarrage de Ludex.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Track {
    last_ok: Option<u64>,
    last_error: Option<(u64, String)>,
}

static SERVICES: Mutex<[Track; Service::ALL.len()]> =
    Mutex::new([const { Track { last_ok: None, last_error: None } }; Service::ALL.len()]);

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Requête réussie pour ce service.
pub fn ok(service: Service) {
    if let Ok(mut all) = SERVICES.lock() {
        all[service.index()].last_ok = Some(now());
    }
}

/// Erreur réseau (ou réponse refusée) pour ce service.
pub fn failed(service: Service, error: impl std::fmt::Display) {
    if let Ok(mut all) = SERVICES.lock() {
        all[service.index()].last_error = Some((now(), error.to_string()));
    }
}

/// État d'une source, en un mot.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceState {
    /// Pas encore lue (aucun scan depuis le démarrage).
    Unknown,
    /// Launcher absent de ce PC.
    Absent,
    /// Installé, aucun jeu.
    Empty,
    Games,
    /// Désactivé dans les Paramètres : `count` = jeux écartés.
    Disabled,
    Error,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceDiag {
    /// Plateforme (`steam`, `epic`…) ou `manual` pour les jeux ajoutés à la main.
    pub id: String,
    pub label: String,
    pub state: SourceState,
    pub count: usize,
    pub error: Option<String>,
    /// Durée de lecture au dernier scan, `None` si inconnue.
    pub millis: Option<u64>,
    pub locations: Vec<Location>,
    pub skipped: Vec<Skipped>,
    /// Applis de cette source masquées d'office (éditeurs, serveurs…).
    pub tools: Vec<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceDiag {
    pub id: Service,
    pub label: String,
    pub host: String,
    /// Une lecture tourne en ce moment.
    pub busy: bool,
    pub last_ok: Option<u64>,
    pub last_error: Option<String>,
    pub error_at: Option<u64>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub version: String,
    pub windows: String,
    pub webview: Option<String>,
    pub data_dir: String,
    /// Fin du dernier scan (secondes Unix), `None` avant le premier.
    pub scanned_at: Option<u64>,
    pub created: u64,
    pub sources: Vec<SourceDiag>,
    pub services: Vec<ServiceDiag>,
    /// Statut Discord (client local, aucun réseau).
    pub discord: crate::discord::Status,
    /// Compte GOG connecté ou non (jamais d'identifiant).
    pub gog: crate::gog_online::AccountState,
    /// Compte Epic connecté ou non (jamais d'identifiant).
    pub epic: crate::gog_online::AccountState,
    /// Clé Web API Steam enregistrée ou non (jamais la clé).
    pub steam_key: crate::gog_online::AccountState,
    /// Rapport texte anonymisé, prêt à copier.
    pub report: String,
}

/// Ce que la commande rassemble avant de construire le diagnostic.
pub struct Inputs<'a> {
    pub version: String,
    pub windows: String,
    pub webview: Option<String>,
    pub data_dir: PathBuf,
    /// Dernier scan : date et rapports des sources (`library::last_scan`).
    pub last: Option<(u64, Vec<SourceReport>)>,
    /// Jeux de l'état (outils masqués d'office, jeux manuels présents).
    pub games: &'a [Game],
    /// Jeux ajoutés à la main : titre, exécutable présent.
    pub manual: Vec<(String, bool)>,
    /// Services dont une lecture tourne.
    pub busy: Vec<Service>,
    /// État du fil du statut Discord.
    pub discord: crate::discord::Status,
    /// État du compte GOG.
    pub gog: crate::gog_online::AccountState,
    /// État du compte Epic.
    pub epic: crate::gog_online::AccountState,
    /// État de la clé Web API Steam.
    pub steam_key: crate::gog_online::AccountState,
}

/// Sources dans l'ordre du scan.
const PLATFORMS: [Platform; 12] = [
    Platform::Steam,
    Platform::Epic,
    Platform::Gog,
    Platform::Ubisoft,
    Platform::Ea,
    Platform::Battlenet,
    Platform::Rockstar,
    Platform::Xbox,
    Platform::Riot,
    Platform::Amazon,
    Platform::Itch,
    Platform::Other,
];

fn platform_id(p: Platform) -> String {
    serde_json::to_value(p).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default()
}

/// Diagnostic d'une source d'après son rapport du dernier scan.
fn source_diag(platform: Platform, report: Option<&SourceReport>, locations: Vec<Location>, tools: Vec<String>) -> SourceDiag {
    let (state, count, error, millis, skipped) = match report {
        None => (SourceState::Unknown, 0, None, None, Vec::new()),
        Some(r) => {
            let state = if r.disabled {
                SourceState::Disabled
            } else if r.error.is_some() {
                SourceState::Error
            } else if !r.installed {
                SourceState::Absent
            } else if r.count == 0 {
                SourceState::Empty
            } else {
                SourceState::Games
            };
            (state, r.count, r.error.clone(), Some(r.millis), r.skipped.clone())
        }
    };
    SourceDiag { id: platform_id(platform), label: platform.label().to_string(), state, count, error, millis, locations, skipped, tools }
}

/// Jeux ajoutés à la main : présents, ou exécutable introuvable (disque débranché, jeu déplacé).
fn manual_diag(manual: &[(String, bool)]) -> SourceDiag {
    let present = manual.iter().filter(|(_, found)| *found).count();
    let skipped = manual
        .iter()
        .filter(|(_, found)| !found)
        .map(|(title, _)| Skipped { title: title.clone(), reason: tr!("Exécutable introuvable", "Executable not found") })
        .collect();
    SourceDiag {
        id: "manual".to_string(),
        label: tr!("Jeux ajoutés à la main", "Games added manually"),
        state: if present > 0 { SourceState::Games } else { SourceState::Empty },
        count: present,
        error: None,
        millis: None,
        locations: Vec::new(),
        skipped,
        tools: Vec::new(),
    }
}

/// Assemble le diagnostic. `locations` lit les emplacements d'une source (`library::locations`),
/// `when` écrit une date, `privacy` sert à anonymiser le rapport.
pub fn build(input: Inputs, locations: impl Fn(Platform) -> Vec<Location>, when: &dyn Fn(u64) -> String, privacy: &Privacy) -> Diagnostic {
    let (scanned_at, reports) = match input.last {
        Some((at, reports)) => (Some(at), reports),
        None => (None, Vec::new()),
    };
    let mut sources: Vec<SourceDiag> = PLATFORMS
        .iter()
        .map(|&p| {
            let tools = input.games.iter().filter(|g| g.platform == p && g.is_tool && !g.manual).map(|g| g.title.clone()).collect();
            source_diag(p, reports.iter().find(|r| r.platform == p), locations(p), tools)
        })
        .collect();
    sources.push(manual_diag(&input.manual));
    let tracks = SERVICES.lock().map(|t| t.clone()).unwrap_or_default();
    let services = Service::ALL
        .iter()
        .map(|&s| {
            let t = &tracks[s.index()];
            ServiceDiag {
                id: s,
                label: s.label().to_string(),
                host: s.host().to_string(),
                busy: input.busy.contains(&s),
                last_ok: t.last_ok,
                last_error: t.last_error.as_ref().map(|(_, e)| e.clone()),
                error_at: t.last_error.as_ref().map(|(at, _)| *at),
            }
        })
        .collect();
    let mut diag = Diagnostic {
        version: input.version,
        windows: input.windows,
        webview: input.webview,
        data_dir: input.data_dir.to_string_lossy().into_owned(),
        scanned_at,
        created: now(),
        sources,
        services,
        discord: input.discord,
        gog: input.gog,
        epic: input.epic,
        steam_key: input.steam_key,
        report: String::new(),
    };
    diag.report = report(&diag, privacy, when);
    diag
}

/// De quoi anonymiser le rapport : dossier de l'utilisateur et nom d'utilisateur Windows.
#[derive(Clone, Debug, Default)]
pub struct Privacy {
    pub home: Option<String>,
    pub user: Option<String>,
}

impl Privacy {
    pub fn of_this_pc() -> Self {
        Privacy {
            home: dirs::home_dir().map(|h| h.to_string_lossy().into_owned()),
            user: std::env::var("USERNAME").ok(),
        }
    }
}

/// Positions de `pat` dans `text`, casse ASCII ignorée (chemins Windows).
fn find_all(text: &str, pat: &str) -> Vec<usize> {
    if pat.is_empty() {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut i = 0;
    while i + pat.len() <= text.len() {
        if text.is_char_boundary(i) && text.get(i..i + pat.len()).is_some_and(|s| s.eq_ignore_ascii_case(pat)) {
            found.push(i);
            i += pat.len();
        } else {
            i += 1;
        }
    }
    found
}

/// Remplace les morceaux `[début, fin)` donnés (triés, disjoints) par `by`.
fn splice(text: &str, ranges: &[(usize, usize)], by: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for &(start, end) in ranges {
        out.push_str(&text[at..start]);
        out.push_str(by);
        at = end;
    }
    out.push_str(&text[at..]);
    out
}

fn is_sep(c: Option<char>) -> bool {
    matches!(c, Some('\\' | '/'))
}

/// Retire ce qui identifie l'utilisateur : dossier de l'utilisateur → `%USERPROFILE%`, nom
/// d'utilisateur pris comme élément d'un chemin → `%USERNAME%`, numéro de compte Steam dans
/// `userdata\<numéro>` → `userdata\<compte>`.
pub fn anonymize(text: &str, privacy: &Privacy) -> String {
    let mut text = text.to_string();
    if let Some(home) = privacy.home.as_deref().map(|h| h.trim_end_matches(['\\', '/'])).filter(|h| h.len() >= 3) {
        for form in [home.to_string(), home.replace('\\', "/")] {
            let ranges: Vec<(usize, usize)> = find_all(&text, &form)
                .into_iter()
                .map(|i| (i, i + form.len()))
                // Pas le début d'un autre dossier (`C:\Users\Anne` dans `C:\Users\Anne-Marie`).
                .filter(|&(_, end)| text[end..].chars().next().is_none_or(|c| !c.is_alphanumeric() && c != '-' && c != '_' && c != '.'))
                .collect();
            text = splice(&text, &ranges, "%USERPROFILE%");
        }
    }
    if let Some(user) = privacy.user.as_deref().map(str::trim).filter(|u| u.chars().count() >= 2) {
        let ranges: Vec<(usize, usize)> = find_all(&text, user)
            .into_iter()
            .map(|i| (i, i + user.len()))
            .filter(|&(start, end)| {
                let next = text[end..].chars().next();
                is_sep(text[..start].chars().next_back()) && (next.is_none() || is_sep(next) || next.is_some_and(|c| c.is_whitespace() || "\"'),;:".contains(c)))
            })
            .collect();
        text = splice(&text, &ranges, "%USERNAME%");
    }
    // Numéro de compte Steam (steamid3) dans les chemins `userdata`.
    let ranges: Vec<(usize, usize)> = find_all(&text, "userdata")
        .into_iter()
        .filter_map(|i| {
            let start = i + "userdata".len() + 1;
            if !is_sep(text[i + "userdata".len()..].chars().next()) {
                return None;
            }
            let digits = text[start..].bytes().take_while(u8::is_ascii_digit).count();
            (digits > 0).then_some((start, start + digits))
        })
        .collect();
    splice(&text, &ranges, tr_str!("<compte>", "<account>"))
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n > 1 { many } else { one })
}

/// Nombre suivi du mot qui convient, dans la langue de Ludex (anglais : singulier pour 1
/// seulement, « 0 games »).
fn count(n: usize, fr: (&str, &str), en: (&str, &str)) -> String {
    if crate::i18n::english() {
        format!("{n} {}", if n == 1 { en.0 } else { en.1 })
    } else {
        plural(n, fr.0, fr.1)
    }
}

/// État d'une source en une ligne, comme dans la page.
pub fn state_text(s: &SourceDiag) -> String {
    match s.state {
        SourceState::Unknown => tr!("Pas encore lu", "Not read yet"),
        SourceState::Absent => tr!("Pas sur ce PC", "Not on this PC"),
        SourceState::Empty if s.id == "manual" => tr!("Aucun", "None"),
        SourceState::Empty => tr!("Installé, aucun jeu", "Installed, no games"),
        SourceState::Games => count(s.count, ("jeu", "jeux"), ("game", "games")),
        SourceState::Disabled if s.count > 0 => {
            let skipped = count(s.count, ("jeu écarté", "jeux écartés"), ("game skipped", "games skipped"));
            tr!("Désactivé dans les Paramètres, {skipped}", "Turned off in Settings, {skipped}")
        }
        SourceState::Disabled => tr!("Désactivé dans les Paramètres", "Turned off in Settings"),
        SourceState::Error => {
            let error = s.error.clone().unwrap_or_else(|| tr!("inconnue", "unknown"));
            tr!("Erreur : {error}", "Error: {error}")
        }
    }
}

/// Écarts listés au plus par source dans le rapport.
const MAX_SKIPPED: usize = 30;

/// Rapport texte, lisible et anonymisé, à coller dans un message pour déboguer un autre PC.
pub fn report(d: &Diagnostic, privacy: &Privacy, when: &dyn Fn(u64) -> String) -> String {
    let mut out = Vec::new();
    out.push(tr!("Rapport de diagnostic de Ludex", "Ludex diagnostics report"));
    let created = when(d.created);
    out.push(tr!("Créé le {created}", "Created {created}"));
    out.push(format!("Ludex {}", d.version));
    let windows = &d.windows;
    out.push(tr!("Windows : {windows}", "Windows: {windows}"));
    let webview = d.webview.clone().unwrap_or_else(|| tr!("version inconnue", "unknown version"));
    out.push(tr!("WebView2 : {webview}", "WebView2: {webview}"));
    let data_dir = &d.data_dir;
    out.push(tr!("Dossier de données : {data_dir}", "Data folder: {data_dir}"));
    let scanned = d.scanned_at.map_or_else(|| tr!("aucun depuis le démarrage", "none since startup"), when);
    out.push(tr!("Dernier scan : {scanned}", "Last scan: {scanned}"));
    out.push(String::new());
    out.push("Launchers".to_string());
    // Deux-points précédé d'une espace en français seulement.
    let colon = tr_str!(" : ", ": ");
    for s in &d.sources {
        let millis = s.millis.map(|ms| format!(" ({ms} ms)")).unwrap_or_default();
        out.push(format!("- {}{colon}{}{millis}", s.label, state_text(s)));
        if s.id == "manual" {
            // Titres choisis par l'utilisateur : seulement le nombre.
            if !s.skipped.is_empty() {
                let missing = count(
                    s.skipped.len(),
                    ("exécutable introuvable", "exécutables introuvables"),
                    ("executable not found", "executables not found"),
                );
                out.push(format!("    {missing}"));
            }
            continue;
        }
        for l in &s.locations {
            let found = if l.found { tr_str!("trouvé", "found") } else { tr_str!("absent", "missing") };
            out.push(format!("    [{found}] {}{colon}{}", l.label, l.path));
        }
        if !s.skipped.is_empty() {
            let n = s.skipped.len();
            out.push(tr!("    Écartés ({n}) :", "    Skipped ({n}):"));
            for k in s.skipped.iter().take(MAX_SKIPPED) {
                out.push(format!("      {}{colon}{}", k.title, k.reason));
            }
            if s.skipped.len() > MAX_SKIPPED {
                let more = s.skipped.len() - MAX_SKIPPED;
                out.push(tr!("      … et {more} autres", "      … and {more} more"));
            }
        }
        if !s.tools.is_empty() {
            let tools = s.tools.join(", ");
            out.push(tr!("    Masqués d'office (applis) : {tools}", "    Hidden by default (apps): {tools}"));
        }
    }
    out.push(String::new());
    out.push(tr!("Services en ligne (depuis le démarrage)", "Online services (since startup)"));
    for s in &d.services {
        let ok = s.last_ok.map_or_else(
            || tr!("aucune réussite", "no success yet"),
            |t| {
                let t = when(t);
                tr!("dernière réussite {t}", "last success {t}")
            },
        );
        let err = match (&s.last_error, s.error_at) {
            (Some(e), Some(at)) => {
                let at = when(at);
                tr!("dernière erreur {at} : {e}", "last error {at}: {e}")
            }
            _ => tr!("aucune erreur", "no errors"),
        };
        let busy = if s.busy { tr_str!(", lecture en cours", ", reading now") } else { "" };
        out.push(format!("- {} ({}){colon}{ok}, {err}{busy}", s.label, s.host));
    }
    out.push(String::new());
    out.push(tr!("Statut Discord : {}", "Discord status: {}", d.discord.text()));
    out.push(tr!("Compte GOG : {}", "GOG account: {}", d.gog.text()));
    out.push(tr!("Compte Epic Games : {}", "Epic Games account: {}", crate::epic_online::state_text(d.epic)));
    out.push(tr!("Clé Web API Steam : {}", "Steam Web API key: {}", crate::steam_owned::state_text(d.steam_key)));
    anonymize(&(out.join("\n") + "\n"), privacy)
}

/// Diagnostic de ce PC d'après le dernier scan (aucun rescan : « Relire » passe par `scan_library`).
#[tauri::command]
pub async fn diagnostic(app: tauri::AppHandle) -> Result<Diagnostic, String> {
    blocking(move || {
        let state = app.state::<AppState>();
        // Jeux puis préférences, jamais tenus ensemble.
        let games = lock(&state.games)?.clone();
        let manual: Vec<(String, bool)> = lock(&state.data)?.manual.iter().map(|m| (m.title.clone(), m.exe.is_file())).collect();
        let busy = [
            (Service::Images, &state.images_busy),
            (Service::Art, &state.art_busy),
            (Service::Details, &state.meta_busy),
            (Service::Hltb, &state.hltb_busy),
            (Service::Saves, &state.saves_busy),
            (Service::Requirements, &state.requirements_busy),
            (Service::News, &state.news_busy),
            (Service::Workshop, &state.workshop_busy),
        ]
        .into_iter()
        .filter(|(_, flag)| flag.load(Ordering::SeqCst))
        .map(|(s, _)| s)
        .collect();
        let input = Inputs {
            version: app.package_info().version.to_string(),
            windows: windows_version(),
            webview: tauri::webview_version().ok(),
            data_dir: state.data_dir.clone(),
            last: crate::library::last_scan(),
            games: &games,
            manual,
            busy,
            discord: app.try_state::<crate::discord::Presence>().map(|p| p.status()).unwrap_or_default(),
            gog: crate::gog_online::diag_state(&app),
            epic: crate::epic_online::diag_state(&app),
            steam_key: crate::steam_owned::diag_state(&app),
        };
        let privacy = Privacy::of_this_pc();
        let mut diag = build(input, crate::library::locations, &local_time, &privacy);
        // Dernières lignes du journal d'erreurs (`logfile.rs`), anonymisées comme le reste.
        let dir = crate::logfile::dir().map(PathBuf::from).unwrap_or_else(|| state.data_dir.clone());
        diag.report.push_str(&crate::logfile::report_section(&crate::logfile::tail(&dir, crate::logfile::TAIL_LINES), &privacy));
        Ok(diag)
    })
    .await
}

/// Version de Windows lisible : « Windows 11 Home 25H2 (build 26300.7171) ». Le registre écrit
/// encore « Windows 10 » sur Windows 11 : le numéro de build tranche (22000 et plus).
pub fn windows_label(product: Option<&str>, display: Option<&str>, build: Option<&str>, ubr: Option<u32>) -> String {
    let number = build.and_then(|b| b.parse::<u32>().ok());
    let mut name = product.unwrap_or("Windows").to_string();
    if number.is_some_and(|n| n >= 22_000) && name.starts_with("Windows 10") {
        name = name.replacen("Windows 10", "Windows 11", 1);
    }
    if let Some(d) = display.filter(|d| !d.is_empty()) {
        name = format!("{name} {d}");
    }
    match (build, ubr) {
        (Some(b), Some(u)) => format!("{name} (build {b}.{u})"),
        (Some(b), None) => format!("{name} (build {b})"),
        _ => name,
    }
}

/// Version de Windows de ce PC, lue dans le registre.
pub fn windows_version() -> String {
    #[cfg(windows)]
    {
        use crate::library::reg;
        let Some(key) = reg::hklm(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion") else { return "Windows".to_string() };
        let ubr = key.get_value::<u32, _>("UBR").ok();
        windows_label(
            reg::string(&key, "ProductName").as_deref(),
            reg::string(&key, "DisplayVersion").as_deref(),
            reg::string(&key, "CurrentBuild").as_deref(),
            ubr,
        )
    }
    #[cfg(not(windows))]
    {
        tr!("Hors Windows", "Not Windows")
    }
}

/// Date et heure locales : « 19/11/2026 14:05 » (« 2026-11-19 14:05 » en anglais : sans
/// l'ambiguïté jour/mois).
pub fn local_time(secs: u64) -> String {
    let format = tr_str!("%d/%m/%Y %H:%M", "%Y-%m-%d %H:%M");
    i64::try_from(secs)
        .ok()
        .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
        .map(|d| d.with_timezone(&chrono::Local).format(format).to_string())
        .unwrap_or_else(|| tr!("date inconnue", "unknown date"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Launch;

    fn privacy() -> Privacy {
        Privacy { home: Some(r"C:\Users\Anne".to_string()), user: Some("Anne".to_string()) }
    }

    #[test]
    fn hides_user_folder_name_and_steam_account() {
        let p = privacy();
        assert_eq!(anonymize(r"C:\Users\Anne\AppData\Roaming\com.ludex.app", &p), r"%USERPROFILE%\AppData\Roaming\com.ludex.app");
        assert_eq!(anonymize(r"c:\users\anne\Documents", &p), r"%USERPROFILE%\Documents");
        assert_eq!(anonymize("C:/Users/Anne/Desktop", &p), "%USERPROFILE%/Desktop");
        // Nom d'utilisateur ailleurs dans un chemin (autre disque, OneDrive).
        assert_eq!(anonymize(r"D:\Anne\Jeux et E:\Backup\Anne", &p), r"D:\%USERNAME%\Jeux et E:\Backup\%USERNAME%");
        // Pas un autre dossier qui commence pareil, ni un mot du texte.
        assert_eq!(anonymize(r"C:\Users\Anne-Marie\x", &p), r"C:\Users\Anne-Marie\x");
        assert_eq!(anonymize("Anne a joué à Annecy", &p), "Anne a joué à Annecy");
        // Compte Steam.
        assert_eq!(
            anonymize(r"E:\Steam\userdata\52870646\config\localconfig.vdf", &p),
            r"E:\Steam\userdata\<compte>\config\localconfig.vdf"
        );
        assert_eq!(anonymize(r"E:\Steam\userdata", &p), r"E:\Steam\userdata");
        // Rien à cacher sans profil connu.
        assert_eq!(anonymize(r"C:\Users\Anne", &Privacy::default()), r"C:\Users\Anne");
    }

    #[test]
    fn reads_windows_version() {
        assert_eq!(windows_label(Some("Windows 10 Home"), Some("25H2"), Some("26300"), Some(7171)), "Windows 11 Home 25H2 (build 26300.7171)");
        assert_eq!(windows_label(Some("Windows 10 Pro"), Some("22H2"), Some("19045"), None), "Windows 10 Pro 22H2 (build 19045)");
        assert_eq!(windows_label(None, None, None, None), "Windows");
    }

    fn report_of(platform: Platform, installed: bool, count: usize, error: Option<&str>, disabled: bool) -> SourceReport {
        SourceReport { platform, installed, count, error: error.map(String::from), disabled, millis: 12, skipped: Vec::new() }
    }

    #[test]
    fn states_from_the_last_scan() {
        let state = |r: Option<SourceReport>| source_diag(Platform::Epic, r.as_ref(), Vec::new(), Vec::new()).state;
        assert_eq!(state(None), SourceState::Unknown);
        assert_eq!(state(Some(report_of(Platform::Epic, false, 0, None, false))), SourceState::Absent);
        assert_eq!(state(Some(report_of(Platform::Epic, true, 0, None, false))), SourceState::Empty);
        assert_eq!(state(Some(report_of(Platform::Epic, true, 3, None, false))), SourceState::Games);
        assert_eq!(state(Some(report_of(Platform::Epic, true, 0, Some("Accès refusé"), false))), SourceState::Error);
        assert_eq!(state(Some(report_of(Platform::Epic, true, 2, None, true))), SourceState::Disabled);

        let manual = manual_diag(&[("Mon jeu".into(), true), ("Jeu perdu".into(), false)]);
        assert_eq!((manual.state, manual.count, manual.skipped.len()), (SourceState::Games, 1, 1));
        assert_eq!(manual_diag(&[]).state, SourceState::Empty);
    }

    fn game(platform: Platform, title: &str, tool: bool) -> Game {
        let mut g = Game::new(platform, title, title, Launch::Uri(String::new()));
        g.is_tool = tool;
        g
    }

    #[test]
    fn report_is_readable_and_anonymous() {
        let mut steam = report_of(Platform::Steam, true, 2, None, false);
        steam.skipped = vec![Skipped { title: "Steamworks Common Redistributables".into(), reason: "Composant de Steam, pas un jeu".into() }];
        let games = [game(Platform::Steam, "Portal 2", false), game(Platform::Steam, "Wallpaper Engine", true)];
        failed(Service::Hltb, "réponse 429");
        let input = Inputs {
            version: "0.1.0".into(),
            windows: "Windows 11 Home 25H2 (build 26300.7171)".into(),
            webview: Some("142.0.3595.80".into()),
            data_dir: PathBuf::from(r"C:\Users\Anne\AppData\Roaming\com.ludex.app"),
            last: Some((1_000, vec![steam, report_of(Platform::Epic, false, 0, None, false), report_of(Platform::Riot, true, 2, None, true)])),
            games: &games,
            manual: vec![("Titre très perso".into(), true), ("Autre titre perso".into(), false)],
            busy: vec![Service::Art],
            discord: crate::discord::Status { state: crate::discord::State::Absent, error: Some("Discord n'est pas ouvert.".into()) },
            gog: crate::gog_online::AccountState::Connected,
            epic: crate::gog_online::AccountState::Expired,
            steam_key: crate::gog_online::AccountState::Connected,
        };
        let locations = |p: Platform| match p {
            Platform::Steam => vec![
                Location { label: "Dossier de Steam".into(), path: r"E:\Steam".into(), found: true },
                Location { label: "Temps de jeu du compte".into(), path: r"E:\Steam\userdata\52870646\config\localconfig.vdf".into(), found: true },
            ],
            Platform::Epic => vec![Location { label: "Manifestes".into(), path: r"C:\ProgramData\Epic".into(), found: false }],
            _ => Vec::new(),
        };
        let d = build(input, locations, &|t| format!("<{t}>"), &privacy());
        let text = &d.report;
        assert!(text.starts_with("Rapport de diagnostic de Ludex\n"), "{text}");
        assert!(text.contains("Windows : Windows 11 Home 25H2 (build 26300.7171)\n"));
        assert!(text.contains("Dossier de données : %USERPROFILE%\\AppData\\Roaming\\com.ludex.app\n"));
        assert!(text.contains("Dernier scan : <1000>\n"));
        assert!(text.contains("- Steam : 2 jeux (12 ms)\n"));
        assert!(text.contains("    [trouvé] Dossier de Steam : E:\\Steam\n"));
        assert!(text.contains("userdata\\<compte>\\config"));
        assert!(text.contains("      Steamworks Common Redistributables : Composant de Steam, pas un jeu\n"));
        assert!(text.contains("    Masqués d'office (applis) : Wallpaper Engine\n"));
        assert!(text.contains("- Epic Games : Pas sur ce PC (12 ms)\n    [absent] Manifestes : C:\\ProgramData\\Epic\n"));
        assert!(text.contains("- Riot : Désactivé dans les Paramètres, 2 jeux écartés"));
        assert!(text.contains("- GOG : Pas encore lu\n"));
        assert!(text.contains("- Jeux ajoutés à la main : 1 jeu\n    1 exécutable introuvable\n"));
        assert!(text.contains("- Durées de vie (HowLongToBeat) : "));
        assert!(text.contains("réponse 429"));
        assert!(text.contains(", lecture en cours\n"));
        assert!(text.contains("Compte Epic Games : à reconnecter") && text.contains("Clé Web API Steam : enregistrée\n"));
        // Rien de personnel : ni nom d'utilisateur, ni compte Steam, ni titres des jeux manuels.
        for secret in ["Anne", "52870646", "Titre très perso", "Autre titre perso"] {
            assert!(!text.contains(secret), "{secret} dans le rapport :\n{text}");
        }
        assert!(!text.contains('\u{2014}'), "pas de tiret cadratin");
        // La page garde les vrais chemins (affichés sur ce PC seulement).
        assert!(d.data_dir.contains("Anne"));
        assert_eq!(d.sources.len(), 13);
    }

    #[test]
    fn report_in_english() {
        crate::i18n::with(crate::i18n::Lang::En, || {
            let mut steam = report_of(Platform::Steam, true, 1, None, false);
            steam.skipped = vec![Skipped { title: "SteamVR".into(), reason: "Steam component, not a game".into() }];
            let games = [game(Platform::Steam, "Wallpaper Engine", true)];
            let input = Inputs {
                version: "0.1.0".into(),
                windows: "Windows 11 Home".into(),
                webview: None,
                data_dir: PathBuf::from(r"C:\Users\Anne\AppData\Roaming\com.ludex.app"),
                last: Some((1_000, vec![steam, report_of(Platform::Epic, true, 0, None, false), report_of(Platform::Riot, true, 2, None, true)])),
                games: &games,
                manual: vec![("Titre très perso".into(), false)],
                busy: Vec::new(),
                discord: crate::discord::Status::default(),
                gog: crate::gog_online::AccountState::default(),
                epic: crate::gog_online::AccountState::default(),
                steam_key: crate::gog_online::AccountState::default(),
            };
            let locations = |p: Platform| match p {
                Platform::Steam => vec![Location { label: "Account playtime".into(), path: r"E:\Steam\userdata\52870646\config\localconfig.vdf".into(), found: false }],
                _ => Vec::new(),
            };
            let d = build(input, locations, &|t| format!("<{t}>"), &privacy());
            let text = &d.report;
            assert!(text.starts_with("Ludex diagnostics report\nCreated <"), "{text}");
            assert!(text.contains("WebView2: unknown version\n"), "{text}");
            assert!(text.contains("Data folder: %USERPROFILE%\\AppData\\Roaming\\com.ludex.app\n"), "{text}");
            assert!(text.contains("Last scan: <1000>\n"));
            assert!(text.contains("- Steam: 1 game (12 ms)\n"), "{text}");
            assert!(text.contains("    [missing] Account playtime: E:\\Steam\\userdata\\<account>\\config\\localconfig.vdf\n"), "{text}");
            assert!(text.contains("    Skipped (1):\n      SteamVR: Steam component, not a game\n"), "{text}");
            assert!(text.contains("    Hidden by default (apps): Wallpaper Engine\n"));
            assert!(text.contains("- Epic Games: Installed, no games (12 ms)\n"), "{text}");
            assert!(text.contains("- Riot: Turned off in Settings, 2 games skipped"), "{text}");
            assert!(text.contains("- Games added manually: None\n    1 executable not found\n"), "{text}");
            assert!(text.contains("Online services (since startup)\n"));
            assert!(text.contains("- Time to beat (HowLongToBeat): "), "{text}");
            for secret in ["Anne", "52870646", "Titre très perso"] {
                assert!(!text.contains(secret), "{secret} dans le rapport :\n{text}");
            }
            // Avant les services (leurs erreurs, partagées entre les tests, viennent d'ailleurs).
            let sources = text.split("Online services").next().unwrap_or_default();
            for french in ["Dossier", "Écartés", "jeux", " : ", "Autres"] {
                assert!(!sources.contains(french), "« {french} » dans le rapport anglais :\n{text}");
            }
            assert!(!text.contains('\u{2014}'), "pas de tiret cadratin");
        });
    }

    /// Diagnostic réel de ce PC (lecture seule, aucun fichier de l'app) :
    /// `cargo test diagnostic_this_pc -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn diagnostic_this_pc() {
        let scan = crate::library::scan();
        let input = Inputs {
            version: env!("CARGO_PKG_VERSION").to_string(),
            windows: windows_version(),
            webview: tauri::webview_version().ok(),
            data_dir: dirs::data_dir().unwrap_or_default().join("com.ludex.app"),
            last: crate::library::last_scan(),
            games: &scan.games,
            manual: Vec::new(),
            busy: Vec::new(),
            discord: crate::discord::Status::default(),
            gog: crate::gog_online::AccountState::default(),
            epic: crate::gog_online::AccountState::default(),
            steam_key: crate::gog_online::AccountState::default(),
        };
        let d = build(input, crate::library::locations, &local_time, &Privacy::of_this_pc());
        println!("{}", d.report);
        println!("(jeux ajoutés à la main non lus : `user-data.json` jamais ouvert par ce test)");
    }
}
