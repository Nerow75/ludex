//! Performances pendant les parties lancées depuis Ludex.
//!
//! Le fil des sessions (`session.rs`) relève déjà les processus toutes les 5 secondes. Pendant
//! qu'un jeu tourne, le même relevé lit aussi le processeur et la mémoire de ses processus et de
//! tout le PC (crate `sysinfo`), et la charge et la mémoire dédiée de la carte graphique par les
//! compteurs de performances de Windows (PDH, objets « GPU Engine » et « GPU Process Memory »,
//! lisibles sans droits d'administrateur ni outil tiers). Rien n'est lu entre deux parties.
//!
//! Chaque session garde une moyenne et un pic par mesure (sur les relevés de 5 s) et une courbe
//! d'une valeur par minute au plus (`MAX_POINTS` points : au-delà, les minutes sont regroupées).
//! À la fin d'une session gardée dans l'historique (une minute au moins), le résultat va dans
//! `performances.json` du dossier de données (écriture tmp + renommage), `KEEP` sessions par jeu,
//! sans toucher à `sessions.json`.
//!
//! Pas de FPS ni de températures : PresentMon (ETW, droits en plus) et LibreHardwareMonitor
//! (pilote) demandent des outils ou des droits que Ludex n'a pas.

use crate::session::Proc;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use sysinfo::System;

/// Sessions gardées par jeu, la plus récente d'abord.
pub const KEEP: usize = 6;
/// Points d'une courbe au plus (4 h à une valeur par minute) ; au-delà, les minutes sont regroupées.
pub const MAX_POINTS: usize = 240;
/// Version du fichier : une autre version est ignorée (et remplacée à la session suivante).
const VERSION: u32 = 1;
/// Au-delà de cette durée après le début, plus aucun relevé (plafond d'une session de `session.rs`).
const MAX_SPAN: u64 = 24 * 3600 + 3600;
const MIB: f64 = 1024.0 * 1024.0;

/// Mesure suivie.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Metric {
    /// Processeur, en % de tous les cœurs.
    Cpu,
    /// Mémoire vive, en Mo (ensemble de travail des processus, mémoire utilisée du PC).
    Memory,
    /// Carte graphique, en % du moteur le plus chargé.
    Gpu,
    /// Mémoire dédiée de la carte graphique, en Mo.
    Vram,
}

pub const METRICS: [Metric; 4] = [Metric::Cpu, Metric::Memory, Metric::Gpu, Metric::Vram];

impl Metric {
    fn index(self) -> usize {
        METRICS.iter().position(|m| *m == self).unwrap_or(0)
    }

    /// Arrondi gardé dans le fichier : un dixième de % ou un Mo.
    fn round(self, value: f64) -> f32 {
        match self {
            Metric::Cpu | Metric::Gpu => ((value * 10.0).round() / 10.0) as f32,
            Metric::Memory | Metric::Vram => value.round() as f32,
        }
    }
}

/// Un relevé : pour chaque mesure de `METRICS`, valeur du jeu et de tout le PC (`None` : illisible).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sample {
    /// Heure du relevé (secondes Unix).
    pub at: u64,
    pub values: [Option<(f32, f32)>; 4],
}

/// État du PC à un relevé.
#[derive(Clone, Debug, Default)]
pub struct PcLoad {
    /// Processeur de tout le PC, en %.
    pub cpu: f32,
    /// Mémoire utilisée et installée, en octets.
    pub used_memory: u64,
    pub total_memory: u64,
    /// Processeurs logiques : l'usage d'un processus est donné en % d'un cœur.
    pub cores: usize,
    /// Compteurs « GPU Engine » : nom d'instance et utilisation en %. `None` : illisibles.
    pub engines: Option<Vec<(String, f64)>>,
    /// Compteurs « GPU Process Memory\Dedicated Usage » : nom d'instance et octets.
    pub vram: Option<Vec<(String, f64)>>,
}

/// Processus d'un nom d'instance PDH (`pid_1234_luid_0x…_phys_0_eng_3_engtype_3D`) et reste du nom
/// (moteur ou carte).
fn split_pid(name: &str) -> Option<(u32, &str)> {
    let (pid, rest) = name.strip_prefix("pid_")?.split_once('_')?;
    Some((pid.parse().ok()?, rest))
}

