//! Récap d'une semaine, d'un mois ou d'une année, calculé depuis l'historique de Ludex
//! (`sessions.json`, voir `history.rs`) et les dates de déblocage des succès Steam.
//!
//! Règles :
//! - les jours suivent le fuseau local (heure d'été comprise) : une partie commencée à 23 h 30 et
//!   finie à 1 h compte pour les deux jours, chacun pour sa part ;
//! - semaine du lundi au dimanche, mois et année du calendrier ;
//! - chaque session compte une fois, celles de Steam ou GOG Galaxy (`launcher`) comprises : c'est la
//!   durée observée par Ludex, jamais ajoutée au cumul du launcher ni à un autre temps ;
//! - les entrées fusionnées par jour (`merged`, historique de plus de 5 000 sessions) comptent pour
//!   le temps, le jour et le jeu, mais pas pour la plus longue session ni le moment de la journée
//!   (heure inconnue) ;
//! - une session à cheval sur deux périodes n'y compte que pour sa part (durée supposée continue).

use crate::history::PastSession;
use chrono::{Datelike, Local, NaiveDate, Offset, TimeZone};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};

pub(crate) const DAY: i64 = 86_400;
const HOUR: i64 = 3_600;
/// Numéro du 1er janvier 1970 dans le calendrier de chrono (jours depuis le 1er janvier de l'an 1).
const EPOCH_CE: i64 = 719_163;
/// Recul maximal accepté : bien au-delà de l'historique, sans risque de débordement.
const MAX_BACK: u32 = 10_000;
/// Taille du classement des jeux.
const TOP: usize = 5;

/// Période demandée par le front : valeur fermée.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Period {
    Week,
    Month,
    Year,
}

/// Décalage du fuseau à un instant donné, en secondes à ajouter à l'heure UTC.
pub trait Zone {
    fn offset(&self, utc: i64) -> i64;
}

/// Fuseau de Windows (heure d'été comprise).
pub struct LocalZone;

impl Zone for LocalZone {
    fn offset(&self, utc: i64) -> i64 {
        Local.timestamp_opt(utc, 0).single().map_or(0, |d| i64::from(d.offset().fix().local_minus_utc()))
    }
}

/// Temps et sessions d'un jeu dans la période.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameStat {
    pub game_id: String,
    pub secs: u64,
    pub sessions: usize,
}

/// Plus longue session de la période.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LongSession {
    pub game_id: String,
    pub start: u64,
    pub secs: u64,
}

/// Jeu lancé pour la première fois dans la période.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewGame {
    pub game_id: String,
    pub first_played: u64,
}

/// Succès débloqués d'un jeu dans la période.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Unlocks {
    pub game_id: String,
    pub count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recap {
    pub period: Period,
    /// Nombre de périodes en arrière (0 = la période en cours).
    pub back: u32,
    /// Premier et dernier jour de la période (dates locales, `AAAA-MM-JJ`, bornes comprises).
    pub start: String,
    pub end: String,
    /// Jours de la période déjà commencés (tous pour une période passée).
    pub elapsed_days: usize,
    pub total_secs: u64,
    /// Temps de la période précédente, en entier et sur le même nombre de jours écoulés.
    pub previous_secs: u64,
    pub previous_to_date_secs: u64,
    pub sessions: usize,
    pub games: usize,
    /// Jeux les plus joués, du plus long au plus court.
    pub top: Vec<GameStat>,
    pub longest: Option<LongSession>,
    /// Plus longue série de jours joués dans la période, et série en cours aujourd'hui (ou finie hier).
    pub best_streak: u32,
    pub current_streak: u32,
    /// Temps par jour de la semaine, lundi d'abord, et l'index du jour préféré.
    pub weekday_secs: [u64; 7],
    pub favorite_weekday: Option<usize>,
    /// Temps par moment de la journée : nuit (0-6 h), matin (6-12 h), après-midi (12-18 h),
    /// soir (18-24 h), et l'index du moment préféré.
    pub moment_secs: [u64; 4],
    pub favorite_moment: Option<usize>,
    /// Temps de chaque jour de la période, dans l'ordre.
    pub days: Vec<u64>,
    pub new_games: Vec<NewGame>,
    /// Succès Steam débloqués dans la période, et par jeu (le plus de succès d'abord).
    pub achievements: usize,
    pub achievement_games: Vec<Unlocks>,
    /// Taille de tout l'historique : l'écran explique comment il se remplit quand il est vide.
    pub history_sessions: usize,
    pub history_since: Option<u64>,
}

