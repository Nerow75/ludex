//! Journal d'erreurs sur disque, pour aider un utilisateur qui signale un problème : la console est
//! invisible dans une build installée. Fichier `ludex.log` du dossier de données de l'app
//! (`%APPDATA%\com.ludex.app`, jamais le dossier des données lourdes : il doit rester lisible même
//! disque débranché), une ligne par message : date locale, niveau, module, message. Rotation :
//! `ludex.log`, puis `ludex.1.log` et `ludex.2.log`, 1 Mo chacun au plus.
//!
//! Branché sur la crate `log` (`log::error!`, `log::warn!`, `log::info!` partout dans Ludex) par un
//! petit logger maison : la ligne part dans un canal borné vers un fil d'écriture, l'appelant ne
//! touche jamais au disque (canal plein = ligne abandonnée). Toute erreur d'écriture (disque plein,
//! fichier verrouillé, dossier absent) est ignorée en silence. Chaque ligne est anonymisée comme le
//! rapport du Diagnostic (`diagnostic::anonymize`). Aucun secret, jeton ni contenu de fichier de
//! l'utilisateur n'y est écrit : les messages sont ceux des erreurs, des titres de jeux au plus.
//! En debug, le message va aussi dans la console comme avant.
//!
//! Paniques : `install_panic_hook` écrit le message et l'emplacement tout de suite (sans passer par
//! le canal : le processus s'arrête juste après, `panic = "abort"` en release), puis montre une
//! boîte de dialogue Windows plutôt qu'une fenêtre qui disparaît ou reste blanche.

use crate::diagnostic::{Privacy, anonymize};
use crate::library::SourceReport;
use crate::{AppState, blocking};
use log::{Level, LevelFilter, Log, Metadata, Record};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, SyncSender};
use tauri::Manager;
use tauri_plugin_opener::OpenerExt;

/// Nom du journal en cours ; les anciens sont `ludex.1.log` (le plus récent) puis `ludex.2.log`.
pub const FILE: &str = "ludex.log";
/// Taille maximale de chaque fichier.
const MAX_BYTES: u64 = 1024 * 1024;
/// Anciens fichiers gardés.
const KEPT: usize = 2;
/// Caractères gardés par message (un message plus long est coupé).
const MAX_MESSAGE: usize = 2000;
/// Lignes en attente d'écriture au plus : au-delà, elles sont abandonnées.
const QUEUE: usize = 1024;
/// Lignes du journal ajoutées au rapport du Diagnostic.
pub const TAIL_LINES: usize = 50;

struct Shared {
    dir: PathBuf,
    privacy: Privacy,
    sender: SyncSender<String>,
}

static SHARED: OnceLock<Shared> = OnceLock::new();
static LOGGER: Logger = Logger;
/// Boîte de dialogue de panique déjà montrée (plusieurs fils peuvent paniquer à la fois).
static CRASH_SHOWN: AtomicBool = AtomicBool::new(false);

/// Dossier de données de l'app pour cet identifiant (`%APPDATA%\<identifiant>`), le même que
/// `app_data_dir` de Tauri, connu avant que l'app soit construite.
pub fn data_dir(identifier: &str) -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join(identifier))
}

/// Démarre le journal (une seule fois, au tout début de `run`). Sans dossier ou sans fil
/// d'écriture, Ludex tourne sans journal.
pub fn init(dir: Option<PathBuf>) {
    if let Some(dir) = dir {
        let (sender, receiver) = mpsc::sync_channel::<String>(QUEUE);
        let privacy = Privacy::of_this_pc();
        let (target, filter) = (dir.clone(), privacy.clone());
        let spawned = std::thread::Builder::new().name("ludex-journal".into()).spawn(move || {
            let mut writer = Writer::new(target, MAX_BYTES);
            for line in receiver {
                let _ = writer.write(&anonymize(&line, &filter));
            }
        });
        if spawned.is_ok() {
            let _ = SHARED.set(Shared { dir, privacy, sender });
        }
    }
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(LevelFilter::Info);
    }
}

/// Dossier du journal, `None` s'il n'a pas démarré.
pub fn dir() -> Option<&'static Path> {
    SHARED.get().map(|s| s.dir.as_path())
}

struct Logger;

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        // Les crates utilisées (Tauri, ses plugins…) n'écrivent que leurs erreurs.
        metadata.level() <= Level::Info && (metadata.target().starts_with("ludex") || metadata.level() == Level::Error)
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let message = record.args().to_string();
        if cfg!(debug_assertions) {
            eprintln!("{message}");
        }
        if let Some(shared) = SHARED.get() {
            let line = format_line(&now(), record.level(), &module_of(record.target()), &message);
            // Canal plein ou fil arrêté : la ligne est perdue, l'app continue.
            let _ = shared.sender.try_send(line);
        }
    }

    fn flush(&self) {}
}

