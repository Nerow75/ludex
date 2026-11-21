//! Notifications Windows : téléchargement Steam terminé, actualité d'un jeu « En cours », disque
//! presque plein, mise à jour de Ludex, copie automatique des sauvegardes ratée, promo d'un jeu de
//! la liste de souhaits. Chaque type
//! s'active à part (`settings.notifications` dans `user-data.json`).
//!
//! Toasts WinRT par la crate `windows` (déjà là), pas `tauri-plugin-notification` : le plugin ne
//! sait rien faire d'un clic sous Windows. Identifiant `com.ludex.app` (`jumplist::APP_ID`, posé au
//! démarrage et repris par les raccourcis de l'installeur NSIS), déclaré dans
//! `HKCU\Software\Classes\AppUserModelId\com.ludex.app` (nom « Ludex » et icône) : sans raccourci du
//! menu Démarrer (debug, build de release non installée), Windows n'aurait ni nom ni icône à montrer.
//! Rien d'autre n'est écrit : aucun réglage de Windows n'est touché (notifications coupées pour
//! Ludex, « Ne pas déranger » : Windows décide, la page Paramètres le signale seulement).
//!
//! Règles, toutes côté Rust : rien quand la fenêtre principale a le focus (le toast de l'appli
//! suffit) ; une actualité seulement quand la fenêtre est cachée ou réduite (visible, le toast de
//! l'appli s'affiche) ; un même sujet (jeu, disque, version) pas plus d'une fois par délai
//! (`cooldown`), un disque au plus une fois par jour ; 4 notifications au plus par 10 minutes.
//! Les dates d'envoi sont gardées dans `notifications.json` (dossier de données), pour qu'un
//! redémarrage de Ludex ne répète pas le disque presque plein du jour.
//!
//! Clic : le fil des toasts garde les 20 dernières notifications ; leur événement `Activated`
//! affiche la fenêtre et envoie `notification-open` au front (fiche du jeu sur un onglet, section
//! Stockage, ou fenêtre seulement). Il n'arrive que tant que Ludex tourne.

use crate::library::InstallState;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{Sender, channel};
use std::time::Duration;

/// Événement `notification-open` : clic sur une notification de Ludex.
pub const OPEN_EVENT: &str = "notification-open";

/// Fenêtre glissante de la limite globale, en secondes, et nombre de notifications permis dedans.
const BURST_WINDOW: u64 = 10 * 60;
const BURST_MAX: usize = 4;
/// Dates d'envoi oubliées au-delà (le plus long délai est d'un jour).
const FORGET_AFTER: u64 = 2 * 24 * 3600;
/// Notifications gardées en mémoire pour leur clic.
#[cfg(windows)]
const KEEP_TOASTS: usize = 20;
/// Texte d'une notification coupé au-delà (Windows en affiche peu de toute façon).
const MAX_BODY: usize = 200;

/// Type de notification, tel que le front l'envoie (`set_notification`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Download,
    News,
    LowDisk,
    Update,
    SavesError,
    Sale,
}

impl Kind {
    pub const ALL: [Kind; 6] = [Kind::Download, Kind::News, Kind::LowDisk, Kind::Update, Kind::SavesError, Kind::Sale];

    /// Nom dans `user-data.json` (le même qu'en JSON pour le front).
    fn name(self) -> &'static str {
        match self {
            Kind::Download => "download",
            Kind::News => "news",
            Kind::LowDisk => "lowDisk",
            Kind::Update => "update",
            Kind::SavesError => "savesError",
            Kind::Sale => "sale",
        }
    }

    /// Délai minimal entre deux notifications sur le même sujet, en secondes.
    fn cooldown(self) -> u64 {
        match self {
            Kind::Download => 30 * 60,
            Kind::News => 12 * 3600,
            // La clé porte la date du jour : une fois par jour, quel que soit le délai.
            Kind::LowDisk => 24 * 3600,
            Kind::Update => 24 * 3600,
            Kind::SavesError => 3600,
            // Par jeu : une promo signalée ne l'est pas de nouveau le même jour.
            Kind::Sale => 24 * 3600,
        }
    }
}

