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
//!
//! Le détail (`details`) n'est lu qu'à la demande, jamais pendant le scan : chaque entrée de `bits`
//! porte `name` (nom interne, clé des pourcentages mondiaux) et `display > name, desc` (une valeur
//! par langue, plus `token`), `display > hidden` (0 ou 1) et `display > icon, icon_gray` (fichier
//! haché, servi par le CDN de Steam sous `steamcommunity/public/images/apps/<appid>/`).

use super::kvbin::{self, Kv};
use super::Achievements;
use serde::Serialize;
use std::fs;
use std::path::Path;

/// Un succès avec la progression du compte, pour la liste de la fiche et la vitrine.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Achievement {
    /// Nom interne du schéma : clé des pourcentages mondiaux.
    pub api_name: String,
    /// Nom affiché (français si le schéma le traduit, sinon anglais). Vide pour un succès caché
    /// pas encore débloqué : ni son nom, ni sa description, ni ses icônes ne sont envoyés.
    pub name: String,
    pub description: Option<String>,
    /// Icône en couleur (débloqué) et grisée, sur le CDN de Steam.
    pub icon: Option<String>,
    pub icon_gray: Option<String>,
    pub hidden: bool,
    pub unlocked: bool,
    /// Date du déblocage (secondes Unix), si Steam l'a gardée.
    pub unlocked_at: Option<u64>,
    /// Part des joueurs Steam qui l'ont débloqué, en %. `None` : inconnue (hors ligne…).
    pub global_percent: Option<f32>,
}

/// Hôte et chemin des icônes de succès. L'origine est déjà dans `img-src` de la CSP.
const ICON_BASE: &str = "https://cdn.cloudflare.steamstatic.com/steamcommunity/public/images/apps";

/// URL d'une icône : appid numérique et nom de fichier haché (lettres, chiffres, `.`, `_`, `-`),
/// rien d'autre ne passe dans l'adresse.
fn icon_url(appid: &str, file: &str) -> Option<String> {
    let file = file.trim();
    let safe = !file.is_empty()
        && file.len() <= 128
        && !file.contains("..")
        && file.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    let numeric = !appid.is_empty() && appid.bytes().all(|b| b.is_ascii_digit());
    (safe && numeric).then(|| format!("{ICON_BASE}/{appid}/{file}"))
}

/// Texte d'une langue de `display > name` ou `desc` : langue de Ludex (français, sinon anglais ; en
/// anglais : anglais, sinon français), sinon la première langue renseignée. Certains vieux schémas y mettent directement une chaîne.
fn localized(node: &Kv) -> Option<String> {
    let clean = |s: &str| Some(s.trim().to_string()).filter(|s| !s.is_empty());
    if let Kv::Str(s) = node {
        return clean(s);
    }
    let lang = |key: &str| match node.get(key) {
        Some(Kv::Str(s)) => clean(s),
        _ => None,
    };
    let (first, second) = if crate::i18n::english() { ("english", "french") } else { ("french", "english") };
    lang(first).or_else(|| lang(second)).or_else(|| {
        node.entries().iter().find_map(|(k, v)| match v {
            Kv::Str(s) if !k.eq_ignore_ascii_case("token") => clean(s),
            _ => None,
        })
    })
}

/// `hidden` vaut 0 ou 1 (entier sur ce PC, chaîne dans d'anciens schémas).
fn is_hidden(display: Option<&Kv>) -> bool {
    match display.and_then(|d| d.get("hidden")) {
        Some(Kv::Int(n)) => *n != 0,
        Some(Kv::Str(s)) => s.trim() == "1" || s.trim().eq_ignore_ascii_case("true"),
        _ => false,
    }
}