fn now() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Module d'un message : `ludex_lib::library::steam` → `library::steam`.
pub fn module_of(target: &str) -> String {
    match target.strip_prefix("ludex_lib") {
        Some("") => "ludex".to_string(),
        Some(rest) => rest.trim_start_matches("::").to_string(),
        None => target.to_string(),
    }
}

fn level_label(level: Level) -> &'static str {
    match level {
        Level::Error => "ERREUR",
        Level::Warn => "AVERTISSEMENT",
        _ => "INFO",
    }
}

/// Une ligne du journal : `2026-11-21 14:05:03 ERREUR [art] message`, sur une seule ligne
/// (retours à la ligne et contrôles remplacés par une espace), message coupé à 2 000 caractères.
pub fn format_line(time: &str, level: Level, module: &str, message: &str) -> String {
    let mut text: String = message.trim().chars().take(MAX_MESSAGE).map(|c| if c.is_control() { ' ' } else { c }).collect();
    if message.trim().chars().count() > MAX_MESSAGE {
        text.push('…');
    }
    format!("{time} {} [{module}] {text}\n", level_label(level))
}

/// Écriture dans le journal, avec rotation. Chaque erreur est renvoyée à l'appelant, qui l'ignore :
/// rien ne panique, et un fichier qui ne peut pas tourner ne grossit plus (lignes abandonnées).
pub struct Writer {
    dir: PathBuf,
    file: Option<File>,
    size: u64,
    max: u64,
}

impl Writer {
    pub fn new(dir: PathBuf, max: u64) -> Self {
        Writer { dir, file: None, size: 0, max }
    }

    /// `ludex.log` pour 0, sinon `ludex.<n>.log`.
    fn path(&self, n: usize) -> PathBuf {
        rotated(&self.dir, n)
    }

    fn open(&mut self) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let file = OpenOptions::new().create(true).append(true).open(self.path(0))?;
        self.size = file.metadata()?.len();
        self.file = Some(file);
        Ok(())
    }

    fn rotate(&mut self) -> io::Result<()> {
        // Fermé d'abord : Windows ne renomme pas toujours un fichier ouvert.
        self.file = None;
        match fs::remove_file(self.path(KEPT)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
        for n in (0..KEPT).rev() {
            let from = self.path(n);
            if from.exists() {
                fs::rename(&from, self.path(n + 1))?;
            }
        }
        self.size = 0;
        Ok(())
    }

    pub fn write(&mut self, line: &str) -> io::Result<()> {
        if self.file.is_none() {
            self.open()?;
        }
        if self.size > 0 && self.size + line.len() as u64 > self.max {
            self.rotate()?;
            self.open()?;
        }
        let result = match self.file.as_mut() {
            Some(file) => file.write_all(line.as_bytes()),
            None => Err(io::Error::other("journal fermé")),
        };
        match result {
            Ok(()) => {
                self.size += line.len() as u64;
                Ok(())
            }
            Err(e) => {
                // Rouvert à la ligne suivante (fichier supprimé, disque rebranché…).
                self.file = None;
                Err(e)
            }
        }
    }
}

fn rotated(dir: &Path, n: usize) -> PathBuf {
    if n == 0 { dir.join(FILE) } else { dir.join(format!("ludex.{n}.log")) }
}

/// Écrit une ligne tout de suite, sans le fil d'écriture (panique : le processus s'arrête après).
fn write_now(level: Level, module: &str, message: &str) {
    if let Some(shared) = SHARED.get() {
        let line = anonymize(&format_line(&now(), level, module, message), &shared.privacy);
        let _ = Writer::new(shared.dir.clone(), MAX_BYTES).write(&line);
    }
}

/// Dernières `n` lignes du journal, la plus récente en dernier (complétées par `ludex.1.log`).
pub fn tail(dir: &Path, n: usize) -> Vec<String> {
    let mut lines = last_lines(&rotated(dir, 0), n);
    if lines.len() < n {
        let mut older = last_lines(&rotated(dir, 1), n - lines.len());
        older.append(&mut lines);
        lines = older;
    }
    lines
}

