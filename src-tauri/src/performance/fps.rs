//! Images par seconde pendant les parties lancées depuis Ludex.
//!
//! Windows ne donne aucun compteur d'images par seconde. Comme PresentMon d'Intel (et la Game Bar,
//! CapFrameX, FrameView…), Ludex écoute les événements ETW que Windows écrit à chaque image
//! présentée : `Microsoft-Windows-DXGI` (`Present_Start`, id 42 : Direct3D 10, 11, 12, et Vulkan
//! posé sur DXGI), `Microsoft-Windows-D3D9` (`Present_Start`, id 1) et, pour un processus qui n'a
//! aucun de ces deux événements (Vulkan, OpenGL), `Microsoft-Windows-DxgKrnl` (`Present_Info`,
//! id 184, mot-clé `Present`). Lecture passive : rien n'est injecté dans le jeu, aucun fichier,
//! aucun réglage de Windows touché. Temps d'image = écart entre deux présentations du même processus
//! (« msBetweenPresents » de PresentMon), le processus du jeu qui présente le plus à chaque relevé.
//!
//! Droits : ouvrir une session ETW demande l'administrateur ou le groupe « Utilisateurs du journal
//! de performances » (la Game Bar de Windows propose d'y ajouter l'utilisateur pour son compteur
//! d'images). Sans eux, Ludex ne demande rien, ne change rien : la mesure est absente et la fiche
//! l'explique (`Missing::Denied`).
//!
//! Agrégation : moyenne = images / durée qu'elles couvrent ; « 1 % low » = images par seconde au
//! 99ᵉ centile du temps d'image, lu dans un histogramme logarithmique (`FrameStats`, ~1 % de
//! précision) pour ne jamais garder chaque image d'une longue partie en mémoire.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Un écart d'une seconde ou plus n'est pas une image : jeu en pause, réduit, écran de chargement
/// figé. Il fausserait le « 1 % low » d'une partie entière.
pub const MAX_FRAME_MS: f32 = 1000.0;
/// En dessous, pas de « 1 % low » (trop peu d'images pour un centile).
pub const MIN_LOW_FRAMES: u32 = 100;
/// Plus petit temps d'image de l'histogramme (10 000 images par seconde).
const MIN_MS: f64 = 0.1;
/// Largeur d'une case de l'histogramme : 1 % de plus que la précédente.
const RATIO: f64 = 1.01;

/// Pourquoi une partie n'a pas d'images par seconde.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Missing {
    /// Ni administrateur ni groupe « Utilisateurs du journal de performances ».
    Denied,
    /// Session ETW impossible pour une autre raison (ou système autre que Windows).
    Unavailable,
    /// Lecture en marche, mais aucune image vue pour le jeu (méthode d'affichage non lue, jeu
    /// resté en arrière-plan).
    NoFrames,
}

/// Lecture du traceur à un relevé : temps d'image (ms) par processus depuis le relevé précédent.
#[derive(Clone, Debug, PartialEq)]
pub enum Reading {
    Frames(HashMap<u32, Vec<f32>>),
    Missing(Missing),
}

impl Default for Reading {
    fn default() -> Self {
        Reading::Missing(Missing::Unavailable)
    }
}

/// Images d'un jeu à un relevé.
#[derive(Clone, Debug, PartialEq)]
pub enum GameFrames {
    /// Temps d'image du processus du jeu qui a présenté le plus (vide : aucune image).
    Frames(Vec<f32>),
    Missing(Missing),
}

impl Default for GameFrames {
    fn default() -> Self {
        GameFrames::Missing(Missing::Unavailable)
    }
}

/// Images d'un jeu : celles du processus du jeu (`pids`) qui en a présenté le plus.
pub fn game_frames(reading: &Reading, pids: impl IntoIterator<Item = u32>) -> GameFrames {
    match reading {
        Reading::Missing(m) => GameFrames::Missing(*m),
        Reading::Frames(map) => GameFrames::Frames(
            pids.into_iter().filter_map(|pid| map.get(&pid)).max_by_key(|f| f.len()).cloned().unwrap_or_default(),
        ),
    }
}

/// Case de l'histogramme d'un temps d'image (ms).
fn bin(ms: f64) -> u16 {
    ((ms.max(MIN_MS) / MIN_MS).ln() / RATIO.ln()).floor().clamp(0.0, u16::MAX as f64) as u16
}

