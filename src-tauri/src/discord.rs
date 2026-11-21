//! Statut Discord (Rich Presence) des parties lancées depuis Ludex.
//!
//! Discord ne reconnaît pas tout seul beaucoup de jeux (« Autres », Riot, Xbox…) : Ludex lui dit
//! ce qui tourne, d'après son propre suivi des sessions (`session.rs`). Connexion au client Discord
//! de ce PC par son tube nommé local (`\\.\pipe\discord-ipc-0` à `-9`) : aucun réseau, aucun compte,
//! aucun jeton. Protocole : trames `[opcode u32 LE][longueur u32 LE][JSON]`, poignée de main
//! (`{"v":1,"client_id":…}`, Discord répond `READY`), puis `SET_ACTIVITY` (activité, ou `null` pour
//! l'effacer). Discord efface aussi l'activité dès que la connexion se ferme ou que le processus
//! nommé par `pid` s'arrête : Ludex quitté ou planté, plus rien n'est affiché.
//!
//! Désactivé tant que `DISCORD_APP_ID` est vide (application à créer par le propriétaire de Ludex
//! dans le Discord Developer Portal, voir `CLAUDE.md`) et tant que le réglage `discordPresence` est
//! faux (par défaut : choix de l'utilisateur pour sa vie privée).
//!
//! Tout se passe dans un fil à part (`ludex-discord`) : le fil des sessions ne fait qu'envoyer un
//! message. Discord fermé ou absent : nouvel essai au plus toutes les 30 s, et seulement pendant
//! une partie ; la connexion est fermée dès qu'il n'y a plus rien à montrer.

use crate::library::{Game, Platform};
use crate::session::Phase;
use serde::Serialize;
use serde_json::{Value, json};
use std::io::{self, Read, Write};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Identifiant de l'application Discord « Ludex » (Discord Developer Portal > Applications >
/// Ludex > General Information > Application ID). Vide : statut Discord désactivé partout.
pub const DISCORD_APP_ID: &str = "";

/// Nom de l'image du logo téléversée dans Rich Presence > Art Assets de l'application.
pub const LOGO_ASSET: &str = "ludex";

/// Au plus une tentative de connexion pendant ce délai.
pub const RETRY: Duration = Duration::from_secs(30);
/// Attente maximale d'une réponse de Discord (poignée de main, activité).
const REPLY_TIMEOUT: Duration = Duration::from_secs(2);
/// Trame refusée au-delà (les réponses de Discord font quelques Ko).
const MAX_FRAME: usize = 64 * 1024;
/// Tubes essayés : `discord-ipc-0` à `discord-ipc-9` (plusieurs clients Discord possibles).
const PIPES: u32 = 10;
/// Longueur maximale d'un texte de l'activité (Discord en accepte 128).
const MAX_TEXT: usize = 120;

const OP_HANDSHAKE: u32 = 0;
const OP_FRAME: u32 = 1;
const OP_CLOSE: u32 = 2;
const OP_PING: u32 = 3;
const OP_PONG: u32 = 4;

/// Le statut est-il possible dans cette version de Ludex (identifiant d'application renseigné) ?
pub fn available() -> bool {
    !DISCORD_APP_ID.is_empty()
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

// --- Trames ---------------------------------------------------------------------------------

/// Trame complète : opcode et longueur en petit-boutiste, puis le JSON.
pub fn encode(op: u32, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(8 + payload.len());
    frame.extend_from_slice(&op.to_le_bytes());
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(payload);
    frame
}

/// Opcode et longueur d'un en-tête de 8 octets.
pub fn header(bytes: [u8; 8]) -> (u32, usize) {
    let op = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let len = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    (op, len as usize)
}

/// Erreur d'un échange avec Discord.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Connexion impossible ou perdue (Discord fermé, redémarré, ne répond plus).
    Io(String),
    /// Discord a refusé (identifiant d'application inconnu, activité invalide).
    Refused(String),
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e.to_string())
    }
}

/// Tube vers Discord : lecture, écriture, et octets déjà arrivés (pour ne jamais bloquer).
pub trait Pipe: Read + Write {
    fn available(&mut self) -> io::Result<usize>;
}