pub(crate) fn to_date(day: i64) -> NaiveDate {
    i32::try_from(day + EPOCH_CE).ok().and_then(NaiveDate::from_num_days_from_ce_opt).unwrap_or_default()
}

pub(crate) fn to_day(date: NaiveDate) -> i64 {
    i64::from(date.num_days_from_ce()) - EPOCH_CE
}

/// Jour local (jours depuis le 1er janvier 1970) d'un instant UTC.
pub(crate) fn local_day(utc: i64, zone: &impl Zone) -> i64 {
    (utc + zone.offset(utc)).div_euclid(DAY)
}

/// Premier et dernier jour (compris) de la période qui contient `today`, `back` périodes en arrière.
fn bounds(period: Period, today: i64, back: u32) -> (i64, i64) {
    let date = to_date(today);
    let back = i64::from(back.min(MAX_BACK));
    let first_of = |y: i64, m: i64| -> i64 {
        let months = y * 12 + m - back * i64::from(period == Period::Month);
        let year = i32::try_from(months.div_euclid(12)).unwrap_or(1970);
        to_day(NaiveDate::from_ymd_opt(year, months.rem_euclid(12) as u32 + 1, 1).unwrap_or_default())
    };
    match period {
        Period::Week => {
            let start = today - i64::from(date.weekday().num_days_from_monday()) - 7 * back;
            (start, start + 6)
        }
        Period::Month => {
            let (y, m) = (i64::from(date.year()), i64::from(date.month0()));
            (first_of(y, m), first_of(y, m + 1) - 1)
        }
        Period::Year => {
            let y = i64::from(date.year()) - back;
            (first_of(y, 0), first_of(y + 1, 0) - 1)
        }
    }
}

/// Découpe une session en morceaux par heure locale : (jour, heure ou `None` si inconnue, secondes).
pub(crate) fn pieces(s: &PastSession, zone: &impl Zone, mut f: impl FnMut(i64, Option<i64>, u64)) {
    let start = i64::try_from(s.start).unwrap_or(i64::MAX / 2);
    let mut t = start + zone.offset(start);
    if s.merged {
        // Plusieurs sessions d'un jour regroupées : seul le jour est sûr.
        return f(t.div_euclid(DAY), None, s.secs);
    }
    let mut left = s.secs;
    while left > 0 {
        let len = (((t.div_euclid(HOUR) + 1) * HOUR - t) as u64).min(left);
        f(t.div_euclid(DAY), Some(t.rem_euclid(DAY) / HOUR), len);
        t += len as i64;
        left -= len;
    }
}

/// Sessions sans doublon (même jeu, même début : une seule, comme le journal).
pub(crate) fn unique(sessions: &[PastSession]) -> Vec<&PastSession> {
    let mut seen: HashSet<(&str, u64)> = HashSet::new();
    sessions.iter().filter(|s| s.secs > 0 && seen.insert((s.game_id.as_str(), s.start))).collect()
}

/// Temps joué entre deux jours locaux (compris).
fn time_between(sessions: &[&PastSession], first: i64, last: i64, zone: &impl Zone) -> u64 {
    let mut total = 0u64;
    for s in sessions {
        pieces(s, zone, |day, _, secs| {
            if (first..=last).contains(&day) {
                total = total.saturating_add(secs);
            }
        });
    }
    total
}

/// Index du plus grand temps, `None` si tout est à zéro (le premier en cas d'égalité).
fn favorite(values: &[u64]) -> Option<usize> {
    let max = *values.iter().max()?;
    (max > 0).then(|| values.iter().position(|&v| v == max)).flatten()
}