/// Temps d'image au milieu d'une case.
fn bin_ms(bin: u16) -> f64 {
    MIN_MS * RATIO.powf(bin as f64 + 0.5)
}

/// Images d'une minute, d'un point ou d'une partie.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameStats {
    n: u32,
    sum_ms: f64,
    hist: BTreeMap<u16, u32>,
}

impl FrameStats {
    /// Ajoute un temps d'image. Ignoré s'il est invalide, nul ou d'une seconde ou plus.
    pub fn add(&mut self, ms: f32) {
        if !ms.is_finite() || ms <= 0.0 || ms >= MAX_FRAME_MS {
            return;
        }
        self.n += 1;
        self.sum_ms += ms as f64;
        *self.hist.entry(bin(ms as f64)).or_default() += 1;
    }

    pub fn merge(&mut self, other: &FrameStats) {
        self.n += other.n;
        self.sum_ms += other.sum_ms;
        for (b, c) in &other.hist {
            *self.hist.entry(*b).or_default() += c;
        }
    }

    /// Images par seconde en moyenne : images / temps qu'elles couvrent.
    pub fn average(&self) -> Option<f64> {
        (self.n > 0 && self.sum_ms > 0.0).then(|| 1000.0 * self.n as f64 / self.sum_ms)
    }

    /// « 1 % low » : images par seconde au 99ᵉ centile du temps d'image (le 1 % d'images le plus
    /// lent). Jamais au-dessus de la moyenne (arrondi des cases). `None` sous `MIN_LOW_FRAMES`.
    pub fn low(&self) -> Option<f64> {
        if self.n < MIN_LOW_FRAMES {
            return None;
        }
        let target = (self.n as u64).div_ceil(100).max(1);
        let mut seen = 0u64;
        for (b, c) in self.hist.iter().rev() {
            seen += *c as u64;
            if seen >= target {
                let low = 1000.0 / bin_ms(*b);
                return Some(self.average().map_or(low, |avg| low.min(avg)));
            }
        }
        None
    }
}

/// Événement d'une image présentée : 0 DXGI, 1 D3D9, 2 DxgKrnl (ordre de préférence).
const SOURCES: usize = 3;
/// Processus suivis au plus entre deux relevés.
const MAX_PIDS: usize = 512;
/// Temps d'image gardés au plus par processus et par source entre deux relevés (~16 s à 1 200 i/s).
const MAX_FRAMES: usize = 20_000;

/// Présentations reçues depuis le relevé précédent, par processus.
#[derive(Debug, Default)]
pub struct Log {
    pids: HashMap<u32, PidLog>,
}

#[derive(Debug, Default)]
struct PidLog {
    /// Dernière présentation (unités de 100 ns), par source.
    last: [Option<i64>; SOURCES],
    times: [Vec<f32>; SOURCES],
}

impl Log {
    /// Note une présentation du processus `pid` à l'heure `ticks` (unités de 100 ns).
    pub fn record(&mut self, pid: u32, source: usize, ticks: i64) {
        if source >= SOURCES {
            return;
        }
        if !self.pids.contains_key(&pid) && self.pids.len() >= MAX_PIDS {
            return;
        }
        let log = self.pids.entry(pid).or_default();
        if let Some(last) = log.last[source]
            && ticks > last
            && log.times[source].len() < MAX_FRAMES
        {
            log.times[source].push(((ticks - last) as f64 / 10_000.0) as f32);
        }
        if log.last[source].is_none_or(|last| ticks > last) {
            log.last[source] = Some(ticks);
        }
    }

    /// Temps d'image (ms) par processus depuis le dernier appel, source préférée d'abord (DXGI,
    /// puis D3D9, puis DxgKrnl : un jeu DXGI a aussi des événements DxgKrnl, jamais comptés deux
    /// fois). Les processus sans image sont oubliés.
    pub fn drain(&mut self) -> HashMap<u32, Vec<f32>> {
        let mut out = HashMap::new();
        self.pids.retain(|pid, log| {
            let any = log.times.iter().any(|t| !t.is_empty());
            if let Some(best) = log.times.iter_mut().find(|t| !t.is_empty()) {
                out.insert(*pid, std::mem::take(best));
            }
            log.times.iter_mut().for_each(Vec::clear);
            any
        });
        out
    }
}