/// Attend que `n` octets soient arrivés, au plus jusqu'à `deadline`.
fn wait_for(pipe: &mut dyn Pipe, n: usize, deadline: Instant) -> Result<(), Error> {
    loop {
        if pipe.available()? >= n {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(Error::Io("Discord ne répond pas.".into()));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Lit une trame entière.
fn read_frame(pipe: &mut dyn Pipe, deadline: Instant) -> Result<(u32, Value), Error> {
    wait_for(pipe, 8, deadline)?;
    let mut head = [0u8; 8];
    pipe.read_exact(&mut head)?;
    let (op, len) = header(head);
    if len > MAX_FRAME {
        return Err(Error::Io("Réponse de Discord trop longue.".into()));
    }
    wait_for(pipe, len, deadline)?;
    let mut body = vec![0u8; len];
    pipe.read_exact(&mut body)?;
    let value = serde_json::from_slice(&body).map_err(|_| Error::Io("Réponse de Discord illisible.".into()))?;
    Ok((op, value))
}

fn send(pipe: &mut dyn Pipe, op: u32, value: &Value) -> Result<(), Error> {
    pipe.write_all(&encode(op, value.to_string().as_bytes()))?;
    pipe.flush()?;
    Ok(())
}

/// Message d'une trame de fermeture ou d'erreur de Discord.
fn message_of(value: &Value) -> String {
    let text = value.get("message").or_else(|| value.pointer("/data/message")).and_then(Value::as_str).unwrap_or("raison inconnue");
    text.chars().take(200).collect()
}

/// Connexion établie (poignée de main faite).
pub struct Client {
    pipe: Box<dyn Pipe>,
    nonce: u64,
}

impl Client {
    /// Poignée de main : Discord répond `READY`, ou ferme avec une raison (identifiant inconnu).
    pub fn handshake(mut pipe: Box<dyn Pipe>, client_id: &str) -> Result<Client, Error> {
        send(pipe.as_mut(), OP_HANDSHAKE, &json!({ "v": 1, "client_id": client_id }))?;
        let deadline = Instant::now() + REPLY_TIMEOUT;
        loop {
            match read_frame(pipe.as_mut(), deadline)? {
                (OP_FRAME, v) if v.get("evt").and_then(Value::as_str) == Some("READY") => return Ok(Client { pipe, nonce: 0 }),
                (OP_CLOSE, v) => return Err(Error::Refused(message_of(&v))),
                (OP_PING, v) => send(pipe.as_mut(), OP_PONG, &v)?,
                (OP_FRAME, v) if v.get("evt").and_then(Value::as_str) == Some("ERROR") => return Err(Error::Refused(message_of(&v))),
                _ => {}
            }
        }
    }

    /// Pose (ou efface, `None`) l'activité, et attend la réponse qui porte le même `nonce`.
    pub fn set_activity(&mut self, pid: u32, activity: Option<&Value>) -> Result<(), Error> {
        self.nonce += 1;
        let nonce = format!("ludex-{}", self.nonce);
        let command = json!({ "cmd": "SET_ACTIVITY", "args": { "pid": pid, "activity": activity }, "nonce": nonce });
        send(self.pipe.as_mut(), OP_FRAME, &command)?;
        let deadline = Instant::now() + REPLY_TIMEOUT;
        loop {
            match read_frame(self.pipe.as_mut(), deadline)? {
                (OP_FRAME, v) if v.get("nonce").and_then(Value::as_str) == Some(nonce.as_str()) => {
                    return if v.get("evt").and_then(Value::as_str) == Some("ERROR") { Err(Error::Refused(message_of(&v))) } else { Ok(()) };
                }
                (OP_CLOSE, v) => return Err(Error::Io(format!("Discord a fermé la connexion : {}", message_of(&v)))),
                (OP_PING, v) => send(self.pipe.as_mut(), OP_PONG, &v)?,
                _ => {}
            }
        }
    }
}

// --- Tube nommé de Windows --------------------------------------------------------------------

#[cfg(windows)]
struct NamedPipe(std::fs::File);

#[cfg(windows)]
impl Read for NamedPipe {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

#[cfg(windows)]
impl Write for NamedPipe {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

#[cfg(windows)]
impl Pipe for NamedPipe {
    fn available(&mut self) -> io::Result<usize> {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::Pipes::PeekNamedPipe;
        let mut avail = 0u32;
        // Lecture du nombre d'octets en attente seulement, sans rien retirer du tube.
        unsafe { PeekNamedPipe(HANDLE(self.0.as_raw_handle()), None, 0, None, Some(&mut avail), None) }.map_err(io::Error::other)?;
        Ok(avail as usize)
    }
}

/// Ouvre le premier tube `<base>-0` à `<base>-9` qui répond.
#[cfg(windows)]
pub fn open_pipe(base: &str) -> io::Result<Box<dyn Pipe>> {
    let mut last = io::Error::new(io::ErrorKind::NotFound, "Discord n'est pas ouvert.");
    for n in 0..PIPES {
        match std::fs::OpenOptions::new().read(true).write(true).open(format!("{base}-{n}")) {
            Ok(file) => return Ok(Box::new(NamedPipe(file))),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => last = e,
        }
    }
    Err(last)
}

#[cfg(not(windows))]
pub fn open_pipe(_base: &str) -> io::Result<Box<dyn Pipe>> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "Statut Discord disponible sous Windows seulement."))
}

/// Tubes du client Discord.
const DISCORD_PIPE: &str = r"\\.\pipe\discord-ipc";

// --- Contenu de l'activité ----------------------------------------------------------------------

/// Ce qui est montré d'un jeu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shown {
    pub title: String,
    pub platform: Platform,
    pub steam_appid: Option<u32>,
}

impl Shown {
    pub fn of(game: &Game) -> Self {
        Shown { title: game.title.clone(), platform: game.platform, steam_appid: game.steam_appid }
    }
}

/// Ligne sous le nom du jeu : le launcher.
pub fn launcher_line(platform: Platform) -> &'static str {
    match platform {
        Platform::Steam => "Sur Steam",
        Platform::Epic => "Sur Epic Games",
        Platform::Gog => "Sur GOG",
        Platform::Ubisoft => "Sur Ubisoft Connect",
        Platform::Ea => "Sur l'appli EA",
        Platform::Battlenet => "Sur Battle.net",
        Platform::Rockstar => "Sur Rockstar Games",
        Platform::Xbox => "Sur Xbox",
        Platform::Riot => "Sur Riot Games",
        Platform::Amazon => "Sur Amazon Games",
        Platform::Itch => "Sur itch.io",
        Platform::Other => "Lancé depuis Ludex",
    }
}

