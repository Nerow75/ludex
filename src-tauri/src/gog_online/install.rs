//! Installation d'un jeu GOG par son installeur hors ligne officiel : parties téléchargées dans le
//! dossier des données lourdes (reprise après une pause, une coupure ou un redémarrage de Ludex),
//! taille annoncée par GOG vérifiée, signature Authenticode de GOG vérifiée sur l'exe, puis Inno
//! Setup lancé en silencieux vers le dossier choisi. L'installeur écrit lui-même les clés
//! `GOG.com\Games` que lit la source GOG : le jeu apparaît au rescan suivant.

use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use super::api::{Failure, Part};

/// Dossier des téléchargements, dans le cache des données lourdes.
pub const DOWNLOADS: &str = "gog-installeurs";
/// Suffixe d'une partie en cours de téléchargement.
const PARTIAL: &str = ".part";
/// Morceau lu à la fois.
const CHUNK: usize = 256 * 1024;
/// Événement de progression envoyé au plus toutes les 250 ms.
const EMIT_GAP: Duration = Duration::from_millis(250);
/// Coupures de suite tolérées sans aucun octet reçu entre deux.
const MAX_RETRIES: u32 = 3;
/// Marge de place libre demandée en plus des parties restantes.
pub const SPACE_MARGIN: u64 = 512 * 1024 * 1024;

/// Étape d'une installation, montrée au front.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Preparing,
    Downloading,
    Paused,
    Verifying,
    Installing,
    Done,
    Failed,
    Cancelled,
}

/// Événement `gog-install`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub id: u64,
    pub title: String,
    pub phase: Phase,
    pub received: u64,
    pub total: u64,
    /// Octets par seconde sur les dernières secondes, `None` tant qu'inconnu.
    pub speed: Option<u64>,
    pub error: Option<String>,
    /// Id du jeu dans la bibliothèque une fois installé (`gog:<id>`).
    pub game_id: Option<String>,
}

/// Pause et annulation, posées par les commandes et lues par le fil de téléchargement.
#[derive(Default)]
pub struct Control {
    paused: Mutex<bool>,
    wake: Condvar,
    cancelled: AtomicBool,
}

impl Control {
    pub fn pause(&self) {
        if let Ok(mut p) = self.paused.lock() {
            *p = true;
        }
    }

    pub fn resume(&self) {
        if let Ok(mut p) = self.paused.lock() {
            *p = false;
        }
        self.wake.notify_all();
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.wake.notify_all();
    }

    pub fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn paused(&self) -> bool {
        self.paused.lock().map(|p| *p).unwrap_or(false)
    }

    /// Attend la fin de la pause. `false` : annulé.
    fn wait(&self) -> bool {
        let Ok(mut paused) = self.paused.lock() else { return !self.cancelled() };
        while *paused && !self.cancelled() {
            paused = match self.wake.wait(paused) {
                Ok(p) => p,
                Err(_) => return false,
            };
        }
        !self.cancelled()
    }
}

/// Lien signé d'une partie et nom du fichier attendu par Inno Setup.
pub struct Link {
    pub url: String,
    pub name: String,
}

/// Corps d'une réponse : flux, taille totale annoncée par le serveur, début réellement servi
/// (0 si le serveur a ignoré `Range`).
pub struct Body {
    pub reader: Box<dyn Read + Send>,
    pub total: Option<u64>,
    pub start: u64,
}

/// D'où viennent les octets : GOG en vrai, la mémoire dans les tests.
pub trait Source {
    /// Lien frais d'une partie (les liens du CDN expirent : redemandé à chaque reprise).
    fn link(&self, part: &Part, first: bool) -> Result<Link, Failure>;
    fn open(&self, link: &Link, start: u64) -> Result<Body, Failure>;
}

/// Fin d'un téléchargement qui n'a pas abouti.
#[derive(Debug, PartialEq, Eq)]
pub enum Stop {
    Cancelled,
    Failed(String),
}

/// Débit sur une fenêtre glissante de 5 s.
struct Speed {
    marks: Vec<(Instant, u64)>,
}

impl Speed {
    fn add(&mut self, at: Instant, total: u64) -> Option<u64> {
        self.marks.push((at, total));
        self.marks.retain(|(t, _)| at.duration_since(*t) <= Duration::from_secs(5));
        let (t0, b0) = *self.marks.first()?;
        let secs = at.duration_since(t0).as_secs_f64();
        (secs >= 1.0).then(|| ((total - b0) as f64 / secs) as u64)
    }
}

