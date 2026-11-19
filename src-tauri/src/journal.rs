//! Journal : la vie de joueur jour par jour, du jour le plus récent au plus ancien.
//!
//! Chaque jour (fuseau local, mêmes règles que le récap, `recap.rs`) rassemble :
//! - les sessions de l'historique de Ludex (`sessions.json`), découpées à minuit : une partie à
//!   cheval sur deux jours apparaît dans les deux, chacun pour sa part ; les entrées fusionnées
//!   (`merged`) comptent sans heure ;
//! - les succès Steam débloqués ce jour-là (dates gardées par Steam, donc bien avant le premier
//!   usage de Ludex), le détail n'étant lu que pour les jeux concernés par la page ;
//! - les captures d'écran prises ce jour-là (date du fichier).
//!
//! Le temps du jour suit le récap : chaque session une fois (même jeu, même début = doublon), celles
//! de Steam ou GOG Galaxy pour leur durée observée par Ludex, jamais ajoutée au temps du launcher.
//!
//! Pages d'au plus `PAGE_DAYS` jours actifs (jours sans rien ignorés) ; la suivante commence avant
//! le dernier jour renvoyé (`next`).

use crate::history::PastSession;
use crate::library::Achievement;
use crate::recap::{local_day, pieces, to_date, to_day, unique, Zone};
use crate::screenshots::{Shot, OTHER_TITLE};
use chrono::{Datelike, NaiveDate};
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};

/// Jours actifs par page.
pub const PAGE_DAYS: usize = 30;
/// Titre d'un jeu de l'historique qui n'est plus dans la bibliothèque (désinstallé).
pub const MISSING_TITLE: &str = "Jeu absent de la bibliothèque";

/// Morceau d'une session dans un jour.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaySession {
    /// Début de la session (secondes Unix), `None` pour une entrée fusionnée (heure inconnue).
    pub start: Option<u64>,
    /// Temps joué ce jour-là.
    pub secs: u64,
    /// Session commencée un jour précédent (partie passée minuit).
    pub continued: bool,
}

/// Ce qu'un jeu a laissé dans un jour. `game_id` à `None` : captures sans jeu (« Autres captures »).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayGame {
    pub game_id: Option<String>,
    pub title: String,
    pub secs: u64,
    /// Dans l'ordre de la journée.
    pub sessions: Vec<DaySession>,
    /// Succès débloqués ce jour-là (compte tiré des dates, même sans détail lisible).
    pub unlocked: usize,
    /// Détail de ces succès (nom, icône, % mondial s'il est en cache), dans l'ordre de la journée.
    pub achievements: Vec<Achievement>,
    /// Captures du jour, dans l'ordre de la journée.
    pub shots: Vec<Shot>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalDay {
    /// Date locale, `AAAA-MM-JJ`.
    pub date: String,
    /// Temps joué ce jour-là, tous jeux confondus.
    pub secs: u64,
    /// Le plus joué d'abord, « Autres captures » en dernier.
    pub games: Vec<DayGame>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    /// Du plus récent au plus ancien.
    pub days: Vec<JournalDay>,
    /// Date à redemander (`before`) pour la page suivante, `None` : plus rien avant.
    pub next: Option<String>,
}

/// Ce que le journal rassemble.
pub struct Sources<'a> {
    pub sessions: &'a [PastSession],
    /// Dates de déblocage des succès de chaque jeu (secondes Unix).
    pub unlocks: &'a [(String, Vec<u64>)],
    pub shots: &'a [Shot],
}

/// Date reçue du front : exactement `AAAA-MM-JJ`, une vraie date du calendrier. `None` sinon.
pub fn parse_day(text: &str) -> Option<i64> {
    if text.len() != 10 {
        return None;
    }
    let date = NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()?;
    (1970..=9999).contains(&date.year()).then(|| to_day(date))
}

fn instant(utc: u64) -> i64 {
    i64::try_from(utc).unwrap_or(i64::MAX / 2)
}