/// Texte accepté par Discord : contrôles retirés, espaces regroupés, 120 caractères au plus,
/// deux au moins (en dessous, Discord refuse l'activité entière).
pub fn field(text: &str) -> String {
    let clean = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ");
    let count = clean.chars().count();
    if count > MAX_TEXT {
        let cut: String = clean.chars().take(MAX_TEXT - 1).collect();
        return format!("{}…", cut.trim_end());
    }
    match count {
        0 => "Un jeu".to_string(),
        1 => format!("Jeu {clean}"),
        _ => clean,
    }
}

/// Image du jeu : en-tête de la boutique Steam, adresse publique https reconstruite depuis l'appid
/// (même forme que la liste de souhaits, répond aussi pour les jeux aux images hachées). Jamais
/// une image locale (Discord ne peut pas la lire), jamais une adresse venue d'ailleurs.
pub fn cover_url(appid: u32) -> String {
    format!("https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/{appid}/header.jpg")
}

/// Activité envoyée à Discord. `start` en secondes Unix (compteur « écoulé » de Discord).
/// `rich` : forme complète (nom du jeu dans la liste des membres par `status_display_type`, image
/// du jeu) ; faux après un refus de Discord, logo et textes seulement.
pub fn activity(shown: Option<&Shown>, start: u64, hide_name: bool, rich: bool) -> Value {
    let timestamps = json!({ "start": start.saturating_mul(1000) });
    let Some(shown) = shown.filter(|_| !hide_name) else {
        // Nom caché (ou jeu pas encore relu) : ni titre, ni launcher, ni image du jeu.
        return json!({
            "details": "En jeu",
            "timestamps": timestamps,
            "assets": { "large_image": LOGO_ASSET, "large_text": "Ludex" },
            "instance": false,
        });
    };
    let title = field(&shown.title);
    let mut value = json!({
        "details": title,
        "state": launcher_line(shown.platform),
        "timestamps": timestamps,
        "assets": { "large_image": LOGO_ASSET, "large_text": "Ludex" },
        "instance": false,
    });
    if rich {
        // « Joue à Hades » dans la liste des membres, plutôt que « Joue à Ludex ».
        value["status_display_type"] = json!(2);
        if let Some(appid) = shown.steam_appid.filter(|&a| a > 0) {
            value["assets"] = json!({ "large_image": cover_url(appid), "large_text": title, "small_image": LOGO_ASSET, "small_text": "Ludex" });
        }
    }
    value
}

// --- État pour le Diagnostic -----------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum State {
    /// Identifiant d'application absent de cette version de Ludex.
    Unconfigured,
    /// Réglage coupé.
    #[default]
    Off,
    /// Activé, aucune partie en cours.
    Waiting,
    /// Partie affichée dans Discord.
    Connected,
    /// Discord fermé ou absent de ce PC.
    Absent,
    /// Discord a refusé (identifiant ou activité).
    Refused,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub state: State,
    pub error: Option<String>,
}

impl Status {
    /// État en une ligne, pour le rapport du Diagnostic.
    pub fn text(&self) -> String {
        match self.state {
            State::Unconfigured => "pas configuré dans cette version de Ludex".to_string(),
            State::Off => "désactivé dans les Paramètres".to_string(),
            State::Waiting => "activé, aucune partie en cours".to_string(),
            State::Connected => "connecté, partie affichée".to_string(),
            State::Absent => format!("Discord absent ou fermé ({})", self.error.as_deref().unwrap_or("aucun tube")),
            State::Refused => format!("refusé par Discord : {}", self.error.as_deref().unwrap_or("raison inconnue")),
        }
    }
}

// --- Machine à états ------------------------------------------------------------------------

/// Réglages lus à chaque rafraîchissement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Prefs {
    pub enabled: bool,
    pub hide_name: bool,
}

#[derive(Clone, Debug)]
struct Playing {
    id: String,
    start: u64,
    shown: Option<Shown>,
}

/// Ouvre une connexion (tubes de Discord, ou faux tubes dans les tests).
pub type Connect<'a> = dyn FnMut() -> io::Result<Box<dyn Pipe>> + 'a;

/// Ce que Discord doit montrer, et ce qu'il montre.
pub struct Core {
    client_id: String,
    pid: u32,
    prefs: Prefs,
    /// Parties en cours, dans l'ordre où elles ont commencé : la dernière est montrée.
    playing: Vec<Playing>,
    client: Option<Client>,
    /// Activité que Discord montre (d'après sa dernière réponse), `None` = rien.
    sent: Option<Value>,
    last_attempt: Option<Instant>,
    /// Forme complète de l'activité ; coupée après un refus de Discord.
    rich: bool,
    status: Status,
}

impl Core {
    pub fn new(client_id: &str, pid: u32) -> Self {
        let mut core = Core { client_id: client_id.to_string(), pid, prefs: Prefs::default(), playing: Vec::new(), client: None, sent: None, last_attempt: None, rich: true, status: Status::default() };
        core.status = core.idle_status();
        core
    }

    pub fn status(&self) -> &Status {
        &self.status
    }