/// Types activés. Tous actifs par défaut : ils sont rares et ne parlent que de ce PC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prefs {
    pub download: bool,
    pub news: bool,
    pub low_disk: bool,
    pub update: bool,
    pub saves_error: bool,
    pub sale: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs { download: true, news: true, low_disk: true, update: true, saves_error: true, sale: true }
    }
}

impl Prefs {
    pub fn get(&self, kind: Kind) -> bool {
        match kind {
            Kind::Download => self.download,
            Kind::News => self.news,
            Kind::LowDisk => self.low_disk,
            Kind::Update => self.update,
            Kind::SavesError => self.saves_error,
            Kind::Sale => self.sale,
        }
    }

    pub fn set(&mut self, kind: Kind, value: bool) {
        let slot = match kind {
            Kind::Download => &mut self.download,
            Kind::News => &mut self.news,
            Kind::LowDisk => &mut self.low_disk,
            Kind::Update => &mut self.update,
            Kind::SavesError => &mut self.saves_error,
            Kind::Sale => &mut self.sale,
        };
        *slot = value;
    }
}

/// Relu sans jamais échouer : type inconnu (version future) ignoré, valeur illisible = défaut.
/// Une erreur ici rendrait tout `user-data.json` illisible.
impl<'de> Deserialize<'de> for Prefs {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = serde_json::Value::deserialize(d)?;
        let mut prefs = Prefs::default();
        if let Some(map) = raw.as_object() {
            for kind in Kind::ALL {
                if let Some(value) = map.get(kind.name()).and_then(|v| v.as_bool()) {
                    prefs.set(kind, value);
                }
            }
        }
        Ok(prefs)
    }
}

/// Fenêtre principale au moment de notifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// À l'écran avec le focus : le toast de l'appli suffit.
    Focused,
    /// À l'écran sans le focus (derrière une autre appli).
    Visible,
    /// Cachée dans la zone de notification ou réduite.
    Hidden,
}

/// Une notification de ce type a-t-elle un sens dans cet état de la fenêtre ?
pub fn wanted(kind: Kind, presence: Presence) -> bool {
    match kind {
        // Fenêtre visible : l'annonce part en toast de l'appli, comme avant.
        Kind::News => presence == Presence::Hidden,
        _ => presence != Presence::Focused,
    }
}

/// Ce que le clic ouvre.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "target", rename_all = "camelCase")]
pub enum Target {
    /// Fiche d'un jeu, sur un onglet (`news`, `saves`) ou celui retenu.
    #[serde(rename_all = "camelCase")]
    Game { id: String, tab: Option<String> },
    /// Section Stockage.
    Storage,
    /// Section Souhaits.
    Wishlist,
    /// La fenêtre seulement.
    Window,
}

/// Une notification prête à partir.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    pub kind: Kind,
    /// Sujet, pour l'anti-doublon (`download:steam:730`, `lowDisk:E:@2026-11-20`).
    pub key: String,
    pub title: String,
    pub body: String,
    pub target: Target,
}

/// Téléchargement ou mise à jour fini : l'état passe à « prêt » depuis un autre état.
pub fn download_done(before: InstallState, after: InstallState) -> bool {
    after == InstallState::Ready && before != InstallState::Ready
}

/// Jeu Steam prêt après un téléchargement.
pub fn download_note(id: &str, title: &str, update: bool) -> Note {
    Note {
        kind: Kind::Download,
        key: format!("download:{id}"),
        title: format!("{title} est prêt"),
        body: if update { "Mise à jour Steam terminée." } else { "Téléchargement Steam terminé." }.into(),
        target: Target::Game { id: id.into(), tab: None },
    }
}

/// Plusieurs jeux Steam installés d'un coup (nouvelle bibliothèque, disque rebranché).
pub fn downloads_note(count: usize) -> Note {
    Note {
        kind: Kind::Download,
        key: "download:plusieurs".into(),
        title: format!("{count} jeux Steam installés"),
        body: "Ils sont dans ta bibliothèque.".into(),
        target: Target::Window,
    }
}

