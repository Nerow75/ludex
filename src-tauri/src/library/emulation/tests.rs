//! Tests de la source « Émulation » sur de faux dossiers, puis relevé réel de ce PC (`--ignored`).

use super::*;
use std::fs;

/// Arbre de fichiers d'essai dans un dossier temporaire propre au test.
fn tree(name: &str, files: &[(&str, &[u8])]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("ludex-emu-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    for (path, data) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, data).unwrap();
    }
    root
}

/// Écarts notés pendant `f` (collecteur du scan).
fn with_skips<T>(f: impl FnOnce() -> T) -> (T, Vec<crate::library::Skipped>) {
    crate::library::SKIPPED.with(|s| *s.borrow_mut() = Some(Vec::new()));
    let out = f();
    let skipped = crate::library::SKIPPED.with(|s| s.borrow_mut().take()).unwrap_or_default();
    (out, skipped)
}

fn gamecube_iso() -> Vec<u8> {
    let mut data = vec![0u8; 0x40];
    data[0x1C..0x20].copy_from_slice(&0xC233_9F3Du32.to_be_bytes());
    data
}

fn found_in(root: &Path, dirs: &[&str]) -> Vec<Found> {
    let dirs: Vec<PathBuf> = dirs.iter().map(|d| root.join(d)).collect();
    find_emulators(&dirs)
}

