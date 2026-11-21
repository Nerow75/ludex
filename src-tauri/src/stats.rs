//! Section « Statistiques » : ce qui se calcule depuis l'historique de Ludex (`sessions.json`) et les
//! dates de déblocage des succès Steam, sur les 12 derniers mois et sur tout l'historique.
//!
//! Mêmes règles que le récap (`recap.rs`, dont les fonctions sont reprises) : jours au fuseau local,
//! session découpée à minuit (et donc entre deux mois), sessions sans doublon, celles de Steam ou
//! GOG Galaxy pour leur durée observée par Ludex et jamais pour le cumul du launcher. Le temps des
//! launchers (`Game.playtime_minutes`) n'entre pas ici : le front l'utilise à part (top et
//! répartition par launcher), sans jamais l'additionner à ces chiffres.

use crate::history::PastSession;
use crate::recap::{Zone, local_day, pieces, to_date, unique};
use chrono::{Datelike, NaiveDate};
use serde::Serialize;
use std::collections::BTreeSet;

/// Nombre de mois montrés, le mois en cours compris.
pub const MONTHS: usize = 12;

/// Un mois du calendrier local.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthStat {
    /// `AAAA-MM`.
    pub month: String,
    pub secs: u64,
    /// Sessions qui ont au moins un morceau dans le mois (une partie à cheval compte dans les deux).
    pub sessions: usize,
    /// Jours joués du mois.
    pub days: usize,
    /// Succès Steam débloqués dans le mois.
    pub achievements: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    /// Les 12 derniers mois, le plus ancien d'abord, le mois en cours en dernier.
    pub months: Vec<MonthStat>,
    /// Plus longue série de jours joués de tout l'historique, et son dernier jour (`AAAA-MM-JJ`).
    pub best_streak: u32,
    pub best_streak_end: Option<String>,
    /// Série en cours : jusqu'à aujourd'hui, ou jusqu'à hier si rien n'est encore joué aujourd'hui.
    pub current_streak: u32,
    /// Jours joués de tout l'historique.
    pub played_days: usize,
    /// Taille de tout l'historique : l'écran explique comment il se remplit quand il est vide.
    pub history_sessions: usize,
    pub history_since: Option<u64>,
}

/// Index de mois (année × 12 + mois de 0 à 11) d'un jour local.
fn month_index(day: i64) -> i64 {
    let date = to_date(day);
    i64::from(date.year()) * 12 + i64::from(date.month0())
}

fn month_label(index: i64) -> String {
    let year = i32::try_from(index.div_euclid(12)).unwrap_or(1970);
    let date = NaiveDate::from_ymd_opt(year, index.rem_euclid(12) as u32 + 1, 1).unwrap_or_default();
    format!("{:04}-{:02}", date.year(), date.month())
}