/// Charge de la carte graphique, comme le Gestionnaire des tâches : utilisation de chaque moteur
/// (3D, copie, décodage vidéo… de chaque carte) additionnée sur les processus, puis le moteur le
/// plus chargé. Renvoie (jeu, PC), entre 0 et 100.
fn engine_load(items: &[(String, f64)], game: &HashSet<u32>) -> (f32, f32) {
    let mut all: HashMap<&str, f64> = HashMap::new();
    let mut mine: HashMap<&str, f64> = HashMap::new();
    for (name, value) in items {
        let Some((pid, engine)) = split_pid(name) else { continue };
        if !value.is_finite() || *value < 0.0 {
            continue;
        }
        *all.entry(engine).or_default() += value;
        if game.contains(&pid) {
            *mine.entry(engine).or_default() += value;
        }
    }
    let top = |map: &HashMap<&str, f64>| map.values().copied().fold(0.0, f64::max).clamp(0.0, 100.0) as f32;
    (top(&mine), top(&all))
}

/// Mémoire dédiée de la carte graphique en Mo : (jeu, PC), somme des processus.
fn vram_use(items: &[(String, f64)], game: &HashSet<u32>) -> (f32, f32) {
    let (mut mine, mut all) = (0.0, 0.0);
    for (name, value) in items {
        let Some((pid, _)) = split_pid(name) else { continue };
        if !value.is_finite() || *value < 0.0 {
            continue;
        }
        all += value;
        if game.contains(&pid) {
            mine += value;
        }
    }
    ((mine / MIB) as f32, (all / MIB) as f32)
}

/// Relevé d'un jeu : ses processus (`game`) comparés à tout le PC.
pub fn sample(at: u64, game: &[&Proc], pc: &PcLoad) -> Sample {
    let cores = pc.cores.max(1) as f32;
    let cpu = (game.iter().map(|p| p.cpu.max(0.0)).sum::<f32>() / cores).clamp(0.0, 100.0);
    let memory = (game.iter().map(|p| p.memory).sum::<u64>() as f64 / MIB) as f32;
    let pids: HashSet<u32> = game.iter().map(|p| p.pid).collect();
    let mut values = [None; 4];
    values[Metric::Cpu.index()] = Some((cpu, pc.cpu.clamp(0.0, 100.0)));
    values[Metric::Memory.index()] = Some((memory, (pc.used_memory as f64 / MIB) as f32));
    values[Metric::Gpu.index()] = pc.engines.as_deref().map(|items| engine_load(items, &pids));
    values[Metric::Vram.index()] = pc.vram.as_deref().map(|items| vram_use(items, &pids));
    Sample { at, values }
}

/// Somme et nombre de valeurs.
#[derive(Clone, Copy, Debug, Default)]
struct Acc {
    n: u32,
    sum: f64,
}

impl Acc {
    fn add(&mut self, v: f32) {
        self.n += 1;
        self.sum += v as f64;
    }
    fn merge(&mut self, other: Acc) {
        self.n += other.n;
        self.sum += other.sum;
    }
    fn mean(self) -> Option<f64> {
        (self.n > 0).then(|| self.sum / self.n as f64)
    }
}

/// Moyenne et pic sur les relevés.
#[derive(Clone, Copy, Debug, Default)]
struct Total {
    acc: Acc,
    peak: f32,
}

/// Mesures d'une session en cours.
#[derive(Debug)]
pub struct Recorder {
    /// Début de la session (secondes Unix) : origine des courbes.
    start: u64,
    /// Minute depuis le début → somme par mesure, (jeu, PC).
    minutes: BTreeMap<u64, [[Acc; 2]; 4]>,
    totals: [[Total; 2]; 4],
    samples: u32,
    last: u64,
    /// Mémoire installée, en Mo.
    total_memory: u32,
}

/// Moyenne et pic d'une mesure.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stat {
    pub avg: f32,
    pub peak: f32,
}

/// Courbes et chiffres d'une mesure.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Series {
    pub metric: Metric,
    pub game: Stat,
    pub pc: Stat,
    /// Une valeur par point de `SessionPerf.times` (`null` : rien lu pendant ce point).
    pub game_points: Vec<Option<f32>>,
    pub pc_points: Vec<Option<f32>>,
}

