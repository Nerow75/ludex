//! Section « Stockage » : place prise par les jeux sur chaque disque, par launcher, et jeux qui
//! pourraient être désinstallés (pas lancés depuis six mois, ou jamais).
//!
//! Steam (`SizeOnDisk`) et Epic (`InstallSize`) donnent la taille de leurs jeux. Pour les autres,
//! Ludex additionne les fichiers du dossier d'installation, dans un fil à part (`lib.rs`), sans
//! suivre les liens ni les jonctions. Les mesures sont gardées dans `sizes-cache.json` (dossier de
//! données de l'app) : un dossier est mesuré de nouveau quand sa date de modification change ou
//! après 7 jours. Un dossier trop large (racine d'un lecteur, `Program Files`…) ou qui contient le
//! dossier d'un autre jeu n'est jamais mesuré : il compterait des fichiers qui ne sont pas au jeu.
//!
//! Ludex ne supprime aucun fichier : la désinstallation passe par le launcher (Steam) ou par le
//! dossier du jeu, ouvert dans l'explorateur.

use crate::library::{Game, Platform};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const DAY: u64 = 86_400;
/// Mesure refaite après ce délai, même si le dossier n'a pas changé : sa date ne bouge que pour
/// ses fichiers directs, pas pour ceux des sous-dossiers.
const MAX_AGE: u64 = 7 * DAY;
/// Mesure d'un dossier absent de la bibliothèque depuis ce délai : oubliée.
const FORGET_AFTER: u64 = 60 * DAY;
/// Jeu pas lancé depuis ce délai (six mois) : proposé à la désinstallation.
pub const IDLE_AFTER: u64 = 182 * DAY;
/// À augmenter si la façon de mesurer change : tous les dossiers sont alors mesurés de nouveau.
const CACHE_VERSION: u32 = 1;
const CACHE_FILE: &str = "sizes-cache.json";

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Chemin comparable : séparateurs Windows, minuscules, sans préfixe `\\?\` ni `\` final.
fn key(dir: &str) -> String {
    let dir = dir.replace('/', "\\").to_lowercase();
    let dir = dir.strip_prefix(r"\\?\").unwrap_or(&dir);
    dir.trim_end_matches('\\').to_string()
}

/// `dir` est `parent` ou se trouve dessous (clés déjà normalisées).
fn is_under(dir: &str, parent: &str) -> bool {
    dir == parent || dir.strip_prefix(parent).is_some_and(|rest| rest.starts_with('\\'))
}

/// Dossier qu'on peut mesurer pour ce jeu : ni trop large (même règle que le suivi des sessions),
/// ni parent du dossier d'un autre jeu (ce jeu-là serait compté deux fois).
pub fn measurable(dir: &str, games: &[Game]) -> bool {
    if crate::session::root_key(dir).is_none() {
        return false;
    }
    let k = key(dir);
    !games
        .iter()
        .filter_map(|g| g.install_dir.as_deref())
        .map(key)
        .any(|other| other != k && is_under(&other, &k))
}

/// Taille des fichiers d'un dossier et de ses sous-dossiers, en octets. Liens symboliques et
/// jonctions ignorés (ils mènent hors du jeu, ou en boucle). Un sous-dossier illisible est passé ;
/// le dossier lui-même illisible est une erreur.
pub fn dir_size(dir: &Path) -> std::io::Result<u64> {
    let mut total = 0u64;
    let mut stack = vec![fs::read_dir(dir)?];
    while let Some(entries) = stack.pop() {
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                if let Ok(sub) = fs::read_dir(entry.path()) {
                    stack.push(sub);
                }
            } else if let Ok(meta) = entry.metadata() {
                total = total.saturating_add(meta.len());
            }
        }
    }
    Ok(total)
}

/// Date de modification d'un dossier (secondes Unix), `None` s'il est absent ou illisible.
pub fn modified(dir: &Path) -> Option<u64> {
    let meta = fs::metadata(dir).ok()?;
    meta.is_dir().then_some(())?;
    meta.modified().ok()?.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    bytes: u64,
    /// Date de modification du dossier au moment de la mesure.
    modified: u64,
    /// Date de la mesure (secondes Unix).
    checked: u64,
}

/// Dossier à mesurer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    key: String,
    pub dir: PathBuf,
    modified: u64,
}

/// Tailles mesurées par Ludex, par dossier d'installation.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SizeCache {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    dirs: BTreeMap<String, Entry>,
    #[serde(skip)]
    path: PathBuf,
}

impl SizeCache {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join(CACHE_FILE);
        let mut cache: SizeCache = fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        if cache.version != CACHE_VERSION {
            cache.dirs.clear();
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
        fs::write(&tmp, serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }

    /// Pose la taille mesurée sur un jeu dont le launcher ne donne pas la sienne. Une mesure
    /// ancienne reste affichée en attendant la nouvelle. Renvoie vrai si la taille a changé.
    pub fn apply(&self, game: &mut Game) -> bool {
        let Some(entry) = game.install_dir.as_deref().and_then(|d| self.dirs.get(&key(d))) else { return false };
        put(game, entry.bytes)
    }

    /// Dossiers à mesurer : jeux sans taille du launcher dont le dossier n'a jamais été mesuré, a
    /// changé depuis, ou a été mesuré il y a plus de 7 jours. `modified` lit la date d'un dossier.
    pub fn to_measure(&self, games: &[Game], now: u64, modified: impl Fn(&Path) -> Option<u64>) -> Vec<Job> {
        let mut jobs: Vec<Job> = Vec::new();
        for g in games.iter().filter(|g| g.size_on_disk.is_none() || g.size_measured) {
            let Some(dir) = g.install_dir.as_deref().filter(|d| measurable(d, games)) else { continue };
            let k = key(dir);
            if jobs.iter().any(|j| j.key == k) {
                continue;
            }
            let Some(m) = modified(Path::new(dir)) else { continue };
            let fresh = self.dirs.get(&k).is_some_and(|e| e.modified == m && now.saturating_sub(e.checked) < MAX_AGE);
            if !fresh {
                jobs.push(Job { key: k, dir: PathBuf::from(dir), modified: m });
            }
        }
        jobs
    }

    pub fn record(&mut self, job: &Job, bytes: u64, now: u64) {
        self.dirs.insert(job.key.clone(), Entry { bytes, modified: job.modified, checked: now });
    }

    /// Oublie les mesures des dossiers absents de la bibliothèque depuis longtemps (disque
    /// débranché quelques jours : la mesure reste).
    pub fn forget_old(&mut self, games: &[Game], now: u64) {
        let present: Vec<String> = games.iter().filter_map(|g| g.install_dir.as_deref()).map(key).collect();
        self.dirs.retain(|k, e| present.contains(k) || now.saturating_sub(e.checked) < FORGET_AFTER);
    }
}

/// Pose une taille mesurée sur un jeu, sauf si le launcher donne la sienne. Vrai si elle a changé.
fn put(game: &mut Game, bytes: u64) -> bool {
    if game.size_on_disk.is_some() && !game.size_measured {
        return false;
    }
    let changed = game.size_on_disk != Some(bytes);
    game.size_on_disk = Some(bytes);
    game.size_measured = true;
    changed
}

/// Pose une mesure toute fraîche sur les jeux de ce dossier (sans le cache : jamais le cache et
/// les jeux verrouillés ensemble). Renvoie les jeux dont la taille a changé, avec elle.
pub fn publish(job: &Job, bytes: u64, games: &mut [Game]) -> Vec<(String, u64)> {
    games
        .iter_mut()
        .filter(|g| g.install_dir.as_deref().is_some_and(|d| key(d) == job.key))
        .filter_map(|g| put(g, bytes).then(|| (g.id.clone(), bytes)))
        .collect()
}

/// Disque monté (lecteur ou dossier de montage).
#[derive(Clone, Debug)]
pub struct Volume {
    /// Point de montage : « C:\ », ou un dossier.
    pub mount: String,
    /// Nom du volume (« Jeux »), vide s'il n'en a pas.
    pub name: String,
    pub total: u64,
    pub free: u64,
}

/// Disques de la machine (crate `sysinfo`).
pub fn volumes() -> Vec<Volume> {
    let Ok(disks) = sysinfo::Disks::new_with_refreshed_list() else { return Vec::new() };
    disks
        .iter()
        .map(|d| Volume {
            mount: d.mount_point().to_string_lossy().into_owned(),
            name: d.name().to_string_lossy().trim().to_string(),
            total: d.total_space(),
            free: d.available_space(),
        })
        .collect()
}

/// Disque d'un dossier : le point de montage le plus long qui le contient.
pub(crate) fn volume_of<'a>(dir: &str, volumes: &'a [Volume]) -> Option<&'a Volume> {
    let k = key(dir);
    volumes.iter().filter(|v| is_under(&k, &key(&v.mount))).max_by_key(|v| key(&v.mount).len())
}

/// Lettre affichée : « C: », ou le point de montage sans `\` final.
fn letter(mount: &str) -> String {
    mount.trim_end_matches(['\\', '/']).to_string()
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlatformShare {
    pub platform: Platform,
    pub bytes: u64,
    pub games: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskUsage {
    pub letter: String,
    pub name: String,
    pub total: u64,
    pub free: u64,
    /// Place prise par les jeux de Ludex (tailles connues).
    pub games_bytes: u64,
    pub games: usize,
    /// Jeux de ce disque dont la taille n'est pas encore connue (mesure en cours ou impossible).
    pub unknown: usize,
    /// Par launcher, la plus grosse part d'abord.
    pub platforms: Vec<PlatformShare>,
    /// Moins de 10 % d'espace libre.
    pub low: bool,
    /// Jeux de ce disque (masqués compris), pour la liste « Jeux de ce disque » et « Déplacer… ».
    pub ids: Vec<String>,
}

/// Disques qui contiennent au moins un jeu, avec la place des jeux par launcher. Les jeux masqués
/// comptent aussi : ils occupent le disque.
pub fn disks(games: &[Game], volumes: &[Volume]) -> Vec<DiskUsage> {
    let mut out: Vec<DiskUsage> = Vec::new();
    for g in games {
        let Some(v) = g.install_dir.as_deref().and_then(|d| volume_of(d, volumes)) else { continue };
        let l = letter(&v.mount);
        let i = match out.iter().position(|d| d.letter == l) {
            Some(i) => i,
            None => {
                out.push(DiskUsage {
                    letter: l,
                    name: v.name.clone(),
                    total: v.total,
                    free: v.free,
                    games_bytes: 0,
                    games: 0,
                    unknown: 0,
                    platforms: Vec::new(),
                    low: v.total > 0 && v.free < v.total / 10,
                    ids: Vec::new(),
                });
                out.len() - 1
            }
        };
        let disk = &mut out[i];
        disk.games += 1;
        disk.ids.push(g.id.clone());
        let Some(bytes) = g.size_on_disk.filter(|&b| b > 0) else {
            disk.unknown += 1;
            continue;
        };
        disk.games_bytes += bytes;
        match disk.platforms.iter_mut().find(|p| p.platform == g.platform) {
            Some(p) => {
                p.bytes += bytes;
                p.games += 1;
            }
            None => disk.platforms.push(PlatformShare { platform: g.platform, bytes, games: 1 }),
        }
    }
    for d in &mut out {
        d.platforms.sort_by_key(|p| std::cmp::Reverse(p.bytes));
    }
    out.sort_by(|a, b| a.letter.cmp(&b.letter));
    out
}

/// Jeu qui pourrait être désinstallé.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub id: String,
    pub title: String,
    pub platform: Platform,
    pub bytes: u64,
    /// Dernier lancement (secondes Unix), `None` : jamais lancé.
    pub last_played: Option<u64>,
    /// Le launcher note lui-même les lancements (Steam, GOG Galaxy). Sinon, seuls ceux faits
    /// depuis Ludex sont connus : « jamais lancé » veut dire « jamais depuis Ludex ».
    pub launcher_dates: bool,
    /// Lecteur du jeu (« D: »).
    pub drive: Option<String>,
    /// Le launcher a une désinstallation que Ludex sait ouvrir (sinon : le dossier du jeu).
    pub uninstall: bool,
}

/// Jeux pas lancés depuis six mois ou jamais lancés, de taille connue, le plus gros d'abord. Les
/// jeux masqués (outils compris) n'y sont pas : l'utilisateur les a déjà écartés.
pub fn candidates(games: &[Game], volumes: &[Volume], now: u64) -> Vec<Candidate> {
    let mut list: Vec<Candidate> = games
        .iter()
        .filter(|g| !g.hidden && !g.is_tool)
        .filter(|g| g.last_played.filter(|&t| t > 0).is_none_or(|t| now.saturating_sub(t) >= IDLE_AFTER))
        .filter_map(|g| {
            let bytes = g.size_on_disk.filter(|&b| b > 0)?;
            Some(Candidate {
                id: g.id.clone(),
                title: g.title.clone(),
                platform: g.platform,
                bytes,
                last_played: g.last_played.filter(|&t| t > 0),
                launcher_dates: g.launcher_tracks_playtime,
                drive: g.install_dir.as_deref().and_then(|d| volume_of(d, volumes)).map(|v| letter(&v.mount)),
                uninstall: uninstall_uri(g).is_some(),
            })
        })
        .collect();
    list.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.title.cmp(&b.title)));
    list
}

/// Désinstallation par le launcher, reconstruite depuis le jeu de l'état (jamais depuis le front) :
/// Steam seulement, qui demande confirmation avant de supprimer quoi que ce soit.
pub fn uninstall_uri(game: &Game) -> Option<String> {
    let appid = &game.platform_id;
    (game.platform == Platform::Steam && !appid.is_empty() && appid.bytes().all(|b| b.is_ascii_digit()))
        .then(|| format!("steam://uninstall/{appid}"))
}

/// Relevé de la section « Stockage ».
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Storage {
    pub disks: Vec<DiskUsage>,
    pub candidates: Vec<Candidate>,
    /// Des tailles sont en cours de mesure : le relevé va encore bouger.
    pub measuring: bool,
}

pub fn report(games: &[Game], volumes: &[Volume], now: u64, measuring: bool) -> Storage {
    Storage { disks: disks(games, volumes), candidates: candidates(games, volumes, now), measuring }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Launch;

    const GB: u64 = 1_000_000_000;
    const NOW: u64 = 1_800_000_000;

    fn game(platform: Platform, id: &str, dir: &str, size: Option<u64>, last: Option<u64>) -> Game {
        let mut g = Game::new(platform, id, id, Launch::Uri(String::new()));
        g.install_dir = Some(dir.to_string());
        g.size_on_disk = size;
        g.last_played = last;
        g
    }

    fn vols() -> Vec<Volume> {
        vec![
            Volume { mount: r"C:\".into(), name: "Système".into(), total: 500 * GB, free: 30 * GB },
            Volume { mount: r"E:\".into(), name: "Jeux".into(), total: 2000 * GB, free: 900 * GB },
            Volume { mount: r"F:\".into(), name: String::new(), total: 100 * GB, free: 50 * GB },
            Volume { mount: r"E:\Montage\".into(), name: "Monté".into(), total: 100 * GB, free: 90 * GB },
        ]
    }

    #[test]
    fn aggregates_games_by_disk_and_launcher() {
        let games = vec![
            game(Platform::Steam, "1", r"C:\Steam\steamapps\common\A", Some(40 * GB), None),
            game(Platform::Steam, "2", r"E:\SteamLibrary\steamapps\common\B", Some(10 * GB), None),
            game(Platform::Steam, "3", r"e:/steamlibrary/steamapps/common/C", Some(5 * GB), None),
            game(Platform::Other, "x", r"E:\Jeux\Dofus", Some(20 * GB), None),
            game(Platform::Other, "y", r"E:\Jeux\Encore", None, None),
            game(Platform::Ea, "z", r"E:\Montage\FC 24", Some(GB), None),
            game(Platform::Riot, "r", r"Z:\Riot", Some(GB), None),
        ];
        let d = disks(&games, &vols());
        assert_eq!(d.iter().map(|d| d.letter.as_str()).collect::<Vec<_>>(), [r"C:", r"E:", r"E:\Montage"]);
        let (c, e, m) = (&d[0], &d[1], &d[2]);
        assert!(c.low, "30 Go libres sur 500 : moins de 10 %");
        assert!(!e.low);
        assert_eq!((c.games, c.games_bytes, c.unknown), (1, 40 * GB, 0));
        assert_eq!((e.games, e.games_bytes, e.unknown), (4, 35 * GB, 1));
        assert_eq!(
            e.platforms,
            [
                PlatformShare { platform: Platform::Other, bytes: 20 * GB, games: 1 },
                PlatformShare { platform: Platform::Steam, bytes: 15 * GB, games: 2 },
            ]
        );
        assert_eq!((m.name.as_str(), m.games_bytes), ("Monté", GB));
        // F: sans jeu et Z: inconnu : absents.
        assert!(d.iter().all(|d| d.letter != "F:" && d.letter != "Z:"));
    }

    #[test]
    fn picks_idle_games_biggest_first() {
        let recent = NOW - 30 * DAY;
        let old = NOW - 200 * DAY;
        let mut hidden = game(Platform::Steam, "5", r"E:\S\common\Hidden", Some(90 * GB), None);
        hidden.hidden = true;
        let mut tool = game(Platform::Steam, "6", r"E:\S\common\Tool", Some(80 * GB), None);
        tool.is_tool = true;
        let mut games = vec![
            game(Platform::Steam, "1", r"E:\S\common\Recent", Some(50 * GB), Some(recent)),
            game(Platform::Steam, "2", r"E:\S\common\Old", Some(10 * GB), Some(old)),
            game(Platform::Other, "never", r"C:\Jeux\Never", Some(30 * GB), None),
            game(Platform::Epic, "zero", r"C:\Epic\Zero", Some(2 * GB), Some(0)),
            game(Platform::Other, "unknown", r"C:\Jeux\Unknown", None, None),
            hidden,
            tool,
        ];
        games[1].launcher_tracks_playtime = true;
        let list = candidates(&games, &vols(), NOW);
        assert_eq!(list.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["other:never", "steam:2", "epic:zero"]);
        assert_eq!((list[0].last_played, list[0].drive.as_deref(), list[0].uninstall), (None, Some("C:"), false));
        assert_eq!((list[1].last_played, list[1].bytes, list[1].uninstall), (Some(old), 10 * GB, true));
        assert!(list[1].launcher_dates && !list[0].launcher_dates);
        assert_eq!(list[2].last_played, None, "0 = jamais lancé");
    }

    #[test]
    fn rebuilds_the_uninstall_link_from_the_game() {
        let steam = game(Platform::Steam, "570", r"E:\S\common\dota", None, None);
        assert_eq!(uninstall_uri(&steam).as_deref(), Some("steam://uninstall/570"));
        let odd = game(Platform::Steam, "570/../x", r"E:\S", None, None);
        assert_eq!(uninstall_uri(&odd), None);
        assert_eq!(uninstall_uri(&game(Platform::Epic, "Fortnite", r"E:\Epic\Fortnite", None, None)), None);
    }

    #[test]
    fn refuses_wide_folders_and_parents_of_other_games() {
        let games = vec![
            game(Platform::Other, "a", r"E:\Jeux", None, None),
            game(Platform::Other, "b", r"E:\Jeux\Dofus", None, None),
            game(Platform::Other, "c", r"E:\Jeux\Dofus Retro", None, None),
        ];
        assert!(!measurable(r"E:\", &games));
        assert!(!measurable(r"C:\Program Files", &games));
        assert!(!measurable(r"E:\Jeux", &games), "contient Dofus");
        assert!(measurable(r"E:\Jeux\Dofus", &games), "« Dofus Retro » n'est pas dessous");
        assert!(measurable(r"E:\Jeux\Dofus Retro\", &games));
    }

    #[test]
    fn measures_only_what_changed_or_aged() {
        let dir = std::env::temp_dir().join(format!("ludex-sizes-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut cache = SizeCache::load(&dir);
        let mut steam = game(Platform::Steam, "1", r"E:\S\common\A", Some(GB), None);
        let mut other = game(Platform::Other, "o", r"E:\Jeux\Dofus", None, None);
        let wide = game(Platform::Other, "w", r"E:\", None, None);
        let games = vec![steam.clone(), other.clone(), wide];
        let mtime = |m: u64| move |_: &Path| Some(m);

        // Steam a sa taille, la racine est refusée : seul Dofus est à mesurer.
        let jobs = cache.to_measure(&games, NOW, mtime(100));
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].dir, PathBuf::from(r"E:\Jeux\Dofus"));
        cache.record(&jobs[0], 3 * GB, NOW);

        // Mesure posée sur le jeu, jamais sur un jeu dont le launcher donne la taille.
        assert!(cache.apply(&mut other));
        assert_eq!((other.size_on_disk, other.size_measured), (Some(3 * GB), true));
        assert!(!cache.apply(&mut other), "rien n'a changé");
        steam.install_dir = other.install_dir.clone();
        assert!(!cache.apply(&mut steam));
        assert_eq!(steam.size_on_disk, Some(GB));

        // À jour : rien à faire, même pour le jeu qui porte déjà sa mesure.
        let games = vec![other.clone()];
        assert!(cache.to_measure(&games, NOW + DAY, mtime(100)).is_empty());
        // Dossier modifié, ou mesure de plus de 7 jours : à refaire.
        assert_eq!(cache.to_measure(&games, NOW + DAY, mtime(101)).len(), 1);
        assert_eq!(cache.to_measure(&games, NOW + 8 * DAY, mtime(100)).len(), 1);
        // Dossier absent (disque débranché) : rien.
        assert!(cache.to_measure(&games, NOW + 8 * DAY, |_| None).is_empty());

        // Enregistré, relu, puis oublié après 60 jours d'absence seulement.
        cache.save().unwrap();
        let mut reloaded = SizeCache::load(&dir);
        assert_eq!(reloaded.dirs, cache.dirs);
        assert!(!dir.join("sizes-cache.json.tmp").exists());
        reloaded.forget_old(&[], NOW + 30 * DAY);
        assert_eq!(reloaded.dirs.len(), 1);
        reloaded.forget_old(&games, NOW + 90 * DAY);
        assert_eq!(reloaded.dirs.len(), 1, "toujours dans la bibliothèque");
        reloaded.forget_old(&[], NOW + 90 * DAY);
        assert!(reloaded.dirs.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn publishes_a_new_measure_on_every_game_of_the_folder() {
        let cache = SizeCache::default();
        let mut games = vec![
            game(Platform::Other, "a", r"E:\Jeux\Dofus", None, None),
            game(Platform::Other, "b", r"e:\jeux\dofus\", Some(GB), None),
            game(Platform::Other, "c", r"E:\Jeux\Autre", None, None),
        ];
        games[1].size_measured = true;
        let jobs = cache.to_measure(&games, NOW, |_| Some(1));
        assert_eq!(jobs.len(), 2, "même dossier mesuré une fois");
        let changed = publish(&jobs[0], 2 * GB, &mut games);
        assert_eq!(changed, [("other:a".to_string(), 2 * GB), ("other:b".to_string(), 2 * GB)]);
        assert_eq!(games[2].size_on_disk, None);
    }

    #[test]
    fn sums_a_folder_without_following_links() {
        let dir = std::env::temp_dir().join(format!("ludex-dirsize-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub").join("deeper")).unwrap();
        fs::write(dir.join("a.bin"), vec![0u8; 1000]).unwrap();
        fs::write(dir.join("sub").join("b.bin"), vec![0u8; 200]).unwrap();
        fs::write(dir.join("sub").join("deeper").join("c.bin"), vec![0u8; 30]).unwrap();
        assert_eq!(dir_size(&dir).unwrap(), 1230);
        assert!(modified(&dir).is_some());
        assert!(dir_size(&dir.join("absent")).is_err());
        assert_eq!(modified(&dir.join("a.bin")), None, "un fichier n'est pas un dossier");
        let _ = fs::remove_dir_all(&dir);
    }

    /// Relevé réel : `cargo test storage_this_pc -- --ignored --nocapture`. Mesure les dossiers dans
    /// un cache temporaire : rien n'est écrit dans le dossier de l'app (préférences lues seulement).
    #[test]
    #[ignore]
    fn storage_this_pc() {
        let started = std::time::Instant::now();
        let mut games = crate::library::scan().games;
        let scanned = started.elapsed();
        if let Some(dir) = std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("com.ludex.app")) {
            let data = crate::store::UserData::load(&crate::store::file_in(dir));
            data.customize(&mut games);
            games.iter_mut().for_each(|g| data.apply(g));
        }
        let tmp = std::env::temp_dir().join(format!("ludex-storage-pc-{}", std::process::id()));
        let mut cache = SizeCache::load(&tmp);
        let jobs = cache.to_measure(&games, now(), modified);
        let refused: Vec<&str> = games
            .iter()
            .filter(|g| g.size_on_disk.is_none())
            .filter_map(|g| g.install_dir.as_deref())
            .filter(|d| !measurable(d, &games))
            .collect();
        println!("{} jeux (scan {:.1} s), {} dossiers à mesurer, {} refusés {refused:?}", games.len(), scanned.as_secs_f32(), jobs.len(), refused.len());
        let measuring = std::time::Instant::now();
        for job in &jobs {
            let t = std::time::Instant::now();
            match dir_size(&job.dir) {
                Ok(bytes) => {
                    cache.record(job, bytes, now());
                    println!("  {:>8.2} Go  {:>6} ms  {}", bytes as f64 / 1e9, t.elapsed().as_millis(), job.dir.display());
                }
                Err(e) => println!("  illisible : {} ({e})", job.dir.display()),
            }
        }
        println!("Mesures : {:.1} s", measuring.elapsed().as_secs_f32());
        games.iter_mut().for_each(|g| {
            cache.apply(g);
        });
        let r = report(&games, &volumes(), now(), false);
        for d in &r.disks {
            println!(
                "{} « {} » : {:.0} Go libres sur {:.0} Go{}, jeux {:.1} Go ({} jeux, {} sans taille)",
                d.letter,
                d.name,
                d.free as f64 / 1e9,
                d.total as f64 / 1e9,
                if d.low { " (presque plein)" } else { "" },
                d.games_bytes as f64 / 1e9,
                d.games,
                d.unknown
            );
            for p in &d.platforms {
                println!("    {:?} : {:.1} Go ({} jeux)", p.platform, p.bytes as f64 / 1e9, p.games);
            }
        }
        let total: u64 = r.candidates.iter().map(|c| c.bytes).sum();
        println!("{} jeux à désinstaller ({:.1} Go) :", r.candidates.len(), total as f64 / 1e9);
        for c in r.candidates.iter().take(15) {
            let last = c.last_played.map_or("jamais lancé".to_string(), |t| format!("il y a {} jours", (now() - t) / DAY));
            println!("  {:>7.1} Go  {:?} {}  {}  {last}", c.bytes as f64 / 1e9, c.platform, c.drive.as_deref().unwrap_or("?"), c.title);
        }
        let without: Vec<&str> = games.iter().filter(|g| g.size_on_disk.is_none()).map(|g| g.title.as_str()).collect();
        println!("Sans taille : {without:?}");
        let _ = fs::remove_dir_all(&tmp);
    }
}