fn last_lines(path: &Path, n: usize) -> Vec<String> {
    let read = || -> io::Result<Vec<String>> {
        let mut file = File::open(path)?;
        let len = file.metadata()?.len();
        // Une ligne fait au plus ~2 100 octets (message coupé) : de quoi lire `n` lignes entières.
        let want = (n as u64 + 1) * (MAX_MESSAGE as u64 * 4 + 100);
        let start = len.saturating_sub(want);
        file.seek(SeekFrom::Start(start))?;
        let mut bytes = Vec::new();
        file.take(want).read_to_end(&mut bytes)?;
        let text = String::from_utf8_lossy(&bytes);
        let mut lines: Vec<&str> = text.lines().collect();
        if start > 0 && !lines.is_empty() {
            lines.remove(0); // ligne coupée par le début de la lecture
        }
        let lines: Vec<String> = lines.into_iter().filter(|l| !l.trim().is_empty()).map(String::from).collect();
        Ok(lines[lines.len().saturating_sub(n)..].to_vec())
    };
    read().unwrap_or_default()
}

/// Bloc ajouté au rapport du Diagnostic : les dernières lignes du journal, anonymisées à nouveau
/// (un ancien fichier a pu être écrit avec un autre nom d'utilisateur).
pub fn report_section(lines: &[String], privacy: &Privacy) -> String {
    let mut out = vec![String::new(), format!("Journal ({} dernières lignes)", TAIL_LINES)];
    if lines.is_empty() {
        out.push("(vide)".to_string());
    } else {
        out.extend(lines.iter().cloned());
    }
    anonymize(&(out.join("\n") + "\n"), privacy)
}

/// Ligne du journal à la fin d'un scan complet : durée cumulée des sources et jeux par source.
pub fn scan_line(reports: &[SourceReport], games: usize) -> String {
    let millis: u64 = reports.iter().map(|r| r.millis).sum();
    let sources: Vec<String> = reports
        .iter()
        .map(|r| {
            let name = r.platform.label();
            let state = if r.error.is_some() {
                "erreur".to_string()
            } else if r.disabled {
                format!("désactivé, {}", plural(r.count, "écarté", "écartés"))
            } else if !r.installed {
                "absent".to_string()
            } else {
                plural(r.count, "jeu", "jeux")
            };
            format!("{name} {state} en {} ms", r.millis)
        })
        .collect();
    format!("Scan complet : {}, sources lues en {millis} ms ({})", plural(games, "jeu", "jeux"), sources.join(", "))
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n > 1 { many } else { one })
}

/// Texte d'une panique (`panic!("…")` ou `panic!("{}", …)`).
fn payload_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panique sans message".to_string())
}

/// Ligne écrite pour une panique.
fn panic_line(thread: &str, place: Option<(&str, u32)>, message: &str) -> String {
    let place = place.map_or("emplacement inconnu".to_string(), |(file, line)| format!("{file}:{line}"));
    format!("Panique dans le fil « {thread} » ({place}) : {message}")
}

/// Paniques écrites dans le journal, puis, quand Ludex va s'arrêter (toujours en release, où une
/// panique arrête le processus ; fil principal en debug), boîte de dialogue qui l'explique.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let name = thread.name().unwrap_or("sans nom").to_string();
        let place = info.location().map(|l| (l.file(), l.line()));
        write_now(Level::Error, "panique", &panic_line(&name, place, &payload_text(info.payload())));
        if cfg!(debug_assertions) {
            previous(info);
        }
        let fatal = !cfg!(debug_assertions) || name == "main";
        if fatal && !CRASH_SHOWN.swap(true, Ordering::SeqCst) {
            show_crash_dialog();
        }
    }));
}

const CRASH_TEXT: &str = "Ludex a rencontré un problème imprévu et doit se fermer.\n\nLe détail est noté dans le journal de Ludex (ludex.log). Au prochain lancement : Paramètres, Diagnostic, « Ouvrir le journal » ou « Copier le rapport ».";

fn show_crash_dialog() {
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MB_SETFOREGROUND, MB_TOPMOST, MessageBoxW};
        use windows::core::HSTRING;
        // Bloque le fil qui panique jusqu'au clic : le journal est déjà écrit.
        unsafe {
            MessageBoxW(None, &HSTRING::from(CRASH_TEXT), &HSTRING::from("Ludex"), MB_OK | MB_ICONERROR | MB_SETFOREGROUND | MB_TOPMOST);
        }
    }
}

