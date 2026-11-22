//! Coffre des jetons du compte GOG : `comptes.bin` du dossier de données de l'app, chiffré par
//! DPAPI (`CryptProtectData`, portée de l'utilisateur Windows). Jamais de jeton en clair sur le
//! disque, dans le journal, le Diagnostic, l'export ni le front : `Session` n'est ni `Serialize`
//! vers le front ni affichable (`Debug` masqué).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Nom du fichier chiffré, dans `%APPDATA%\com.ludex.app` (jamais le dossier des données lourdes).
pub const FILE: &str = "comptes.bin";

/// « Entropie » supplémentaire de DPAPI : un autre programme de la même session Windows qui
/// appellerait `CryptUnprotectData` sur ce fichier sans elle échoue.
const ENTROPY: &[u8] = b"ludex-comptes-v1";

/// Jetons du compte GOG, gardés en mémoire et dans le coffre seulement.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub access_token: String,
    pub refresh_token: String,
    /// Fin de validité du jeton d'accès (secondes Unix).
    pub expires_at: u64,
    pub user_id: String,
    /// Pseudo GOG, seule donnée du compte montrée (Paramètres).
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
}

pub fn path_in(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE)
}

/// Lit la session GOG du coffre. `Ok(None)` : pas de coffre ou pas de compte GOG. Une erreur
/// (fichier d'un autre utilisateur Windows, altéré) n'est jamais montrée avec son contenu.
pub fn load(path: &Path) -> Result<Option<Session>, String> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(tr!("Coffre des comptes illisible.", "Accounts vault unreadable.")),
    };
    let plain = unprotect(&bytes)?;
    let vault: Vault = serde_json::from_slice(&plain).map_err(|_| tr!("Coffre des comptes illisible.", "Accounts vault unreadable."))?;
    Ok(vault.gog)
}

/// Enregistre la session (tmp + renommage). `None` efface le coffre.
pub fn save(path: &Path, session: Option<&Session>) -> Result<(), String> {
    let Some(session) = session else { return erase(path) };
    let plain = serde_json::to_vec(&Vault { version: 1, gog: Some(session.clone()) }).map_err(|_| tr!("Coffre des comptes impossible à écrire.", "Couldn't write the accounts vault."))?;
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
