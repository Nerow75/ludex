//! Consoles reconnues : extensions propres, mots de dossier et dépôt d'images libretro.

/// Une console émulée.
#[derive(Debug)]
pub struct System {
    /// Identifiant stable (ids des jeux, réglages) : `snes`, `ps2`, `switch`…
    pub id: &'static str,
    /// Nom affiché, le même en français et en anglais (noms propres).
    pub name: &'static str,
    /// Extensions qui ne désignent que cette console (en minuscules, sans le point).
    pub exts: &'static [&'static str],
    /// Mots d'un nom de dossier qui annoncent cette console (lettres et chiffres en minuscules).
    pub hints: &'static [&'static str],
    /// Dépôt `libretro-thumbnails` de ses jaquettes (`retro_art.rs`).
    pub repo: Option<&'static str>,
}

pub const SYSTEMS: &[System] = &[
    System { id: "nes", name: "NES", exts: &["nes", "fds", "unf", "unif"], hints: &["nes", "famicom", "nintendoentertainmentsystem"], repo: Some("Nintendo_-_Nintendo_Entertainment_System") },
    System { id: "snes", name: "Super Nintendo", exts: &["sfc", "smc", "fig", "swc", "bs"], hints: &["snes", "superfamicom", "supernintendo", "supernes", "sfc"], repo: Some("Nintendo_-_Super_Nintendo_Entertainment_System") },
    System { id: "n64", name: "Nintendo 64", exts: &["n64", "z64", "v64"], hints: &["n64", "nintendo64"], repo: Some("Nintendo_-_Nintendo_64") },
    System { id: "gb", name: "Game Boy", exts: &["gb"], hints: &["gb", "gameboy"], repo: Some("Nintendo_-_Game_Boy") },
    System { id: "gbc", name: "Game Boy Color", exts: &["gbc"], hints: &["gbc", "gameboycolor", "gameboycolour"], repo: Some("Nintendo_-_Game_Boy_Color") },
    System { id: "gba", name: "Game Boy Advance", exts: &["gba"], hints: &["gba", "gameboyadvance"], repo: Some("Nintendo_-_Game_Boy_Advance") },
    System { id: "nds", name: "Nintendo DS", exts: &["nds", "dsi"], hints: &["nds", "ds", "nintendods"], repo: Some("Nintendo_-_Nintendo_DS") },
    System { id: "n3ds", name: "Nintendo 3DS", exts: &["3ds", "cci", "cxi"], hints: &["3ds", "n3ds", "nintendo3ds"], repo: Some("Nintendo_-_Nintendo_3DS") },
    System { id: "gc", name: "GameCube", exts: &["gcm"], hints: &["gc", "gcn", "ngc", "gamecube"], repo: Some("Nintendo_-_GameCube") },
    System { id: "wii", name: "Wii", exts: &["wbfs"], hints: &["wii"], repo: Some("Nintendo_-_Wii") },
    System { id: "wiiu", name: "Wii U", exts: &["wua", "wud", "wux", "rpx"], hints: &["wiiu"], repo: Some("Nintendo_-_Wii_U") },
    System { id: "switch", name: "Nintendo Switch", exts: &["nsp", "xci", "nsz", "xcz"], hints: &["switch", "nsw", "nintendoswitch"], repo: None },
    System { id: "ps1", name: "PlayStation", exts: &[], hints: &["ps1", "psx", "psone", "playstation", "playstation1"], repo: Some("Sony_-_PlayStation") },
    System { id: "ps2", name: "PlayStation 2", exts: &[], hints: &["ps2", "playstation2"], repo: Some("Sony_-_PlayStation_2") },
    System { id: "psp", name: "PSP", exts: &[], hints: &["psp", "playstationportable"], repo: Some("Sony_-_PlayStation_Portable") },
    System { id: "ps3", name: "PlayStation 3", exts: &[], hints: &["ps3", "playstation3"], repo: Some("Sony_-_PlayStation_3") },
    System { id: "xbox", name: "Xbox", exts: &[], hints: &["xbox", "ogxbox"], repo: Some("Microsoft_-_Xbox") },
    System { id: "md", name: "Mega Drive", exts: &["md", "gen", "smd"], hints: &["md", "megadrive", "genesis"], repo: Some("Sega_-_Mega_Drive_-_Genesis") },
    System { id: "sms", name: "Master System", exts: &["sms"], hints: &["sms", "mastersystem"], repo: Some("Sega_-_Master_System_-_Mark_III") },
    System { id: "gg", name: "Game Gear", exts: &["gg"], hints: &["gg", "gamegear"], repo: Some("Sega_-_Game_Gear") },
    System { id: "saturn", name: "Saturn", exts: &[], hints: &["saturn", "segasaturn"], repo: Some("Sega_-_Saturn") },
    System { id: "dreamcast", name: "Dreamcast", exts: &["gdi", "cdi"], hints: &["dc", "dreamcast"], repo: Some("Sega_-_Dreamcast") },
    System { id: "pce", name: "PC Engine", exts: &["pce"], hints: &["pce", "pcengine", "turbografx", "turbografx16", "tg16"], repo: Some("NEC_-_PC_Engine_-_TurboGrafx_16") },
    System { id: "a2600", name: "Atari 2600", exts: &["a26"], hints: &["atari2600", "a2600", "2600"], repo: Some("Atari_-_2600") },
];

