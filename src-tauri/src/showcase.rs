//! Vitrine des succès Steam : pourcentages mondiaux, liste triée de la fiche, et chiffres de la
//! section « Vitrine » (total, jeux à 100 %, succès les plus rares, derniers déblocages).
//!
//! Pourcentages : API publique sans clé `ISteamUserStats/GetGlobalAchievementPercentagesForApp/v2`
//! (`{"achievementpercentages":{"achievements":[{"name":"…","percent":"12.3"}]}}`, le pourcentage
//! en chaîne ou en nombre selon les versions ; appid inconnu = 403 et `{}`). Gardés dans
//! `achievements-global.json` (dossier de données de l'app), redemandés après 7 jours. Une erreur
//! réseau arrête la lecture sans rien effacer : les pourcentages manquent, rien d'autre.

use crate::library::{Achievement, Game};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const REFRESH_AFTER: u64 = 7 * 24 * 3600;
/// À augmenter quand le contenu du cache change : tout est alors redemandé.
const CACHE_VERSION: u32 = 1;
const PERCENTAGES: &str = "https://api.steampowered.com/ISteamUserStats/GetGlobalAchievementPercentagesForApp/v2/";
/// Requêtes simultanées : la première ouverture de la vitrine lit des dizaines de jeux.
const WORKERS: usize = 4;
/// Taille des listes de la vitrine.
const TOP: usize = 10;

/// Pourcentage de joueurs par nom interne de succès.
pub type Percents = BTreeMap<String, f32>;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Entry {
    /// Date de la requête (secondes Unix).
    checked: u64,
    /// `None` : Steam n'a rien donné pour cet appid.
    #[serde(default)]
    percents: Option<Percents>,
}

/// Cache des pourcentages mondiaux, par appid.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GlobalCache {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    apps: BTreeMap<u32, Entry>,
    #[serde(skip)]
    path: PathBuf,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl GlobalCache {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("achievements-global.json");
        let mut cache: GlobalCache = fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        if cache.version != CACHE_VERSION {
            cache.apps.clear();
            cache.version = CACHE_VERSION;
        }
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

    /// Appids jamais demandés, ou demandés il y a plus de 7 jours.
    pub fn to_fetch(&self, appids: &[u32]) -> Vec<u32> {
        self.to_fetch_at(appids, now())
    }

    fn to_fetch_at(&self, appids: &[u32], at: u64) -> Vec<u32> {
        let mut ids: Vec<u32> = appids
            .iter()
            .copied()
            .filter(|id| self.apps.get(id).is_none_or(|e| at.saturating_sub(e.checked) > REFRESH_AFTER))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    pub fn record(&mut self, results: Vec<(u32, Option<Percents>)>) {
        self.record_at(results, now());
    }

    fn record_at(&mut self, results: Vec<(u32, Option<Percents>)>, checked: u64) {
        for (id, percents) in results {
            // Des pourcentages connus ne sont pas effacés par une réponse vide.
            let percents = percents.or_else(|| self.apps.get(&id).and_then(|e| e.percents.clone()));
            self.apps.insert(id, Entry { checked, percents });
        }
    }

    /// Reporte le pourcentage mondial connu sur chaque succès du jeu.
    pub fn apply(&self, appid: u32, list: &mut [Achievement]) {
        let Some(percents) = self.apps.get(&appid).and_then(|e| e.percents.as_ref()) else { return };
        for a in list {
            a.global_percent = percents.get(&a.api_name).copied();
        }
    }
}

/// Lit la réponse de l'API. `None` : rien d'exploitable (appid inconnu, réponse vide).
fn parse(body: &str) -> Option<Percents> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let list = value.get("achievementpercentages")?.get("achievements")?.as_array()?;
    let percents: Percents = list
        .iter()
        .filter_map(|a| {
            let name = a.get("name")?.as_str()?.trim();
            let percent = match a.get("percent")? {
                serde_json::Value::String(s) => s.trim().parse::<f32>().ok()?,
                serde_json::Value::Number(n) => n.as_f64()? as f32,
                _ => return None,
            };
            (!name.is_empty() && (0.0..=100.0).contains(&percent)).then(|| (name.to_string(), percent))
        })
        .collect();
    (!percents.is_empty()).then_some(percents)
}