#[test]
fn finds_emulators_and_their_rom_folders() {
    let root = tree("detect", &[
        (r"Emus\RetroArch-Win64\RetroArch-Win64\retroarch.exe", b""),
        (r"Emus\RetroArch-Win64\RetroArch-Win64\cores\mgba_libretro.dll", b""),
        (r"Emus\RetroArch-Win64\RetroArch-Win64\cores\gambatte_libretro.dll", b""),
        (r"Emus\RetroArch-Win64\RetroArch-Win64\retroarch.cfg", b"rgui_browser_directory = \"Z:\\ROMs\"\nlibretro_directory = \":\\cores\"\n"),
        (r"Emus\Ryujinx\Ryujinx.exe", b""),
        (r"Emus\Ryujinx\games\x.nsp", b""),
        (r"Emus\Dolphin\Dolphin.exe", b""),
        (r"Emus\Dolphin\portable.txt", b""),
        (r"Emus\Dolphin\User\Config\Dolphin.ini", b"[General]\nISOPaths = 1\nISOPath0 = D:\\Jeux GameCube\n"),
        (r"Emus\Outil\outil.exe", b""),
    ]);
    let found = found_in(&root, &[r"Emus\RetroArch-Win64", r"Emus\Ryujinx", r"Emus\Dolphin", r"Emus\Outil"]);
    let keys: Vec<&str> = found.iter().map(|f| f.def.key).collect();
    assert_eq!(keys, ["dolphin", "ryujinx", "retroarch"]); // ordre de la table, outil ignoré
    let retroarch = &found[2];
    assert_eq!(retroarch.systems(), ["gb", "gbc", "gba"]);
    assert!(retroarch.core_for("gba").is_some_and(|c| c.ends_with("mgba_libretro.dll")));
    assert!(retroarch.core_for("gb").is_some_and(|c| c.ends_with("gambatte_libretro.dll")));
    assert!(retroarch.core_for("snes").is_none());
    assert!(retroarch.rom_dirs.contains(&PathBuf::from(r"Z:\ROMs")));
    assert!(found[0].rom_dirs.contains(&PathBuf::from(r"D:\Jeux GameCube")));
    assert!(found[1].rom_dirs.iter().any(|d| d.ends_with(r"Ryujinx\games")));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn builds_games_from_rom_folders() {
    crate::i18n::with(crate::i18n::Lang::Fr, || {
        let cue = |bin: &str| format!("FILE \"{bin}\" BINARY\n  TRACK 01 MODE2/2352\n");
        let cue1 = cue("FF7 (Disc 1).bin");
        let cue2 = cue("FF7 (Disc 2).bin");
        let iso = gamecube_iso();
        let root = tree("build", &[
            (r"Emus\RetroArch\retroarch.exe", b""),
            (r"Emus\RetroArch\cores\mgba_libretro.dll", b""),
            (r"Emus\RetroArch\cores\gambatte_libretro.dll", b""),
            (r"Emus\mGBA\mGBA.exe", b""),
            (r"Emus\Ryujinx\Ryujinx.exe", b""),
            (r"Emus\Ryujinx\games\Pokemon Legends Z-A [0100F43008C44000][v0] (4.02 GB).nsp", b"nsp"),
            (r"Emus\Ryujinx\games\Pokemon Legends Z-A [0100F43008C44800][v65536].nsp", b""),
            (r"Emus\Ryujinx\games\v-pokemon_lets_go_eevee_v65536.nsp", b""),
            (r"Emus\Ryujinx\games\DLC\Pokemon Legends Z-A [0100F43008C45002][v0].nsp", b""),
            (r"ROMs\gba\Pokemon - Version Rubis (France).gba", b"rubis"),
            (r"ROMs\gba\Pokemon - Version Rubis (USA).gba", b"ruby"),
            (r"ROMs\gba\mods\Hack (France).gba", b""),
            (r"ROMs\Pokemon - Version Or (France) (SGB Enhanced).gbc", b"or"),
            (r"ROMs\Pokemon - Version Or (France) (SGB Enhanced).sav", b""),
            (r"ROMs\ps1\FF7.m3u", b"FF7 (Disc 1).cue\nFF7 (Disc 2).cue\n"),
            (r"ROMs\ps1\FF7 (Disc 1).cue", cue1.as_bytes()),
            (r"ROMs\ps1\FF7 (Disc 1).bin", b"1"),
            (r"ROMs\ps1\FF7 (Disc 2).cue", cue2.as_bytes()),
            (r"ROMs\ps1\FF7 (Disc 2).bin", b"2"),
            (r"ROMs\Divers\jeu.iso", &iso),
            (r"ROMs\Divers\film.iso", b"pas un jeu"),
        ]);
        let found = found_in(&root, &[r"Emus\RetroArch", r"Emus\mGBA", r"Emus\Ryujinx"]);
        // Dossier des ROMs déclaré par RetroArch.
        let roms = Root { dir: root.join("ROMs"), system: None, owner: Some("retroarch"), origin: "emulator" };
        let ryujinx = found.iter().find(|f| f.def.key == "ryujinx").unwrap();
        let switch = Root { dir: ryujinx.rom_dirs[0].clone(), system: systems::find("switch"), owner: Some("ryujinx"), origin: "emulator" };
        let ((games, summary), skipped) = with_skips(|| build(&found, &[roms, switch], &Prefs::default()));
        let ids: Vec<&str> = games.iter().map(|g| g.id.as_str()).collect();
        assert_eq!(ids.len(), 3, "{ids:?}");
        let get = |id: &str| games.iter().find(|g| g.id == id).unwrap_or_else(|| panic!("{id} absent de {ids:?}"));

        // GBA : version française gardée, lancée par RetroArch (qui déclare le dossier) et son cœur.
        let rubis = get("emu:gba-pokemon-version-rubis");
        assert_eq!(rubis.title, "Pokemon - Version Rubis");
        let Launch::Exec { path, args } = &rubis.launch else { panic!() };
        assert!(path.ends_with("retroarch.exe"));
        assert_eq!(args[0], "-L");
        assert!(args[1].ends_with("mgba_libretro.dll"));
        assert!(args[2].ends_with("Pokemon - Version Rubis (France).gba"));
        assert_eq!(rubis.size_on_disk, Some(5));
        assert!(rubis.install_dir.as_deref().is_some_and(|d| d.ends_with(r"ROMs\gba")));
        assert_eq!(rubis.platform, Platform::Emulation);
        let info = rubis.emulation.as_ref().unwrap();
        assert_eq!((info.system, info.emulator_key), ("gba", "retroarch"));
        assert!(rubis.icon_url.is_some() && rubis.local_art.icon.as_deref() == Some(path.as_path()));

        let or = get("emu:gbc-pokemon-version-or");
        assert_eq!(or.title, "Pokemon - Version Or");

        // Switch : jeu de base seulement, identifiant de titre dans l'id, ROM en argument seul.
        let za = get("emu:switch-0100f43008c44000");
        assert_eq!(za.title, "Pokemon Legends Z-A");
        let Launch::Exec { args, .. } = &za.launch else { panic!() };
        assert_eq!(args.len(), 1);
        assert_eq!(za.launch_label, "Ryujinx.exe Pokemon Legends Z-A [0100F43008C44000][v0] (4.02 GB).nsp");

        let reasons: Vec<(&str, &str)> = skipped.iter().map(|s| (s.title.as_str(), s.reason.as_str())).collect();
        assert!(reasons.contains(&("Pokemon Legends Z-A [0100F43008C44800][v65536].nsp", "Mise à jour ou contenu additionnel")));
        assert!(reasons.contains(&("v-pokemon_lets_go_eevee_v65536.nsp", "Mise à jour ou contenu additionnel")));
        assert!(reasons.contains(&("Pokemon - Version Rubis (USA).gba", "Autre version du même jeu déjà retenue")));
        assert!(reasons.contains(&("FF7.m3u", "Aucun émulateur installé pour PlayStation")), "{reasons:?}");
        assert!(reasons.contains(&("jeu.iso", "Aucun émulateur installé pour GameCube")));
        assert!(reasons.contains(&("film.iso", "Console inconnue (dossier sans nom de console)")));
        // Ni le contenu additionnel (dossier « DLC »), ni le hack (dossier « mods »), ni les disques du `.m3u`.
        assert!(!reasons.iter().any(|(t, _)| t.contains("45002") || t.contains("Hack") || t.ends_with(".cue")));

        // Choix dans les Paramètres : mGBA pour la GBA.
        let prefs = Prefs { rom_dirs: Vec::new(), choices: [("gba".to_string(), "mgba".to_string())].into() };
        let roms = Root { dir: root.join("ROMs"), system: None, owner: Some("retroarch"), origin: "emulator" };
        let (games2, summary2) = build(&found, &[roms], &prefs);
        let rubis = games2.iter().find(|g| g.id == "emu:gba-pokemon-version-rubis").unwrap();
        let Launch::Exec { path, args } = &rubis.launch else { panic!() };
        assert!(path.ends_with("mGBA.exe") && args.len() == 1);

        // Résumé pour les Paramètres.
        let gba = summary.systems.iter().find(|s| s.id == "gba").unwrap();
        assert_eq!((gba.games, gba.chosen), (1, None));
        assert_eq!(gba.emulators.iter().map(|c| c.key).collect::<Vec<_>>(), ["mgba", "retroarch"]);
        assert_eq!(summary2.systems.iter().find(|s| s.id == "gba").unwrap().chosen, Some("mgba"));
        assert!(summary.folders.iter().all(|f| f.found));
        let _ = fs::remove_dir_all(root);
    });
}

#[test]
fn prefers_disc_one_and_reads_folder_games() {
    let meta = b"<?xml version=\"1.0\"?><menu><title_id type=\"hexBinary\" length=\"8\">0005000010145D00</title_id><longname_en type=\"string\">Super Mario\n3D World</longname_en></menu>";
    let update = b"<menu><title_id type=\"hexBinary\" length=\"8\">0005000E10145D00</title_id></menu>";
    let root = tree("folders", &[
        (r"Emus\Cemu\Cemu.exe", b""),
        (r"Emus\DuckStation\duckstation-qt-x64-ReleaseLTCG.exe", b""),
        (r"Wii U\Mario 3D World\code\RedPro.rpx", b""),
        (r"Wii U\Mario 3D World\meta\meta.xml", meta),
        (r"Wii U\Mario 3D World Update\code\RedPro.rpx", b""),
        (r"Wii U\Mario 3D World Update\meta\meta.xml", update),
        (r"PS1\Metal Gear Solid (France) (Disc 2).chd", b"2"),
        (r"PS1\Metal Gear Solid (France) (Disc 1).chd", b"1"),
    ]);
    let found = found_in(&root, &[r"Emus\Cemu", r"Emus\DuckStation"]);
    let roots = [
        Root { dir: root.join("Wii U"), system: None, owner: None, origin: "user" },
        Root { dir: root.join("PS1"), system: None, owner: None, origin: "user" },
    ];
    let (games, _) = build(&found, &roots, &Prefs::default());
    let mario = games.iter().find(|g| g.id == "emu:wiiu-0005000010145d00").expect("Wii U");
    assert_eq!(mario.title, "Super Mario 3D World");
    let Launch::Exec { args, .. } = &mario.launch else { panic!() };
    assert_eq!(args[0], "-g");
    assert!(args[1].ends_with(r"Mario 3D World\code\RedPro.rpx"));
    assert!(mario.install_dir.as_deref().is_some_and(|d| d.ends_with("Mario 3D World")));
    let mgs = games.iter().find(|g| g.id == "emu:ps1-metal-gear-solid").expect("PS1");
    let Launch::Exec { args, .. } = &mgs.launch else { panic!() };
    assert_eq!(args[..2], ["-batch", "--"]);
    assert!(args[2].ends_with("(Disc 1).chd"));
    assert_eq!(games.len(), 2);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn cleans_settings_and_refuses_wide_folders() {
    let prefs = Prefs {
        rom_dirs: vec![PathBuf::from(r"D:\ROMs"), PathBuf::from(r"d:\roms\"), PathBuf::from("relatif"), PathBuf::from(r"D:\a\..\b")],
        choices: [("gba", "retroarch"), ("gba2", "mgba"), ("snes", "dolphin"), ("switch", "ryujinx")].into_iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
    }
    .clean();
    assert_eq!(prefs.rom_dirs, [PathBuf::from(r"D:\ROMs")]);
    assert_eq!(prefs.choices.len(), 2);
    assert!(valid_choice("gba", "mgba") && valid_choice("nes", "retroarch") && !valid_choice("nes", "dolphin"));
    assert!(!usable_root(Path::new(r"C:\")));
    assert!(!usable_root(Path::new(r"C:\Windows")));
    if let Some(home) = dirs::home_dir() {
        assert!(!usable_root(&home));
    }
    let root = tree("usable", &[(r"ROMs\x.gba", b"")]);
    assert!(usable_root(&root.join("ROMs")));
    let _ = fs::remove_dir_all(root);
    assert_eq!(slug("Pokémon - Version Or"), "pokemon-version-or");
    assert_eq!(slug("!!!").len(), 16);
}

/// Relevé réel de ce PC, en lecture seule : émulateurs, dossiers de ROMs, jeux par console, écarts.
/// `$env:LUDEX_ROM_DIR` ajoute un dossier comme s'il était choisi dans les Paramètres.
#[test]
#[ignore]
fn emulation_this_pc() {
    let mut prefs = Prefs::default();
    if let Some(dir) = std::env::var_os("LUDEX_ROM_DIR") {
        prefs.rom_dirs.push(PathBuf::from(dir));
    }
    configure(&prefs);
    let start = std::time::Instant::now();
    let (result, skipped) = with_skips(scan);
    let elapsed = start.elapsed();
    let games = result.expect("scan").unwrap_or_default();
    let summary = summary().unwrap_or_default();
    println!("Scan Émulation : {} jeux en {:.2} s", games.len(), elapsed.as_secs_f64());
    println!("\nÉmulateurs ({}) :", summary.emulators.len());
    for e in &summary.emulators {
        println!("  {} : {} ({})", e.name, e.path, e.systems.join(", "));
    }
    println!("\nDossiers de ROMs ({}) :", summary.folders.len());
    for f in &summary.folders {
        println!("  [{}] {} {}{}", if f.found { "lu" } else { "absent" }, f.path, f.origin, f.emulator.map(|e| format!(" ({e})")).unwrap_or_default());
    }
    println!("\nConsoles :");
    for s in &summary.systems {
        let emus: Vec<&str> = s.emulators.iter().map(|c| c.name).collect();
        println!("  {} : {} jeux, émulateurs {}", s.name, s.games, emus.join(", "));
    }
    println!("\nJeux :");
    for g in &games {
        let info = g.emulation.as_ref().unwrap();
        println!("  {} | {} | {} | {} | {}", g.id, g.title, info.system_name, info.emulator, g.launch_label);
    }
    println!("\nÉcartés ({}) :", skipped.len());
    for s in &skipped {
        println!("  {} : {}", s.title, s.reason);
    }
    println!("\nDossiers d'émulateurs écartés de « Autres » : {:?}", EMULATOR_DIRS.lock().unwrap());

    // Jaquettes libretro : listes lues sur GitHub dans un dossier temporaire (jamais celui de l'app).
    let tmp = std::env::temp_dir().join(format!("ludex-emulation-this-pc-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let start = std::time::Instant::now();
    let mut lists = Vec::new();
    crate::retro_art::resolve(&games, &tmp, |repo| lists.push(repo));
    println!("
Jaquettes libretro : {} listes lues en {:.1} s ({lists:?})", lists.len(), start.elapsed().as_secs_f64());
    let mut games = games;
    let mut first = None;
    for g in &mut games {
        if crate::retro_art::apply(g, &tmp) {
            println!("  {} : {}", g.title, g.cover_url.as_deref().unwrap_or_default());
            first = first.or(g.cover_url.clone());
        } else {
            println!("  {} : aucune jaquette", g.title);
        }
    }
    // ROM au nom No-Intro (aucune sur ce PC) : même chemin qu'un vrai jeu, sur une copie.
    if let Some(mut sample) = games.iter().find(|g| g.emulation.as_ref().is_some_and(|e| e.system == "nds")).cloned() {
        sample.emulation.as_mut().unwrap().stem = "Pokemon - Version Or HeartGold (France)".into();
        if crate::retro_art::apply(&mut sample, &tmp) {
            println!("  (essai No-Intro) {} : {}", sample.emulation.as_ref().unwrap().stem, sample.cover_url.as_deref().unwrap_or_default());
            first = first.or(sample.cover_url.clone());
        }
    }
    if let Some(url) = first {
        assert!(crate::images::allowed(&url));
        let response = crate::net::agent(std::time::Duration::from_secs(20)).get(&url).call().expect("image");
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut std::io::Read::take(response.into_reader(), 8 << 20), &mut bytes).unwrap();
        println!("  première image : {} octets, signature {:?}", bytes.len(), crate::images::sniff(&bytes));
    }
    let _ = std::fs::remove_dir_all(tmp);
}