/// Annonces récentes de jeux « En cours » (mêmes textes que le toast `newsToast` du front).
pub fn news_note(alerts: &[crate::news::Alert]) -> Option<Note> {
    let first = alerts.first()?;
    let games = {
        let mut ids: Vec<&str> = alerts.iter().map(|a| a.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        ids.len()
    };
    let (title, body) = if games > 1 {
        (format!("Actualités de {games} jeux en cours"), format!("{} : {}", first.game, first.title))
    } else if first.update {
        (format!("Mise à jour de {}", first.game), first.title.clone())
    } else {
        (format!("Actualité de {}", first.game), first.title.clone())
    };
    Some(Note {
        kind: Kind::News,
        key: format!("news:{}:{}", first.id, first.gid),
        title,
        body,
        target: Target::Game { id: first.id.clone(), tab: Some("news".into()) },
    })
}

/// Disques presque pleins (moins de 10 % libres), une notification par disque et par jour
/// (`day` = date locale `AAAA-MM-JJ`).
pub fn low_disk_notes(disks: &[crate::storage::DiskUsage], day: &str) -> Vec<Note> {
    disks
        .iter()
        .filter(|d| d.low)
        .map(|d| {
            let percent = if d.total > 0 { d.free as f64 * 100.0 / d.total as f64 } else { 0.0 };
            Note {
                kind: Kind::LowDisk,
                key: format!("lowDisk:{}@{day}", d.letter),
                title: format!("Disque {} presque plein", d.letter),
                body: format!(
                    "{} libres sur {} ({} %). La section Stockage montre les jeux à désinstaller.",
                    gigabytes(d.free),
                    gigabytes(d.total),
                    decimal(percent)
                ),
                target: Target::Storage,
            }
        })
        .collect()
}

/// Mise à jour de Ludex trouvée.
pub fn update_note(version: &str) -> Note {
    Note {
        kind: Kind::Update,
        key: format!("update:{version}"),
        title: format!("Ludex {version} est disponible"),
        body: "Ouvre Ludex pour l'installer et le relancer.".into(),
        target: Target::Window,
    }
}

/// Copie automatique des sauvegardes ratée à la fin d'une partie.
pub fn saves_error_note(id: &str, title: &str, error: &str) -> Note {
    Note {
        kind: Kind::SavesError,
        key: format!("savesError:{id}"),
        title: format!("Sauvegardes de {title} non copiées"),
        body: error.into(),
        target: Target::Game { id: id.into(), tab: Some("saves".into()) },
    }
}

/// Promos nouvelles de jeux de la liste de souhaits (`wishlist.rs`).
pub fn sale_note(sales: &[crate::wishlist::Sale]) -> Option<Note> {
    let first = sales.first()?;
    let money = |cents: u32, s: &crate::wishlist::Sale| crate::wishlist::money(cents, &s.price.currency);
    let (key, title, body) = if sales.len() > 1 {
        let list: Vec<String> = sales.iter().take(3).map(|s| format!("{} -{}\u{a0}%", s.title, s.price.discount)).collect();
        let more = match sales.len() {
            n if n > 4 => format!(" et {} autres", n - 3),
            4 => " et 1 autre".to_string(),
            _ => String::new(),
        };
        (format!("sale:plusieurs:{}", today()), format!("{} jeux de ta liste en promo", sales.len()), format!("{}{more}.", list.join(", ")))
    } else {
        (
            format!("sale:{}", first.appid),
            format!("{} en promo : -{}\u{a0}%", first.title, first.price.discount),
            format!("{} au lieu de {}.", money(first.price.now, first), money(first.price.initial, first)),
        )
    };
    Some(Note { kind: Kind::Sale, key, title, body, target: Target::Wishlist })
}

/// « 1 118 Go » : gigaoctets entiers, milliers séparés par une espace insécable.
fn gigabytes(bytes: u64) -> String {
    let n = (bytes as f64 / 1e9).round() as u64;
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('\u{a0}');
        }
        out.push(c);
    }
    format!("{out}\u{a0}Go")
}

/// « 8,4 » : une décimale, virgule française.
fn decimal(value: f64) -> String {
    format!("{value:.1}").replace('.', ",")
}

/// Texte coupé à `max` caractères, sur une ligne.
fn shorten(text: &str, max: usize) -> String {
    let line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= max {
        return line;
    }
    let mut cut: String = line.chars().take(max - 1).collect();
    cut.push('…');
    cut
}