/// Performances d'une session terminée.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPerf {
    /// Début de la session (secondes Unix), le même que dans l'historique.
    pub start: u64,
    /// Dernier relevé (secondes Unix).
    pub end: u64,
    /// Secondes couvertes par un point (60, ou plus pour une longue session).
    pub step: u32,
    /// Mémoire installée, en Mo.
    pub total_memory: u32,
    /// Début de chaque point, en secondes depuis `start`.
    pub times: Vec<u32>,
    /// Mesures lues, dans l'ordre de `METRICS` (carte graphique absente si illisible).
    pub series: Vec<Series>,
}

impl Recorder {
    pub fn new(start: u64) -> Self {
        Recorder { start, minutes: BTreeMap::new(), totals: Default::default(), samples: 0, last: start, total_memory: 0 }
    }

    /// Ajoute un relevé. Ignoré s'il précède le début ou dépasse le plafond d'une session.
    pub fn add(&mut self, sample: &Sample, total_memory: u64) {
        let Some(offset) = sample.at.checked_sub(self.start) else { return };
        if offset > MAX_SPAN {
            return;
        }
        let minute = self.minutes.entry(offset / 60).or_default();
        for (i, value) in sample.values.iter().enumerate() {
            let Some((game, pc)) = value else { continue };
            for (side, v) in [*game, *pc].into_iter().enumerate() {
                if !v.is_finite() {
                    continue;
                }
                minute[i][side].add(v);
                let total = &mut self.totals[i][side];
                total.acc.add(v);
                total.peak = total.peak.max(v);
            }
        }
        self.samples += 1;
        self.last = self.last.max(sample.at);
        self.total_memory = (total_memory as f64 / MIB).round() as u32;
    }

    /// Résultat de la session. `None` avec moins de deux relevés.
    pub fn finish(self) -> Option<SessionPerf> {
        if self.samples < 2 {
            return None;
        }
        let first = *self.minutes.keys().next()?;
        let last = *self.minutes.keys().next_back()?;
        let span = (last - first + 1) as usize;
        let factor = span.div_ceil(MAX_POINTS).max(1) as u64;
        // Minutes regroupées par `factor` : une valeur par point, moyenne des relevés du point.
        let mut groups: BTreeMap<u64, [[Acc; 2]; 4]> = BTreeMap::new();
        for (minute, accs) in &self.minutes {
            let group = groups.entry((minute - first) / factor).or_default();
            for (i, pair) in accs.iter().enumerate() {
                group[i][0].merge(pair[0]);
                group[i][1].merge(pair[1]);
            }
        }
        let times = groups.keys().map(|g| ((first + g * factor) * 60) as u32).collect();
        let series = METRICS
            .iter()
            .enumerate()
            .filter(|(i, _)| self.totals[*i][0].acc.n > 0 && self.totals[*i][1].acc.n > 0)
            .map(|(i, &metric)| {
                let stat = |t: Total| Stat { avg: metric.round(t.acc.mean().unwrap_or(0.0)), peak: metric.round(t.peak as f64) };
                let points = |side: usize| groups.values().map(|g| g[i][side].mean().map(|v| metric.round(v))).collect();
                Series { metric, game: stat(self.totals[i][0]), pc: stat(self.totals[i][1]), game_points: points(0), pc_points: points(1) }
            })
            .collect();
        Some(SessionPerf {
            start: self.start,
            end: self.last,
            step: (factor * 60) as u32,
            total_memory: self.total_memory,
            times,
            series,
        })
    }
}

/// Contenu de `performances.json`.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct PerfFile {
    version: u32,
    /// Id du jeu → sessions, la plus récente d'abord.
    games: BTreeMap<String, Vec<SessionPerf>>,
}

/// Performances enregistrées. Seul le fil des sessions écrit ; les commandes lisent.
pub struct Store {
    path: PathBuf,
    file: Mutex<PerfFile>,
}