/// Calcule les statistiques. `unlocks` : dates de déblocage des succès de chaque jeu (secondes Unix).
pub fn compute(sessions: &[PastSession], unlocks: &[(String, Vec<u64>)], now: u64, zone: &impl Zone) -> Stats {
    let now = i64::try_from(now).unwrap_or(i64::MAX / 2);
    let today = local_day(now, zone);
    let last = month_index(today);
    let first = last - (MONTHS as i64 - 1);
    let slot = |day: i64| {
        let m = month_index(day);
        (first..=last).contains(&m).then(|| (m - first) as usize)
    };
    let all = unique(sessions);

    let mut months: Vec<MonthStat> = (first..=last)
        .map(|m| MonthStat { month: month_label(m), secs: 0, sessions: 0, days: 0, achievements: 0 })
        .collect();
    let mut played: BTreeSet<i64> = BTreeSet::new();
    for s in &all {
        let mut touched = [false; MONTHS];
        pieces(s, zone, |day, _, secs| {
            played.insert(day);
            if let Some(i) = slot(day) {
                months[i].secs = months[i].secs.saturating_add(secs);
                touched[i] = true;
            }
        });
        for (i, t) in touched.iter().enumerate() {
            if *t {
                months[i].sessions += 1;
            }
        }
    }
    for &day in &played {
        if let Some(i) = slot(day) {
            months[i].days += 1;
        }
    }
    for t in unlocks.iter().flat_map(|(_, times)| times) {
        if let Some(i) = slot(local_day(i64::try_from(*t).unwrap_or(i64::MAX / 2), zone)) {
            months[i].achievements += 1;
        }
    }

    // Plus longue série de tout l'historique (la plus récente à égalité).
    let (mut best, mut best_end, mut run, mut prev) = (0u32, None, 0u32, None::<i64>);
    for &day in &played {
        run = if prev == Some(day - 1) { run + 1 } else { 1 };
        if run >= best {
            best = run;
            best_end = Some(day);
        }
        prev = Some(day);
    }
    let mut day = if played.contains(&today) { today } else { today - 1 };
    let mut current = 0u32;
    while played.contains(&day) {
        current += 1;
        day -= 1;
    }

    Stats {
        months,
        best_streak: best,
        best_streak_end: best_end.map(|d| to_date(d).to_string()),
        current_streak: current,
        played_days: played.len(),
        history_sessions: all.len(),
        history_since: all.iter().map(|s| s.start).min(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// Samedi 21 novembre 2026, 15 h UTC.
    fn now() -> u64 {
        at(2026, 11, 21, 15, 0)
    }

    #[test]
    fn twelve_months_ending_with_the_current_one() {
        let s = compute(&[], &[], now(), &Fixed(0));
        assert_eq!(s.months.len(), MONTHS);
        assert_eq!(s.months[0].month, "2025-12");
        assert_eq!(s.months[11].month, "2026-11");
        // En janvier, les mois remontent dans l'année précédente.
        let s = compute(&[], &[], at(2027, 1, 3, 12, 0), &Fixed(0));
        assert_eq!((s.months[0].month.as_str(), s.months[11].month.as_str()), ("2026-02", "2027-01"));
        assert_eq!((s.best_streak, s.current_streak, s.played_days, s.history_since), (0, 0, 0, None));
        assert_eq!(s.best_streak_end, None);
    }

    #[test]
    fn months_split_at_midnight_and_follow_the_zone() {
        // 31 octobre, 23 h → 1er novembre, 1 h : une heure dans chaque mois, une session dans chacun.
        let s = compute(&[past("epic:Sugar", at(2026, 10, 31, 23, 0), 7200)], &[], now(), &Fixed(0));
        assert_eq!((s.months[10].secs, s.months[10].sessions, s.months[10].days), (3600, 1, 1));
        assert_eq!((s.months[11].secs, s.months[11].sessions, s.months[11].days), (3600, 1, 1));
        // 30 novembre 23 h 30 UTC = 1er décembre à +1 h : hors des 12 mois montrés (décembre à venir).
        let s = compute(&[past("epic:Sugar", at(2026, 11, 30, 23, 30), 600)], &[], at(2026, 11, 30, 23, 45), &Fixed(1));
        assert_eq!(s.months[11].month, "2026-12");
        assert_eq!(s.months[11].secs, 600);
        // Plus vieux que 12 mois : compté dans l'historique, pas dans les mois.
        let s = compute(&[past("epic:Sugar", at(2025, 11, 30, 12, 0), 600)], &[], now(), &Fixed(0));
        assert!(s.months.iter().all(|m| m.secs == 0));
        assert_eq!((s.history_sessions, s.played_days), (1, 1));
    }

    #[test]
    fn launcher_sessions_and_duplicates_count_once() {
        let steam = PastSession { launcher: true, ..past("steam:730", at(2026, 11, 10, 20, 0), 3600) };
        let merged = PastSession { merged: true, ..past("epic:Sugar", at(2026, 11, 12, 9, 0), 1800) };
        let s = compute(&[steam.clone(), steam, merged], &[], now(), &Fixed(0));
        assert_eq!((s.months[11].secs, s.months[11].sessions, s.months[11].days), (5400, 2, 2));
        assert_eq!(s.history_sessions, 2);
    }

    #[test]
    fn streaks_over_the_whole_history() {
        let day = |y, m, d| past("epic:Sugar", at(y, m, d, 20, 0), 1800);
        // Cinq jours de suite l'an dernier (à cheval sur deux mois), puis trois jusqu'à hier.
        let s: Vec<_> = vec![
            day(2025, 1, 30),
            day(2025, 1, 31),
            day(2025, 2, 1),
            day(2025, 2, 2),
            day(2025, 2, 3),
            day(2026, 11, 18),
            day(2026, 11, 19),
            day(2026, 11, 20),
        ];
        let r = compute(&s, &[], now(), &Fixed(0));
        assert_eq!((r.best_streak, r.best_streak_end.as_deref(), r.current_streak, r.played_days), (5, Some("2025-02-03"), 3, 8));
        // Avant-hier seulement : plus de série en cours.
        let r = compute(&s, &[], at(2026, 11, 22, 12, 0), &Fixed(0));
        assert_eq!(r.current_streak, 0);
        // Égalité : la plus récente l'emporte.
        let r = compute(&[day(2026, 3, 1), day(2026, 5, 1)], &[], now(), &Fixed(0));
        assert_eq!((r.best_streak, r.best_streak_end.as_deref()), (1, Some("2026-05-01")));
    }

    #[test]
    fn achievements_per_month() {
        let unlocks = vec![
            ("steam:730".to_string(), vec![at(2026, 11, 2, 10, 0), at(2026, 11, 3, 10, 0), at(2024, 1, 1, 10, 0)]),
            ("steam:440".to_string(), vec![at(2026, 6, 15, 10, 0)]),
        ];
        let s = compute(&[], &unlocks, now(), &Fixed(0));
        assert_eq!(s.months[11].achievements, 2);
        assert_eq!(s.months[6].achievements, 1); // juin 2026
        assert_eq!(s.months.iter().map(|m| m.achievements).sum::<usize>(), 3);
    }
}