    pub fn set_prefs(&mut self, prefs: Prefs) {
        self.prefs = prefs;
    }

    /// Un jeu tourne : ajouté en dernier s'il est nouveau (il devient celui qu'on montre), sinon
    /// sa place est gardée et ses informations relues.
    pub fn running(&mut self, id: &str, start: u64, shown: Option<Shown>) {
        match self.playing.iter_mut().find(|p| p.id == id) {
            Some(p) => {
                if shown.is_some() {
                    p.shown = shown;
                }
            }
            None => self.playing.push(Playing { id: id.to_string(), start, shown }),
        }
    }

    pub fn ended(&mut self, id: &str) {
        self.playing.retain(|p| p.id != id);
    }

    /// Relit les informations des jeux en cours (titre renommé, jeu trouvé au scan après une reprise).
    pub fn reresolve(&mut self, resolve: &dyn Fn(&str) -> Option<Shown>) {
        for p in &mut self.playing {
            if let Some(shown) = resolve(&p.id) {
                p.shown = Some(shown);
            }
        }
    }

    /// Plus rien à montrer : réglage coupé (Ludex quitté).
    pub fn stop(&mut self) {
        self.prefs.enabled = false;
    }

    fn idle_status(&self) -> Status {
        let state = if self.client_id.is_empty() {
            State::Unconfigured
        } else if !self.prefs.enabled {
            State::Off
        } else {
            State::Waiting
        };
        Status { state, error: None }
    }

    /// Activité voulue, `None` = rien à montrer.
    pub fn wanted(&self) -> Option<Value> {
        if self.client_id.is_empty() || !self.prefs.enabled {
            return None;
        }
        let p = self.playing.last()?;
        Some(activity(p.shown.as_ref(), p.start, self.prefs.hide_name, self.rich))
    }

    /// Met Discord d'accord avec ce qui est voulu. Renvoie le délai avant un nouvel essai
    /// (Discord absent), `None` si rien n'attend.
    pub fn sync(&mut self, now: Instant, connect: &mut Connect) -> Option<Duration> {
        let Some(mut wanted) = self.wanted() else {
            // Fermer la connexion efface l'activité ; l'effacer avant est plus net.
            if let Some(mut client) = self.client.take() {
                let _ = client.set_activity(self.pid, None);
            }
            self.sent = None;
            self.status = self.idle_status();
            return None;
        };
        if self.client.is_some() && self.sent.as_ref() == Some(&wanted) {
            return None;
        }
        // Deux passages au plus : une connexion perdue (Discord redémarré) est rouverte tout de
        // suite si le dernier essai est assez ancien.
        for _ in 0..2 {
            if self.client.is_none() {
                if let Some(at) = self.last_attempt {
                    let waited = now.saturating_duration_since(at);
                    if waited < RETRY {
                        return Some(RETRY - waited);
                    }
                }
                self.last_attempt = Some(now);
                let opened = connect().map_err(Error::from).and_then(|pipe| Client::handshake(pipe, &self.client_id));
                match opened {
                    Ok(client) => self.client = Some(client),
                    Err(e) => {
                        self.status = match e {
                            Error::Io(msg) => Status { state: State::Absent, error: Some(msg) },
                            Error::Refused(msg) => Status { state: State::Refused, error: Some(msg) },
                        };
                        return Some(RETRY);
                    }
                }
            }
            let Some(client) = self.client.as_mut() else { return Some(RETRY) };
            match client.set_activity(self.pid, Some(&wanted)) {
                Ok(()) => {
                    self.sent = Some(wanted);
                    self.status = Status { state: State::Connected, error: None };
                    return None;
                }
                Err(Error::Refused(msg)) if self.rich => {
                    // Forme complète refusée (client Discord ancien) : logo et textes seulement.
                    eprintln!("Statut Discord : forme complète refusée ({msg}), forme simple à la place.");
                    self.rich = false;
                    wanted = self.wanted()?;
                    if let Some(client) = self.client.as_mut()
                        && client.set_activity(self.pid, Some(&wanted)).is_ok()
                    {
                        self.sent = Some(wanted);
                        self.status = Status { state: State::Connected, error: None };
                        return None;
                    }
                    self.client = None;
                    self.sent = None;
                    self.status = Status { state: State::Refused, error: Some(msg) };
                    return Some(RETRY);
                }
                Err(Error::Refused(msg)) => {
                    // Même refus à chaque essai : on attend un changement (autre jeu, réglage).
                    self.sent = Some(wanted);
                    self.status = Status { state: State::Refused, error: Some(msg) };
                    return None;
                }
                Err(Error::Io(msg)) => {
                    self.client = None;
                    self.sent = None;
                    self.status = Status { state: State::Absent, error: Some(msg) };
                }
            }
        }
        Some(RETRY)
    }
}

// --- Fil ------------------------------------------------------------------------------------

enum Msg {
    Running { id: String, start: u64 },
    Ended(String),
    /// Réglages changés, ou bibliothèque relue (titres).
    Refresh,
    /// Ludex se ferme : activité effacée, puis réponse.
    Quit(mpsc::Sender<()>),
}

/// Fil du statut Discord, géré par Tauri.
pub struct Presence {
    tx: mpsc::Sender<Msg>,
    status: Arc<Mutex<Status>>,
}