/// Tous les succès du schéma, dans son ordre, avec la progression du compte.
fn list(schema: &Kv, user: &Kv, appid: &str) -> Vec<Achievement> {
    let Some(stats) = schema.get(appid).and_then(|game| game.get("stats")) else { return Vec::new() };
    let cache = user.get("cache");
    let mut out = Vec::new();
    for (id, stat) in stats.entries() {
        let Some(bits) = stat.get("bits") else { continue };
        let progress = cache.and_then(|c| c.get(id));
        // int32 signé dans le fichier : seul le motif de bits compte.
        let mask = progress.and_then(|s| s.get("data")).and_then(Kv::int).unwrap_or(0) as u32;
        let times = progress.and_then(|s| s.get("AchievementTimes"));
        for (n, bit) in bits.entries() {
            let Some(n) = n.parse::<u32>().ok().filter(|&n| n < 32) else { continue };
            let display = bit.get("display");
            let text = |key: &str| display.and_then(|d| d.get(key)).and_then(localized);
            let icon = |key: &str| match display.and_then(|d| d.get(key)) {
                Some(Kv::Str(file)) => icon_url(appid, file),
                _ => None,
            };
            let unlocked = mask >> n & 1 == 1;
            let unlocked_at = times
                .filter(|_| unlocked)
                .and_then(|t| t.get(&n.to_string()))
                .and_then(Kv::int)
                .and_then(|t| u64::try_from(t).ok())
                .filter(|&t| t > 0);
            let api_name = match bit.get("name") {
                Some(Kv::Str(s)) if !s.trim().is_empty() => s.trim().to_string(),
                _ => format!("{id}_{n}"),
            };
            let hidden = is_hidden(display);
            // Succès caché pas encore débloqué : rien qui le dévoile ne sort d'ici.
            let secret = hidden && !unlocked;
            out.push(Achievement {
                name: if secret { String::new() } else { text("name").unwrap_or_else(|| api_name.clone()) },
                description: if secret { None } else { text("desc") },
                icon: if secret { None } else { icon("icon") },
                icon_gray: if secret { None } else { icon("icon_gray").or_else(|| icon("icon")) },
                api_name,
                hidden,
                unlocked,
                unlocked_at,
                global_percent: None,
            });
        }
    }
    out
}

