//! Coffre des comptes (GOG, Epic, clé Web API Steam) : `comptes.bin` du dossier de données de l'app, chiffré par
//! DPAPI (`CryptProtectData`, portée de l'utilisateur Windows). Jamais de jeton en clair sur le
//! disque, dans le journal, le Diagnostic, l'export ni le front : `Session` n'est ni `Serialize`
//! vers le front ni affichable (`Debug` masqué). Un compte par case (`Slot`) : enregistrer ou
//! effacer une case garde les autres ; le fichier disparaît quand toutes sont vides.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Nom du fichier chiffré, dans `%APPDATA%\com.ludex.app` (jamais le dossier des données lourdes).
pub const FILE: &str = "comptes.bin";

/// « Entropie » supplémentaire de DPAPI : un autre programme de la même session Windows qui
/// appellerait `CryptUnprotectData` sur ce fichier sans elle échoue.
const ENTROPY: &[u8] = b"ludex-comptes-v1";

/// Jetons d'un compte (GOG ou Epic), gardés en mémoire et dans le coffre seulement.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub access_token: String,
    pub refresh_token: String,
    /// Fin de validité du jeton d'accès (secondes Unix).
    pub expires_at: u64,
    pub user_id: String,
    /// Pseudo du compte, seule donnée montrée (Paramètres).
    #[serde(default)]
    pub username: Option<String>,
}

impl std::fmt::Debug for Session {
    // Jamais les jetons, même dans un message de test.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session").field("expires_at", &self.expires_at).finish_non_exhaustive()
    }
}

/// Contenu du coffre (versionné : un champ inconnu d'une version future ne casse rien).
#[derive(Default, Serialize, Deserialize)]
struct Vault {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    gog: Option<Session>,
    #[serde(default)]
    epic: Option<Session>,
    /// Clé Web API Steam personnelle (facultative), jamais renvoyée au front.
    #[serde(default)]
    steam_key: Option<SteamKey>,
}

/// Clé Web API Steam : 32 chiffres hexadécimaux, gardée seulement en mémoire et dans le coffre.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SteamKey(pub String);

impl std::fmt::Debug for SteamKey {
    // Jamais la clé, même dans un message de test.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SteamKey(..)")
    }
}

/// Case du coffre : un compte par boutique.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Gog,
    Epic,
}

impl Vault {
    fn slot(&mut self, slot: Slot) -> &mut Option<Session> {
        match slot {
            Slot::Gog => &mut self.gog,
            Slot::Epic => &mut self.epic,
        }
    }

    fn is_empty(&self) -> bool {
        self.gog.is_none() && self.epic.is_none() && self.steam_key.is_none()
    }
}

/// Une seule lecture-écriture du coffre à la fois (GOG et Epic écrivent chacun leur case).
static WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn path_in(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE)
}

/// Lit la session GOG du coffre. `Ok(None)` : pas de coffre ou pas de compte GOG. Une erreur
/// (fichier d'un autre utilisateur Windows, altéré) n'est jamais montrée avec son contenu.
pub fn load(path: &Path) -> Result<Option<Session>, String> {
    load_slot(path, Slot::Gog)
}

/// Enregistre la session GOG (`None` l'efface), sans toucher aux autres comptes.
pub fn save(path: &Path, session: Option<&Session>) -> Result<(), String> {
    save_slot(path, Slot::Gog, session)
}

/// Lit le coffre entier. `Ok(None)` : pas de coffre.
fn read_all(path: &Path) -> Result<Option<Vault>, String> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(tr!("Coffre des comptes illisible.", "Accounts vault unreadable.")),
    };
    let plain = unprotect(&bytes)?;
    serde_json::from_slice(&plain).map(Some).map_err(|_| tr!("Coffre des comptes illisible.", "Accounts vault unreadable."))
}

/// Session d'une case du coffre.
pub fn load_slot(path: &Path, slot: Slot) -> Result<Option<Session>, String> {
    Ok(read_all(path)?.and_then(|mut v| v.slot(slot).take()))
}

/// Enregistre une case (`None` l'efface) : relit le coffre, garde les autres cases, réécrit
/// (tmp + renommage). Un coffre illisible est remplacé (ses comptes l'étaient déjà). Toutes les
/// cases vides : fichier effacé.
pub fn save_slot(path: &Path, slot: Slot, session: Option<&Session>) -> Result<(), String> {
    update(path, |vault| *vault.slot(slot) = session.cloned())
}

