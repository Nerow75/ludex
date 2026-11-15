//! Lecteur du format KeyValues BINAIRE de Valve (fichiers `appcache/stats/*.bin` de Steam).
//!
//! Un objet est une suite d'entrées terminée par l'octet 8. Chaque entrée : un octet de type,
//! le nom en chaîne C (UTF-8 terminé par 0), puis la valeur selon le type :
//! 0 = sous-objet (entrées jusqu'au 8), 1 = chaîne C, 2 = int32, 3 = float32, 4 = pointeur (4 octets),
//! 5 = chaîne UTF-16 terminée par 0u16, 6 = couleur (4 octets), 7 = uint64, 10 = int64,
//! 8 (ou 11) = fin de l'objet. Nombres en petit-boutiste. Le fichier entier est l'objet racine :
//! il se termine donc lui aussi par 8.
//!
//! Lecture défensive : un fichier tronqué, un type inconnu ou une imbrication excessive donnent
//! `None`, jamais de panic. Les chaînes empruntent les octets du fichier (pas de copie) : un
//! schéma de plusieurs Mo, traduit en 30 langues, se lit vite.

use std::borrow::Cow;

/// Valeur d'un document KeyValues binaire.
#[derive(Debug, Clone, PartialEq)]
pub enum Kv<'a> {
    Obj(Vec<(Cow<'a, str>, Kv<'a>)>),
    Str(Cow<'a, str>),
    Int(i64),
    Float(f32),
}

impl<'a> Kv<'a> {
    /// Entrée d'un objet (nom sans tenir compte de la casse, comme Steam).
    pub fn get(&self, key: &str) -> Option<&Kv<'a>> {
        self.entries().iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v)
    }

    pub fn entries(&self) -> &[(Cow<'a, str>, Kv<'a>)] {
        match self {
            Kv::Obj(entries) => entries,
            _ => &[],
        }
    }

    /// Valeur entière (int32, uint64, int64 ou pointeur).
    pub fn int(&self) -> Option<i64> {
        match self {
            Kv::Int(n) => Some(*n),
            _ => None,
        }
    }
}

/// Imbrication maximale acceptée : les fichiers de Steam n'en ont que quelques niveaux.
const MAX_DEPTH: usize = 32;