impl Store {
    /// Lit `performances.json` du dossier de données. Absent, illisible ou d'une autre version : vide.
    pub fn open(dir: &Path) -> Self {
        let path = dir.join("performances.json");
        let file = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<PerfFile>(&text).ok())
            .filter(|f| f.version == VERSION)
            .unwrap_or_default();
        Store { path, file: Mutex::new(file) }
    }

    /// Sessions mesurées d'un jeu, la plus récente d'abord.
    pub fn sessions(&self, game_id: &str) -> Vec<SessionPerf> {
        self.file.lock().map(|f| f.games.get(game_id).cloned().unwrap_or_default()).unwrap_or_default()
    }

    /// Ajoute une session (une session du même début est remplacée), garde les `KEEP` plus
    /// récentes du jeu, puis enregistre. Écriture refusée : la mémoire garde la session.
    pub fn record(&self, game_id: &str, perf: SessionPerf) -> Result<(), String> {
        let mut file = self.file.lock().map_err(|_| tr!("Performances indisponibles.", "Performance data unavailable."))?;
        file.version = VERSION;
        let list = file.games.entry(game_id.to_string()).or_default();
        list.retain(|s| s.start != perf.start);
        list.push(perf);
        list.sort_by_key(|s| std::cmp::Reverse(s.start));
        list.truncate(KEEP);
        let text = serde_json::to_string(&*file).map_err(|e| tr!("Performances illisibles : {e}", "Unreadable performance data: {e}"))?;
        write_atomic(&self.path, &text)
    }
}

/// Écrit dans un fichier temporaire puis le renomme : jamais de fichier à moitié écrit.
fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| tr!("Dossier de données inaccessible : {e}", "Data folder not accessible: {e}"))?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text).map_err(|e| tr!("Enregistrement impossible : {e}", "Couldn't save: {e}"))?;
    fs::rename(&tmp, path).map_err(|e| tr!("Enregistrement impossible : {e}", "Couldn't save: {e}"))
}

/// Relevé des processus et du PC, gardé d'un relevé à l'autre tant qu'une session est suivie.
pub struct Probe {
    system: System,
    #[cfg(windows)]
    gpu: Option<pdh::Counters>,
    /// Le processeur se mesure entre deux relevés : rien de juste avant le deuxième.
    warm: bool,
}

impl Probe {
    pub fn new() -> Option<Self> {
        Some(Probe {
            system: System::new().ok()?,
            #[cfg(windows)]
            gpu: None,
            warm: false,
        })
    }

    /// Processus en cours, avec leur processeur et leur mémoire.
    pub fn processes(&mut self) -> Vec<Proc> {
        crate::session::list_processes(&mut self.system, true)
    }

    /// État du PC, à appeler après `processes` quand un jeu tourne. `None` au premier appel
    /// (processeur pas encore mesurable). Compteurs de la carte graphique ouverts au premier appel.
    pub fn pc(&mut self) -> Option<PcLoad> {
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        #[cfg(windows)]
        let (engines, vram) = {
            if self.gpu.is_none() {
                self.gpu = pdh::Counters::open();
            }
            match self.gpu.as_mut().and_then(pdh::Counters::collect) {
                Some((engines, vram)) => (Some(engines), vram),
                None => (None, None),
            }
        };
        #[cfg(not(windows))]
        let (engines, vram) = (None, None);
        if !self.warm {
            self.warm = true;
            return None;
        }
        Some(PcLoad {
            cpu: self.system.global_cpu_usage(),
            used_memory: self.system.used_memory(),
            total_memory: self.system.total_memory(),
            cores: self.system.cpus().len(),
            engines,
            vram,
        })
    }
}

/// Compteurs de performances de Windows (PDH) pour la carte graphique.
#[cfg(windows)]
mod pdh {
    use windows::Win32::System::Performance::{
        PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA, PDH_FMT, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER,
        PDH_HQUERY, PDH_MORE_DATA, PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
        PdhOpenQueryW,
    };
    use windows::core::{HSTRING, PCWSTR};

    /// Lecture : moteurs (%) et, si le compteur existe, mémoire dédiée (octets), par instance.
    pub type Readings = (Vec<(String, f64)>, Option<Vec<(String, f64)>>);

    /// Valeurs au-delà de 100 % gardées telles quelles (somme de plusieurs processus).
    const PDH_FMT_NOCAP100: u32 = 0x8000;

    pub struct Counters {
        query: PDH_HQUERY,
        engines: PDH_HCOUNTER,
        vram: Option<PDH_HCOUNTER>,
    }

    fn add(query: PDH_HQUERY, path: &str) -> Option<PDH_HCOUNTER> {
        let mut counter = PDH_HCOUNTER::default();
        // SAFETY : requête ouverte, chemin terminé par un zéro (HSTRING), compteur écrit par PDH.
        let status = unsafe { PdhAddEnglishCounterW(query, &HSTRING::from(path), 0, &mut counter) };
        (status == 0).then_some(counter)
    }