/// Clé Web API Steam du coffre. `Ok(None)` : pas de coffre ou pas de clé.
pub fn load_steam_key(path: &Path) -> Result<Option<SteamKey>, String> {
    Ok(read_all(path)?.and_then(|v| v.steam_key))
}

/// Enregistre la clé Web API Steam (`None` l'efface), sans toucher aux comptes.
pub fn save_steam_key(path: &Path, key: Option<&SteamKey>) -> Result<(), String> {
    update(path, |vault| vault.steam_key = key.cloned())
}

/// Relit le coffre, applique `change`, réécrit (ou efface s'il est vide).
fn update(path: &Path, change: impl FnOnce(&mut Vault)) -> Result<(), String> {
    let _guard = WRITE.lock().unwrap_or_else(|e| e.into_inner());
    let mut vault = read_all(path).ok().flatten().unwrap_or_default();
    change(&mut vault);
    if vault.is_empty() {
        return erase(path);
    }
    vault.version = 1;
    let plain = serde_json::to_vec(&vault).map_err(|_| tr!("Coffre des comptes impossible à écrire.", "Couldn't write the accounts vault."))?;
    let sealed = protect(&plain)?;
    let tmp = path.with_extension("bin.tmp");
    let write = std::fs::write(&tmp, &sealed).and_then(|_| std::fs::rename(&tmp, path));
    if write.is_err() {
        let _ = std::fs::remove_file(&tmp);
        return Err(tr!("Coffre des comptes impossible à écrire.", "Couldn't write the accounts vault."));
    }
    Ok(())
}

/// Efface le coffre (et un éventuel reste temporaire).
pub fn erase(path: &Path) -> Result<(), String> {
    let _ = std::fs::remove_file(path.with_extension("bin.tmp"));
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(tr!("Coffre des comptes impossible à effacer.", "Couldn't delete the accounts vault.")),
    }
}

#[cfg(windows)]
fn protect(plain: &[u8]) -> Result<Vec<u8>, String> {
    dpapi(plain, true)
}

#[cfg(windows)]
fn unprotect(sealed: &[u8]) -> Result<Vec<u8>, String> {
    dpapi(sealed, false)
}

/// Chiffre ou déchiffre par DPAPI, sans interface (`CRYPTPROTECT_UI_FORBIDDEN`).
#[cfg(windows)]
fn dpapi(input: &[u8], seal: bool) -> Result<Vec<u8>, String> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB};
    let blob_in = CRYPT_INTEGER_BLOB { cbData: input.len() as u32, pbData: input.as_ptr() as *mut u8 };
    let entropy = CRYPT_INTEGER_BLOB { cbData: ENTROPY.len() as u32, pbData: ENTROPY.as_ptr() as *mut u8 };
    let mut out = CRYPT_INTEGER_BLOB::default();
    // SAFETY : les tampons d'entrée vivent jusqu'au retour ; celui de sortie est alloué par Windows
    // puis copié et libéré par `LocalFree`.
    let done = unsafe {
        if seal {
            CryptProtectData(&blob_in, None, Some(&entropy), None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        } else {
            CryptUnprotectData(&blob_in, None, Some(&entropy), None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out)
        }
    };
    if done.is_err() || out.pbData.is_null() {
        return Err(if seal {
            tr!("Chiffrement des jetons impossible.", "Couldn't encrypt the tokens.")
        } else {
            tr!("Coffre des comptes illisible (autre utilisateur Windows ou fichier abîmé).", "Accounts vault unreadable (another Windows user or damaged file).")
        });
    }
    // SAFETY : `out` désigne `cbData` octets alloués par DPAPI.
    let bytes = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
    unsafe {
        let _ = LocalFree(Some(HLOCAL(out.pbData.cast())));
    }
    Ok(bytes)
}

#[cfg(not(windows))]
fn protect(_: &[u8]) -> Result<Vec<u8>, String> {
    Err(tr!("Coffre des comptes disponible sous Windows seulement.", "Accounts vault only available on Windows."))
}