/// Télécharge toutes les parties dans `dir` ; renvoie le chemin de l'exe. `report(reçu, débit)`
/// est appelé au plus toutes les 250 ms ; `paused()` signale une pause.
pub fn download(
    source: &dyn Source,
    parts: &[Part],
    dir: &Path,
    control: &Control,
    mut report: impl FnMut(Phase, u64, Option<u64>),
) -> Result<PathBuf, Stop> {
    std::fs::create_dir_all(dir).map_err(|_| Stop::Failed(io_text()))?;
    let total: u64 = parts.iter().map(|p| p.size).sum();
    let mut done_before: u64 = 0;
    let mut exe = None;
    let mut speed = Speed { marks: Vec::new() };
    let mut last_emit = Instant::now() - EMIT_GAP;
    // Dernier compte d'octets reçus (montré pendant une pause).
    let mut received: u64 = 0;
    for (index, part) in parts.iter().enumerate() {
        let first = index == 0;
        let mut retries = 0;
        let path = loop {
            if control.cancelled() {
                return Err(Stop::Cancelled);
            }
            if control.paused() {
                report(Phase::Paused, received.max(done_before), None);
                if !control.wait() {
                    return Err(Stop::Cancelled);
                }
                speed.marks.clear();
            }
            let link = match source.link(part, first) {
                Ok(l) => l,
                Err(e) => {
                    retries += 1;
                    if retries > MAX_RETRIES || e == Failure::Unauthorized {
                        return Err(Stop::Failed(e.message()));
                    }
                    pause_before_retry(control, retries);
                    continue;
                }
            };
            let final_path = dir.join(&link.name);
            let partial = dir.join(format!("{}{PARTIAL}", link.name));
            // Partie déjà complète (reprise après un redémarrage de Ludex).
            if std::fs::metadata(&final_path).is_ok_and(|m| m.len() == part.size) {
                break final_path;
            }
            let mut start = std::fs::metadata(&partial).map(|m| m.len()).unwrap_or(0);
            if start > part.size {
                start = 0;
            }
            if start == part.size {
                std::fs::rename(&partial, &final_path).map_err(|_| Stop::Failed(io_text()))?;
                break final_path;
            }
            let body = match source.open(&link, start) {
                Ok(b) => b,
                Err(e) => {
                    retries += 1;
                    if retries > MAX_RETRIES {
                        return Err(Stop::Failed(e.message()));
                    }
                    pause_before_retry(control, retries);
                    continue;
                }
            };
            if body.total.is_some_and(|t| t != part.size) {
                return Err(Stop::Failed(tr!(
                    "Le fichier servi par GOG n'a pas la taille annoncée : téléchargement arrêté.",
                    "The file served by GOG doesn't have the announced size: download stopped."
                )));
            }
            let mut file = if body.start == 0 {
                File::create(&partial)
            } else {
                OpenOptions::new().append(true).open(&partial)
            }
            .map_err(|_| Stop::Failed(io_text()))?;
            let mut have = body.start;
            let mut reader = body.reader;
            let mut buf = vec![0u8; CHUNK];
            let mut got_any = false;
            let interrupted = loop {
                if control.cancelled() || control.paused() {
                    break true;
                }
                let n = match reader.read(&mut buf) {
                    Ok(0) => break false,
                    Ok(n) => n,
                    Err(_) => break true,
                };
                have += n as u64;
                if have > part.size {
                    drop(file);
                    let _ = std::fs::remove_file(&partial);
                    return Err(Stop::Failed(tr!(
                        "Le fichier servi par GOG dépasse la taille annoncée : téléchargement arrêté.",
                        "The file served by GOG is larger than announced: download stopped."
                    )));
                }
                file.write_all(&buf[..n]).map_err(|_| Stop::Failed(disk_text()))?;
                got_any = true;
                let now = Instant::now();
                received = done_before + have;
                let sp = speed.add(now, received);
                if now.duration_since(last_emit) >= EMIT_GAP {
                    last_emit = now;
                    report(Phase::Downloading, done_before + have, sp);
                }
            };
            file.flush().map_err(|_| Stop::Failed(disk_text()))?;
            drop(file);
            if have == part.size {
                std::fs::rename(&partial, &final_path).map_err(|_| Stop::Failed(io_text()))?;
                break final_path;
            }
            // Pause, annulation ou coupure : reprise à la position atteinte.
            if got_any {
                retries = 0;
            }
            if !interrupted || !(control.cancelled() || control.paused()) {
                retries += 1;
                if retries > MAX_RETRIES {
                    return Err(Stop::Failed(tr!("Connexion à GOG perdue pendant le téléchargement.", "Lost connection to GOG during the download.")));
                }
                pause_before_retry(control, retries);
            }
        };
        if first {
            exe = Some(path);
        }
        done_before += part.size;
        received = done_before;
        report(Phase::Downloading, done_before, None);
    }
    debug_assert_eq!(done_before, total);
    exe.ok_or_else(|| Stop::Failed(io_text()))
}