/// Montre le journal dans l'Explorateur (fichier sélectionné), ou son dossier s'il n'existe pas
/// encore. Chemin reconstruit ici, jamais reçu du front.
#[tauri::command]
pub async fn open_log(app: tauri::AppHandle) -> Result<(), String> {
    blocking(move || {
        let dir = dir().map(Path::to_path_buf).unwrap_or_else(|| app.state::<AppState>().data_dir.clone());
        let file = dir.join(FILE);
        if file.is_file() {
            app.opener().reveal_item_in_dir(&file).map_err(|e| format!("Impossible d'ouvrir le journal : {e}"))
        } else {
            fs::create_dir_all(&dir).map_err(|e| format!("Dossier du journal introuvable : {e}"))?;
            app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|e| format!("Impossible d'ouvrir le dossier du journal : {e}"))
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Platform;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ludex-journal-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn une_ligne_par_message() {
        let line = format_line("2026-11-21 14:05:03", Level::Error, "art", "Cache non enregistré :\r\naccès refusé\t(5)");
        assert_eq!(line, "2026-11-21 14:05:03 ERREUR [art] Cache non enregistré :  accès refusé (5)\n");
        assert_eq!(format_line("t", Level::Warn, "watch", " x ").as_str(), "t AVERTISSEMENT [watch] x\n");
        assert_eq!(format_line("t", Level::Info, "ludex", "y").as_str(), "t INFO [ludex] y\n");
        assert_eq!(format_line("t", Level::Debug, "ludex", "y").as_str(), "t INFO [ludex] y\n");
        let long = format_line("t", Level::Info, "m", &"é".repeat(MAX_MESSAGE + 10));
        assert_eq!(long.chars().filter(|&c| c == 'é').count(), MAX_MESSAGE);
        assert!(long.ends_with("…\n"));
    }

    #[test]
    fn nom_du_module() {
        assert_eq!(module_of("ludex_lib::library::steam"), "library::steam");
        assert_eq!(module_of("ludex_lib::art"), "art");
        assert_eq!(module_of("ludex_lib"), "ludex");
        assert_eq!(module_of("tauri::manager"), "tauri::manager");
    }

    #[test]
    fn rotation_a_trois_fichiers() {
        let dir = temp("rotation");
        let mut w = Writer::new(dir.clone(), 100);
        let line = |n: usize| format!("{n:02} {}\n", "x".repeat(36)); // 40 octets
        for n in 0..10 {
            w.write(&line(n)).unwrap();
        }
        let read = |n| fs::read_to_string(rotated(&dir, n)).unwrap_or_default();
        // 2 lignes par fichier (une troisième dépasserait 100 octets), 3 fichiers gardés.
        assert_eq!(read(0), line(8) + &line(9));
        assert_eq!(read(1), line(6) + &line(7));
        assert_eq!(read(2), line(4) + &line(5));
        assert!(!rotated(&dir, 3).exists());
        for n in 0..=KEPT {
            assert!(fs::metadata(rotated(&dir, n)).unwrap().len() <= 100);
        }
        // Un journal déjà plein au démarrage tourne dès la première ligne.
        let mut again = Writer::new(dir.clone(), 80);
        again.write(&line(10)).unwrap();
        assert_eq!(read(0), line(10));
        assert_eq!(read(1), line(8) + &line(9));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ligne_plus_longue_que_le_fichier() {
        let dir = temp("long");
        let mut w = Writer::new(dir.clone(), 10);
        w.write("une ligne bien plus longue\n").unwrap();
        w.write("seconde\n").unwrap();
        assert_eq!(fs::read_to_string(rotated(&dir, 0)).unwrap(), "seconde\n");
        assert_eq!(fs::read_to_string(rotated(&dir, 1)).unwrap(), "une ligne bien plus longue\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ecriture_impossible_sans_panique() {
        // Le « dossier » du journal est un fichier : rien ne peut s'y créer.
        let base = temp("bloque");
        fs::create_dir_all(&base).unwrap();
        let blocker = base.join("pas-un-dossier");
        fs::write(&blocker, "x").unwrap();
        let mut w = Writer::new(blocker.clone(), 100);
        assert!(w.write("a\n").is_err());
        assert!(w.write("b\n").is_err());
        assert!(tail(&blocker, 5).is_empty());
        // Rotation refusée (un dossier à la place de `ludex.2.log`) : lignes abandonnées, fichier
        // jamais plus gros que la limite.
        let dir = temp("rotation-bloquee");
        fs::create_dir_all(rotated(&dir, 2).join("plein")).unwrap();
        let mut w = Writer::new(dir.clone(), 20);
        w.write("0123456789\n").unwrap();
        assert!(w.write("0123456789\n").is_err());
        assert!(w.write("0123456789\n").is_err());
        assert_eq!(fs::read_to_string(rotated(&dir, 0)).unwrap(), "0123456789\n");
        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn dernieres_lignes_sur_deux_fichiers() {
        let dir = temp("tail");
        fs::create_dir_all(&dir).unwrap();
        assert!(tail(&dir, 3).is_empty());
        fs::write(rotated(&dir, 1), "a\nb\nc\n").unwrap();
        fs::write(rotated(&dir, 0), "d\n\ne\n").unwrap();
        assert_eq!(tail(&dir, 3), ["c", "d", "e"]);
        assert_eq!(tail(&dir, 10), ["a", "b", "c", "d", "e"]);
        assert_eq!(tail(&dir, 1), ["e"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn journal_anonymise() {
        let privacy = Privacy { home: Some(r"C:\Users\Anne".to_string()), user: Some("Anne".to_string()) };
        let lines = vec![
            r"2026-11-21 10:00:00 AVERTISSEMENT [art] Cache non enregistré (C:\Users\Anne\AppData\Roaming\com.ludex.app\art-cache.json)".to_string(),
            r"2026-11-21 10:00:01 ERREUR [watch] Dossier non surveillé D:\Steam\userdata\123456\config : refus (D:\Anne\Jeux)".to_string(),
        ];
        let text = report_section(&lines, &privacy);
        assert!(text.contains(r"%USERPROFILE%\AppData\Roaming\com.ludex.app\art-cache.json"));
        assert!(text.contains(r"userdata\<compte>\config"));
        assert!(text.contains(r"D:\%USERNAME%\Jeux"));
        assert!(!text.contains("Anne") && !text.contains("123456"));
        assert!(text.contains("Journal (50 dernières lignes)"));
        assert!(report_section(&[], &privacy).contains("(vide)"));
    }

    #[test]
    fn ligne_du_scan() {
        let report = |platform, installed, count, error: Option<&str>, disabled, millis| SourceReport {
            platform,
            installed,
            count,
            error: error.map(String::from),
            disabled,
            millis,
            skipped: Vec::new(),
        };
        let line = scan_line(
            &[
                report(Platform::Steam, true, 70, None, false, 142),
                report(Platform::Gog, false, 0, None, false, 0),
                report(Platform::Riot, true, 2, None, true, 3),
                report(Platform::Xbox, true, 1, None, false, 24),
                report(Platform::Epic, true, 0, None, false, 0),
                report(Platform::Ea, true, 0, Some("base illisible"), false, 1200),
            ],
            72,
        );
        assert_eq!(
            line,
            "Scan complet : 72 jeux, sources lues en 1369 ms (Steam 70 jeux en 142 ms, GOG absent en 0 ms, Riot désactivé, 2 écartés en 3 ms, Xbox 1 jeu en 24 ms, Epic Games 0 jeu en 0 ms, EA erreur en 1200 ms)"
        );
    }

    /// Bout en bout dans un dossier temporaire : logger, fil d'écriture, anonymisation de ce PC,
    /// panique d'un fil secondaire (pas de boîte de dialogue en debug), puis fin du journal.
    #[test]
    #[ignore]
    fn logfile_this_pc() {
        let dir = temp("bout-en-bout");
        init(Some(dir.clone()));
        install_panic_hook();
        let home = dirs::home_dir().unwrap_or_default();
        log::info!("Démarrage d'essai du journal");
        log::warn!("Cache non enregistré ({}) :\naccès refusé", home.join(r"AppData\Roaming\x.json").display());
        log::error!("Dossier non surveillé {} : refus", r"D:\Steam\userdata\123456\config");
        let _ = std::thread::Builder::new().name("essai".into()).spawn(|| panic!("panique d'essai")).unwrap().join();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let lines = tail(&dir, TAIL_LINES);
        for l in &lines {
            println!("{l}");
        }
        assert_eq!(lines.len(), 4);
        let all = lines.join("\n");
        let user = std::env::var("USERNAME").unwrap_or_default();
        assert!(!user.is_empty() && !all.contains(&user));
        assert!(all.contains("%USERPROFILE%") && all.contains(r"userdata\<compte>") && all.contains("panique d'essai"));
        let _ = std::panic::take_hook();
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn texte_des_paniques() {
        let boxed: Box<dyn std::any::Any + Send> = Box::new("boum");
        assert_eq!(payload_text(boxed.as_ref()), "boum");
        let boxed: Box<dyn std::any::Any + Send> = Box::new(format!("index {}", 3));
        assert_eq!(payload_text(boxed.as_ref()), "index 3");
        let boxed: Box<dyn std::any::Any + Send> = Box::new(42u8);
        assert_eq!(payload_text(boxed.as_ref()), "panique sans message");
        assert_eq!(panic_line("main", Some(("src\\lib.rs", 12)), "boum"), "Panique dans le fil « main » (src\\lib.rs:12) : boum");
        assert_eq!(panic_line("ludex-journal", None, "x"), "Panique dans le fil « ludex-journal » (emplacement inconnu) : x");
    }
}