/// Entrée d'un jeu (ou des captures sans jeu) dans un jour, créée au premier besoin.
fn slot_of<'a>(days: &'a mut HashMap<(i64, Option<String>), DayGame>, title: &impl Fn(&str) -> Option<String>, day: i64, id: Option<&str>) -> &'a mut DayGame {
    days.entry((day, id.map(str::to_string))).or_insert_with(|| DayGame {
        game_id: id.map(str::to_string),
        title: id.map_or_else(|| OTHER_TITLE.to_string(), |id| title(id).unwrap_or_else(|| MISSING_TITLE.to_string())),
        secs: 0,
        sessions: Vec::new(),
        unlocked: 0,
        achievements: Vec::new(),
        shots: Vec::new(),
    })
}

/// Une page du journal : les `PAGE_DAYS` jours actifs avant `before` (non compris, tous si `None`).
/// `game` : un seul jeu. `title` donne le titre d'un jeu de la bibliothèque ; `details` la liste
/// des succès d'un jeu, appelée seulement pour ceux qui en ont débloqué dans la page.
pub fn compute(
    src: &Sources,
    before: Option<i64>,
    game: Option<&str>,
    zone: &impl Zone,
    title: impl Fn(&str) -> Option<String>,
    mut details: impl FnMut(&str) -> Option<Vec<Achievement>>,
) -> Page {
    let keep = |id: Option<&str>| game.is_none_or(|g| id == Some(g));
    let sessions: Vec<&PastSession> = unique(src.sessions).into_iter().filter(|s| keep(Some(&s.game_id))).collect();
    let unlocks: Vec<&(String, Vec<u64>)> = src.unlocks.iter().filter(|(id, _)| keep(Some(id))).collect();
    let shots: Vec<&Shot> = src.shots.iter().filter(|s| keep(s.game_id.as_deref())).collect();

    // Jours actifs, puis la page : les plus récents avant `before`.
    let mut active: BTreeSet<i64> = BTreeSet::new();
    for s in &sessions {
        pieces(s, zone, |day, _, _| {
            active.insert(day);
        });
    }
    active.extend(unlocks.iter().flat_map(|(_, times)| times.iter().map(|&t| local_day(instant(t), zone))));
    active.extend(shots.iter().map(|s| local_day(instant(s.taken), zone)));
    let mut page: Vec<i64> = active.range(..before.unwrap_or(i64::MAX)).rev().take(PAGE_DAYS + 1).copied().collect();
    let more = page.len() > PAGE_DAYS;
    page.truncate(PAGE_DAYS);
    let (Some(&newest), Some(&oldest)) = (page.first(), page.last()) else { return Page::default() };
    // Tous les jours actifs entre ces deux bornes sont dans la page.
    let inside = |day: i64| (oldest..=newest).contains(&day);

    let mut days: HashMap<(i64, Option<String>), DayGame> = HashMap::new();

    for s in &sessions {
        let first = local_day(instant(s.start), zone);
        let mut parts: Vec<(i64, u64)> = Vec::new();
        pieces(s, zone, |day, _, secs| match parts.last_mut() {
            Some((d, total)) if *d == day => *total += secs,
            _ => parts.push((day, secs)),
        });
        for (day, secs) in parts.into_iter().filter(|&(d, _)| inside(d)) {
            let g = slot_of(&mut days, &title, day, Some(&s.game_id));
            g.secs += secs;
            g.sessions.push(DaySession { start: (!s.merged).then_some(s.start), secs, continued: !s.merged && day != first });
        }
    }

    for (id, times) in &unlocks {
        let in_page: Vec<i64> = times.iter().map(|&t| local_day(instant(t), zone)).filter(|&d| inside(d)).collect();
        if in_page.is_empty() {
            continue;
        }
        for &day in &in_page {
            slot_of(&mut days, &title, day, Some(id)).unlocked += 1;
        }
        for a in details(id).unwrap_or_default().into_iter().filter(|a| a.unlocked) {
            let Some(day) = a.unlocked_at.map(|t| local_day(instant(t), zone)).filter(|&d| inside(d)) else { continue };
            slot_of(&mut days, &title, day, Some(id)).achievements.push(a);
        }
    }

    for s in &shots {
        let day = local_day(instant(s.taken), zone);
        if inside(day) {
            slot_of(&mut days, &title, day, s.game_id.as_deref()).shots.push((*s).clone());
        }
    }

    let mut by_day: HashMap<i64, Vec<DayGame>> = HashMap::new();
    for ((day, _), mut g) in days {
        // Dans l'ordre de la journée ; une entrée fusionnée (sans heure) en tête.
        g.sessions.sort_by_key(|s| s.start);
        g.achievements.sort_by_key(|a| a.unlocked_at);
        g.shots.sort_by(|a, b| a.taken.cmp(&b.taken).then_with(|| a.key.cmp(&b.key)));
        by_day.entry(day).or_default().push(g);
    }
    let days = page
        .iter()
        .map(|&day| {
            let mut games = by_day.remove(&day).unwrap_or_default();
            games.sort_by(|a, b| {
                a.game_id
                    .is_none()
                    .cmp(&b.game_id.is_none())
                    .then_with(|| b.secs.cmp(&a.secs))
                    .then_with(|| b.unlocked.cmp(&a.unlocked))
                    .then_with(|| b.shots.len().cmp(&a.shots.len()))
                    .then_with(|| a.title.cmp(&b.title))
            });
            JournalDay { date: to_date(day).to_string(), secs: games.iter().map(|g| g.secs).sum(), games }
        })
        .collect();
    Page { days, next: more.then(|| to_date(oldest).to_string()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screenshots::Source;
    use std::path::PathBuf;

    const HOUR: i64 = 3_600;

    struct Fixed(i64);
    impl Zone for Fixed {
        fn offset(&self, _: i64) -> i64 {
            self.0 * HOUR
        }
    }

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> u64 {
        NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(h, min, 0).unwrap().and_utc().timestamp() as u64
    }

    fn past(id: &str, start: u64, secs: u64) -> PastSession {
        PastSession { game_id: id.into(), start, secs, launcher: false, merged: false }
    }

    fn shot(id: Option<&str>, taken: u64) -> Shot {
        let key = format!("{taken:016x}");
        Shot {
            key: key.clone(),
            game_id: id.map(str::to_string),
            source: Source::Steam,
            steam_appid: None,
            taken,
            size: 1,
            width: None,
            height: None,
            video: false,
            thumb_url: format!("http://ludex.localhost/capture/{key}/thumb"),
            url: format!("http://ludex.localhost/capture/{key}/full"),
            path: PathBuf::from(format!("C:/captures/{key}.jpg")),
            thumb: None,
        }
    }

    fn ach(api: &str, at: Option<u64>) -> Achievement {
        Achievement {
            api_name: api.into(),
            name: api.to_lowercase(),
            description: None,
            icon: None,
            icon_gray: None,
            hidden: false,
            unlocked: at.is_some(),
            unlocked_at: at,
            global_percent: Some(12.5),
        }
    }

    fn titles(id: &str) -> Option<String> {
        (id != "epic:Gone").then(|| format!("Titre {id}"))
    }

    fn page(src: &Sources, before: Option<i64>, game: Option<&str>, zone: i64) -> Page {
        compute(src, before, game, &Fixed(zone), titles, |_| None)
    }

    #[test]
    fn parses_only_real_dates() {
        assert_eq!(parse_day("1970-01-02"), Some(1));
        assert_eq!(parse_day("2026-11-18").map(to_date).map(|d| d.to_string()).as_deref(), Some("2026-11-18"));
        for bad in ["", "2026-11-1", "2026-02-30", "2026-13-01", "26-11-18", "2026/11/18", "../../etc", "2026-11-18 ", "1969-12-31"] {
            assert_eq!(parse_day(bad), None, "{bad}");
        }
    }

    #[test]
    fn groups_by_local_day_and_splits_at_midnight() {
        let sessions = [
            // Lundi 16, 23 h UTC → mardi 17, 1 h UTC.
            past("epic:Sugar", at(2026, 11, 16, 23, 0), 2 * 3600),
            past("epic:Sugar", at(2026, 11, 17, 20, 0), 1800),
            past("steam:440", at(2026, 11, 17, 10, 0), 600),
        ];
        let src = Sources { sessions: &sessions, unlocks: &[], shots: &[] };
        let p = page(&src, None, None, 0);
        assert_eq!(p.days.iter().map(|d| d.date.as_str()).collect::<Vec<_>>(), ["2026-11-17", "2026-11-16"]);
        let tue = &p.days[0];
        assert_eq!(tue.secs, 3600 + 1800 + 600);
        assert_eq!(tue.games[0].game_id.as_deref(), Some("epic:Sugar"));
        assert_eq!(tue.games[0].title, "Titre epic:Sugar");
        assert_eq!(
            tue.games[0].sessions,
            [
                DaySession { start: Some(at(2026, 11, 16, 23, 0)), secs: 3600, continued: true },
                DaySession { start: Some(at(2026, 11, 17, 20, 0)), secs: 1800, continued: false },
            ]
        );
        assert_eq!((tue.games[1].game_id.as_deref(), tue.games[1].secs), (Some("steam:440"), 600));
        assert!(!p.days[1].games[0].sessions[0].continued);
        // À Paris (+1 h), la partie de lundi 23 h UTC commence mardi à minuit : un seul jour.
        let p = page(&src, None, None, 1);
        assert_eq!(p.days.len(), 1);
        assert_eq!(p.days[0].secs, 2 * 3600 + 1800 + 600);
    }

    #[test]
    fn days_with_only_achievements_or_shots() {
        let unlocks = vec![("steam:730".to_string(), vec![at(2019, 5, 1, 12, 0), at(2019, 5, 1, 13, 0), at(2020, 1, 1, 9, 0)])];
        let shots = [shot(Some("steam:730"), at(2019, 5, 1, 12, 30)), shot(None, at(2021, 3, 3, 8, 0))];
        let src = Sources { sessions: &[], unlocks: &unlocks, shots: &shots };
        let mut asked = Vec::new();
        let p = compute(&src, None, None, &Fixed(0), titles, |id| {
            asked.push(id.to_string());
            Some(vec![ach("A", Some(at(2019, 5, 1, 13, 0))), ach("B", Some(at(2019, 5, 1, 12, 0))), ach("C", Some(at(2020, 1, 1, 9, 0))), ach("D", None)])
        });
        assert_eq!(asked, ["steam:730"]);
        assert_eq!(p.days.iter().map(|d| d.date.as_str()).collect::<Vec<_>>(), ["2021-03-03", "2020-01-01", "2019-05-01"]);
        assert!(p.days.iter().all(|d| d.secs == 0));
        let other = &p.days[0].games[0];
        assert_eq!((other.game_id.as_deref(), other.title.as_str(), other.shots.len()), (None, OTHER_TITLE, 1));
        let may = &p.days[2].games[0];
        assert_eq!((may.unlocked, may.shots.len(), may.sessions.len()), (2, 1, 0));
        // Dans l'ordre de la journée, les non débloqués jamais.
        assert_eq!(may.achievements.iter().map(|a| a.api_name.as_str()).collect::<Vec<_>>(), ["B", "A"]);
        assert_eq!(p.days[1].games[0].achievements.len(), 1);
        assert_eq!(p.next, None);
    }

    #[test]
    fn paginates_by_active_days() {
        // 70 jours actifs, un tous les deux jours.
        let sessions: Vec<_> = (0..70u64).map(|i| past("epic:Sugar", at(2026, 1, 1, 20, 0) + i * 2 * 86_400, 600)).collect();
        let src = Sources { sessions: &sessions, unlocks: &[], shots: &[] };
        let first = page(&src, None, None, 0);
        assert_eq!(first.days.len(), PAGE_DAYS);
        assert_eq!(first.days[0].date, to_date(local_day(instant(sessions[69].start), &Fixed(0))).to_string());
        let next = first.next.clone().unwrap();
        assert_eq!(next, first.days[PAGE_DAYS - 1].date);
        let second = page(&src, parse_day(&next), None, 0);
        assert_eq!(second.days.len(), PAGE_DAYS);
        assert!(second.days[0].date < next);
        let third = page(&src, parse_day(second.next.as_deref().unwrap()), None, 0);
        assert_eq!((third.days.len(), third.next.as_deref()), (10, None));
        assert_eq!(third.days.last().unwrap().date, "2026-01-01");
        // Exactement une page : pas de suivante.
        let src = Sources { sessions: &sessions[..PAGE_DAYS], unlocks: &[], shots: &[] };
        assert_eq!(page(&src, None, None, 0).next, None);
        // Rien avant la première date.
        assert!(page(&src, parse_day("2026-01-01"), None, 0).days.is_empty());
    }

    #[test]
    fn counts_sessions_once_and_keeps_merged_without_time() {
        let steam = PastSession { launcher: true, ..past("steam:730", at(2026, 11, 17, 20, 0), 3600) };
        let merged = PastSession { merged: true, ..past("epic:Gone", at(2026, 11, 17, 9, 0), 3 * 3600) };
        let sessions = [steam.clone(), steam, merged];
        let src = Sources { sessions: &sessions, unlocks: &[], shots: &[] };
        let p = page(&src, None, None, 0);
        let day = &p.days[0];
        assert_eq!(day.secs, 4 * 3600);
        assert_eq!(day.games[0].title, MISSING_TITLE);
        assert_eq!(day.games[0].sessions, [DaySession { start: None, secs: 3 * 3600, continued: false }]);
        assert_eq!(day.games[1].sessions.len(), 1);
    }

    #[test]
    fn one_game_only() {
        let sessions = [past("epic:Sugar", at(2026, 11, 17, 20, 0), 600), past("steam:440", at(2026, 11, 15, 20, 0), 600)];
        let unlocks = vec![("steam:440".to_string(), vec![at(2026, 11, 10, 12, 0)])];
        let shots = [shot(Some("steam:440"), at(2026, 11, 12, 12, 0)), shot(None, at(2026, 11, 13, 12, 0))];
        let src = Sources { sessions: &sessions, unlocks: &unlocks, shots: &shots };
        let p = page(&src, None, Some("steam:440"), 0);
        assert_eq!(p.days.iter().map(|d| d.date.as_str()).collect::<Vec<_>>(), ["2026-11-15", "2026-11-12", "2026-11-10"]);
        assert!(p.days.iter().all(|d| d.games.len() == 1 && d.games[0].game_id.as_deref() == Some("steam:440")));
        assert!(page(&src, None, Some("epic:Inconnu"), 0).days.is_empty());
    }

    /// Journal réel de ce PC (lecture seule) : `cargo test journal_this_pc -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn journal_this_pc() {
        let started = std::time::Instant::now();
        let games = crate::library::scan().games;
        let dir = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default().join("com.ludex.app");
        let sessions = crate::history::Journal::open(&dir).shared().lock().map(|s| s.clone()).unwrap_or_default();
        let unlocks: Vec<(String, Vec<u64>)> = games
            .iter()
            .filter_map(|g| Some((g.id.clone(), g.achievements.as_ref()?.unlock_times.clone())))
            .filter(|(_, t)| !t.is_empty())
            .collect();
        let roots = crate::screenshots::Roots::this_pc();
        let shots = crate::screenshots::collect(&roots, &games, None);
        println!("Scan et relevés : {:?} ({} sessions, {} jeux avec succès datés, {} captures)", started.elapsed(), sessions.len(), unlocks.len(), shots.len());
        let stats = crate::library::SteamStats::locate();
        let src = Sources { sessions: &sessions, unlocks: &unlocks, shots: &shots };
        let title = |id: &str| games.iter().find(|g| g.id == id).map(|g| g.title.clone());
        let mut before = None;
        let (mut days, mut pages, mut read) = (0usize, 0usize, 0usize);
        let mut oldest = String::new();
        loop {
            let started = std::time::Instant::now();
            let p = compute(&src, before, None, &crate::recap::LocalZone, title, |id| {
                read += 1;
                let g = games.iter().find(|g| g.id == id)?;
                stats.as_ref()?.details(&g.platform_id)
            });
            pages += 1;
            if pages == 1 {
                println!("Première page : {} jours en {:?}", p.days.len(), started.elapsed());
                for d in p.days.iter().take(5) {
                    let parts: Vec<String> = d
                        .games
                        .iter()
                        .map(|g| format!("{} ({} min, {} succès, {} captures)", g.title, g.secs / 60, g.unlocked, g.shots.len()))
                        .collect();
                    println!("  {} : {}", d.date, parts.join(", "));
                }
            }
            days += p.days.len();
            if let Some(d) = p.days.last() {
                oldest = d.date.clone();
            }
            match p.next {
                Some(next) => before = parse_day(&next),
                None => break,
            }
        }
        println!("{days} jours actifs en {pages} pages, du {oldest} à aujourd'hui, détail des succès lu {read} fois");
    }
}