/// Une requête. `Ok(None)` : Steam ne connaît pas l'appid (4xx) ; `Err` : réseau, quota ou serveur,
/// à retenter plus tard.
fn request(agent: &ureq::Agent, appid: u32) -> Result<Option<Percents>, String> {
    match agent.get(PERCENTAGES).query("gameid", &appid.to_string()).call() {
        Ok(response) => Ok(parse(&response.into_string().map_err(|e| e.to_string())?)),
        Err(ureq::Error::Status(code, _)) if (400..500).contains(&code) && code != 429 => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Demande les pourcentages de plusieurs jeux, quatre à la fois. La première erreur réseau arrête
/// tout : les appids non lus ne sont pas renvoyés et seront redemandés la prochaine fois.
pub fn fetch(appids: &[u32]) -> Vec<(u32, Option<Percents>)> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(8))
        .redirects(0)
        .user_agent("Ludex (lanceur de jeux)")
        .build();
    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let results = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..WORKERS.min(appids.len()) {
            scope.spawn(|| {
                while !stop.load(AtomicOrdering::Relaxed) {
                    let Some(&appid) = appids.get(next.fetch_add(1, AtomicOrdering::Relaxed)) else { break };
                    match request(&agent, appid) {
                        Ok(percents) => {
                            if let Ok(mut r) = results.lock() {
                                r.push((appid, percents));
                            }
                        }
                        Err(e) => {
                            log::warn!("Pourcentages des succès interrompus : {e}");
                            stop.store(true, AtomicOrdering::Relaxed);
                        }
                    }
                }
            });
        }
    });
    results.into_inner().unwrap_or_default()
}

/// Pourcentage décroissant, inconnu en dernier.
fn easiest_first(a: Option<f32>, b: Option<f32>) -> Ordering {
    match (a, b) {
        (Some(x), Some(y)) => y.total_cmp(&x),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// Ordre de la liste de la fiche : non débloqués d'abord, les plus faciles (pourcentage le plus
/// haut) en tête ; puis débloqués, le plus récent d'abord. À égalité, l'ordre du schéma.
pub fn sort_for_display(list: &mut [Achievement]) {
    list.sort_by(|a, b| match (a.unlocked, b.unlocked) {
        (false, true) => Ordering::Less,
        (true, false) => Ordering::Greater,
        (false, false) => easiest_first(a.global_percent, b.global_percent),
        (true, true) => b.unlocked_at.cmp(&a.unlocked_at),
    });
}

/// Succès débloqué montré par la vitrine.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Unlock {
    pub game_id: String,
    pub game_title: String,
    pub api_name: String,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub percent: Option<f32>,
    pub unlocked_at: Option<u64>,
}

/// Jeu dont tous les succès sont débloqués.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Perfect {
    pub game_id: String,
    pub game_title: String,
    pub total: u32,
    /// Dernier déblocage, donc date du 100 % si Steam l'a gardée.
    pub completed_at: Option<u64>,
}

/// Contenu de la section « Vitrine ».
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Showcase {
    /// Succès débloqués, tous jeux confondus.
    pub unlocked: u32,
    pub total: u32,
    /// Jeux qui ont des succès (et une progression connue).
    pub games: u32,
    /// Jeux à 100 %, le plus récemment terminé d'abord.
    pub perfect: Vec<Perfect>,
    /// Les dix succès débloqués les plus rares (pourcentage connu), le plus rare d'abord.
    pub rarest: Vec<Unlock>,
    /// Les dix derniers déblocages datés.
    pub recent: Vec<Unlock>,
}

