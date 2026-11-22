//! Lecteur minimal de `appcache/appinfo.vdf` du client Steam : nom et type (`Game`, `DLC`,
//! `Application`, `Tool`…) de chaque application que Steam connaît sur ce PC. Lecture seule.
//!
//! Format (versions 27 à 29, petit-boutiste) : `magic` u32 (`0x07564427` + version), univers u32,
//! en version 29 la position (i64) de la table des chaînes en fin de fichier ; puis des entrées
//! `appid` u32 (0 = fin), taille u32 de la suite, état u32, mise à jour u32, jeton PICS u64,
//! SHA-1 du texte (20 octets), numéro de changement u32, SHA-1 du binaire (20 octets, version 28 et
//! plus), puis un document KeyValues binaire (voir `library/kvbin.rs`) ; en version 29, le nom de
//! chaque clé est un index u32 dans la table des chaînes (`u32` nombre, puis chaînes C).
//!
//! Seuls `appinfo > common > name` et `type` sont gardés, sans copier le reste : le fichier peut
//! peser plus de 100 Mo sur une grosse bibliothèque. Tout écart (version inconnue, entrée tronquée,
//! type de valeur inconnu) arrête la lecture sans panic : ce qui est déjà lu reste.

use std::collections::HashMap;

/// Nom et type d'une application.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppInfo {
    pub name: String,
    /// Type de `common > type`, en minuscules (`game`, `dlc`, `application`…), vide si absent.
    pub kind: String,
}

impl AppInfo {
    pub fn is_game(&self) -> bool {
        self.kind == "game"
    }
}

const MAGIC_BASE: u32 = 0x0756_4400;
const MAX_DEPTH: usize = 32;
/// Nom et type plus longs : écartés (jamais vus, protège la mémoire).
const MAX_TEXT: usize = 512;

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    /// Table des chaînes (version 29), sinon noms de clés en chaînes C.
    names: Option<Vec<&'a str>>,
}

impl<'a> Reader<'a> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let bytes = self.data.get(self.pos..self.pos.checked_add(N)?)?;
        self.pos += N;
        bytes.try_into().ok()
    }

    fn u32(&mut self) -> Option<u32> {
        self.take::<4>().map(u32::from_le_bytes)
    }

    fn skip(&mut self, n: usize) -> Option<()> {
        let end = self.pos.checked_add(n)?;
        (end <= self.data.len()).then(|| self.pos = end)
    }

    fn cstr(&mut self) -> Option<&'a [u8]> {
        let rest: &'a [u8] = self.data.get(self.pos..)?;
        let len = rest.iter().position(|&b| b == 0)?;
        self.pos += len + 1;
        Some(&rest[..len])
    }

    fn key(&mut self) -> Option<&'a [u8]> {
        if self.names.is_none() {
            return self.cstr();
        }
        let i = self.u32()? as usize;
        self.names.as_ref()?.get(i).copied().map(str::as_bytes)
    }

    /// Parcourt un objet ; `at` = profondeur utile (0 racine, 1 `appinfo`, 2 `common`, 3 ailleurs).
    fn object(&mut self, depth: usize, at: u8, found: &mut (Option<String>, Option<String>)) -> Option<()> {
        if depth > MAX_DEPTH {
            return None;
        }
        loop {
            let kind = self.take::<1>()?[0];
            if kind == 8 || kind == 11 {
                return Some(());
            }
            let key = self.key()?;
            match kind {
                0 => {
                    let next = match at {
                        0 if key.eq_ignore_ascii_case(b"appinfo") => 1,
                        1 if key.eq_ignore_ascii_case(b"common") => 2,
                        _ => 3,
                    };
                    self.object(depth + 1, next, found)?;
                }
                1 => {
                    let value = self.cstr()?;
                    if at == 2 && value.len() <= MAX_TEXT {
                        let text = || String::from_utf8_lossy(value).trim().to_string();
                        if key.eq_ignore_ascii_case(b"name") {
                            found.0 = Some(text());
                        } else if key.eq_ignore_ascii_case(b"type") {
                            found.1 = Some(text().to_ascii_lowercase());
                        }
                    }
                }
                5 => loop {
                    if u16::from_le_bytes(self.take::<2>()?) == 0 {
                        break;
                    }
                },
                2..=4 | 6 => self.skip(4)?,
                7 | 10 => self.skip(8)?,
                _ => return None,
            }
        }
    }
}

/// Table des chaînes de la version 29 : `u32` nombre puis chaînes C.
fn string_table(data: &[u8], offset: usize) -> Option<Vec<&str>> {
    let mut r = Reader { data, pos: offset, names: None };
    let count = r.u32()? as usize;
    // Chaque chaîne prend au moins un octet : un nombre plus grand que le reste est faux.
    if count > data.len().saturating_sub(r.pos) {
        return None;
    }
    let mut names = Vec::with_capacity(count);
    for _ in 0..count {
        names.push(std::str::from_utf8(r.cstr()?).unwrap_or(""));
    }
    Some(names)
}