const OBJ: u8 = 0;
const STR: u8 = 1;
const INT32: u8 = 2;
const FLOAT32: u8 = 3;
const PTR: u8 = 4;
const WSTR: u8 = 5;
const COLOR: u8 = 6;
const UINT64: u8 = 7;
const END: u8 = 8;
const INT64: u8 = 10;
const END_ALT: u8 = 11;

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let bytes = self.data.get(self.pos..self.pos.checked_add(N)?)?;
        self.pos += N;
        bytes.try_into().ok()
    }

    fn byte(&mut self) -> Option<u8> {
        self.take::<1>().map(|[b]| b)
    }

    fn cstr(&mut self) -> Option<Cow<'a, str>> {
        let rest: &'a [u8] = self.data.get(self.pos..)?;
        let len = rest.iter().position(|&b| b == 0)?;
        self.pos += len + 1;
        Some(String::from_utf8_lossy(&rest[..len]))
    }

    fn wstr(&mut self) -> Option<Cow<'a, str>> {
        let mut units = Vec::new();
        loop {
            match u16::from_le_bytes(self.take::<2>()?) {
                0 => return Some(Cow::Owned(String::from_utf16_lossy(&units))),
                u => units.push(u),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Option<Vec<(Cow<'a, str>, Kv<'a>)>> {
        if depth > MAX_DEPTH {
            return None;
        }
        let mut entries = Vec::new();
        loop {
            let kind = self.byte()?;
            if kind == END || kind == END_ALT {
                return Some(entries);
            }
            let name = self.cstr()?;
            let value = match kind {
                OBJ => Kv::Obj(self.object(depth + 1)?),
                STR => Kv::Str(self.cstr()?),
                WSTR => Kv::Str(self.wstr()?),
                INT32 => Kv::Int(i32::from_le_bytes(self.take()?).into()),
                PTR | COLOR => Kv::Int(u32::from_le_bytes(self.take()?).into()),
                FLOAT32 => Kv::Float(f32::from_le_bytes(self.take()?)),
                // Au-delà de i64::MAX, les bits sont gardés tels quels : Ludex ne lit aucun uint64.
                UINT64 => Kv::Int(u64::from_le_bytes(self.take()?) as i64),
                INT64 => Kv::Int(i64::from_le_bytes(self.take()?)),
                _ => return None,
            };
            entries.push((name, value));
        }
    }
}

/// Document complet, `None` s'il est tronqué ou contient un type inconnu.
pub fn parse(data: &[u8]) -> Option<Kv<'_>> {
    Reader { data, pos: 0 }.object(0).map(Kv::Obj)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{parse, Kv};

    /// Petit constructeur d'octets pour écrire les tests (et ceux des succès) à la main.
    #[derive(Default)]
    pub struct Bytes(pub Vec<u8>);

    impl Bytes {
        fn head(mut self, kind: u8, name: &str) -> Self {
            self.0.push(kind);
            self.0.extend_from_slice(name.as_bytes());
            self.0.push(0);
            self
        }
        pub fn open(self, name: &str) -> Self {
            self.head(0, name)
        }
        pub fn str(self, name: &str, value: &str) -> Self {
            let mut b = self.head(1, name);
            b.0.extend_from_slice(value.as_bytes());
            b.0.push(0);
            b
        }
        pub fn i32(self, name: &str, value: i32) -> Self {
            let mut b = self.head(2, name);
            b.0.extend_from_slice(&value.to_le_bytes());
            b
        }
        pub fn close(mut self) -> Self {
            self.0.push(8);
            self
        }
    }

    #[test]
    fn reads_nested_objects_and_values() {
        let mut bytes = Bytes::default()
            .open("cache")
            .i32("crc", -5)
            .str("nom", "Élan")
            .open("1")
            .i32("data", -1)
            .close()
            .close()
            .0;
        bytes.push(3);
        bytes.extend_from_slice(b"f\0");
        bytes.extend_from_slice(&1.5f32.to_le_bytes());
        bytes.push(7);
        bytes.extend_from_slice(b"big\0");
        bytes.extend_from_slice(&(1u64 << 40).to_le_bytes());
        bytes.push(5);
        bytes.extend_from_slice(b"w\0");
        bytes.extend_from_slice(&[b'h', 0, b'i', 0, 0, 0]);
        bytes.push(8);

        let doc = parse(&bytes).unwrap();
        let cache = doc.get("cache").unwrap();
        assert_eq!(cache.get("CRC").and_then(Kv::int), Some(-5));
        assert_eq!(cache.get("nom"), Some(&Kv::Str("Élan".into())));
        assert_eq!(cache.get("1").and_then(|s| s.get("data")).and_then(Kv::int), Some(-1));
        assert_eq!(doc.get("f"), Some(&Kv::Float(1.5)));
        assert_eq!(doc.get("big").and_then(Kv::int), Some(1 << 40));
        assert_eq!(doc.get("w"), Some(&Kv::Str("hi".into())));
        assert_eq!(doc.get("absent"), None);
        assert!(doc.get("f").unwrap().entries().is_empty());
    }

    #[test]
    fn rejects_truncated_or_unknown_data() {
        let full = Bytes::default().open("a").i32("n", 7).str("s", "x").close().close().0;
        assert!(parse(&full).is_some());
        // Toute coupure, à n'importe quel octet, donne None sans panic.
        for len in 0..full.len() {
            assert_eq!(parse(&full[..len]), None, "coupé à {len}");
        }
        assert_eq!(parse(&[9, b'x', 0, 8]), None); // type inconnu
        assert_eq!(parse(&[0xFF; 64]), None);
        // Imbrication sans fin.
        let deep: Vec<u8> = std::iter::repeat_n([0u8, b'a', 0], 100).flatten().collect();
        assert_eq!(parse(&deep), None);
        assert_eq!(parse(&[8]), Some(Kv::Obj(Vec::new())));
    }
}