/// Liste complète des succès d'un jeu pour un compte, lue à la demande. `None` : pas de succès,
/// fichier absent ou illisible (même règle que `read` : sans progression, on ne dit pas « 0 »).
pub fn details(stats_dir: &Path, account: u64, appid: &str) -> Option<Vec<Achievement>> {
    let schema = fs::read(stats_dir.join(format!("UserGameStatsSchema_{appid}.bin"))).ok()?;
    let user = fs::read(stats_dir.join(format!("UserGameStats_{account}_{appid}.bin"))).ok()?;
    let all = list(&kvbin::parse(&schema)?, &kvbin::parse(&user)?, appid);
    (!all.is_empty()).then_some(all)
}

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
    let mut unlock_times: Vec<u64> = Vec::new();
    for (id, numbers) in bits {
        let Some(stat) = cache.get(id) else { continue };
        // int32 signé dans le fichier : seul le motif de bits compte.
        let mask = stat.get("data").and_then(Kv::int).unwrap_or(0) as u32;
        let times = stat.get("AchievementTimes");
        for &n in numbers.iter().filter(|&&n| mask >> n & 1 == 1) {
            unlocked += 1;
            let time = times.and_then(|t| t.get(&n.to_string())).and_then(Kv::int);
            if let Some(t) = time.and_then(|t| u64::try_from(t).ok()).filter(|&t| t > 0) {
                unlock_times.push(t);
            }
        }
    }
    unlock_times.sort_unstable();
    Some(Achievements { unlocked, total, last_unlocked: unlock_times.last().copied(), unlock_times })
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
    use super::{details, icon_url, progress, read, schema_bits};
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
        assert_eq!(got, Some(Achievements {
            unlocked: 2,
            total: 4,
            last_unlocked: Some(1_700_000_000),
            unlock_times: vec![1_600_000_000, 1_700_000_000],
        }));
        // Aucun succès débloqué : 0 sur 4, sans date.
        let empty = Bytes::default().open("cache").i32("crc", 0).close().close().0;
        assert_eq!(progress(&bits, &kvbin::parse(&empty).unwrap()), Some(Achievements { unlocked: 0, total: 4, ..Default::default() }));
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

    /// Succès complet, comme dans les schémas de ce PC : nom interne, nom et description par langue,
    /// caché ou non, icônes hachées.
    fn full_bit(b: Bytes, n: &str, api: &str, french: Option<&str>, hidden: i32, icon: &str) -> Bytes {
        let mut b = b.open(n).str("name", api).open("display").open("name").str("english", &format!("{api} en"));
        if let Some(fr) = french {
            b = b.str("french", fr);
        }
        b.str("token", "NEW_ACHIEVEMENT_1_0_NAME")
            .close()
            .open("desc")
            .str("english", "Description")
            .str("token", "NEW_ACHIEVEMENT_1_0_DESC")
            .close()
            .i32("hidden", hidden)
            .str("icon", icon)
            .str("icon_gray", "gris.jpg")
            .close()
            .close()
    }

    #[test]
    fn reads_names_icons_hidden_and_dates() {
        let mut b = Bytes::default().open("440").open("stats").open("1").str("type", "ACHIEVEMENTS").open("bits");
        b = full_bit(b, "0", "FIRST", Some("Premier"), 0, "abc123.jpg");
        b = full_bit(b, "1", "SECRET", Some("Secret"), 1, "def456.jpg");
        b = full_bit(b, "2", "OPEN_SECRET", None, 1, "../../evil");
        b = full_bit(b, "3", "LOCKED", None, 0, "aaa.jpg");
        let schema = b.close().close().close().close().close().0;
        // Débloqués : 0 (avec date) et 2 (caché, sans date).
        let user = Bytes::default()
            .open("cache")
            .open("1")
            .i32("data", 0b0101)
            .open("AchievementTimes")
            .i32("0", 1_650_000_000)
            .close()
            .close()
            .close()
            .close()
            .0;
        let dir = std::env::temp_dir().join(format!("ludex-stats-detail-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("UserGameStatsSchema_440.bin"), &schema).unwrap();
        assert_eq!(details(&dir, 5, "440"), None); // sans progression : inconnu
        fs::write(dir.join("UserGameStats_5_440.bin"), &user).unwrap();
        let all = details(&dir, 5, "440").unwrap();
        let _ = fs::remove_dir_all(&dir);

        assert_eq!(all.len(), 4);
        let first = &all[0];
        assert_eq!((first.api_name.as_str(), first.name.as_str()), ("FIRST", "Premier")); // français d'abord
        assert_eq!(first.description.as_deref(), Some("Description"));
        assert_eq!(first.icon.as_deref(), Some("https://cdn.cloudflare.steamstatic.com/steamcommunity/public/images/apps/440/abc123.jpg"));
        assert!(first.icon_gray.as_deref().unwrap().ends_with("/440/gris.jpg"));
        assert!(first.unlocked && first.unlocked_at == Some(1_650_000_000) && !first.hidden);

        // Caché et verrouillé : rien ne le dévoile.
        let secret = &all[1];
        assert!(secret.hidden && !secret.unlocked);
        assert!(secret.name.is_empty() && secret.description.is_none() && secret.icon.is_none() && secret.icon_gray.is_none());
        assert_eq!(secret.api_name, "SECRET");

        // Caché mais débloqué : affiché, nom anglais à défaut de français ; icône douteuse écartée.
        let open = &all[2];
        assert!(open.hidden && open.unlocked && open.unlocked_at.is_none());
        assert_eq!(open.name, "OPEN_SECRET en");
        assert_eq!(open.icon, None);

        assert!(!all[3].unlocked && all[3].unlocked_at.is_none());
        // Même total que la lecture du scan.
        assert_eq!(super::schema_bits(&kvbin::parse(&schema).unwrap(), "440")[0].1.len(), all.len());
    }

    #[test]
    fn names_follow_the_language() {
        let bytes = Bytes::default()
            .open("name")
            .str("french", "Premier")
            .str("english", "First")
            .close()
            .open("desc")
            .str("french", "Seulement en français")
            .close()
            .close()
            .0;
        let root = kvbin::parse(&bytes).unwrap();
        let (name, desc) = (root.get("name").unwrap(), root.get("desc").unwrap());
        assert_eq!(super::localized(name).as_deref(), Some("Premier"));
        crate::i18n::with(crate::i18n::Lang::En, || {
            assert_eq!(super::localized(name).as_deref(), Some("First"));
            // Pas d'anglais : le français à défaut.
            assert_eq!(super::localized(desc).as_deref(), Some("Seulement en français"));
        });
    }

    #[test]
    fn icon_urls_stay_on_the_cdn() {
        assert!(icon_url("440", "a1b2.jpg").is_some());
        for bad in ["", "../x.jpg", "a/b.jpg", "a?b=1", "x y.jpg", "https://evil/x.jpg"] {
            assert_eq!(icon_url("440", bad), None, "{bad}");
        }
        assert_eq!(icon_url("44a", "a.jpg"), None);
        assert_eq!(icon_url("", "a.jpg"), None);
    }
}