/// Lit `appinfo.vdf`. `None` : version inconnue ou en-tête illisible ; sinon les applications
/// lues jusqu'à la fin (ou jusqu'au premier écart), celles sans nom écartées.
pub fn parse(data: &[u8]) -> Option<HashMap<u32, AppInfo>> {
    let mut r = Reader { data, pos: 0, names: None };
    let version = r.u32()?.checked_sub(MAGIC_BASE).filter(|v| (0x27..=0x29).contains(v))?;
    r.u32()?; // univers
    // Fin des entrées : table des chaînes (version 29) ou fin du fichier.
    let mut end = data.len();
    if version == 0x29 {
        let offset = usize::try_from(i64::from_le_bytes(r.take::<8>()?)).ok().filter(|&o| o >= r.pos && o < data.len())?;
        r.names = Some(string_table(data, offset)?);
        end = offset;
    }
    let header = 4 + 4 + 8 + 20 + 4 + if version >= 0x28 { 20 } else { 0 };
    let mut apps = HashMap::new();
    while r.pos < end {
        let Some(appid) = r.u32() else { break };
        if appid == 0 {
            break;
        }
        let Some(size) = r.u32().map(|s| s as usize) else { break };
        let start = r.pos;
        let Some(next) = start.checked_add(size).filter(|&n| n <= end) else { break };
        if size < header {
            break;
        }
        r.pos = start + header;
        let mut found = (None, None);
        let mut entry = Reader { data: &data[..next], pos: r.pos, names: r.names.take() };
        let read = entry.object(0, 0, &mut found);
        r.names = entry.names.take();
        if read.is_some()
            && let Some(name) = found.0.filter(|n| !n.is_empty())
        {
            apps.insert(appid, AppInfo { name, kind: found.1.unwrap_or_default() });
        }
        r.pos = next;
    }
    Some(apps)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Fabrique un `appinfo.vdf` : `apps` = (appid, nom, type), version 28 ou 29.
    pub fn build(version: u32, apps: &[(u32, &str, &str)]) -> Vec<u8> {
        let mut table: Vec<String> = Vec::new();
        let mut key = |name: &str, out: &mut Vec<u8>| {
            if version == 0x29 {
                let i = table.iter().position(|t| t == name).unwrap_or_else(|| {
                    table.push(name.to_string());
                    table.len() - 1
                });
                out.extend((i as u32).to_le_bytes());
            } else {
                out.extend(name.as_bytes());
                out.push(0);
            }
        };
        let mut body = Vec::new();
        for (appid, name, kind) in apps {
            let mut kv = Vec::new();
            kv.push(0);
            key("appinfo", &mut kv);
            kv.push(2);
            key("appid", &mut kv);
            kv.extend(appid.to_le_bytes());
            kv.push(0);
            key("common", &mut kv);
            kv.push(1);
            key("name", &mut kv);
            kv.extend(name.as_bytes());
            kv.push(0);
            kv.push(1);
            key("type", &mut kv);
            kv.extend(kind.as_bytes());
            kv.push(0);
            kv.push(7);
            key("gameid", &mut kv);
            kv.extend(u64::from(*appid).to_le_bytes());
            kv.push(8); // fin de common
            kv.push(0);
            key("extended", &mut kv);
            kv.push(1);
            key("name", &mut kv);
            kv.extend(b"pas le nom\0");
            kv.push(8); // fin d'extended
            kv.push(8); // fin d'appinfo
            kv.push(8); // fin de la racine
            let mut entry = vec![0u8; 4 + 4 + 8 + 20 + 4 + if version >= 0x28 { 20 } else { 0 }];
            entry.extend(kv);
            body.extend(appid.to_le_bytes());
            body.extend((entry.len() as u32).to_le_bytes());
            body.extend(entry);
        }
        body.extend(0u32.to_le_bytes());
        let mut out = Vec::new();
        out.extend((MAGIC_BASE + version).to_le_bytes());
        out.extend(1u32.to_le_bytes());
        if version == 0x29 {
            let offset = out.len() + 8 + body.len();
            out.extend((offset as i64).to_le_bytes());
            out.extend(body);
            out.extend((table.len() as u32).to_le_bytes());
            for t in &table {
                out.extend(t.as_bytes());
                out.push(0);
            }
        } else {
            out.extend(body);
        }
        out
    }

    #[test]
    fn reads_names_and_types_of_both_versions() {
        let apps = [(105600, "Terraria", "Game"), (1_000_000, "Bande originale", "Music"), (431960, "Wallpaper Engine", "Application")];
        for version in [0x28, 0x29] {
            let parsed = parse(&build(version, &apps)).unwrap();
            assert_eq!(parsed.len(), 3, "version {version:x}");
            let terraria = &parsed[&105600];
            assert_eq!(terraria.name, "Terraria");
            assert!(terraria.is_game());
            assert_eq!(parsed[&1_000_000].kind, "music");
            assert!(!parsed[&431960].is_game());
        }
    }

    #[test]
    fn broken_files_never_panic() {
        assert_eq!(parse(b""), None);
        assert_eq!(parse(&[0x99, 0x44, 0x56, 0x07, 1, 0, 0, 0]), None, "version inconnue");
        let full = build(0x28, &[(10, "Counter-Strike", "game"), (20, "Team Fortress Classic", "game")]);
        // Fichier coupé au milieu de la seconde entrée : la première reste.
        let cut = parse(&full[..full.len() - 12]).unwrap();
        assert_eq!(cut.len(), 1);
        assert_eq!(cut[&10].name, "Counter-Strike");
        for len in 0..full.len() {
            let _ = parse(&full[..len]);
        }
        // Version 29 avec une table des chaînes qui pointe hors du fichier.
        let mut bad = build(0x29, &[(10, "Counter-Strike", "game")]);
        bad[8..16].copy_from_slice(&(9_999_999i64).to_le_bytes());
        assert_eq!(parse(&bad), None);
    }
}