fn pause_before_retry(control: &Control, retries: u32) {
    let wait = Duration::from_secs(2 * u64::from(retries));
    let end = Instant::now() + wait;
    while Instant::now() < end && !control.cancelled() {
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn io_text() -> String {
    tr!("Écriture du téléchargement impossible.", "Couldn't write the download.")
}

fn disk_text() -> String {
    tr!("Écriture impossible : disque plein ?", "Couldn't write: disk full?")
}

// ---------------------------------------------------------------------------------------------
// Signature
// ---------------------------------------------------------------------------------------------

/// Signataires acceptés (nom simple du certificat, casse ignorée) : les sociétés de GOG.
const GOG_SIGNERS: [&str; 2] = ["GOG sp. z o.o.", "GOG Limited"];

pub fn signer_ok(name: &str) -> bool {
    GOG_SIGNERS.iter().any(|s| s.eq_ignore_ascii_case(name.trim()))
}

/// Vérifie la signature Authenticode de l'installeur (chaîne de confiance de Windows, révocation
/// comprise) puis le nom du signataire. Rien n'est lancé avant.
#[cfg(windows)]
pub fn verify_signature(path: &Path) -> Result<(), String> {
    let signer = authenticode_signer(path).ok_or_else(|| {
        tr!("L'installeur n'a pas de signature valide : il ne sera pas lancé.", "The installer doesn't have a valid signature: it won't be run.")
    })?;
    if signer_ok(&signer) {
        Ok(())
    } else {
        Err(tr!("L'installeur est signé par « {} », pas par GOG : il ne sera pas lancé.", "The installer is signed by \"{}\", not GOG: it won't be run.", signer))
    }
}

#[cfg(not(windows))]
pub fn verify_signature(_: &Path) -> Result<(), String> {
    Err(tr!("Vérification de signature disponible sous Windows seulement.", "Signature check only available on Windows."))
}

/// Nom du signataire d'un fichier dont la signature Authenticode est valide (`WinVerifyTrust`),
/// `None` sinon (absente, invalide, révoquée, certificat non approuvé).
#[cfg(windows)]
pub fn authenticode_signer(path: &Path) -> Option<String> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::{HANDLE, HWND};
    use windows::Win32::Security::Cryptography::{CertGetNameStringW, CERT_NAME_SIMPLE_DISPLAY_TYPE};
    use windows::Win32::Security::WinTrust::{
        WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData, WinVerifyTrust, WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0,
        WINTRUST_FILE_INFO, WTD_CHOICE_FILE, WTD_REVOKE_WHOLECHAIN, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
    };
    let wide = HSTRING::from(path.as_os_str());
    let mut file = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(wide.as_ptr()),
        hFile: HANDLE::default(),
        pgKnownSubject: std::ptr::null_mut(),
    };
    let mut data = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_WHOLECHAIN,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 { pFile: &mut file },
        dwStateAction: WTD_STATEACTION_VERIFY,
        ..Default::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // SAFETY : `data`, `file` et `wide` vivent jusqu'à la fermeture de l'état ci-dessous.
    let status = unsafe { WinVerifyTrust(HWND(-1isize as *mut _), &mut action, (&mut data as *mut WINTRUST_DATA).cast()) };
    let mut name = None;
    if status == 0 {
        // SAFETY : état valide tant que `WTD_STATEACTION_CLOSE` n'est pas passé ; pointeurs vérifiés.
        unsafe {
            let prov = WTHelperProvDataFromStateData(data.hWVTStateData);
            if !prov.is_null() {
                let signer = WTHelperGetProvSignerFromChain(prov, 0, false, 0);
                if !signer.is_null() && !(*signer).pasCertChain.is_null() && (*signer).csCertChain > 0 {
                    let cert = (*(*signer).pasCertChain).pCert;
                    if !cert.is_null() {
                        let mut buf = [0u16; 256];
                        let len = CertGetNameStringW(cert, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, None, Some(&mut buf));
                        if len > 1 {
                            name = Some(String::from_utf16_lossy(&buf[..len as usize - 1]));
                        }
                    }
                }
            }
        }
    }
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    // SAFETY : ferme l'état ouvert par l'appel précédent.
    unsafe { WinVerifyTrust(HWND(-1isize as *mut _), &mut action, (&mut data as *mut WINTRUST_DATA).cast()) };
    name
}