/// Contenu XML d'un toast (texte échappé, aucune image, aucun lien).
pub fn toast_xml(note: &Note) -> String {
    format!(
        "<toast activationType=\"foreground\" launch=\"ludex\"><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
        escape(&shorten(&note.title, 120)),
        escape(&shorten(&note.body, MAX_BODY))
    )
}

/// Échappement XML, caractères de contrôle retirés (refusés par le parseur de Windows).
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// Anti-doublon et limite globale. Pur : l'heure est passée en argument.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Limiter {
    /// Dernier envoi par sujet, en secondes Unix.
    sent: BTreeMap<String, u64>,
    /// Envois récents (limite globale), en mémoire seulement.
    #[serde(skip)]
    recent: VecDeque<u64>,
}

impl Limiter {
    /// Vrai si la notification peut partir ; l'envoi est alors noté.
    pub fn allow(&mut self, note: &Note, now: u64) -> bool {
        if self.sent.get(&note.key).is_some_and(|&at| now.saturating_sub(at) < note.kind.cooldown()) {
            return false;
        }
        while self.recent.front().is_some_and(|&at| now.saturating_sub(at) >= BURST_WINDOW) {
            self.recent.pop_front();
        }
        if self.recent.len() >= BURST_MAX {
            return false;
        }
        self.recent.push_back(now);
        self.sent.insert(note.key.clone(), now);
        self.sent.retain(|_, at| now.saturating_sub(*at) < FORGET_AFTER);
        true
    }