pub fn find(id: &str) -> Option<&'static System> {
    SYSTEMS.iter().find(|s| s.id == id)
}

/// Console d'une extension qui n'appartient qu'à elle.
pub fn by_ext(ext: &str) -> Option<&'static System> {
    let ext = ext.to_ascii_lowercase();
    SYSTEMS.iter().find(|s| s.exts.contains(&ext.as_str()))
}

/// Lettres et chiffres en minuscules, accents retirés pour les lettres courantes.
pub fn compact(s: &str) -> String {
    s.chars().filter_map(fold).filter(char::is_ascii_alphanumeric).collect()
}

/// Lettre ASCII minuscule d'un caractère (accents usuels retirés), `None` sinon.
pub fn fold(c: char) -> Option<char> {
    let c = c.to_lowercase().next().unwrap_or(c);
    Some(match c {
        'à' | 'â' | 'ä' | 'á' | 'ã' | 'å' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'î' | 'ï' | 'í' | 'ì' => 'i',
        'ô' | 'ö' | 'ó' | 'ò' | 'õ' => 'o',
        'û' | 'ü' | 'ú' | 'ù' => 'u',
        'ç' => 'c',
        'ñ' => 'n',
        'ÿ' => 'y',
        'œ' => 'o',
        'æ' => 'a',
        c if c.is_ascii() => c,
        _ => return None,
    })
}

/// Console annoncée par un nom de dossier (« Jeux Switch », « PS2 », « Game Boy Advance »).
/// Mot exact, ou nom collé qui contient un mot de 5 lettres au moins ; le plus long l'emporte
/// (« wiiu » plutôt que « wii », « gameboyadvance » plutôt que « gameboy »).
pub fn by_folder(name: &str) -> Option<&'static System> {
    let whole = compact(name);
    let words: Vec<String> = name
        .split(|c: char| !c.is_alphanumeric())
        .map(compact)
        .filter(|w| !w.is_empty())
        .collect();
    // Mots consécutifs collés (« Game Boy Advance » → « gameboyadvance »).
    let mut joined = Vec::new();
    for i in 0..words.len() {
        let mut acc = String::new();
        for w in &words[i..] {
            acc.push_str(w);
            joined.push(acc.clone());
        }
    }
    let mut best: Option<(&'static System, usize)> = None;
    for system in SYSTEMS {
        for hint in system.hints {
            let hit = whole == *hint || joined.iter().any(|w| w == hint) || (hint.len() >= 5 && whole.contains(hint));
            if hit && best.is_none_or(|(_, len)| hint.len() > len) {
                best = Some((system, hint.len()));
            }
        }
    }
    best.map(|(s, _)| s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_folder_names() {
        let id = |n: &str| by_folder(n).map(|s| s.id);
        assert_eq!(id("Jeux Switch"), Some("switch"));
        assert_eq!(id("Wii U"), Some("wiiu"));
        assert_eq!(id("Wii"), Some("wii"));
        assert_eq!(id("Game Boy Advance"), Some("gba"));
        assert_eq!(id("gameboy"), Some("gb"));
        assert_eq!(id("PlayStation 2"), Some("ps2"));
        assert_eq!(id("PS2"), Some("ps2"));
        assert_eq!(id("psx"), Some("ps1"));
        assert_eq!(id("n3ds"), Some("n3ds"));
        assert_eq!(id("nds"), Some("nds"));
        assert_eq!(id("Mega Drive"), Some("md"));
        assert_eq!(id("Pokemon Scarlet"), None);
        assert_eq!(id("rom"), None);
        assert_eq!(id("Jeux"), None);
    }

    #[test]
    fn extensions_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for s in SYSTEMS {
            for e in s.exts {
                assert!(seen.insert(*e), "{e} en double");
            }
        }
        assert_eq!(by_ext("NSP").map(|s| s.id), Some("switch"));
        assert!(by_ext("iso").is_none());
    }
}