impl Presence {
    /// `resolve` retrouve un jeu de la bibliothèque, `prefs` lit les réglages.
    pub fn spawn(resolve: impl Fn(&str) -> Option<Shown> + Send + 'static, prefs: impl Fn() -> Prefs + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel::<Msg>();
        let status = Arc::new(Mutex::new(Status::default()));
        let shared = status.clone();
        let spawned = std::thread::Builder::new().name("ludex-discord".into()).spawn(move || {
            let mut core = Core::new(DISCORD_APP_ID, std::process::id());
            core.set_prefs(prefs());
            let publish = |core: &Core| {
                if let Ok(mut s) = shared.lock() {
                    *s = core.status().clone();
                }
            };
            publish(&core);
            let mut connect = || open_pipe(DISCORD_PIPE);
            let mut wait: Option<Duration> = None;
            loop {
                let msg = match wait {
                    Some(d) => match rx.recv_timeout(d) {
                        Ok(m) => Some(m),
                        Err(RecvTimeoutError::Timeout) => None,
                        Err(RecvTimeoutError::Disconnected) => return,
                    },
                    None => match rx.recv() {
                        Ok(m) => Some(m),
                        Err(_) => return,
                    },
                };
                let mut done = None;
                match msg {
                    Some(Msg::Running { id, start }) => {
                        let shown = resolve(&id);
                        core.running(&id, start, shown);
                    }
                    Some(Msg::Ended(id)) => core.ended(&id),
                    Some(Msg::Refresh) => {
                        core.set_prefs(prefs());
                        core.reresolve(&resolve);
                    }
                    Some(Msg::Quit(reply)) => {
                        core.stop();
                        done = Some(reply);
                    }
                    None => {}
                }
                wait = core.sync(Instant::now(), &mut connect);
                publish(&core);
                if let Some(reply) = done {
                    let _ = reply.send(());
                    return;
                }
            }
        });
        if let Err(e) = spawned {
            eprintln!("Statut Discord indisponible : {e}");
        }
        Presence { tx, status }
    }

    /// Suite d'une session (`session.rs`) : `running` pose l'activité, `ended` l'efface ;
    /// `detecting` ne change rien (jeu brièvement invisible, lanceur intermédiaire).
    pub fn session(&self, id: &str, phase: Phase, start: Option<u64>) {
        let msg = match phase {
            Phase::Running => Msg::Running { id: id.to_string(), start: start.unwrap_or_else(unix_now) },
            Phase::Ended => Msg::Ended(id.to_string()),
            Phase::Detecting => return,
        };
        let _ = self.tx.send(msg);
    }

    /// Réglages changés ou bibliothèque relue.
    pub fn refresh(&self) {
        let _ = self.tx.send(Msg::Refresh);
    }

    /// Efface l'activité avant la fermeture de Ludex, sans attendre plus de `timeout`.
    pub fn shutdown(&self, timeout: Duration) {
        let (reply, done) = mpsc::channel();
        if self.tx.send(Msg::Quit(reply)).is_ok() {
            let _ = done.recv_timeout(timeout);
        }
    }

    pub fn status(&self) -> Status {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    /// Ce qu'un faux Discord a reçu : trames décodées.
    #[derive(Default)]
    struct Log {
        frames: Vec<(u32, Value)>,
    }

    type Script = Box<dyn FnMut(u32, &Value) -> Vec<(u32, Value)>>;

    /// Faux tube : chaque trame écrite est passée au script, ses réponses arrivent aussitôt.
    struct Fake {
        inbox: VecDeque<u8>,
        log: Rc<RefCell<Log>>,
        script: Script,
        broken: bool,
    }

    impl Read for Fake {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = buf.len().min(self.inbox.len());
            for (i, b) in self.inbox.drain(..n).enumerate() {
                buf[i] = b;
            }
            Ok(n)
        }
    }