/// Calcule le récap. `unlocks` : dates de déblocage des succès de chaque jeu (secondes Unix).
pub fn compute(sessions: &[PastSession], unlocks: &[(String, Vec<u64>)], now: u64, period: Period, back: u32, zone: &impl Zone) -> Recap {
    let back = back.min(MAX_BACK);
    let now = i64::try_from(now).unwrap_or(i64::MAX / 2);
    let today = local_day(now, zone);
    let (first, last) = bounds(period, today, back);
    let (prev_first, prev_last) = bounds(period, today, back + 1);
    let len = (last - first + 1) as usize;
    let elapsed = if today > last { len } else { (today - first + 1).clamp(0, len as i64) as usize };
    let all = unique(sessions);

    let mut days = vec![0u64; len];
    let mut weekday_secs = [0u64; 7];
    let mut moment_secs = [0u64; 4];
    let mut per_game: HashMap<&str, GameStat> = HashMap::new();
    let mut longest: Option<&PastSession> = None;
    let mut played_days: BTreeSet<i64> = BTreeSet::new();
    let mut count = 0usize;

    for s in &all {
        let mut inside = 0u64;
        pieces(s, zone, |day, hour, secs| {
            played_days.insert(day);
            if !(first..=last).contains(&day) {
                return;
            }
            inside += secs;
            days[(day - first) as usize] += secs;
            // 1er janvier 1970 = jeudi (index 3, lundi = 0).
            weekday_secs[(day + 3).rem_euclid(7) as usize] += secs;
            if let Some(h) = hour {
                moment_secs[(h / 6) as usize] += secs;
            }
        });
        if inside == 0 {
            continue;
        }
        count += 1;
        let stat = per_game.entry(s.game_id.as_str()).or_insert_with(|| GameStat { game_id: s.game_id.clone(), secs: 0, sessions: 0 });
        stat.secs += inside;
        stat.sessions += 1;
        if !s.merged && longest.is_none_or(|l| s.secs > l.secs || (s.secs == l.secs && s.start > l.start)) {
            longest = Some(s);
        }
    }

    let mut top: Vec<GameStat> = per_game.into_values().collect();
    top.sort_by(|a, b| b.secs.cmp(&a.secs).then_with(|| b.sessions.cmp(&a.sessions)).then_with(|| a.game_id.cmp(&b.game_id)));
    let games = top.len();
    top.truncate(TOP);

    // Série la plus longue dans la période.
    let mut best_streak = 0u32;
    let mut run = 0u32;
    for d in first..=last {
        run = if played_days.contains(&d) { run + 1 } else { 0 };
        best_streak = best_streak.max(run);
    }
    // Série en cours : jusqu'à aujourd'hui, ou jusqu'à hier si rien n'est encore joué aujourd'hui.
    let mut day = if played_days.contains(&today) { today } else { today - 1 };
    let mut current_streak = 0u32;
    while played_days.contains(&day) {
        current_streak += 1;
        day -= 1;
    }

    // Comparaison : période précédente entière, et sur autant de jours que ceux écoulés ici.
    let previous_secs = time_between(&all, prev_first, prev_last, zone);
    let previous_to_date_secs = time_between(&all, prev_first, (prev_first + elapsed as i64 - 1).min(prev_last), zone);

    // Premier lancement de chaque jeu dans l'historique.
    let mut first_seen: HashMap<&str, u64> = HashMap::new();
    for s in &all {
        let slot = first_seen.entry(s.game_id.as_str()).or_insert(s.start);
        *slot = (*slot).min(s.start);
    }
    let in_period = |utc: u64| (first..=last).contains(&local_day(i64::try_from(utc).unwrap_or(i64::MAX / 2), zone));
    let mut new_games: Vec<NewGame> =
        first_seen.into_iter().filter(|&(_, t)| in_period(t)).map(|(id, t)| NewGame { game_id: id.to_string(), first_played: t }).collect();
    new_games.sort_by(|a, b| a.first_played.cmp(&b.first_played).then_with(|| a.game_id.cmp(&b.game_id)));

    let mut achievement_games: Vec<Unlocks> = unlocks
        .iter()
        .map(|(id, times)| Unlocks { game_id: id.clone(), count: times.iter().filter(|&&t| in_period(t)).count() })
        .filter(|u| u.count > 0)
        .collect();
    achievement_games.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.game_id.cmp(&b.game_id)));

    Recap {
        period,
        back,
        start: to_date(first).to_string(),
        end: to_date(last).to_string(),
        elapsed_days: elapsed,
        total_secs: days.iter().sum(),
        previous_secs,
        previous_to_date_secs,
        sessions: count,
        games,
        top,
        longest: longest.map(|s| LongSession { game_id: s.game_id.clone(), start: s.start, secs: s.secs }),
        best_streak,
        current_streak,
        favorite_weekday: favorite(&weekday_secs),
        weekday_secs,
        favorite_moment: favorite(&moment_secs),
        moment_secs,
        days,
        new_games,
        achievements: achievement_games.iter().map(|u| u.count).sum(),
        achievement_games,
        history_sessions: all.len(),
        history_since: all.iter().map(|s| s.start).min(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fuseau fixe, en heures.
    struct Fixed(i64);
    impl Zone for Fixed {
        fn offset(&self, _: i64) -> i64 {
            self.0 * HOUR
        }
    }

    /// Heure d'été simplifiée : +2 h à partir de `switch` (UTC), +1 h avant.
    struct Summer {
        switch: i64,
    }
    impl Zone for Summer {
        fn offset(&self, utc: i64) -> i64 {
            if utc >= self.switch { 2 * HOUR } else { HOUR }
        }
    }

    /// Instant UTC d'une date et heure.
    fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> u64 {
        NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(h, min, 0).unwrap().and_utc().timestamp() as u64
    }

    fn past(id: &str, start: u64, secs: u64) -> PastSession {
        PastSession { game_id: id.into(), start, secs, launcher: false, merged: false }
    }

    fn recap(sessions: &[PastSession], now: u64, period: Period, back: u32) -> Recap {
        compute(sessions, &[], now, period, back, &Fixed(0))
    }

    /// Mercredi 18 novembre 2026, 15 h UTC.
    fn now() -> u64 {
        at(2026, 11, 18, 15, 0)
    }

    #[test]
    fn period_bounds() {
        let r = recap(&[], now(), Period::Week, 0);
        assert_eq!((r.start.as_str(), r.end.as_str(), r.elapsed_days, r.days.len()), ("2026-11-16", "2026-11-22", 3, 7));
        let r = recap(&[], now(), Period::Week, 1);
        assert_eq!((r.start.as_str(), r.end.as_str(), r.elapsed_days), ("2026-11-09", "2026-11-15", 7));
        let r = recap(&[], now(), Period::Month, 0);
        assert_eq!((r.start.as_str(), r.end.as_str(), r.elapsed_days, r.days.len()), ("2026-11-01", "2026-11-30", 18, 30));
        // Onze mois en arrière : décembre de l'année précédente.
        let r = recap(&[], now(), Period::Month, 11);
        assert_eq!((r.start.as_str(), r.end.as_str()), ("2025-12-01", "2025-12-31"));
        // Février bissextile.
        let r = recap(&[], at(2024, 3, 10, 12, 0), Period::Month, 1);
        assert_eq!((r.start.as_str(), r.end.as_str(), r.days.len()), ("2024-02-01", "2024-02-29", 29));
        let r = recap(&[], now(), Period::Year, 0);
        assert_eq!((r.start.as_str(), r.end.as_str(), r.days.len(), r.elapsed_days), ("2026-01-01", "2026-12-31", 365, 322));
        let r = recap(&[], now(), Period::Year, 2);
        assert_eq!((r.start.as_str(), r.days.len()), ("2024-01-01", 366));
        // Le dimanche reste dans la semaine qui a commencé le lundi.
        let r = recap(&[], at(2026, 11, 22, 23, 0), Period::Week, 0);
        assert_eq!((r.start.as_str(), r.elapsed_days), ("2026-11-16", 7));
        // Recul absurde : borné, pas de panic.
        assert_eq!(recap(&[], now(), Period::Year, u32::MAX).back, MAX_BACK);
    }

    #[test]
    fn days_follow_the_local_zone() {
        // Dimanche 22 novembre, 23 h 30 UTC = lundi 23, 0 h 30 à Paris (+1 h) : semaine suivante.
        let s = [past("epic:Sugar", at(2026, 11, 22, 23, 30), 1800)];
        let monday = at(2026, 11, 23, 12, 0);
        let paris = compute(&s, &[], monday, Period::Week, 0, &Fixed(1));
        assert_eq!((paris.start.as_str(), paris.total_secs, paris.days[0]), ("2026-11-23", 1800, 1800));
        assert_eq!(paris.moment_secs, [1800, 0, 0, 0]); // nuit
        let utc = compute(&s, &[], monday, Period::Week, 0, &Fixed(0));
        assert_eq!(utc.total_secs, 0);
        assert_eq!(utc.previous_secs, 1800);
        // Heure d'été : le même instant UTC change de jour local selon le décalage du moment.
        let s = [past("epic:Sugar", at(2026, 6, 10, 22, 30), 600)];
        let summer = compute(&s, &[], at(2026, 6, 12, 12, 0), Period::Week, 0, &Summer { switch: at(2026, 3, 29, 1, 0) as i64 });
        assert_eq!(summer.weekday_secs[3], 600); // jeudi 11 juin, 0 h 30 à l'heure d'été
        let winter = compute(&s, &[], at(2026, 6, 12, 12, 0), Period::Week, 0, &Summer { switch: i64::MAX });
        assert_eq!(winter.weekday_secs[2], 600); // mercredi 10 juin, 23 h 30
    }

    #[test]
    fn sessions_across_midnight_and_periods_are_split() {
        // Mardi 17, 23 h → mercredi 18, 1 h : une heure chaque jour.
        let s = [past("epic:Sugar", at(2026, 11, 17, 23, 0), 7200)];
        let r = recap(&s, now(), Period::Week, 0);
        assert_eq!(&r.days[..3], &[0, 3600, 3600]);
        assert_eq!((r.sessions, r.games, r.moment_secs), (1, 1, [3600, 0, 0, 3600]));
        // Dimanche 15, 23 h → lundi 16, 1 h : une heure dans chaque semaine, une session dans chacune.
        let s = [past("epic:Sugar", at(2026, 11, 15, 23, 0), 7200)];
        let r = recap(&s, now(), Period::Week, 0);
        assert_eq!((r.total_secs, r.sessions, r.previous_secs), (3600, 1, 3600));
    }

    #[test]
    fn streaks() {
        let day = |d: u32| past("epic:Sugar", at(2026, 11, d, 20, 0), 1800);
        // 2, 3, 4 puis 10, 11, 12, 13 (série la plus longue : 4), puis 16, 17 (en cours, rien aujourd'hui).
        let s: Vec<_> = [2, 3, 4, 10, 11, 12, 13, 16, 17].into_iter().map(day).collect();
        let r = recap(&s, now(), Period::Month, 0);
        assert_eq!((r.best_streak, r.current_streak), (4, 2));
        // Une partie aujourd'hui prolonge la série.
        let mut with_today = s.clone();
        with_today.push(past("epic:Sugar", at(2026, 11, 18, 8, 0), 600));
        assert_eq!(recap(&with_today, now(), Period::Month, 0).current_streak, 3);
        // Avant-hier seulement : plus de série en cours. Dans la semaine, la plus longue est 2.
        let r = recap(&s, at(2026, 11, 19, 12, 0), Period::Week, 0);
        assert_eq!((r.best_streak, r.current_streak), (2, 0));
        // Série coupée par le début de la période : seuls les jours de la période comptent.
        assert_eq!(recap(&s, now(), Period::Week, 1).best_streak, 4);
    }

    #[test]
    fn compares_with_previous_period() {
        let s = [
            past("epic:Sugar", at(2026, 11, 9, 20, 0), 3600),  // lundi précédent
            past("epic:Sugar", at(2026, 11, 13, 20, 0), 7200), // vendredi précédent
            past("epic:Sugar", at(2026, 11, 17, 20, 0), 1800), // mardi
        ];
        let r = recap(&s, now(), Period::Week, 0);
        assert_eq!((r.total_secs, r.previous_secs, r.previous_to_date_secs), (1800, 10_800, 3600));
        let r = recap(&s, now(), Period::Week, 1);
        assert_eq!((r.total_secs, r.previous_secs, r.previous_to_date_secs), (10_800, 0, 0));
        // Mois de 31 jours après un mois de 30 : le « à date » ne dépasse pas la période précédente.
        let r = recap(&[past("epic:Sugar", at(2026, 11, 30, 20, 0), 600)], at(2026, 12, 31, 12, 0), Period::Month, 0);
        assert_eq!((r.previous_secs, r.previous_to_date_secs), (600, 600));
    }

    #[test]
    fn launcher_sessions_count_once() {
        let steam = PastSession { launcher: true, ..past("steam:730", at(2026, 11, 17, 20, 0), 3600) };
        // Le doublon (même jeu, même début) n'est compté qu'une fois ; aucun autre temps ne s'y ajoute.
        let r = recap(&[steam.clone(), steam.clone(), past("epic:Sugar", at(2026, 11, 16, 20, 0), 600)], now(), Period::Week, 0);
        assert_eq!((r.total_secs, r.sessions, r.games, r.history_sessions), (4200, 2, 2, 2));
        assert_eq!(r.top[0], GameStat { game_id: "steam:730".into(), secs: 3600, sessions: 1 });
        assert_eq!(r.longest, Some(LongSession { game_id: "steam:730".into(), start: steam.start, secs: 3600 }));
    }

    #[test]
    fn merged_sessions_stay_usable() {
        // Trois heures regroupées le lundi (heure inconnue), une vraie session le mardi soir.
        let merged = PastSession { merged: true, ..past("epic:Sugar", at(2026, 11, 16, 9, 0), 3 * 3600) };
        let r = recap(&[merged, past("steam:440", at(2026, 11, 17, 20, 0), 1800)], now(), Period::Week, 0);
        assert_eq!((r.total_secs, r.days[0], r.games), (3 * 3600 + 1800, 3 * 3600, 2));
        assert_eq!(r.weekday_secs[0], 3 * 3600);
        assert_eq!(r.favorite_weekday, Some(0));
        // Ni moment de la journée, ni plus longue session pour l'entrée regroupée.
        assert_eq!(r.moment_secs, [0, 0, 0, 1800]);
        assert_eq!(r.favorite_moment, Some(3));
        assert_eq!(r.longest.map(|l| l.game_id), Some("steam:440".to_string()));
    }

    #[test]
    fn top_new_games_and_achievements() {
        let mut s: Vec<_> = (0..6).map(|i| past(&format!("epic:G{i}"), at(2026, 11, 16, 10 + i, 0), 600 * (u64::from(i) + 1))).collect();
        s.push(past("epic:G5", at(2026, 11, 17, 10, 0), 600));
        // G0 déjà joué le mois dernier : pas nouveau.
        s.push(past("epic:G0", at(2026, 10, 1, 10, 0), 600));
        let unlocks = vec![
            ("steam:730".to_string(), vec![at(2026, 10, 1, 10, 0), at(2026, 11, 17, 10, 0), at(2026, 11, 18, 10, 0)]),
            ("steam:440".to_string(), vec![at(2026, 11, 16, 10, 0)]),
            ("steam:570".to_string(), vec![at(2026, 1, 1, 10, 0)]),
        ];
        let r = compute(&s, &unlocks, now(), Period::Week, 0, &Fixed(0));
        assert_eq!(r.top.len(), 5);
        assert_eq!(r.top[0], GameStat { game_id: "epic:G5".into(), secs: 4200, sessions: 2 });
        assert_eq!(r.top.iter().map(|g| g.game_id.as_str()).collect::<Vec<_>>(), ["epic:G5", "epic:G4", "epic:G3", "epic:G2", "epic:G1"]);
        assert_eq!((r.games, r.sessions), (6, 7));
        assert_eq!(r.new_games.iter().map(|g| g.game_id.as_str()).collect::<Vec<_>>(), ["epic:G1", "epic:G2", "epic:G3", "epic:G4", "epic:G5"]);
        assert_eq!(r.achievements, 3);
        assert_eq!(r.achievement_games, vec![Unlocks { game_id: "steam:730".into(), count: 2 }, Unlocks { game_id: "steam:440".into(), count: 1 }]);
        // Moment préféré : l'après-midi (G2 à G5, 12 h-16 h) plutôt que le matin (G0, G1 et G5 le mardi).
        assert_eq!(r.favorite_moment, Some(2));
        assert_eq!(r.history_since, Some(at(2026, 10, 1, 10, 0)));
    }

    #[test]
    fn empty_history() {
        let r = recap(&[], now(), Period::Month, 0);
        assert_eq!((r.total_secs, r.sessions, r.history_sessions, r.history_since), (0, 0, 0, None));
        assert_eq!((r.favorite_weekday, r.favorite_moment, r.longest, r.best_streak, r.current_streak), (None, None, None, 0, 0));
        assert!(r.top.is_empty() && r.new_games.is_empty());
    }
}
