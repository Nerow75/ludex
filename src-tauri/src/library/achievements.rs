//! Succès Steam, lus dans le cache local `<racine Steam>/appcache/stats/` (KeyValues binaire,
//! voir `kvbin.rs`) : aucun compte ni clé d'API.
//!
//! `UserGameStatsSchema_<appid>.bin` : schéma du jeu.
//! `<appid> > stats > <id de stat>` ; une stat de succès (`type` = « ACHIEVEMENTS » sur ce PC,
//! 4 dans d'anciens schémas) porte `bits > <n° de bit 0 à 31> > name, display > name > english…`.
//! Un jeu de plus de 32 succès en a plusieurs ; des numéros peuvent manquer (succès retirés).
//! Le total est donc le nombre d'entrées de `bits`, pas un nombre de bits.
//!
//! `UserGameStats_<accountid>_<appid>.bin` : progression du compte (accountid = steamid3).
//! `cache > crc, PendingChanges` puis `cache > <id de stat> > data` (int32) : pour une stat de
//! succès, masque des succès débloqués (bit n = succès n), et `AchievementTimes > <n° de bit>`
//! (int32, secondes Unix) pour chaque succès débloqué. Stat absente = rien de débloqué.

use super::kvbin::{self, Kv};
use super::Achievements;
use std::fs;
use std::path::Path;

/// Succès déclarés par le schéma : (id de stat, numéros de bit).
fn schema_bits(schema: &Kv, appid: &str) -> Vec<(String, Vec<u32>)> {
    let Some(stats) = schema.get(appid).and_then(|game| game.get("stats")) else { return Vec::new() };
    stats
        .entries()
        .iter()
        .filter_map(|(id, stat)| {
            let bits: Vec<u32> = stat
                .get("bits")?
                .entries()
                .iter()
                .filter_map(|(n, _)| n.parse().ok())
                .filter(|&n| n < 32)
                .collect();
            (!bits.is_empty()).then(|| (id.to_string(), bits))
        })
        .collect()
}

/// Progression du compte d'après son fichier de stats. `None` si le schéma n'a aucun succès.
fn progress(bits: &[(String, Vec<u32>)], user: &Kv) -> Option<Achievements> {
    let total = bits.iter().map(|(_, b)| b.len() as u32).sum();
    if total == 0 {
        return None;
    }
    let cache = user.get("cache")?;
    let mut unlocked = 0;
    let mut last_unlocked: Option<u64> = None;
    for (id, numbers) in bits {
        let Some(stat) = cache.get(id) else { continue };
        // int32 signé dans le fichier : seul le motif de bits compte.
        let mask = stat.get("data").and_then(Kv::int).unwrap_or(0) as u32;
        let times = stat.get("AchievementTimes");
        for &n in numbers.iter().filter(|&&n| mask >> n & 1 == 1) {
            unlocked += 1;
            let time = times.and_then(|t| t.get(&n.to_string())).and_then(Kv::int);
            if let Some(t) = time.and_then(|t| u64::try_from(t).ok()).filter(|&t| t > 0) {
                last_unlocked = Some(last_unlocked.map_or(t, |l| l.max(t)));
            }
        }
    }
    Some(Achievements { unlocked, total, last_unlocked })
}

/// Succès d'un jeu pour un compte. `None` : pas de succès, fichier absent ou illisible.
/// Sans fichier de progression, Steam n'a jamais chargé les stats du compte : on ne dit pas « 0 ».
pub fn read(stats_dir: &Path, account: u64, appid: &str) -> Option<Achievements> {
    let schema = fs::read(stats_dir.join(format!("UserGameStatsSchema_{appid}.bin"))).ok()?;
    let bits = schema_bits(&kvbin::parse(&schema)?, appid);
    if bits.is_empty() {
        return None;
    }
    let user = fs::read(stats_dir.join(format!("UserGameStats_{account}_{appid}.bin"))).ok()?;
    progress(&bits, &kvbin::parse(&user)?)
}