#[cfg(not(windows))]
fn unprotect(_: &[u8]) -> Result<Vec<u8>, String> {
    Err(tr!("Coffre des comptes disponible sous Windows seulement.", "Accounts vault only available on Windows."))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    fn session() -> Session {
        Session {
            access_token: "jeton-acces-de-test-123".into(),
            refresh_token: "jeton-rafraichissement-de-test-456".into(),
            expires_at: 1_800_000_000,
            user_id: "4862".into(),
            username: Some("JoueurEssai".into()),
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ludex-vault-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn round_trip_never_writes_tokens_in_clear() {
        let dir = temp_dir("aller-retour");
        let path = path_in(&dir);
        assert_eq!(load(&path).unwrap(), None);
        save(&path, Some(&session())).unwrap();
        let raw = std::fs::read(&path).unwrap();
        let text = String::from_utf8_lossy(&raw);
        assert!(!text.contains("jeton-acces"), "jeton en clair dans le coffre");
        assert!(!text.contains("rafraichissement"));
        assert!(!text.contains("JoueurEssai"));
        assert_eq!(load(&path).unwrap(), Some(session()));
        assert!(!path.with_extension("bin.tmp").exists());
        save(&path, None).unwrap();
        assert!(!path.exists());
        assert_eq!(load(&path).unwrap(), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn damaged_or_foreign_vault_is_refused_without_content() {
        let dir = temp_dir("abime");
        let path = path_in(&dir);
        save(&path, Some(&session())).unwrap();
        let mut raw = std::fs::read(&path).unwrap();
        let last = raw.len() - 1;
        raw[last] ^= 0xFF;
        std::fs::write(&path, &raw).unwrap();
        let err = load(&path).unwrap_err();
        assert!(!err.contains("jeton"));
        // Même octets chiffrés sans l'entropie de Ludex : refusés.
        save(&path, Some(&session())).unwrap();
        let sealed = std::fs::read(&path).unwrap();
        assert!(unprotect(&sealed).is_ok());
        assert!(dpapi_without_entropy(&sealed).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn slots_are_kept_apart() {
        let dir = temp_dir("cases");
        let path = path_in(&dir);
        let mut epic = session();
        epic.user_id = "0123456789abcdef0123456789abcdef".into();
        save(&path, Some(&session())).unwrap();
        save_slot(&path, Slot::Epic, Some(&epic)).unwrap();
        assert_eq!(load(&path).unwrap(), Some(session()));
        assert_eq!(load_slot(&path, Slot::Epic).unwrap(), Some(epic.clone()));
        // Déconnexion de GOG : Epic reste.
        save(&path, None).unwrap();
        assert!(path.exists());
        assert_eq!(load(&path).unwrap(), None);
        assert_eq!(load_slot(&path, Slot::Epic).unwrap(), Some(epic));
        save_slot(&path, Slot::Epic, None).unwrap();
        assert!(!path.exists(), "coffre vide effacé");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn steam_key_lives_beside_accounts() {
        let dir = temp_dir("cle-steam");
        let path = path_in(&dir);
        let key = SteamKey("0123456789ABCDEF0123456789ABCDEF".into());
        save_steam_key(&path, Some(&key)).unwrap();
        let raw = std::fs::read(&path).unwrap();
        assert!(!String::from_utf8_lossy(&raw).contains("0123456789ABCDEF"), "clé en clair dans le coffre");
        save(&path, Some(&session())).unwrap();
        assert_eq!(load_steam_key(&path).unwrap(), Some(key.clone()));
        // Clé retirée : le compte GOG reste ; plus rien = coffre effacé.
        save_steam_key(&path, None).unwrap();
        assert_eq!(load_steam_key(&path).unwrap(), None);
        assert_eq!(load(&path).unwrap(), Some(session()));
        save(&path, None).unwrap();
        assert!(!path.exists());
        assert!(!format!("{key:?}").contains("0123"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn debug_hides_tokens() {
        let shown = format!("{:?}", session());
        assert!(!shown.contains("jeton"));
        assert!(!shown.contains("JoueurEssai"));
    }

    fn dpapi_without_entropy(sealed: &[u8]) -> Result<Vec<u8>, ()> {
        use windows::Win32::Security::Cryptography::{CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB};
        let blob_in = CRYPT_INTEGER_BLOB { cbData: sealed.len() as u32, pbData: sealed.as_ptr() as *mut u8 };
        let mut out = CRYPT_INTEGER_BLOB::default();
        unsafe { CryptUnprotectData(&blob_in, None, None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out) }.map_err(|_| ())?;
        Ok(Vec::new())
    }
}