/// Chiffres de la vitrine. `games` : jeux avec une progression de succès connue ; `details` : la
/// liste complète (pourcentages appliqués) des jeux qui en ont débloqué au moins un.
pub fn build(games: &[Game], details: &[(&Game, Vec<Achievement>)]) -> Showcase {
    let with = || games.iter().filter_map(|g| g.achievements.as_ref().map(|a| (g, a))).filter(|(_, a)| a.total > 0);
    let mut perfect: Vec<Perfect> = with()
        .filter(|(_, a)| a.unlocked >= a.total)
        .map(|(g, a)| Perfect { game_id: g.id.clone(), game_title: g.title.clone(), total: a.total, completed_at: a.last_unlocked })
        .collect();
    perfect.sort_by(|a, b| b.completed_at.cmp(&a.completed_at).then_with(|| a.game_title.cmp(&b.game_title)));

    let unlocks: Vec<Unlock> = details
        .iter()
        .flat_map(|(game, list)| {
            list.iter().filter(|a| a.unlocked).map(|a| Unlock {
                game_id: game.id.clone(),
                game_title: game.title.clone(),
                api_name: a.api_name.clone(),
                name: a.name.clone(),
                description: a.description.clone(),
                icon: a.icon.clone(),
                percent: a.global_percent,
                unlocked_at: a.unlocked_at,
            })
        })
        .collect();
    // Le plus rare d'abord ; à égalité, le plus récent.
    let mut rarest: Vec<Unlock> = unlocks.iter().filter(|u| u.percent.is_some()).cloned().collect();
    rarest.sort_by(|a, b| easiest_first(b.percent, a.percent).then_with(|| b.unlocked_at.cmp(&a.unlocked_at)));
    rarest.truncate(TOP);
    let mut recent: Vec<Unlock> = unlocks.into_iter().filter(|u| u.unlocked_at.is_some()).collect();
    recent.sort_by_key(|u| std::cmp::Reverse(u.unlocked_at));
    recent.truncate(TOP);

    Showcase {
        unlocked: with().map(|(_, a)| a.unlocked).sum(),
        total: with().map(|(_, a)| a.total).sum(),
        games: with().count() as u32,
        perfect,
        rarest,
        recent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{Achievements, Launch, Platform};

    fn ach(api: &str, unlocked: bool, at: Option<u64>, percent: Option<f32>) -> Achievement {
        Achievement {
            api_name: api.into(),
            name: api.to_lowercase(),
            description: None,
            icon: None,
            icon_gray: None,
            hidden: false,
            unlocked,
            unlocked_at: at,
            global_percent: percent,
        }
    }

    fn game(appid: &str, title: &str, unlocked: u32, total: u32, last: Option<u64>) -> Game {
        let mut g = Game::new(Platform::Steam, appid, title, Launch::Uri(String::new()));
        g.achievements = Some(Achievements { unlocked, total, last_unlocked: last, unlock_times: Vec::new() });
        g
    }

    #[test]
    fn parses_string_or_number_percents() {
        let body = r#"{"achievementpercentages":{"achievements":[
            {"name":"A","percent":"97.1"},{"name":"B","percent":0.3},{"name":"C","percent":"abc"},
            {"name":"","percent":"5"},{"name":"D","percent":"250"}]}}"#;
        let p = parse(body).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p["A"], 97.1);
        assert_eq!(p["B"], 0.3);
        assert_eq!(parse("{}"), None); // appid inconnu
        assert_eq!(parse(r#"{"achievementpercentages":{"achievements":[]}}"#), None);
        assert_eq!(parse("<html>"), None);
    }

    #[test]
    fn sorts_locked_easiest_first_then_recent_unlocks() {
        let mut list = vec![
            ach("U_OLD", true, Some(100), Some(1.0)),
            ach("L_RARE", false, None, Some(2.0)),
            ach("L_UNKNOWN", false, None, None),
            ach("U_NEW", true, Some(300), Some(50.0)),
            ach("L_EASY", false, None, Some(80.0)),
            ach("U_UNDATED", true, None, None),
        ];
        sort_for_display(&mut list);
        let order: Vec<&str> = list.iter().map(|a| a.api_name.as_str()).collect();
        assert_eq!(order, ["L_EASY", "L_RARE", "L_UNKNOWN", "U_NEW", "U_OLD", "U_UNDATED"]);
    }

    #[test]
    fn builds_showcase_by_rarity() {
        let a = game("10", "Alpha", 3, 3, Some(500));
        let b = game("20", "Beta", 2, 4, Some(400));
        let c = game("30", "Gamma", 0, 5, None);
        let none = Game::new(Platform::Steam, "40", "Sans succès", Launch::Uri(String::new()));
        let games = vec![a.clone(), b.clone(), c, none];
        let mut a_list = vec![ach("A1", true, Some(100), Some(40.0)), ach("A2", true, Some(500), Some(0.5)), ach("A3", true, Some(200), None)];
        let mut b_list = vec![ach("B1", true, Some(400), Some(0.5)), ach("B2", true, Some(300), Some(12.0)), ach("B3", false, None, Some(0.1))];
        // Une liste lue telle quelle : seul l'ordre de rareté compte ici.
        sort_for_display(&mut a_list);
        sort_for_display(&mut b_list);
        let s = build(&games, &[(&a, a_list), (&b, b_list)]);

        assert_eq!((s.unlocked, s.total, s.games), (5, 12, 3));
        assert_eq!(s.perfect.len(), 1);
        assert_eq!((s.perfect[0].game_title.as_str(), s.perfect[0].completed_at), ("Alpha", Some(500)));
        // Rareté croissante ; à égalité (0,5 %), le plus récent d'abord ; sans pourcentage : absent.
        let rare: Vec<&str> = s.rarest.iter().map(|u| u.api_name.as_str()).collect();
        assert_eq!(rare, ["A2", "B1", "B2", "A1"]);
        assert_eq!(s.rarest[0].game_id, "steam:10");
        // B3 n'est pas débloqué : ni rare ni récent.
        let recent: Vec<&str> = s.recent.iter().map(|u| u.api_name.as_str()).collect();
        assert_eq!(recent, ["A2", "B1", "B2", "A3", "A1"]);
        assert_eq!(build(&[], &[]), Showcase::default());
    }

    #[test]
    fn keeps_top_ten_only() {
        let g = game("10", "Alpha", 15, 15, None);
        let list: Vec<Achievement> = (0..15).map(|i| ach(&format!("A{i}"), true, Some(i), Some(i as f32))).collect();
        let s = build(std::slice::from_ref(&g), &[(&g, list)]);
        assert_eq!(s.rarest.len(), TOP);
        assert_eq!(s.rarest[0].api_name, "A0");
        assert_eq!(s.recent.len(), TOP);
        assert_eq!(s.recent[0].api_name, "A14");
    }

    #[test]
    fn cache_refreshes_after_a_week_and_survives_reload() {
        let dir = std::env::temp_dir().join(format!("ludex-global-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut cache = GlobalCache::load(&dir);
        assert_eq!(cache.to_fetch_at(&[30, 10, 10], 1_000), [10, 30]);
        let percents: Percents = [("A".to_string(), 12.5)].into();
        cache.record_at(vec![(10, Some(percents)), (30, None)], 1_000);
        assert!(cache.to_fetch_at(&[10, 30], 1_000 + REFRESH_AFTER).is_empty());
        assert_eq!(cache.to_fetch_at(&[10, 30], 1_001 + REFRESH_AFTER), [10, 30]);
        // Réponse vide plus tard : les pourcentages connus restent.
        cache.record_at(vec![(10, None)], 2_000);
        cache.save().unwrap();

        let reloaded = GlobalCache::load(&dir);
        let mut list = vec![ach("A", true, None, None), ach("B", false, None, None)];
        reloaded.apply(10, &mut list);
        assert_eq!((list[0].global_percent, list[1].global_percent), (Some(12.5), None));
        reloaded.apply(30, &mut list); // rien de connu : inchangé
        assert_eq!(list[0].global_percent, Some(12.5));
        assert!(reloaded.to_fetch_at(&[10], 2_000).is_empty());

        // Autre version du cache : tout est redemandé.
        fs::write(dir.join("achievements-global.json"), r#"{"version":0,"apps":{"10":{"checked":2000}}}"#).unwrap();
        assert_eq!(GlobalCache::load(&dir).to_fetch_at(&[10], 2_000), [10]);
        let _ = fs::remove_dir_all(&dir);
    }

    /// Vitrine réelle de ce PC : `cargo test showcase_this_pc -- --ignored --nocapture`.
    /// Pourcentages demandés à Steam, cache dans un dossier temporaire (jamais celui de l'app).
    #[test]
    #[ignore]
    fn showcase_this_pc() {
        let start = std::time::Instant::now();
        let games: Vec<Game> = crate::library::scan().games.into_iter().filter(|g| g.achievements.is_some()).collect();
        println!("Scan : {:?}, {} jeux avec succès", start.elapsed(), games.len());
        let stats = crate::library::SteamStats::locate().expect("Steam introuvable");
        let unlocked: Vec<&Game> = games.iter().filter(|g| g.achievements.as_ref().is_some_and(|a| a.unlocked > 0)).collect();
        let appids: Vec<u32> = unlocked.iter().filter_map(|g| g.platform_id.parse().ok()).collect();

        let dir = std::env::temp_dir().join("ludex-showcase-this-pc");
        let mut cache = GlobalCache::load(&dir);
        let start = std::time::Instant::now();
        let todo = cache.to_fetch(&appids);
        let fetched = fetch(&todo);
        println!("Pourcentages : {} jeux demandés, {} lus en {:?}", todo.len(), fetched.len(), start.elapsed());
        cache.record(fetched);
        cache.save().unwrap();

        let start = std::time::Instant::now();
        let details: Vec<(&Game, Vec<Achievement>)> = unlocked
            .iter()
            .filter_map(|&g| {
                let mut list = stats.details(&g.platform_id)?;
                cache.apply(g.platform_id.parse().ok()?, &mut list);
                Some((g, list))
            })
            .collect();
        println!("Détail de {} jeux lu en {:?}", details.len(), start.elapsed());
        let s = build(&games, &details);
        println!("{} succès sur {} dans {} jeux, {} jeux à 100 %", s.unlocked, s.total, s.games, s.perfect.len());
        for p in &s.perfect {
            println!("  100 % : {} ({} succès)", p.game_title, p.total);
        }
        println!("Les plus rares :");
        for (i, u) in s.rarest.iter().enumerate() {
            println!("  {:>2}. {:>5.1} %  {} ({})", i + 1, u.percent.unwrap_or(0.0), u.name, u.game_title);
        }
        println!("Derniers déblocages :");
        for u in &s.recent {
            println!("  {}  {} ({})", u.unlocked_at.unwrap_or(0), u.name, u.game_title);
        }
        let hidden = details.iter().flat_map(|(_, l)| l).filter(|a| a.hidden && !a.unlocked).count();
        println!("Succès cachés non débloqués (masqués) : {hidden}");
    }
}