#[cfg(test)]
mod tests {
    use super::{progress, read, schema_bits};
    use crate::library::kvbin::tests::Bytes;
    use crate::library::kvbin;
    use crate::library::Achievements;
    use std::fs;

    /// Schéma : stat 1 = succès 0, 1, 3 (le 2 retiré) ; stat 2 = succès 0 ; stat 3 = compteur.
    fn schema() -> Vec<u8> {
        let bit = |b: Bytes, n: &str| b.open(n).str("name", "ACH").open("display").open("name").str("english", "Nom").close().close().close();
        let mut b = Bytes::default().open("10").open("stats").open("1").str("type", "ACHIEVEMENTS").open("bits");
        for n in ["0", "1", "3"] {
            b = bit(b, n);
        }
        b = b.close().close().open("2").i32("type", 4).open("bits");
        b = bit(b, "0").close().close();
        b.open("3").str("type", "INT").str("name", "kills").close().close().str("gamename", "Jeu").close().close().0
    }

    /// Progression : stat 1 = bits 0 et 3 (+ bit 2 sans succès), stat 2 absente.
    fn user() -> Vec<u8> {
        Bytes::default()
            .open("cache")
            .i32("crc", 123)
            .i32("PendingChanges", 0)
            .open("1")
            .i32("data", 0b1101)
            .open("AchievementTimes")
            .i32("0", 1_600_000_000)
            .i32("3", 1_700_000_000)
            .close()
            .close()
            .open("3")
            .i32("data", 42)
            .close()
            .close()
            .close()
            .0
    }

    #[test]
    fn counts_declared_bits_only() {
        let bits = schema_bits(&kvbin::parse(&schema()).unwrap(), "10");
        assert_eq!(bits, vec![("1".to_string(), vec![0, 1, 3]), ("2".to_string(), vec![0])]);
        let got = progress(&bits, &kvbin::parse(&user()).unwrap());
        assert_eq!(got, Some(Achievements { unlocked: 2, total: 4, last_unlocked: Some(1_700_000_000) }));
        // Aucun succès débloqué : 0 sur 4, sans date.
        let empty = Bytes::default().open("cache").i32("crc", 0).close().close().0;
        assert_eq!(progress(&bits, &kvbin::parse(&empty).unwrap()), Some(Achievements { unlocked: 0, total: 4, last_unlocked: None }));
        // Masque négatif (bit 31) : int32 lu comme un motif de bits.
        let all = [("1".to_string(), vec![0, 31])];
        let full = Bytes::default().open("cache").open("1").i32("data", i32::MIN | 1).close().close().close().0;
        assert_eq!(progress(&all, &kvbin::parse(&full).unwrap()).map(|a| a.unlocked), Some(2));
        // Mauvais appid ou schéma sans succès : rien.
        assert!(schema_bits(&kvbin::parse(&schema()).unwrap(), "20").is_empty());
        assert_eq!(progress(&[], &kvbin::parse(&user()).unwrap()), None);
    }

    #[test]
    fn reads_files_of_one_account() {
        let dir = std::env::temp_dir().join(format!("ludex-stats-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("UserGameStatsSchema_10.bin"), schema()).unwrap();
        // Sans fichier de progression : inconnu.
        assert_eq!(read(&dir, 77, "10"), None);
        fs::write(dir.join("UserGameStats_77_10.bin"), user()).unwrap();
        assert_eq!(read(&dir, 77, "10").map(|a| (a.unlocked, a.total)), Some((2, 4)));
        assert_eq!(read(&dir, 78, "10"), None); // autre compte
        // Fichier tronqué : None, pas de panic.
        let u = user();
        fs::write(dir.join("UserGameStats_77_10.bin"), &u[..u.len() / 2]).unwrap();
        assert_eq!(read(&dir, 77, "10"), None);
        assert_eq!(read(&dir, 77, "30"), None); // pas de schéma
        let _ = fs::remove_dir_all(&dir);
    }
}