/// Session ETW temps réel qui écoute les présentations de tout le PC.
#[cfg(windows)]
pub use etw::Tracer;

#[cfg(windows)]
mod etw {
    use super::{Log, Missing};
    use std::collections::HashMap;
    use std::ffi::c_void;
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;
    use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, WIN32_ERROR};
    use windows::Win32::System::Diagnostics::Etw::{
        CONTROLTRACE_HANDLE, CloseTrace, ControlTraceW, ENABLE_TRACE_PARAMETERS, ENABLE_TRACE_PARAMETERS_VERSION_2,
        EVENT_CONTROL_CODE_ENABLE_PROVIDER, EVENT_FILTER_DESCRIPTOR, EVENT_FILTER_EVENT_ID, EVENT_FILTER_TYPE_EVENT_ID,
        EVENT_RECORD, EVENT_TRACE_CONTROL_STOP, EVENT_TRACE_LOGFILEW, EVENT_TRACE_PROPERTIES, EVENT_TRACE_REAL_TIME_MODE,
        EnableTraceEx2, OpenTraceW, PROCESS_TRACE_MODE_EVENT_RECORD, PROCESS_TRACE_MODE_REAL_TIME, PROCESSTRACE_HANDLE,
        ProcessTrace, StartTraceW, TRACE_LEVEL_INFORMATION, WNODE_FLAG_TRACED_GUID,
    };
    use windows::core::{GUID, HSTRING, PWSTR};

    /// Nom de la session (une seule à la fois ; une session restée d'un Ludex arrêté net est
    /// arrêtée puis rouverte).
    const NAME: &str = "Ludex-Images-par-seconde";

    const DXGI: GUID = GUID::from_u128(0xCA11C036_0102_4A2D_A6AD_F03CFED5D3C9);
    const D3D9: GUID = GUID::from_u128(0x783ACA0A_790E_4D7F_8451_AA850511C6B9);
    const DXGKRNL: GUID = GUID::from_u128(0x802EC45A_1E99_4B83_9920_87C98277BA9D);

    /// Fournisseur, mots-clés, événement retenu. DXGI et D3D9 : `Events` + canal analytique, comme
    /// PresentMon ; DxgKrnl : `Present` seul (le mot-clé `Base` donne ~20 000 événements par seconde).
    const PROVIDERS: [(GUID, u64, u16); 3] =
        [(DXGI, 0x8000_0000_0000_0002, 42), (D3D9, 0x8000_0000_0000_0002, 1), (DXGKRNL, 0x0800_0000, 184)];

    /// Source (indice de `PROVIDERS`) d'un événement, s'il annonce une image présentée.
    fn source_of(provider: &GUID, id: u16) -> Option<usize> {
        PROVIDERS.iter().position(|(guid, _, event)| guid == provider && *event == id)
    }

    /// Reçoit chaque événement de la session (fil `ludex-images`). Ne panique jamais.
    unsafe extern "system" fn on_event(record: *mut EVENT_RECORD) {
        // SAFETY : ETW passe un enregistrement valide pendant l'appel ; `UserContext` = `Context`
        // donné à `OpenTraceW`, un `Arc<Mutex<Log>>` libéré seulement après la fin de `ProcessTrace`.
        let Some(record) = (unsafe { record.as_ref() }) else { return };
        let header = &record.EventHeader;
        let Some(source) = source_of(&header.ProviderId, header.EventDescriptor.Id) else { return };
        let log = record.UserContext as *const Mutex<Log>;
        if log.is_null() {
            return;
        }
        if let Ok(mut log) = unsafe { &*log }.lock() {
            log.record(header.ProcessId, source, header.TimeStamp);
        }
    }

    /// Propriétés d'une session suivies de son nom (mémoire alignée, taille donnée à ETW).
    fn properties() -> Vec<u64> {
        let name: Vec<u16> = NAME.encode_utf16().chain(Some(0)).collect();
        let head = size_of::<EVENT_TRACE_PROPERTIES>();
        let size = head + name.len() * 2;
        let mut buffer = vec![0u64; size.div_ceil(8)];
        let props = buffer.as_mut_ptr().cast::<EVENT_TRACE_PROPERTIES>();
        // SAFETY : tampon aligné de `size` octets au moins, propriétés au début, nom juste après.
        unsafe {
            (*props).Wnode.BufferSize = size as u32;
            (*props).Wnode.Flags = WNODE_FLAG_TRACED_GUID;
            (*props).Wnode.ClientContext = 1; // horloge QPC, convertie en unités de 100 ns
            (*props).LogFileMode = EVENT_TRACE_REAL_TIME_MODE;
            (*props).BufferSize = 64; // Ko
            (*props).FlushTimer = 1; // événements livrés au moins chaque seconde
            (*props).LoggerNameOffset = head as u32;
            std::ptr::copy_nonoverlapping(name.as_ptr(), buffer.as_mut_ptr().cast::<u8>().add(head).cast::<u16>(), name.len());
        }
        buffer
    }

    fn stop_named() {
        let mut props = properties();
        // SAFETY : propriétés bien formées ; arrêt par le nom de notre session seulement.
        unsafe {
            let _ = ControlTraceW(CONTROLTRACE_HANDLE::default(), &HSTRING::from(NAME), props.as_mut_ptr().cast(), EVENT_TRACE_CONTROL_STOP);
        }
    }

    /// Active un fournisseur, filtré sur son seul événement d'image ; sans filtre si Windows le
    /// refuse (le rappel trie de toute façon).
    fn enable(handle: CONTROLTRACE_HANDLE, (guid, keywords, event): (GUID, u64, u16)) -> WIN32_ERROR {
        let ids = EVENT_FILTER_EVENT_ID { FilterIn: true, Reserved: 0, Count: 1, Events: [event] };
        let mut filter = EVENT_FILTER_DESCRIPTOR {
            Ptr: &ids as *const _ as u64,
            Size: size_of::<EVENT_FILTER_EVENT_ID>() as u32,
            Type: EVENT_FILTER_TYPE_EVENT_ID,
        };
        let params = ENABLE_TRACE_PARAMETERS {
            Version: ENABLE_TRACE_PARAMETERS_VERSION_2,
            EnableFilterDesc: &mut filter,
            FilterDescCount: 1,
            ..Default::default()
        };
        let level = TRACE_LEVEL_INFORMATION as u8;
        let code = EVENT_CONTROL_CODE_ENABLE_PROVIDER.0;
        // SAFETY : session ouverte, filtre et paramètres valides pendant l'appel.
        let status = unsafe { EnableTraceEx2(handle, &guid, code, level, keywords, 0, 0, Some(&params)) };
        if status.is_ok() {
            return status;
        }
        // SAFETY : comme au-dessus, sans paramètres.
        unsafe { EnableTraceEx2(handle, &guid, code, level, keywords, 0, 0, None) }
    }

    pub struct Tracer {
        handle: CONTROLTRACE_HANDLE,
        trace: PROCESSTRACE_HANDLE,
        log: Arc<Mutex<Log>>,
        /// `Arc::into_raw` passé à ETW, repris après la fin du fil.
        context: *const Mutex<Log>,
        thread: Option<JoinHandle<()>>,
    }

    // SAFETY : `context` n'est lu que par le fil ETW et repris dans `Drop` après sa fin.
    unsafe impl Send for Tracer {}

    impl Tracer {
        /// Ouvre la session et commence à écouter. `Denied` sans les droits.
        pub fn start() -> Result<Tracer, Missing> {
            let mut handle = CONTROLTRACE_HANDLE::default();
            let mut props = properties();
            // SAFETY : propriétés bien formées, poignée écrite par ETW.
            let mut status = unsafe { StartTraceW(&mut handle, &HSTRING::from(NAME), props.as_mut_ptr().cast()) };
            if status == ERROR_ALREADY_EXISTS {
                stop_named();
                let mut props = properties();
                // SAFETY : comme au-dessus.
                status = unsafe { StartTraceW(&mut handle, &HSTRING::from(NAME), props.as_mut_ptr().cast()) };
            }
            if status == ERROR_ACCESS_DENIED {
                return Err(Missing::Denied);
            }
            if status.is_err() {
                log::warn!("Images par seconde : session ETW impossible ({})", status.0);
                return Err(Missing::Unavailable);
            }
            let enabled = PROVIDERS.iter().filter(|p| enable(handle, **p).is_ok()).count();
            if enabled == 0 {
                stop_named();
                log::warn!("Images par seconde : aucun fournisseur ETW activé");
                return Err(Missing::Unavailable);
            }

            let log: Arc<Mutex<Log>> = Arc::default();
            let context = Arc::into_raw(log.clone());
            let mut name: Vec<u16> = NAME.encode_utf16().chain(Some(0)).collect();
            let mut file = EVENT_TRACE_LOGFILEW { LoggerName: PWSTR(name.as_mut_ptr()), Context: context as *mut c_void, ..Default::default() };
            file.Anonymous1.ProcessTraceMode = PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD;
            file.Anonymous2.EventRecordCallback = Some(on_event);
            // SAFETY : description valide pendant l'appel (nom copié par ETW).
            let trace = unsafe { OpenTraceW(&mut file) };
            if trace.Value == u64::MAX {
                stop_named();
                // SAFETY : pointeur tiré de `Arc::into_raw` juste au-dessus, jamais donné à ETW.
                drop(unsafe { Arc::from_raw(context) });
                log::warn!("Images par seconde : lecture de la session ETW impossible");
                return Err(Missing::Unavailable);
            }
            let thread = std::thread::Builder::new()
                .name("ludex-images".into())
                // SAFETY : poignée ouverte ; `ProcessTrace` rend la main quand la session s'arrête.
                .spawn(move || unsafe {
                    let _ = ProcessTrace(&[trace], None, None);
                })
                .ok();
            let tracer = Tracer { handle, trace, log, context, thread };
            if tracer.thread.is_none() {
                return Err(Missing::Unavailable); // `Drop` arrête et libère
            }
            Ok(tracer)
        }

        /// Temps d'image par processus depuis le dernier appel.
        pub fn drain(&self) -> HashMap<u32, Vec<f32>> {
            self.log.lock().map(|mut l| l.drain()).unwrap_or_default()
        }
    }

    impl Drop for Tracer {
        fn drop(&mut self) {
            let mut props = properties();
            // SAFETY : session ouverte par `start`, arrêtée une fois ; puis lecture fermée.
            unsafe {
                let _ = ControlTraceW(self.handle, &HSTRING::from(NAME), props.as_mut_ptr().cast(), EVENT_TRACE_CONTROL_STOP);
                let _ = CloseTrace(self.trace);
            }
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
            // SAFETY : fil ETW terminé (ou jamais lancé) : plus aucun rappel ne lit ce pointeur.
            drop(unsafe { Arc::from_raw(self.context) });
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn recognizes_present_events_only() {
            assert_eq!(source_of(&DXGI, 42), Some(0));
            assert_eq!(source_of(&D3D9, 1), Some(1));
            assert_eq!(source_of(&DXGKRNL, 184), Some(2));
            assert_eq!(source_of(&DXGI, 43), None); // Present_Stop
            assert_eq!(source_of(&DXGKRNL, 215), None);
            assert_eq!(source_of(&GUID::zeroed(), 42), None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: i64 = 10_000; // une milliseconde en unités de 100 ns

    #[test]
    fn frame_times_are_gaps_between_presents_of_one_process() {
        let mut log = Log::default();
        for i in 0..4 {
            log.record(10, 0, 1_000 * MS + i * 16 * MS);
        }
        log.record(20, 2, 5 * MS);
        log.record(20, 2, 25 * MS);
        log.record(20, 2, 25 * MS); // même heure : rien
        log.record(20, 2, 20 * MS); // en arrière : rien
        log.record(20, 9, 30 * MS); // source inconnue : rien
        let out = log.drain();
        assert_eq!(out[&10], vec![16.0, 16.0, 16.0]);
        assert_eq!(out[&20], vec![20.0]);
        // Après la lecture, l'écart avec la présentation d'avant compte encore.
        log.record(10, 0, 1_000 * MS + 64 * MS);
        assert_eq!(log.drain()[&10], vec![16.0]);
        // Processus sans image : oublié.
        assert!(log.drain().is_empty());
        assert!(log.pids.is_empty());
    }

    #[test]
    fn dxgi_wins_over_the_kernel_events_of_the_same_process() {
        let mut log = Log::default();
        for i in 0..3 {
            log.record(10, 2, i * 10 * MS); // DxgKrnl : chaque image vue deux fois par le noyau
            log.record(10, 2, i * 10 * MS + 2 * MS);
            log.record(10, 0, i * 10 * MS); // DXGI
        }
        assert_eq!(log.drain()[&10], vec![10.0, 10.0]);
        // Vulkan ou OpenGL : seulement DxgKrnl.
        log.record(30, 2, 0);
        log.record(30, 2, 7 * MS);
        assert_eq!(log.drain()[&30], vec![7.0]);
    }

    #[test]
    fn caps_processes_and_frames() {
        let mut log = Log::default();
        for pid in 0..(MAX_PIDS as u32 + 10) {
            log.record(pid, 0, 0);
            log.record(pid, 0, MS);
        }
        assert_eq!(log.drain().len(), MAX_PIDS);
        let mut log = Log::default();
        for i in 0..(MAX_FRAMES as i64 + 50) {
            log.record(1, 0, i * MS);
        }
        assert_eq!(log.drain()[&1].len(), MAX_FRAMES);
    }

    #[test]
    fn picks_the_game_process_that_presents_the_most() {
        let map: HashMap<u32, Vec<f32>> = [(10, vec![16.0; 3]), (11, vec![8.0; 7]), (99, vec![1.0; 50])].into();
        let reading = Reading::Frames(map);
        assert_eq!(game_frames(&reading, [10, 11]), GameFrames::Frames(vec![8.0; 7]));
        assert_eq!(game_frames(&reading, [12]), GameFrames::Frames(vec![]));
        assert_eq!(game_frames(&Reading::Missing(Missing::Denied), [10]), GameFrames::Missing(Missing::Denied));
    }

    #[test]
    fn average_and_one_percent_low() {
        let mut s = FrameStats::default();
        assert_eq!((s.average(), s.low()), (None, None));
        // 990 images à 10 ms (100 i/s) et 10 à 50 ms (saccades).
        for _ in 0..990 {
            s.add(10.0);
        }
        for _ in 0..10 {
            s.add(50.0);
        }
        // 1 000 images en 10,4 s.
        let avg = s.average().unwrap();
        assert!((avg - 1000.0 / 10.4).abs() < 1e-6);
        // 99ᵉ centile : les 10 images à 50 ms, soit ~20 i/s (case de 1 %).
        let low = s.low().unwrap();
        assert!((low - 20.0).abs() < 0.3, "{low}");
        // Écarts invalides ignorés : pause d'une seconde ou plus, zéro, négatif, NaN.
        let before = s.clone();
        for v in [1000.0, 5000.0, 0.0, -3.0, f32::NAN, f32::INFINITY] {
            s.add(v);
        }
        assert_eq!(s, before);
    }

    #[test]
    fn low_needs_enough_frames_and_never_exceeds_the_average() {
        let mut s = FrameStats::default();
        for _ in 0..(MIN_LOW_FRAMES - 1) {
            s.add(16.667);
        }
        assert!(s.low().is_none());
        assert!(s.average().is_some());
        s.add(16.667);
        // Images toutes égales : 1 % low = moyenne (jamais au-dessus à cause de l'arrondi des cases).
        let (avg, low) = (s.average().unwrap(), s.low().unwrap());
        assert!(low <= avg && avg - low < 0.6, "{avg} {low}");
    }

    #[test]
    fn merged_stats_equal_stats_of_all_frames() {
        let (mut a, mut b, mut all) = (FrameStats::default(), FrameStats::default(), FrameStats::default());
        for i in 0..500 {
            let v = 5.0 + (i % 37) as f32;
            if i % 2 == 0 { a.add(v) } else { b.add(v) }
            all.add(v);
        }
        a.merge(&b);
        assert_eq!(a.n, all.n);
        assert_eq!(a.hist, all.hist);
        assert!((a.sum_ms - all.sum_ms).abs() < 1e-6);
        assert_eq!(a.low(), all.low());
    }

    #[test]
    fn bins_are_one_percent_wide() {
        assert_eq!(bin(0.0), 0);
        assert_eq!(bin(MIN_MS), 0);
        for ms in [0.5, 4.0, 16.667, 33.3, 120.0, 999.0] {
            let center = bin_ms(bin(ms));
            assert!((center / ms - 1.0).abs() < 0.01, "{ms} {center}");
        }
    }
}