// ---------------------------------------------------------------------------------------------
// Installeur
// ---------------------------------------------------------------------------------------------

/// Arguments d'Inno Setup : silencieux, sans message ni redémarrage, dossier choisi.
pub fn installer_args(dir: &Path, language: &str) -> String {
    let lang = match language {
        "fr" => "french",
        "de" => "german",
        "es" => "spanish",
        "it" => "italian",
        "pl" => "polish",
        "ru" => "russian",
        _ => "english",
    };
    format!("/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP- /LANG={lang} /DIR=\"{}\"", dir.display())
}

/// Dossier d'installation : `<base>\<titre nettoyé>`. Refusé : chemin non absolu, partage réseau,
/// caractère qui casserait l'argument `/DIR="…"`, dossier existant non vide.
pub fn target_dir(base: &Path, title: &str) -> Result<PathBuf, String> {
    let text = base.to_string_lossy();
    if !base.is_absolute() || text.starts_with(r"\\") || text.contains('"') {
        return Err(tr!("Dossier d'installation refusé : choisis un dossier sur un disque de ce PC.", "Install folder refused: choose a folder on a drive of this PC."));
    }
    let dir = base.join(crate::shortcut::file_stem(title));
    if std::fs::read_dir(&dir).is_ok_and(|mut d| d.next().is_some()) {
        return Err(tr!("Le dossier « {} » existe déjà et n'est pas vide.", "The folder \"{}\" already exists and isn't empty.", dir.display()));
    }
    Ok(dir)
}

/// Lance l'installeur vérifié (`ShellExecuteExW`, Windows demande l'autorisation administrateur si
/// besoin) et attend sa fin. Code de sortie d'Inno Setup : 0 = réussi.
#[cfg(windows)]
pub fn run_installer(exe: &Path, args: &str) -> Result<(), String> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    let file = HSTRING::from(exe.as_os_str());
    let params = HSTRING::from(args);
    let dir = HSTRING::from(exe.parent().unwrap_or(Path::new("")).as_os_str());
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI | SEE_MASK_NOCLOSEPROCESS,
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        lpDirectory: PCWSTR(dir.as_ptr()),
        nShow: 1,
        ..Default::default()
    };
    // SAFETY : `info` et les chaînes qu'il désigne vivent jusqu'au retour de l'appel.
    unsafe { ShellExecuteExW(&mut info) }.map_err(|e| {
        if e.code() == windows::core::HRESULT::from_win32(1223) {
            tr!("Installation annulée : Windows n'a pas reçu l'autorisation.", "Install canceled: Windows didn't get permission.")
        } else {
            tr!("Impossible de lancer l'installeur : {}", "Couldn't start the installer: {}", e.message())
        }
    })?;
    if info.hProcess.is_invalid() {
        return Ok(());
    }
    let mut code = 0u32;
    // SAFETY : poignée du processus rendue par `ShellExecuteExW`, fermée ensuite.
    unsafe {
        WaitForSingleObject(info.hProcess, INFINITE);
        let _ = GetExitCodeProcess(info.hProcess, &mut code);
        let _ = CloseHandle(info.hProcess);
    }
    match code {
        0 => Ok(()),
        2 | 5 => Err(tr!("Installation annulée.", "Install canceled.")),
        _ => Err(tr!("L'installeur s'est arrêté avec le code {}.", "The installer stopped with code {}.", code)),
    }
}