    impl Write for Fake {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.broken {
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "tube fermé"));
            }
            let (op, len) = header(buf[..8].try_into().unwrap());
            assert_eq!(len, buf.len() - 8, "une trame entière par écriture");
            let value: Value = serde_json::from_slice(&buf[8..]).unwrap();
            self.log.borrow_mut().frames.push((op, value.clone()));
            for (op, reply) in (self.script)(op, &value) {
                self.inbox.extend(encode(op, reply.to_string().as_bytes()));
            }
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Pipe for Fake {
        fn available(&mut self) -> io::Result<usize> {
            Ok(self.inbox.len())
        }
    }

    /// Discord qui accepte tout (refuse `status_display_type` si `old`).
    fn discord(old: bool) -> Script {
        Box::new(move |op, v| match op {
            OP_HANDSHAKE => vec![(OP_FRAME, json!({ "cmd": "DISPATCH", "evt": "READY", "data": { "v": 1 } }))],
            OP_FRAME => {
                let nonce = v["nonce"].clone();
                if old && v.pointer("/args/activity/status_display_type").is_some() {
                    vec![(OP_FRAME, json!({ "cmd": "SET_ACTIVITY", "evt": "ERROR", "nonce": nonce, "data": { "code": 4000, "message": "child \"activity\" fails" } }))]
                } else {
                    vec![(OP_FRAME, json!({ "cmd": "SET_ACTIVITY", "data": v["args"]["activity"], "evt": null, "nonce": nonce }))]
                }
            }
            _ => Vec::new(),
        })
    }

    fn fake(log: &Rc<RefCell<Log>>, script: Script) -> Box<dyn Pipe> {
        Box::new(Fake { inbox: VecDeque::new(), log: log.clone(), script, broken: false })
    }

    fn hades() -> Shown {
        Shown { title: "Hades".into(), platform: Platform::Steam, steam_appid: Some(1_145_360) }
    }

    fn on() -> Prefs {
        Prefs { enabled: true, hide_name: false }
    }

    #[test]
    fn frames_are_little_endian_with_their_length() {
        let frame = encode(1, br#"{"a":1}"#);
        assert_eq!(&frame[..8], &[1, 0, 0, 0, 7, 0, 0, 0]);
        assert_eq!(&frame[8..], br#"{"a":1}"#);
        assert_eq!(header(frame[..8].try_into().unwrap()), (1, 7));
        assert_eq!(header([2, 0, 0, 0, 0, 1, 0, 0]), (2, 256));
    }

    #[test]
    fn activity_shows_title_launcher_start_and_cover() {
        let a = activity(Some(&hades()), 1_700_000_000, false, true);
        assert_eq!(a["details"], "Hades");
        assert_eq!(a["state"], "Sur Steam");
        assert_eq!(a["timestamps"]["start"], 1_700_000_000_000u64);
        assert_eq!(a["status_display_type"], 2);
        assert_eq!(a["assets"]["large_image"], "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/1145360/header.jpg");
        assert_eq!(a["assets"]["small_image"], LOGO_ASSET);
        assert_eq!(a["assets"]["large_text"], "Hades");

        // Sans appid : logo de Ludex, pas d'image du jeu.
        let riot = Shown { title: "League of Legends".into(), platform: Platform::Riot, steam_appid: None };
        let a = activity(Some(&riot), 10, false, true);
        assert_eq!(a["assets"], json!({ "large_image": LOGO_ASSET, "large_text": "Ludex" }));
        assert_eq!(a["state"], "Sur Riot Games");

        // Forme simple : ni image du jeu, ni nom dans la liste des membres.
        let a = activity(Some(&hades()), 10, false, false);
        assert!(a.get("status_display_type").is_none());
        assert_eq!(a["assets"]["large_image"], LOGO_ASSET);
    }

    #[test]
    fn hidden_name_shows_nothing_about_the_game() {
        let a = activity(Some(&hades()), 10, true, true);
        assert_eq!(a["details"], "En jeu");
        assert!(a.get("state").is_none() && a.get("status_display_type").is_none());
        assert_eq!(a["assets"]["large_image"], LOGO_ASSET);
        assert!(!a.to_string().contains("Hades") && !a.to_string().contains("1145360"));
        // Jeu pas encore relu (reprise avant le premier scan) : même forme.
        assert_eq!(activity(None, 10, false, true)["details"], "En jeu");
    }

    #[test]
    fn texts_fit_discord_limits() {
        assert_eq!(field("  Hades\u{0}  II \n"), "Hades II");
        assert_eq!(field("X"), "Jeu X");
        assert_eq!(field("   "), "Un jeu");
        let long = "a".repeat(300);
        let cut = field(&long);
        assert_eq!(cut.chars().count(), MAX_TEXT);
        assert!(cut.ends_with('…'));
        assert_eq!(launcher_line(Platform::Other), "Lancé depuis Ludex");
    }

    #[test]
    fn handshake_then_activity_then_clear() {
        let log = Rc::new(RefCell::new(Log::default()));
        let mut core = Core::new("123", 42);
        core.set_prefs(on());
        core.running("steam:1145360", 1_700_000_000, Some(hades()));
        let l = log.clone();
        let mut connect = move || Ok(fake(&l, discord(false)));
        assert_eq!(core.sync(Instant::now(), &mut connect), None);
        assert_eq!(core.status().state, State::Connected);
        {
            let log = log.borrow();
            assert_eq!(log.frames[0], (OP_HANDSHAKE, json!({ "v": 1, "client_id": "123" })));
            let (op, cmd) = &log.frames[1];
            assert_eq!(*op, OP_FRAME);
            assert_eq!(cmd["cmd"], "SET_ACTIVITY");
            assert_eq!(cmd["args"]["pid"], 42);
            assert_eq!(cmd["args"]["activity"]["details"], "Hades");
            assert!(cmd["nonce"].as_str().is_some_and(|n| !n.is_empty()));
        }
        // Rien de changé : rien d'envoyé.
        core.sync(Instant::now(), &mut connect);
        assert_eq!(log.borrow().frames.len(), 2);
        // Fin de partie : activité effacée, connexion fermée.
        core.ended("steam:1145360");
        assert_eq!(core.sync(Instant::now(), &mut connect), None);
        assert_eq!(log.borrow().frames[2].1["args"]["activity"], Value::Null);
        assert_eq!(core.status().state, State::Waiting);
        assert!(core.client.is_none());
    }

    #[test]
    fn last_started_game_is_shown_then_the_previous_one() {
        let log = Rc::new(RefCell::new(Log::default()));
        let mut core = Core::new("123", 1);
        core.set_prefs(on());
        let l = log.clone();
        let mut connect = move || Ok(fake(&l, discord(false)));
        core.running("a", 100, Some(Shown { title: "Premier".into(), platform: Platform::Epic, steam_appid: None }));
        core.running("b", 200, Some(Shown { title: "Second".into(), platform: Platform::Xbox, steam_appid: None }));
        core.sync(Instant::now(), &mut connect);
        assert_eq!(core.sent.as_ref().unwrap()["details"], "Second");
        // Le premier qui redevient « running » (temps ajouté) ne passe pas devant.
        core.running("a", 100, None);
        core.sync(Instant::now(), &mut connect);
        assert_eq!(core.sent.as_ref().unwrap()["details"], "Second");
        core.ended("b");
        core.sync(Instant::now(), &mut connect);
        assert_eq!(core.sent.as_ref().unwrap()["details"], "Premier");
        assert_eq!(core.sent.as_ref().unwrap()["timestamps"]["start"], 100_000);
    }

    #[test]
    fn absent_discord_is_retried_at_most_every_30_seconds() {
        let mut core = Core::new("123", 1);
        core.set_prefs(on());
        core.running("a", 1, Some(hades()));
        let attempts = Rc::new(RefCell::new(0));
        let a = attempts.clone();
        let mut connect = move || -> io::Result<Box<dyn Pipe>> {
            *a.borrow_mut() += 1;
            Err(io::Error::new(io::ErrorKind::NotFound, "absent"))
        };
        let t0 = Instant::now();
        assert_eq!(core.sync(t0, &mut connect), Some(RETRY));
        assert_eq!(core.status().state, State::Absent);
        assert_eq!(core.sync(t0 + Duration::from_secs(10), &mut connect), Some(Duration::from_secs(20)));
        assert_eq!(*attempts.borrow(), 1);
        core.sync(t0 + RETRY, &mut connect);
        assert_eq!(*attempts.borrow(), 2);
        // Plus de partie : aucun essai, état d'attente.
        core.ended("a");
        assert_eq!(core.sync(t0 + RETRY * 3, &mut connect), None);
        assert_eq!(*attempts.borrow(), 2);
        assert_eq!(core.status().state, State::Waiting);
    }

    #[test]
    fn lost_connection_is_reopened_right_away() {
        let log = Rc::new(RefCell::new(Log::default()));
        let mut core = Core::new("123", 1);
        core.set_prefs(on());
        core.running("a", 1, Some(hades()));
        let l = log.clone();
        let opened = Rc::new(RefCell::new(0));
        let o = opened.clone();
        let mut connect = move || {
            *o.borrow_mut() += 1;
            Ok(fake(&l, discord(false)))
        };
        let t0 = Instant::now();
        core.sync(t0, &mut connect);
        // Discord redémarré : la connexion casse au prochain envoi.
        if let Some(client) = core.client.as_mut() {
            client.pipe = Box::new(Fake { inbox: VecDeque::new(), log: log.clone(), script: discord(false), broken: true });
        }
        core.running("b", 2, Some(Shown { title: "Autre".into(), platform: Platform::Gog, steam_appid: None }));
        assert_eq!(core.sync(t0 + RETRY * 2, &mut connect), None);
        assert_eq!(*opened.borrow(), 2);
        assert_eq!(core.sent.as_ref().unwrap()["details"], "Autre");
    }

    #[test]
    fn old_discord_gets_the_simple_form() {
        let log = Rc::new(RefCell::new(Log::default()));
        let mut core = Core::new("123", 1);
        core.set_prefs(on());
        core.running("a", 1, Some(hades()));
        let l = log.clone();
        let mut connect = move || Ok(fake(&l, discord(true)));
        assert_eq!(core.sync(Instant::now(), &mut connect), None);
        assert_eq!(core.status().state, State::Connected);
        let sent = core.sent.clone().unwrap();
        assert!(sent.get("status_display_type").is_none());
        assert_eq!(sent["assets"]["large_image"], LOGO_ASSET);
    }

    #[test]
    fn refused_handshake_is_reported() {
        let log = Rc::new(RefCell::new(Log::default()));
        let mut core = Core::new("999", 1);
        core.set_prefs(on());
        core.running("a", 1, Some(hades()));
        let l = log.clone();
        let script: Script = Box::new(|_, _| vec![(OP_CLOSE, json!({ "code": 4000, "message": "Invalid Client ID" }))]);
        let mut script = Some(script);
        let mut connect = move || Ok(fake(&l, script.take().unwrap_or_else(|| discord(false))));
        assert_eq!(core.sync(Instant::now(), &mut connect), Some(RETRY));
        assert_eq!(core.status(), &Status { state: State::Refused, error: Some("Invalid Client ID".into()) });
        assert!(core.status().text().contains("Invalid Client ID"));
    }

    #[test]
    fn ping_is_answered_before_the_reply() {
        let log = Rc::new(RefCell::new(Log::default()));
        let l = log.clone();
        let mut inner = discord(false);
        let script: Script = Box::new(move |op, v| {
            let mut out = if op == OP_FRAME { vec![(OP_PING, json!({ "p": 1 }))] } else { Vec::new() };
            out.extend(inner(op, v));
            out
        });
        let mut client = Client::handshake(fake(&l, script), "123").unwrap();
        client.set_activity(1, None).unwrap();
        assert!(log.borrow().frames.iter().any(|(op, v)| *op == OP_PONG && v["p"] == 1));
    }

    #[test]
    fn nothing_without_application_id_or_setting() {
        let mut connect = || -> io::Result<Box<dyn Pipe>> { panic!("aucune connexion attendue") };
        let mut core = Core::new("", 1);
        core.set_prefs(on());
        core.running("a", 1, Some(hades()));
        assert_eq!(core.sync(Instant::now(), &mut connect), None);
        assert_eq!(core.status().state, State::Unconfigured);

        let mut core = Core::new("123", 1);
        core.running("a", 1, Some(hades()));
        assert_eq!(core.sync(Instant::now(), &mut connect), None);
        assert_eq!(core.status().state, State::Off);
    }

    #[test]
    fn setting_turned_off_clears_the_activity() {
        let log = Rc::new(RefCell::new(Log::default()));
        let mut core = Core::new("123", 1);
        core.set_prefs(on());
        core.running("a", 1, Some(hades()));
        let l = log.clone();
        let mut connect = move || Ok(fake(&l, discord(false)));
        core.sync(Instant::now(), &mut connect);
        core.set_prefs(Prefs { enabled: true, hide_name: true });
        core.sync(Instant::now(), &mut connect);
        assert_eq!(core.sent.as_ref().unwrap()["details"], "En jeu");
        core.stop();
        core.sync(Instant::now(), &mut connect);
        assert_eq!(log.borrow().frames.last().unwrap().1["args"]["activity"], Value::Null);
        assert_eq!(core.status().state, State::Off);
    }

    /// Vrai client Discord de ce PC : `cargo test discord_this_pc -- --ignored --nocapture`.
    /// Sans `LUDEX_DISCORD_ID`, identifiant bidon : Discord doit refuser la poignée de main (rien
    /// n'est affiché). Avec l'identifiant de l'application, « Hades » est montré 15 s puis effacé.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn discord_this_pc() {
        let id = std::env::var("LUDEX_DISCORD_ID").unwrap_or_else(|_| "1".to_string());
        let started = Instant::now();
        let pipe = match open_pipe(DISCORD_PIPE) {
            Ok(p) => p,
            Err(e) => return println!("Discord absent : {e}"),
        };
        match Client::handshake(pipe, &id) {
            Err(e) => println!("Poignée de main en {:?} : {e:?}", started.elapsed()),
            Ok(mut client) => {
                println!("Connecté en {:?}", started.elapsed());
                let a = activity(Some(&hades()), unix_now(), false, true);
                println!("Activité : {:?}", client.set_activity(std::process::id(), Some(&a)));
                std::thread::sleep(Duration::from_secs(15));
                println!("Effacée : {:?}", client.set_activity(std::process::id(), None));
            }
        }
    }

    /// Vrai tube nommé de Windows, côté serveur un faux Discord : le client de Ludex s'y connecte,
    /// fait la poignée de main, pose l'activité puis l'efface.
    #[cfg(windows)]
    #[test]
    fn talks_to_a_named_pipe_server() {
        use std::os::windows::io::FromRawHandle;
        use windows::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
        use windows::Win32::System::Pipes::{ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_WAIT};
        use windows::core::HSTRING;

        let base = format!(r"\\.\pipe\ludex-essai-discord-{}", std::process::id());
        // Tube n° 3 : le client doit essayer 0, 1, 2 puis trouver celui-ci.
        let name = HSTRING::from(format!("{base}-3"));
        let handle = unsafe { CreateNamedPipeW(&name, PIPE_ACCESS_DUPLEX, PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT, 1, 65536, 65536, 0, None) };
        assert!(!handle.is_invalid(), "tube d'essai non créé");
        let raw = handle.0 as usize;
        let server = std::thread::spawn(move || {
            let handle = windows::Win32::Foundation::HANDLE(raw as *mut _);
            let _ = unsafe { ConnectNamedPipe(handle, None) };
            let mut file = unsafe { std::fs::File::from_raw_handle(raw as *mut _) };
            let mut received = Vec::new();
            for _ in 0..3 {
                let mut head = [0u8; 8];
                file.read_exact(&mut head).unwrap();
                let (op, len) = header(head);
                let mut body = vec![0u8; len];
                file.read_exact(&mut body).unwrap();
                let v: Value = serde_json::from_slice(&body).unwrap();
                let reply = if op == OP_HANDSHAKE {
                    json!({ "cmd": "DISPATCH", "evt": "READY" })
                } else {
                    json!({ "cmd": "SET_ACTIVITY", "evt": null, "nonce": v["nonce"] })
                };
                file.write_all(&encode(OP_FRAME, reply.to_string().as_bytes())).unwrap();
                received.push((op, v));
            }
            received
        });
        let pipe = open_pipe(&base).expect("tube trouvé");
        let mut client = Client::handshake(pipe, "123").unwrap();
        client.set_activity(7, Some(&activity(Some(&hades()), 5, false, true))).unwrap();
        client.set_activity(7, None).unwrap();
        let received = server.join().unwrap();
        assert_eq!(received[0].1["client_id"], "123");
        assert_eq!(received[1].1["args"]["activity"]["details"], "Hades");
        assert_eq!(received[2].1["args"]["activity"], Value::Null);
        // Aucun tube : Discord absent.
        assert!(open_pipe(&format!("{base}-absent")).is_err());
    }
}
