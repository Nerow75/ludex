//! Page de diagnostic des launchers (Paramètres) : pour chaque source, état, emplacements lus,
//! jeux écartés et durée de lecture, d'après le dernier scan (aucun rescan) ; état des services en
//! ligne depuis le démarrage ; versions de Ludex, de Windows et de WebView2. Le rapport texte à
//! copier est anonymisé : nom d'utilisateur Windows et numéro de compte Steam remplacés, aucun
//! contenu de `user-data.json` hormis le nombre de jeux ajoutés à la main.

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
}

impl Service {
    pub const ALL: [Service; 7] = [
        Service::Images,
        Service::Art,
        Service::Details,
        Service::Hltb,
        Service::Saves,
        Service::Requirements,
        Service::News,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Service::Images => "Cache des images",
            Service::Art => "Recherche des jaquettes",
            Service::Details => "Fiches des jeux",
            Service::Hltb => "Durées de vie",
            Service::Saves => "Base des sauvegardes",
            Service::Requirements => "Configurations requises",
            Service::News => "Actualités des jeux",
        }
    }

    /// Ce que le service contacte.
    pub fn host(self) -> &'static str {
        match self {
            Service::Images => "CDN de Steam et du Microsoft Store",
            Service::Art => "boutique Steam, Microsoft Store",
            Service::Details => "boutique Steam",
            Service::Hltb => "HowLongToBeat",
            Service::Saves => "manifeste Ludusavi sur GitHub",
            Service::Requirements => "boutique Steam",
            Service::News => "annonces Steam",
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
}

/// Sources dans l'ordre du scan.
const PLATFORMS: [Platform; 10] = [
    Platform::Steam,
    Platform::Epic,
    Platform::Gog,
    Platform::Ubisoft,
    Platform::Ea,
    Platform::Battlenet,
    Platform::Rockstar,
    Platform::Xbox,
    Platform::Riot,
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
        .map(|(title, _)| Skipped { title: title.clone(), reason: "Exécutable introuvable".to_string() })
        .collect();
    SourceDiag {
        id: "manual".to_string(),
        label: "Jeux ajoutés à la main".to_string(),
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
    splice(&text, &ranges, "<compte>")
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n > 1 { many } else { one })
}

/// État d'une source en une ligne, comme dans la page.
pub fn state_text(s: &SourceDiag) -> String {
    match s.state {
        SourceState::Unknown => "Pas encore lu".to_string(),
        SourceState::Absent => "Pas sur ce PC".to_string(),
        SourceState::Empty if s.id == "manual" => "Aucun".to_string(),
        SourceState::Empty => "Installé, aucun jeu".to_string(),
        SourceState::Games => plural(s.count, "jeu", "jeux"),
        SourceState::Disabled if s.count > 0 => format!("Désactivé dans les Paramètres, {}", plural(s.count, "jeu écarté", "jeux écartés")),
        SourceState::Disabled => "Désactivé dans les Paramètres".to_string(),
        SourceState::Error => format!("Erreur : {}", s.error.as_deref().unwrap_or("inconnue")),
    }
}

/// Écarts listés au plus par source dans le rapport.
const MAX_SKIPPED: usize = 30;

/// Rapport texte, lisible et anonymisé, à coller dans un message pour déboguer un autre PC.
pub fn report(d: &Diagnostic, privacy: &Privacy, when: &dyn Fn(u64) -> String) -> String {
    let mut out = Vec::new();
    out.push("Rapport de diagnostic de Ludex".to_string());
    out.push(format!("Créé le {}", when(d.created)));
    out.push(format!("Ludex {}", d.version));
    out.push(format!("Windows : {}", d.windows));
    out.push(format!("WebView2 : {}", d.webview.as_deref().unwrap_or("version inconnue")));
    out.push(format!("Dossier de données : {}", d.data_dir));
    out.push(format!("Dernier scan : {}", d.scanned_at.map_or("aucun depuis le démarrage".to_string(), when)));
    out.push(String::new());
    out.push("Launchers".to_string());
    for s in &d.sources {
        let millis = s.millis.map(|ms| format!(" ({ms} ms)")).unwrap_or_default();
        out.push(format!("- {} : {}{millis}", s.label, state_text(s)));
        if s.id == "manual" {
            // Titres choisis par l'utilisateur : seulement le nombre.
            if !s.skipped.is_empty() {
                out.push(format!("    {} introuvable", plural(s.skipped.len(), "exécutable", "exécutables")));
            }
            continue;
        }
        for l in &s.locations {
            out.push(format!("    [{}] {} : {}", if l.found { "trouvé" } else { "absent" }, l.label, l.path));
        }
        if !s.skipped.is_empty() {
            out.push(format!("    Écartés ({}) :", s.skipped.len()));
            for k in s.skipped.iter().take(MAX_SKIPPED) {
                out.push(format!("      {} : {}", k.title, k.reason));
            }
            if s.skipped.len() > MAX_SKIPPED {
                out.push(format!("      … et {} autres", s.skipped.len() - MAX_SKIPPED));
            }
        }
        if !s.tools.is_empty() {
            out.push(format!("    Masqués d'office (applis) : {}", s.tools.join(", ")));
        }
    }
    out.push(String::new());
    out.push("Services en ligne (depuis le démarrage)".to_string());
    for s in &d.services {
        let ok = s.last_ok.map_or("aucune réussite".to_string(), |t| format!("dernière réussite {}", when(t)));
        let err = match (&s.last_error, s.error_at) {
            (Some(e), Some(at)) => format!("dernière erreur {} : {e}", when(at)),
            _ => "aucune erreur".to_string(),
        };
        let busy = if s.busy { ", lecture en cours" } else { "" };
        out.push(format!("- {} ({}) : {ok}, {err}{busy}", s.label, s.host));
    }
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
        };
        Ok(build(input, crate::library::locations, &local_time, &Privacy::of_this_pc()))
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
        "Hors Windows".to_string()
    }
}

/// Date et heure locales : « 19/11/2026 14:05 ».
pub fn local_time(secs: u64) -> String {
    i64::try_from(secs)
        .ok()
        .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
        .map(|d| d.with_timezone(&chrono::Local).format("%d/%m/%Y %H:%M").to_string())
        .unwrap_or_else(|| "date inconnue".to_string())
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
        // Rien de personnel : ni nom d'utilisateur, ni compte Steam, ni titres des jeux manuels.
        for secret in ["Anne", "52870646", "Titre très perso", "Autre titre perso"] {
            assert!(!text.contains(secret), "{secret} dans le rapport :\n{text}");
        }
        assert!(!text.contains('\u{2014}'), "pas de tiret cadratin");
        // La page garde les vrais chemins (affichés sur ce PC seulement).
        assert!(d.data_dir.contains("Anne"));
        assert_eq!(d.sources.len(), 11);
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
        };
        let d = build(input, crate::library::locations, &local_time, &Privacy::of_this_pc());
        println!("{}", d.report);
        println!("(jeux ajoutés à la main non lus : `user-data.json` jamais ouvert par ce test)");
    }
}