#[cfg(not(windows))]
pub fn run_installer(_: &Path, _: &str) -> Result<(), String> {
    Err(tr!("Installation disponible sous Windows seulement.", "Install only available on Windows."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Faux CDN en mémoire : chaque partie a son contenu ; `cut` coupe la première réponse après
    /// n octets ; `lie` annonce une autre taille.
    struct Fake {
        files: Vec<(String, Vec<u8>)>,
        cut: Mutex<Option<usize>>,
        lie: Option<u64>,
        opens: Mutex<Vec<u64>>,
        pause_after: Option<(Arc<Control>, usize)>,
    }

    struct Slow {
        data: Vec<u8>,
        pos: usize,
        stop_at: Option<usize>,
        control: Option<(Arc<Control>, usize)>,
    }

    impl Read for Slow {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if let Some((c, at)) = &self.control
                && self.pos >= *at
            {
                c.pause();
            }
            let end = self.stop_at.unwrap_or(self.data.len()).min(self.data.len());
            if self.pos >= end {
                return if self.stop_at.is_some() { Err(std::io::ErrorKind::ConnectionReset.into()) } else { Ok(0) };
            }
            let n = buf.len().min(end - self.pos).min(1000);
            buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
            self.pos += n;
            Ok(n)
        }
    }

    impl Source for Fake {
        fn link(&self, part: &Part, _first: bool) -> Result<Link, Failure> {
            let (name, _) = self.files.iter().find(|(n, _)| n.contains(&part.id)).ok_or(Failure::Status(404))?;
            Ok(Link { url: format!("https://cdn.gog.com/{name}"), name: name.clone() })
        }
        fn open(&self, link: &Link, start: u64) -> Result<Body, Failure> {
            self.opens.lock().unwrap().push(start);
            let (_, data) = self.files.iter().find(|(n, _)| *n == link.name).ok_or(Failure::Status(404))?;
            let cut = self.cut.lock().unwrap().take();
            Ok(Body {
                reader: Box::new(Slow {
                    data: data[start as usize..].to_vec(),
                    pos: 0,
                    stop_at: cut,
                    control: self.pause_after.as_ref().map(|(c, n)| (c.clone(), *n)),
                }),
                total: Some(self.lie.unwrap_or(data.len() as u64)),
                start,
            })
        }
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ludex-gog-dl-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn fake(lie: Option<u64>) -> (Fake, Vec<Part>) {
        let exe: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
        let bin: Vec<u8> = (0..12000u32).map(|i| (i % 13) as u8).collect();
        let parts = vec![Part { id: "en1installer0".into(), size: exe.len() as u64 }, Part { id: "en1installer1".into(), size: bin.len() as u64 }];
        let f = Fake {
            files: vec![("setup_jeu_en1installer0.exe".into(), exe), ("setup_jeu_en1installer1-1.bin".into(), bin)],
            cut: Mutex::new(None),
            lie,
            opens: Mutex::new(Vec::new()),
            pause_after: None,
        };
        (f, parts)
    }

    #[test]
    fn downloads_all_parts_and_resumes_after_a_cut() {
        let dir = temp("coupure");
        let (f, parts) = fake(None);
        *f.cut.lock().unwrap() = Some(3000);
        let control = Control::default();
        let mut last = 0;
        let exe = download(&f, &parts, &dir, &control, |_, got, _| last = got).unwrap();
        assert_eq!(exe, dir.join("setup_jeu_en1installer0.exe"));
        assert_eq!(std::fs::read(&exe).unwrap(), f.files[0].1);
        assert_eq!(std::fs::read(dir.join("setup_jeu_en1installer1-1.bin")).unwrap(), f.files[1].1);
        assert_eq!(last, 17000);
        // Coupure à 3000 octets : la reprise demande la suite (`Range`), pas le début.
        assert_eq!(*f.opens.lock().unwrap(), vec![0, 3000, 0]);
        assert!(!dir.join("setup_jeu_en1installer0.exe.part").exists());
        // Relancé : tout est déjà là, rien n'est redemandé.
        let again = download(&f, &parts, &dir, &control, |_, _, _| {}).unwrap();
        assert_eq!(again, exe);
        assert_eq!(f.opens.lock().unwrap().len(), 3);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pause_then_resume_continues_where_it_stopped() {
        let dir = temp("pause");
        let (mut f, parts) = fake(None);
        let control = Arc::new(Control::default());
        f.pause_after = Some((control.clone(), 2000));
        let f = Arc::new(f);
        let (c2, d2, f2, p2) = (control.clone(), dir.clone(), f.clone(), parts.clone());
        let phases = Arc::new(Mutex::new(Vec::new()));
        let seen = phases.clone();
        let worker = std::thread::spawn(move || download(f2.as_ref(), &p2, &d2, &c2, |phase, _, _| seen.lock().unwrap().push(phase)));
        // Attente de la pause, puis reprise.
        for _ in 0..200 {
            if phases.lock().unwrap().contains(&Phase::Paused) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(phases.lock().unwrap().contains(&Phase::Paused));
        control.resume();
        // La pause se repose à chaque lecture au-delà de 2000 octets du flux : on reprend en boucle.
        let mut result = None;
        for _ in 0..400 {
            control.resume();
            if worker.is_finished() {
                result = Some(worker.join().unwrap());
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let exe = result.expect("téléchargement fini").unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), f.files[0].1);
        let opens = f.opens.lock().unwrap().clone();
        assert!(opens.len() > 2 && opens[1] > 0, "reprise à la position atteinte : {opens:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn wrong_size_is_refused_and_cancel_stops() {
        let dir = temp("taille");
        let (f, parts) = fake(Some(999));
        let control = Control::default();
        assert!(matches!(download(&f, &parts, &dir, &control, |_, _, _| {}), Err(Stop::Failed(_))));
        let (f, mut parts) = fake(None);
        parts[0].size = 4000; // le serveur sert plus que l'annonce
        let f = Fake { lie: Some(4000), ..f };
        assert!(matches!(download(&f, &parts, &dir, &control, |_, _, _| {}), Err(Stop::Failed(_))));
        assert!(!dir.join("setup_jeu_en1installer0.exe.part").exists());
        let (f, parts) = fake(None);
        control.cancel();
        assert_eq!(download(&f, &parts, &dir, &control, |_, _, _| {}), Err(Stop::Cancelled));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn signer_and_target_rules() {
        assert!(signer_ok("GOG sp. z o.o."));
        assert!(signer_ok("GOG Sp. z o.o. "));
        assert!(signer_ok("GOG Limited"));
        assert!(!signer_ok("Microsoft Corporation"));
        assert!(!signer_ok("GOG sp. z o.o. (faux)"));
        assert!(!signer_ok(""));
        let base = temp("cible");
        std::fs::create_dir_all(&base).unwrap();
        let dir = target_dir(&base, "The Witcher: Enhanced Edition").unwrap();
        assert_eq!(dir, base.join("The Witcher Enhanced Edition"));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(target_dir(&base, "The Witcher: Enhanced Edition").is_ok(), "dossier vide accepté");
        std::fs::write(dir.join("x.txt"), b"x").unwrap();
        assert!(target_dir(&base, "The Witcher: Enhanced Edition").is_err());
        assert!(target_dir(Path::new(r"\\nas\jeux"), "Jeu").is_err());
        assert!(target_dir(Path::new("jeux"), "Jeu").is_err());
        assert_eq!(
            installer_args(Path::new(r"E:\Jeux\The Witcher"), "fr"),
            r#"/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP- /LANG=french /DIR="E:\Jeux\The Witcher""#
        );
        assert!(installer_args(Path::new(r"E:\Jeux\X"), "xx").contains("/LANG=english"));
        let _ = std::fs::remove_dir_all(base);
    }

    /// Signature réelle d'un fichier signé de ce PC (WebView2, signé par Microsoft) : valide, mais
    /// refusée parce que le signataire n'est pas GOG. Un faux exe sans signature est refusé.
    #[cfg(windows)]
    #[test]
    fn authenticode_reads_real_signer_and_refuses_unsigned() {
        let dir = temp("signature");
        std::fs::create_dir_all(&dir).unwrap();
        let fake_exe = dir.join("setup.exe");
        std::fs::write(&fake_exe, b"MZ pas un vrai exe").unwrap();
        assert_eq!(authenticode_signer(&fake_exe), None);
        assert!(verify_signature(&fake_exe).is_err());
        let roots = [r"C:\Program Files (x86)\Microsoft\EdgeWebView\Application", r"C:\Program Files\Microsoft\EdgeWebView\Application"];
        let signed = roots.iter().filter_map(|r| std::fs::read_dir(r).ok()).flatten().flatten().map(|e| e.path().join("msedgewebview2.exe")).find(|p| p.is_file());
        if let Some(signed) = signed {
            let name = authenticode_signer(&signed).expect("signature de Microsoft valide");
            assert_eq!(name, "Microsoft Corporation");
            let err = verify_signature(&signed).unwrap_err();
            assert!(err.contains("Microsoft Corporation"));
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