    fn load(path: &Path) -> Limiter {
        std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    /// Écrit dans un fichier temporaire puis le renomme.
    fn save(&self, path: &Path) -> Result<(), String> {
        let text = serde_json::to_string(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Date locale du jour, `AAAA-MM-JJ`.
pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

enum Request {
    Show(Note),
    /// Windows bloque-t-il les notifications de Ludex ?
    Probe(Sender<bool>),
}

/// Notifications de Ludex : limiteur et fil des toasts. Géré par Tauri (`app.manage`).
pub struct Notifier {
    tx: Option<Sender<Request>>,
    limiter: Mutex<Limiter>,
    path: PathBuf,
}

impl Notifier {
    /// Lance le fil des toasts. `on_click` reçoit la cible d'une notification cliquée (appelé
    /// depuis un fil de Windows).
    pub fn spawn(data_dir: &Path, on_click: impl Fn(Target) + Send + Sync + 'static) -> Notifier {
        let path = data_dir.join("notifications.json");
        let limiter = Mutex::new(Limiter::load(&path));
        let (tx, rx) = channel::<Request>();
        let icon = data_dir.join("notification-icon.png");
        let spawned = std::thread::Builder::new().name("ludex-notifications".into()).spawn(move || toasts::run(rx, &icon, on_click));
        let tx = match spawned {
            Ok(_) => Some(tx),
            Err(e) => {
                log::warn!("Notifications Windows indisponibles : {e}");
                None
            }
        };
        Notifier { tx, limiter, path }
    }

    /// Envoie la notification si le limiteur l'accepte. `false` : rien n'est parti.
    pub fn show(&self, note: Note) -> bool {
        let Some(tx) = &self.tx else { return false };
        {
            let Ok(mut limiter) = self.limiter.lock() else { return false };
            if !limiter.allow(&note, now()) {
                return false;
            }
            if let Err(e) = limiter.save(&self.path) {
                log::error!("Dates des notifications non enregistrées : {e}");
            }
        }
        tx.send(Request::Show(note)).is_ok()
    }

    /// Windows a coupé les notifications de Ludex (ou toutes) : affiché dans les Paramètres.
    pub fn blocked(&self) -> bool {
        let Some(tx) = &self.tx else { return false };
        let (reply, answer) = channel();
        if tx.send(Request::Probe(reply)).is_err() {
            return false;
        }
        answer.recv_timeout(Duration::from_millis(500)).unwrap_or(false)
    }
}

#[cfg(windows)]
mod toasts {
    use super::{KEEP_TOASTS, Note, Request, Target};
    use std::collections::VecDeque;
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::mpsc::Receiver;
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::Foundation::TypedEventHandler;
    use windows::UI::Notifications::{NotificationSetting, ToastNotification, ToastNotificationManager, ToastNotifier};
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
    use windows::core::{HSTRING, IInspectable, Result};

    /// Icône des notifications (celle de l'app), écrite dans le dossier de données.
    const ICON: &[u8] = include_bytes!("../icons/128x128.png");

    pub(super) fn run(rx: Receiver<Request>, icon: &Path, on_click: impl Fn(Target) + Send + Sync + 'static) {
        // SAFETY : appel sans pointeur conservé ; le fil garde son appartement jusqu'à sa fin.
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let on_click = Arc::new(on_click);
        let mut notifier: Option<ToastNotifier> = None;
        let mut kept: VecDeque<ToastNotification> = VecDeque::new();
        let mut registered = false;
        for request in rx {
            // Déclaré au premier envoi seulement : ouvrir les Paramètres n'écrit rien.
            if !registered && matches!(request, Request::Show(_)) {
                register(icon);
                registered = true;
            }
            if notifier.is_none() {
                match ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(crate::jumplist::APP_ID)) {
                    Ok(n) => notifier = Some(n),
                    Err(e) => log::warn!("Notifications Windows indisponibles : {e}"),
                }
            }
            let Some(n) = &notifier else {
                if let Request::Probe(reply) = request {
                    let _ = reply.send(false);
                }
                continue;
            };
            match request {
                Request::Probe(reply) => {
                    let _ = reply.send(n.Setting().is_ok_and(|s| s != NotificationSetting::Enabled));
                }
                Request::Show(note) => match show(n, &note, on_click.clone()) {
                    Ok(toast) => {
                        kept.push_back(toast);
                        if kept.len() > KEEP_TOASTS {
                            kept.pop_front();
                        }
                    }
                    Err(e) => log::warn!("Notification « {} » non affichée : {e}", note.title),
                },
            }
        }
    }

    pub(super) fn show(notifier: &ToastNotifier, note: &Note, on_click: Arc<impl Fn(Target) + Send + Sync + 'static>) -> Result<ToastNotification> {
        let doc = XmlDocument::new()?;
        doc.LoadXml(&HSTRING::from(super::toast_xml(note)))?;
        let toast = ToastNotification::CreateToastNotification(&doc)?;
        // Même sujet : la nouvelle notification remplace l'ancienne dans le centre de notifications.
        toast.SetTag(&HSTRING::from(format!("{:016x}", crate::images::fnv(&note.key))))?;
        toast.SetGroup(&HSTRING::from("ludex"))?;
        let target = note.target.clone();
        toast.Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(move |_, _| {
            on_click(target.clone());
            Ok(())
        }))?;
        notifier.Show(&toast)?;
        Ok(toast)
    }

    /// Nom et icône de `com.ludex.app` pour le centre de notifications (clé de l'utilisateur,
    /// comme le fait le Windows App SDK pour une appli non empaquetée). Écrit seulement si différent.
    fn register(icon: &Path) {
        if std::fs::read(icon).ok().as_deref() != Some(ICON)
            && let Err(e) = std::fs::write(icon, ICON)
        {
            log::warn!("Icône des notifications non écrite : {e}");
        }
        let key = format!(r"Software\Classes\AppUserModelId\{}", crate::jumplist::APP_ID);
        let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
        let result = hkcu.create_subkey(&key).and_then(|(k, _)| {
            let icon = icon.display().to_string();
            if k.get_value::<String, _>("DisplayName").ok().as_deref() != Some("Ludex") {
                k.set_value("DisplayName", &"Ludex")?;
            }
            if k.get_value::<String, _>("IconUri").ok().as_deref() != Some(icon.as_str()) {
                k.set_value("IconUri", &icon)?;
            }
            Ok(())
        });
        if let Err(e) = result {
            log::warn!("Identifiant des notifications non déclaré : {e}");
        }
    }
}

#[cfg(not(windows))]
mod toasts {
    use super::{Request, Target};
    use std::path::Path;
    use std::sync::mpsc::Receiver;

    pub(super) fn run(rx: Receiver<Request>, _icon: &Path, _on_click: impl Fn(Target) + Send + Sync + 'static) {
        for request in rx {
            if let Request::Probe(reply) = request {
                let _ = reply.send(false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::news::Alert;
    use crate::storage::DiskUsage;

    fn note(kind: Kind, key: &str) -> Note {
        Note { kind, key: key.into(), title: "Titre".into(), body: "Texte".into(), target: Target::Window }
    }

    #[test]
    fn prefs_are_all_on_by_default_and_read_without_failing() {
        assert_eq!(Prefs::default(), Prefs { download: true, news: true, low_disk: true, update: true, saves_error: true, sale: true });
        let p: Prefs = serde_json::from_str(r#"{"news": false, "lowDisk": false, "promos": true, "update": "oui"}"#).unwrap();
        assert!(!p.news && !p.low_disk, "types connus relus");
        assert!(p.update, "valeur illisible : défaut");
        assert!(p.download && p.saves_error);
        let p: Prefs = serde_json::from_str("42").unwrap();
        assert_eq!(p, Prefs::default(), "autre chose qu'un objet : défaut");
        let text = serde_json::to_string(&Prefs { low_disk: false, ..Prefs::default() }).unwrap();
        assert!(text.contains(r#""lowDisk":false"#) && text.contains(r#""savesError":true"#), "{text}");
    }

    #[test]
    fn prefs_set_each_kind() {
        let mut p = Prefs::default();
        for kind in Kind::ALL {
            p.set(kind, false);
            assert!(!p.get(kind));
        }
        assert_eq!(serde_json::from_str::<Kind>(r#""savesError""#).unwrap(), Kind::SavesError);
        assert!(serde_json::from_str::<Kind>(r#""promos""#).is_err(), "type inconnu refusé par la commande");
    }

    #[test]
    fn nothing_when_focused_and_news_only_when_hidden() {
        for kind in Kind::ALL {
            assert!(!wanted(kind, Presence::Focused), "{kind:?}");
            assert!(wanted(kind, Presence::Hidden), "{kind:?}");
        }
        assert!(!wanted(Kind::News, Presence::Visible), "fenêtre visible : toast de l'appli");
        assert!(wanted(Kind::Download, Presence::Visible));
        assert!(wanted(Kind::LowDisk, Presence::Visible));
    }

    #[test]
    fn download_done_only_when_becoming_ready() {
        let updating = InstallState::Updating { progress: Some(80) };
        assert!(download_done(updating, InstallState::Ready));
        assert!(download_done(InstallState::Paused { progress: None }, InstallState::Ready));
        assert!(download_done(InstallState::Queued, InstallState::Ready));
        assert!(!download_done(InstallState::Ready, InstallState::Ready), "rien n'a changé");
        assert!(!download_done(InstallState::Queued, updating), "pas fini");
        assert!(!download_done(InstallState::Ready, InstallState::UpdateRequired));
    }

    #[test]
    fn same_subject_waits_for_its_cooldown() {
        let mut l = Limiter::default();
        let n = note(Kind::Download, "download:steam:730");
        assert!(l.allow(&n, 1000));
        assert!(!l.allow(&n, 1000 + 29 * 60), "même jeu, moins de 30 min");
        assert!(l.allow(&n, 1000 + 31 * 60));
        assert!(l.allow(&note(Kind::Download, "download:steam:570"), 1000 + 31 * 60), "autre jeu");
    }

    #[test]
    fn a_disk_once_per_day() {
        let disk = DiskUsage {
            letter: "E:".into(),
            name: "4To Jeux".into(),
            total: 4_001_000_000_000,
            free: 112_000_000_000,
            games_bytes: 0,
            games: 3,
            unknown: 0,
            platforms: Vec::new(),
            low: true,
        };
        let fine = DiskUsage { letter: "C:".into(), free: 1_500_000_000_000, low: false, ..disk.clone() };
        let notes = low_disk_notes(&[disk.clone(), fine], "2026-11-20");
        assert_eq!(notes.len(), 1, "seul le disque presque plein");
        assert_eq!(notes[0].title, "Disque E: presque plein");
        assert!(notes[0].body.starts_with("112\u{a0}Go libres sur 4\u{a0}001\u{a0}Go (2,8 %)"), "{}", notes[0].body);
        assert_eq!(notes[0].target, Target::Storage);
        let mut l = Limiter::default();
        let morning = 1_000_000;
        assert!(l.allow(&notes[0], morning));
        assert!(!l.allow(&low_disk_notes(std::slice::from_ref(&disk), "2026-11-20")[0], morning + 6 * 3600), "même jour");
        assert!(l.allow(&low_disk_notes(&[disk], "2026-11-21")[0], morning + 6 * 3600), "lendemain (après minuit)");
    }

    #[test]
    fn at_most_four_in_ten_minutes() {
        let mut l = Limiter::default();
        for i in 0..4 {
            assert!(l.allow(&note(Kind::SavesError, &format!("savesError:{i}")), 100 + i));
        }
        assert!(!l.allow(&note(Kind::SavesError, "savesError:9"), 200), "cinquième refusée");
        assert!(l.allow(&note(Kind::SavesError, "savesError:9"), 100 + BURST_WINDOW), "fenêtre passée");
    }

    #[test]
    fn refused_note_is_not_recorded() {
        let mut l = Limiter::default();
        for i in 0..4 {
            assert!(l.allow(&note(Kind::Update, &format!("update:{i}")), 0));
        }
        assert!(!l.allow(&note(Kind::Update, "update:9"), 1));
        assert!(l.allow(&note(Kind::Update, "update:9"), BURST_WINDOW + 1), "le refus n'a rien noté");
    }

    #[test]
    fn limiter_dates_survive_a_restart_and_old_ones_are_forgotten() {
        let dir = std::env::temp_dir().join(format!("ludex-notify-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("notifications.json");
        let mut l = Limiter::default();
        assert!(l.allow(&note(Kind::LowDisk, "lowDisk:E:@2026-11-20"), 10));
        l.save(&path).unwrap();
        let mut back = Limiter::load(&path);
        assert!(!back.allow(&note(Kind::LowDisk, "lowDisk:E:@2026-11-20"), 20), "relu après redémarrage");
        assert!(back.allow(&note(Kind::Update, "update:1"), 10 + FORGET_AFTER));
        assert!(!back.sent.contains_key("lowDisk:E:@2026-11-20"), "oublié après deux jours");
        std::fs::write(&path, "pas du json").unwrap();
        assert!(Limiter::load(&path).sent.is_empty(), "fichier illisible : vide");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn alert(id: &str, game: &str, title: &str, update: bool) -> Alert {
        Alert { id: id.into(), game: game.into(), title: title.into(), gid: "123".into(), update }
    }

    #[test]
    fn news_texts_match_the_app_toast() {
        let one = news_note(&[alert("steam:252950", "Rocket League", "Patch v2.50", true)]).unwrap();
        assert_eq!((one.title.as_str(), one.body.as_str()), ("Mise à jour de Rocket League", "Patch v2.50"));
        assert_eq!(one.target, Target::Game { id: "steam:252950".into(), tab: Some("news".into()) });
        let plain = news_note(&[alert("steam:1", "Terraria", "Concours", false)]).unwrap();
        assert_eq!(plain.title, "Actualité de Terraria");
        let many = news_note(&[alert("steam:1", "Terraria", "Concours", false), alert("steam:2", "CS2", "Patch", true), alert("steam:1", "Terraria", "Autre", false)]).unwrap();
        assert_eq!(many.title, "Actualités de 2 jeux en cours");
        assert_eq!(many.body, "Terraria : Concours");
        assert!(news_note(&[]).is_none());
    }

    #[test]
    fn sale_texts() {
        use crate::wishlist::{Price, Sale};
        let sale = |appid: u32, title: &str, discount: u8| Sale {
            appid,
            title: title.into(),
            price: Price { currency: "EUR".into(), initial: 3999, now: 799, discount },
        };
        let one = sale_note(&[sale(1426210, "It Takes Two", 80)]).unwrap();
        assert_eq!(one.title, "It Takes Two en promo : -80\u{a0}%");
        assert_eq!(one.body, "7,99\u{a0}€ au lieu de 39,99\u{a0}€.");
        assert_eq!((one.key.as_str(), &one.target), ("sale:1426210", &Target::Wishlist));
        let many = sale_note(&[sale(1, "A", 10), sale(2, "B", 20), sale(3, "C", 30), sale(4, "D", 40)]).unwrap();
        assert_eq!(many.title, "4 jeux de ta liste en promo");
        assert_eq!(many.body, "A -10\u{a0}%, B -20\u{a0}%, C -30\u{a0}% et 1 autre.");
        assert!(sale_note(&[]).is_none());
        assert!(wanted(Kind::Sale, Presence::Visible) && !wanted(Kind::Sale, Presence::Focused));
    }

    #[test]
    fn other_texts() {
        let d = download_note("steam:730", "Counter-Strike 2", true);
        assert_eq!((d.title.as_str(), d.body.as_str()), ("Counter-Strike 2 est prêt", "Mise à jour Steam terminée."));
        assert_eq!(download_note("steam:730", "CS2", false).body, "Téléchargement Steam terminé.");
        assert_eq!(downloads_note(3).title, "3 jeux Steam installés");
        assert_eq!(update_note("0.3.0").title, "Ludex 0.3.0 est disponible");
        let s = saves_error_note("steam:105600", "Terraria", "Disque plein.");
        assert_eq!(s.target, Target::Game { id: "steam:105600".into(), tab: Some("saves".into()) });
        assert_eq!(s.key, "savesError:steam:105600");
    }

    #[test]
    fn toast_xml_is_escaped_and_short() {
        let n = Note {
            kind: Kind::News,
            key: "k".into(),
            title: "Tom & Jerry <\"2\">".into(),
            body: format!("ligne\u{7}\nsuite {}", "x".repeat(400)),
            target: Target::Window,
        };
        let xml = toast_xml(&n);
        assert!(xml.contains("<text>Tom &amp; Jerry &lt;&quot;2&quot;&gt;</text>"), "{xml}");
        assert!(xml.contains("<text>ligne suite xxx"), "contrôles retirés, une ligne : {xml}");
        assert!(xml.contains("…</text>"), "coupé");
        assert!(!xml.contains('\u{7}'));
    }

    #[test]
    fn target_is_sent_in_camel_case() {
        let t = serde_json::to_string(&Target::Game { id: "steam:1".into(), tab: Some("news".into()) }).unwrap();
        assert_eq!(t, r#"{"target":"game","id":"steam:1","tab":"news"}"#);
        assert_eq!(serde_json::to_string(&Target::Storage).unwrap(), r#"{"target":"storage"}"#);
    }

    /// Montre une seule vraie notification d'essai (et déclare `com.ludex.app` dans la clé de
    /// l'utilisateur, comme Ludex au premier envoi). Cliquer dessus dans les 20 s affiche le clic.
    #[test]
    #[ignore]
    fn notifications_this_pc() {
        let dir = std::env::temp_dir().join("ludex-notifications-this-pc");
        std::fs::create_dir_all(&dir).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = Mutex::new(tx);
        // Même identifiant que Ludex, pour que Windows montre son nom et son icône.
        #[cfg(windows)]
        {
            let id = windows::core::HSTRING::from(crate::jumplist::APP_ID);
            // SAFETY : appel sans pointeur conservé.
            let _ = unsafe { windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(&id) };
        }
        let n = Notifier::spawn(&dir, move |t| {
            let _ = tx.lock().map(|tx| tx.send(t));
        });
        println!("Notifications bloquées par Windows : {}", n.blocked());
        let started = std::time::Instant::now();
        let shown = n.show(Note {
            kind: Kind::Update,
            key: format!("essai:{}", now()),
            title: "Essai de Ludex".into(),
            body: "Notification d'essai : clique dessus.".into(),
            target: Target::Storage,
        });
        println!("Envoyée : {shown}");
        match rx.recv_timeout(Duration::from_secs(20)) {
            Ok(t) => println!("Cliquée après {:.1} s : {t:?}", started.elapsed().as_secs_f32()),
            Err(_) => println!("Pas de clic en 20 s."),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