    impl Counters {
        /// Ouvre la requête : `None` si les compteurs « GPU Engine » n'existent pas (ancien Windows,
        /// pilote sans WDDM 2) ou sont refusés.
        pub fn open() -> Option<Self> {
            let mut query = PDH_HQUERY::default();
            // SAFETY : sortie écrite par PDH ; requête fermée par `Drop` (ou ici en cas d'échec).
            if unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut query) } != 0 {
                return None;
            }
            let Some(engines) = add(query, r"\GPU Engine(*)\Utilization Percentage") else {
                // SAFETY : requête ouverte juste au-dessus, plus utilisée ensuite.
                unsafe { PdhCloseQuery(query) };
                return None;
            };
            let vram = add(query, r"\GPU Process Memory(*)\Dedicated Usage");
            // Première lecture : base des taux, valeurs lisibles à la lecture suivante.
            // SAFETY : requête ouverte.
            unsafe { PdhCollectQueryData(query) };
            Some(Counters { query, engines, vram })
        }

        /// Nouvelle lecture : moteurs (%) et mémoire dédiée (octets) par instance.
        pub fn collect(&mut self) -> Option<Readings> {
            // SAFETY : requête ouverte tant que `self` vit.
            if unsafe { PdhCollectQueryData(self.query) } != 0 {
                return None;
            }
            let engines = read(self.engines)?;
            Some((engines, self.vram.and_then(read)))
        }
    }

    impl Drop for Counters {
        fn drop(&mut self) {
            // SAFETY : requête ouverte par `open`, fermée une seule fois (compteurs compris).
            unsafe { PdhCloseQuery(self.query) };
        }
    }

    /// Valeurs de toutes les instances d'un compteur à joker. Le nombre d'instances peut changer
    /// entre la demande de taille et la lecture (processus apparu) : trois essais.
    fn read(counter: PDH_HCOUNTER) -> Option<Vec<(String, f64)>> {
        let format = PDH_FMT(PDH_FMT_DOUBLE.0 | PDH_FMT_NOCAP100);
        for _ in 0..3 {
            let (mut size, mut count) = (0u32, 0u32);
            // SAFETY : demande de taille seulement (aucun tampon).
            let status = unsafe { PdhGetFormattedCounterArrayW(counter, format, &mut size, &mut count, None) };
            if status == 0 {
                return Some(Vec::new());
            }
            if status != PDH_MORE_DATA {
                return None;
            }
            // Tampon aligné sur 8 octets : tableau d'éléments puis leurs noms.
            let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
            let items = buffer.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
            // SAFETY : tampon de `size` octets au moins, aligné ; PDH y écrit `count` éléments.
            let status = unsafe { PdhGetFormattedCounterArrayW(counter, format, &mut size, &mut count, Some(items)) };
            if status == PDH_MORE_DATA {
                continue;
            }
            if status != 0 {
                return None;
            }
            // SAFETY : `count` éléments valides au début du tampon, noms dans le même tampon.
            let slice = unsafe { std::slice::from_raw_parts(items, count as usize) };
            let values = slice
                .iter()
                .filter(|item| matches!(item.FmtValue.CStatus, PDH_CSTATUS_VALID_DATA | PDH_CSTATUS_NEW_DATA))
                .filter_map(|item| {
                    // SAFETY : nom terminé par un zéro dans le tampon ; valeur demandée en double.
                    let name = unsafe { item.szName.to_string() }.ok()?;
                    let value = unsafe { item.FmtValue.Anonymous.doubleValue };
                    Some((name, value))
                })
                .collect();
            return Some(values);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Heure actuelle en secondes Unix (relevé réel).
    fn now() -> u64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
    }

    fn proc(pid: u32, cpu: f32, memory: u64) -> Proc {
        Proc { exe: format!(r"d:\jeux\hades\hades{pid}.exe"), start: 1, pid, cpu, memory }
    }

    fn pc(cpu: f32) -> PcLoad {
        PcLoad {
            cpu,
            used_memory: 16 * 1024 * 1024 * 1024,
            total_memory: 64 * 1024 * 1024 * 1024,
            cores: 16,
            engines: None,
            vram: None,
        }
    }

    /// Relevé simple : processeur et mémoire du jeu, rien sur la carte graphique.
    fn at(t: u64, cpu: f32, pc_cpu: f32) -> Sample {
        let mut values = [None; 4];
        values[0] = Some((cpu, pc_cpu));
        values[1] = Some((2048.0, 16384.0));
        Sample { at: t, values }
    }

    #[test]
    fn reads_pdh_instance_names() {
        assert_eq!(split_pid("pid_1234_luid_0x00000000_0x0000D1E4_phys_0_eng_3_engtype_3D"), Some((1234, "luid_0x00000000_0x0000D1E4_phys_0_eng_3_engtype_3D")));
        assert_eq!(split_pid("pid_x_luid"), None);
        assert_eq!(split_pid("_Total"), None);
    }

    #[test]
    fn gpu_load_is_the_busiest_engine_like_task_manager() {
        let e = |pid: u32, eng: &str, v: f64| (format!("pid_{pid}_luid_0x0_0x1_phys_0_eng_{eng}"), v);
        let items = vec![
            e(10, "0_engtype_3D", 40.0),
            e(11, "0_engtype_3D", 25.0), // second processus du jeu
            e(10, "4_engtype_VideoDecode", 5.0),
            e(99, "0_engtype_3D", 20.0), // navigateur
            e(99, "2_engtype_Copy", 70.0),
            ("pid_7_luid_x".into(), f64::NAN),
            ("_Total".into(), 300.0),
        ];
        let game: HashSet<u32> = [10, 11].into();
        // Jeu : 3D à 65 %. PC : 3D à 85 %, copie à 70 % : 85.
        assert_eq!(engine_load(&items, &game), (65.0, 85.0));
        // Somme au-delà de 100 (instances arrondies) : plafonnée.
        let over = vec![e(10, "0_engtype_3D", 70.0), e(11, "0_engtype_3D", 45.0)];
        assert_eq!(engine_load(&over, &game), (100.0, 100.0));
        assert_eq!(engine_load(&[], &game), (0.0, 0.0));
    }

    #[test]
    fn vram_sums_processes_in_megabytes() {
        let items = vec![
            ("pid_10_luid_0x0_0x1_phys_0".to_string(), 3.0 * MIB * 1024.0),
            ("pid_10_luid_0x0_0x2_phys_0".to_string(), 512.0 * MIB),
            ("pid_99_luid_0x0_0x1_phys_0".to_string(), 1024.0 * MIB),
        ];
        assert_eq!(vram_use(&items, &[10].into()), (3584.0, 4608.0));
    }

    #[test]
    fn samples_a_game_against_the_pc() {
        let (a, b) = (proc(10, 160.0, 3 * 1024 * 1024 * 1024), proc(11, 32.0, 512 * 1024 * 1024));
        let mut load = pc(40.0);
        let s = sample(100, &[&a, &b], &load);
        // 192 % d'un cœur sur 16 cœurs = 12 % du processeur.
        assert_eq!(s.values[0], Some((12.0, 40.0)));
        assert_eq!(s.values[1], Some((3584.0, 16384.0)));
        assert_eq!((s.values[2], s.values[3]), (None, None));

        load.engines = Some(vec![("pid_11_luid_0x0_0x1_phys_0_eng_0_engtype_3D".into(), 55.0)]);
        load.vram = Some(vec![]);
        let s = sample(100, &[&a, &b], &load);
        assert_eq!(s.values[2], Some((55.0, 55.0)));
        assert_eq!(s.values[3], Some((0.0, 0.0)));
        // Processus qui dépasse tous les cœurs (mesure aberrante) : plafonné.
        let wild = proc(12, 5000.0, 0);
        assert_eq!(sample(100, &[&wild], &pc(150.0)).values[0], Some((100.0, 100.0)));
    }

    #[test]
    fn aggregates_average_peak_and_one_point_per_minute() {
        let mut r = Recorder::new(1000);
        // 3 minutes de relevés toutes les 5 s : le jeu à 10 %, puis 30 %, puis 20 % avec un pic à 90.
        for i in 0..36u64 {
            let minute = i / 12;
            let cpu = match minute {
                0 => 10.0,
                1 => 30.0,
                _ if i == 30 => 90.0,
                _ => 20.0,
            };
            r.add(&at(1000 + i * 5, cpu, 50.0), 64 * 1024 * 1024 * 1024);
        }
        let p = r.finish().unwrap();
        assert_eq!((p.start, p.end, p.step, p.total_memory), (1000, 1175, 60, 65536));
        assert_eq!(p.times, vec![0, 60, 120]);
        assert_eq!(p.series.len(), 2); // carte graphique illisible : absente
        let cpu = &p.series[0];
        assert_eq!(cpu.metric, Metric::Cpu);
        assert_eq!(cpu.game_points, vec![Some(10.0), Some(30.0), Some(25.8)]);
        assert_eq!(cpu.game, Stat { avg: 21.9, peak: 90.0 });
        assert_eq!(cpu.pc, Stat { avg: 50.0, peak: 50.0 });
        assert_eq!(p.series[1].metric, Metric::Memory);
        assert_eq!(p.series[1].game, Stat { avg: 2048.0, peak: 2048.0 });
    }

    #[test]
    fn long_sessions_are_grouped_and_gaps_stay_gaps() {
        let mut r = Recorder::new(0);
        // 10 h de jeu, un relevé par minute, sauf une pause d'une heure (veille) au milieu.
        for minute in (0..600u64).filter(|m| !(300..360).contains(m)) {
            r.add(&at(minute * 60, minute as f32 % 10.0, 50.0), 0);
        }
        let p = r.finish().unwrap();
        assert!(p.times.len() <= MAX_POINTS);
        assert_eq!(p.step, 180); // 600 minutes / 240 points : 3 minutes par point
        assert_eq!(p.times.len(), 200 - 20);
        assert_eq!(p.series[0].game_points.len(), p.times.len());
        // Le premier point d'après la pause commence bien après.
        let after = p.times.iter().position(|&t| t >= 300 * 60).unwrap();
        assert_eq!(p.times[after], 360 * 60);
    }

    #[test]
    fn ignores_too_few_samples_and_out_of_range_ones() {
        let mut r = Recorder::new(1000);
        r.add(&at(900, 10.0, 10.0), 0); // avant le début
        r.add(&at(1000, 10.0, 10.0), 0);
        assert!(r.finish().is_none());

        let mut r = Recorder::new(1000);
        r.add(&at(1000, 10.0, 10.0), 0);
        r.add(&at(1000 + MAX_SPAN + 60, 99.0, 99.0), 0); // après le plafond
        r.add(&at(1005, f32::NAN, 20.0), 0);
        let p = r.finish().unwrap();
        assert_eq!(p.end, 1005);
        assert_eq!(p.series[0].game, Stat { avg: 10.0, peak: 10.0 });
        assert_eq!(p.series[0].pc, Stat { avg: 15.0, peak: 20.0 });
    }

    #[test]
    fn gpu_series_appear_only_when_read() {
        let mut r = Recorder::new(0);
        let mut s = at(0, 10.0, 10.0);
        r.add(&s, 0);
        s.at = 5;
        s.values[2] = Some((80.0, 95.0));
        r.add(&s, 0);
        let p = r.finish().unwrap();
        let gpu = p.series.iter().find(|x| x.metric == Metric::Gpu).unwrap();
        assert_eq!(gpu.game, Stat { avg: 80.0, peak: 80.0 });
        assert!(p.series.iter().all(|x| x.metric != Metric::Vram));
    }

    fn perf(start: u64) -> SessionPerf {
        let mut r = Recorder::new(start);
        r.add(&at(start, 10.0, 20.0), 0);
        r.add(&at(start + 5, 10.0, 20.0), 0);
        r.finish().unwrap()
    }

    #[test]
    fn store_keeps_the_last_sessions_and_survives_a_restart() {
        let dir = std::env::temp_dir().join(format!("ludex-perf-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let store = Store::open(&dir);
        assert!(store.sessions("steam:1").is_empty());
        for start in [100, 700, 300, 900, 200, 500, 800, 600] {
            store.record("steam:1", perf(start)).unwrap();
        }
        store.record("epic:Sugar", perf(50)).unwrap();
        // Même début : remplacée, pas ajoutée.
        store.record("steam:1", perf(900)).unwrap();
        let starts = |s: &Store| s.sessions("steam:1").iter().map(|p| p.start).collect::<Vec<_>>();
        assert_eq!(starts(&store), vec![900, 800, 700, 600, 500, 300]);
        assert!(!dir.join("performances.json.tmp").exists());

        let again = Store::open(&dir);
        assert_eq!(starts(&again), vec![900, 800, 700, 600, 500, 300]);
        assert_eq!(again.sessions("epic:Sugar").len(), 1);
        assert_eq!(again.sessions("steam:1")[0], perf(900));

        // Fichier d'une autre version ou abîmé : ignoré, sans erreur.
        fs::write(dir.join("performances.json"), r#"{"version":99,"games":{"steam:1":[]}}"#).unwrap();
        assert!(Store::open(&dir).sessions("epic:Sugar").is_empty());
        fs::write(dir.join("performances.json"), "{ abîmé").unwrap();
        assert!(Store::open(&dir).sessions("steam:1").is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn serializes_for_the_front() {
        let json = serde_json::to_value(perf(100)).unwrap();
        assert_eq!(json["totalMemory"], 0);
        assert_eq!(json["series"][0]["metric"], "cpu");
        assert_eq!(json["series"][0]["gamePoints"][0], 10.0);
        assert_eq!(json["series"][1]["pc"]["peak"], 16384.0);
    }

    /// Relevé réel : `cargo test performance_this_pc -- --ignored --nocapture`. Lecture seule : suit
    /// pendant ~30 s le processus qui occupe le plus de mémoire (ou celui dont l'exécutable contient
    /// `LUDEX_PERF_EXE`) et ses voisins du même dossier, comme un jeu, sans rien écrire.
    #[test]
    #[ignore]
    fn performance_this_pc() {
        use std::time::{Duration, Instant};
        let mut probe = Probe::new().expect("sysinfo indisponible");
        let started = Instant::now();
        let first = probe.processes();
        println!("{} processus lus en {:?}", first.len(), started.elapsed());
        let started = Instant::now();
        assert!(probe.pc().is_none(), "premier relevé : rien à mesurer");
        println!("Ouverture des compteurs PDH : {:?}", started.elapsed());

        std::thread::sleep(Duration::from_secs(5));
        let procs = probe.processes();
        let wanted = std::env::var("LUDEX_PERF_EXE").ok().map(|s| s.to_lowercase());
        let target = match &wanted {
            Some(name) => procs.iter().filter(|p| p.exe.contains(name.as_str())).max_by_key(|p| p.memory),
            None => procs.iter().filter(|p| !p.exe.contains(r"\windows\")).max_by_key(|p| p.memory),
        }
        .expect("aucun processus suivi")
        .clone();
        let root = format!("{}\\", Path::new(&target.exe).parent().unwrap().to_string_lossy());
        println!("Suivi : {} (dossier {root})", target.exe);

        let mut recorder: Option<Recorder> = None;
        for tick in 0..7 {
            let started = Instant::now();
            let procs = probe.processes();
            let listed = started.elapsed();
            let pc = probe.pc().expect("deuxième relevé : mesurable");
            let cost = started.elapsed();
            let game: Vec<&Proc> = procs.iter().filter(|p| p.exe.starts_with(&root)).collect();
            let s = sample(now(), &game, &pc);
            println!(
                "  relevé {tick} : processus {listed:?}, total {cost:?} ; {} processus du « jeu » ; processeur {:?} ; mémoire {:?} ; carte graphique {:?} ; mémoire vidéo {:?} ({} moteurs lus)",
                game.len(),
                s.values[0],
                s.values[1],
                s.values[2],
                s.values[3],
                pc.engines.as_ref().map_or(0, Vec::len),
            );
            recorder.get_or_insert_with(|| Recorder::new(s.at)).add(&s, pc.total_memory);
            if tick < 6 {
                std::thread::sleep(Duration::from_secs(5));
            }
        }
        let perf = recorder.unwrap().finish().expect("assez de relevés");
        println!("Session : {} s, {} points de {} s, {} Mo installés", perf.end - perf.start, perf.times.len(), perf.step, perf.total_memory);
        for s in &perf.series {
            println!("  {:?} : jeu moyenne {} pic {} ; PC moyenne {} pic {}", s.metric, s.game.avg, s.game.peak, s.pc.avg, s.pc.peak);
        }
        println!("JSON : {} octets", serde_json::to_string(&perf).unwrap().len());
    }
}
