//! Options de lancement choisies par l'utilisateur pour un jeu (`-novid`, `-windowed`,
//! `+fps_max 240`). Le texte est validé et découpé ici, comme Windows découpe une ligne de
//! commande (espaces, guillemets), puis passé au jeu sans jamais passer par un shell : arguments
//! du processus (`Command::args`) pour un exécutable, URI `steam://run/<appid>//<args>/` encodée
//! pour Steam. Les launchers qui ne transmettent rien par leur URI ou leur client (Epic, Ubisoft,
//! GOG Galaxy, Xbox, Riot, Battle.net par son client, lanceurs tiers) sont refusés.

use crate::library::{Game, Launch, Platform};

/// Longueur maximale du texte, en caractères.
pub const MAX_LEN: usize = 256;
/// Nombre maximal d'arguments.
pub const MAX_ARGS: usize = 32;

/// Caractères refusés : métacaractères des shells Windows (`cmd`) qui pourraient faire sortir le
/// texte de l'argument si un lanceur le recopiait dans une ligne de commande.
const FORBIDDEN: [char; 6] = ['&', '|', '<', '>', '^', '%'];

/// Texte saisi, nettoyé et validé : une ligne, sans caractère de contrôle, invisible ni
/// métacaractère de shell, guillemets appariés, `MAX_LEN` caractères et `MAX_ARGS` arguments au
/// plus. Renvoie la forme normalisée (un espace entre deux arguments, guillemets seulement où il
/// en faut), `None` si le texte est vide (options effacées).
pub fn clean(raw: &str) -> Result<Option<String>, String> {
    let text = raw.trim_matches(|c: char| c == ' ' || c == '\t');
    if text.is_empty() {
        return Ok(None);
    }
    if text.chars().count() > MAX_LEN {
        return Err(format!("{MAX_LEN} caractères au plus."));
    }
    for c in text.chars() {
        if c == '\n' || c == '\r' {
            return Err("Les options tiennent sur une seule ligne.".into());
        }
        if c != '\t' && c.is_control() || crate::library::manual::is_invisible(c) || c == '\u{00AD}' {
            return Err("Caractère invisible ou de contrôle refusé.".into());
        }
        if FORBIDDEN.contains(&c) {
            return Err(format!("Le caractère « {c} » n'est pas accepté dans les options."));
        }
    }
    let args = split(text)?;
    if args.len() > MAX_ARGS {
        return Err(format!("{MAX_ARGS} options au plus."));
    }
    Ok((!args.is_empty()).then(|| join(&args)))
}

/// Découpe un texte en arguments comme `CommandLineToArgvW` : espaces et tabulations séparent les
/// arguments hors guillemets, `"` ouvre ou ferme un passage entre guillemets, `\"` est un
/// guillemet littéral (2n barres obliques inverses devant `"` = n barres et un guillemet qui
/// ouvre ou ferme, 2n+1 = n barres et un guillemet littéral), ailleurs `\` est littéral.
/// Un guillemet resté ouvert est refusé ; les arguments vides (`""`) sont écartés.
pub fn split(text: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let mut count = 1;
                while chars.peek() == Some(&'\\') {
                    chars.next();
                    count += 1;
                }
                if chars.peek() == Some(&'"') {
                    current.extend(std::iter::repeat_n('\\', count / 2));
                    if count % 2 == 1 {
                        chars.next();
                        current.push('"');
                    }
                } else {
                    current.extend(std::iter::repeat_n('\\', count));
                }
            }
            '"' => quoted = !quoted,
            ' ' | '\t' if !quoted => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(c),
        }
    }
    if quoted {
        return Err("Un guillemet n'est pas fermé.".into());
    }
    if !current.is_empty() {
        args.push(current);
    }
    Ok(args)
}

/// Réécrit des arguments en une ligne que `split` relit à l'identique (règles de Windows).
pub fn join(args: &[String]) -> String {
    args.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ")
}

fn quote(arg: &str) -> String {
    let spaced = arg.is_empty() || arg.contains([' ', '\t']);
    if !spaced && !arg.contains('"') {
        return arg.to_string();
    }
    let mut out = String::new();
    if spaced {
        out.push('"');
    }
    let mut backslashes = 0;
    for c in arg.chars() {
        if c == '\\' {
            backslashes += 1;
        } else {
            if c == '"' {
                // Les barres déjà écrites sont doublées, plus une pour le guillemet littéral.
                out.extend(std::iter::repeat_n('\\', backslashes + 1));
            }
            backslashes = 0;
        }
        out.push(c);
    }
    if spaced {
        out.extend(std::iter::repeat_n('\\', backslashes));
        out.push('"');
    }
    out
}

/// Encodage pour un segment d'URI : seuls les caractères non réservés (lettres et chiffres ASCII,
/// `-`, `_`, `.`, `~`) restent tels quels, tout le reste en `%XX` (UTF-8), `+` et `/` compris.
fn encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// URI Steam qui lance un jeu avec des options : `steam://run/<appid>//<options encodées>/`.
/// `None` si l'appid n'est pas un nombre.
pub fn steam_uri(appid: &str, args: &[String]) -> Option<String> {
    if appid.is_empty() || !appid.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(format!("steam://run/{appid}//{}/", encode(&join(args))))
}

/// Pourquoi ce jeu ne peut pas recevoir d'options, `None` s'il le peut : jeu Steam, ou
/// exécutable du jeu lui-même (« Autres », jeux manuels, EA, Battle.net sans son client,
/// Rockstar, GOG sans Galaxy, itch.io).
pub fn refusal(game: &Game) -> Option<&'static str> {
    match (game.platform, &game.launch) {
        (Platform::Steam, Launch::Uri(_)) if steam_uri(&game.platform_id, &[]).is_some() => None,
        (Platform::Steam, _) => Some("Steam ne reconnaît pas ce jeu : options impossibles."),
        (Platform::Epic, _) => {
            Some("Epic Games ne transmet pas d'options : règle-les dans le launcher Epic (Paramètres, Arguments de ligne de commande supplémentaires).")
        }
        (Platform::Ubisoft, _) => Some("Ubisoft Connect ne transmet pas d'options : règle-les dans les propriétés du jeu chez Ubisoft Connect."),
        (Platform::Gog, Launch::Exec { .. }) if !game.launcher_tracks_playtime => None,
        (Platform::Gog, _) => Some("Lancé par GOG Galaxy, qui ne transmet pas d'options : règle-les dans Galaxy."),
        (Platform::Xbox, _) => Some("Les jeux du Microsoft Store ne reçoivent pas d'options de lancement."),
        (Platform::Riot, _) => Some("Le client Riot ne transmet pas d'options de lancement."),
        (Platform::Amazon, _) => Some("L'appli Amazon Games ne transmet pas d'options de lancement."),
        (Platform::Itch, Launch::Uri(_)) => Some("Lancé par l'appli itch (pas d'exécutable Windows), qui ne transmet pas d'options."),
        (Platform::Battlenet, Launch::Uri(_)) => {
            Some("Lancé par Battle.net, qui ne transmet pas d'options : règle-les dans ses paramètres du jeu.")
        }
        (_, Launch::Exec { .. }) if game.via_launcher => Some("Ce jeu passe par son propre lanceur, qui ne transmet pas d'options."),
        (Platform::Other | Platform::Ea | Platform::Battlenet | Platform::Rockstar | Platform::Itch, Launch::Exec { .. }) => None,
        (_, Launch::Uri(_)) => Some("Ce lanceur ne transmet pas d'options de lancement."),
    }
}

/// Lancement à faire, options comprises, reconstruit depuis le jeu de l'état. Options refusées
/// pour ce jeu (launcher changé depuis l'enregistrement) : lancement sans elles. Texte devenu
/// invalide (fichier modifié à la main) : erreur plutôt qu'un lancement sans les options voulues.
pub fn launch_for(game: &Game) -> Result<Launch, String> {
    let Some(text) = game.launch_args.as_deref() else { return Ok(game.launch.clone()) };
    if refusal(game).is_some() {
        return Ok(game.launch.clone());
    }
    let cleaned = clean(text).map_err(|e| format!("Options de lancement invalides : {e} Corrige-les dans la fiche."))?;
    let Some(cleaned) = cleaned else { return Ok(game.launch.clone()) };
    let extra = split(&cleaned)?;
    match &game.launch {
        Launch::Uri(_) => steam_uri(&game.platform_id, &extra).map(Launch::Uri).ok_or_else(|| "Appid Steam invalide.".to_string()),
        Launch::Exec { path, args } => Ok(Launch::Exec { path: path.clone(), args: args.iter().cloned().chain(extra).collect() }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn exec(platform: Platform) -> Game {
        Game::new(platform, "x", "Jeu", Launch::Exec { path: PathBuf::from(r"C:\Jeux\jeu.exe"), args: vec!["-base".into()] })
    }

    fn steam() -> Game {
        Game::new(Platform::Steam, "730", "Counter-Strike 2", Launch::Uri("steam://rungameid/730".into()))
    }

    #[test]
    fn nettoie_et_normalise() {
        assert_eq!(clean("  -novid   -windowed ").unwrap().as_deref(), Some("-novid -windowed"));
        assert_eq!(clean("+fps_max 240\t-dx11").unwrap().as_deref(), Some("+fps_max 240 -dx11"));
        assert_eq!(clean(r#"-config "D:\Mes jeux\cfg.ini""#).unwrap().as_deref(), Some(r#"-config "D:\Mes jeux\cfg.ini""#));
        assert_eq!(clean(r#"-name "solo""#).unwrap().as_deref(), Some("-name solo"));
        assert_eq!(clean("   ").unwrap(), None);
        assert_eq!(clean(r#""""#).unwrap(), None);
        assert_eq!(clean("-écran").unwrap().as_deref(), Some("-écran"));
    }

    #[test]
    fn refuse_ce_qui_sort_de_l_argument() {
        for bad in ["-novid & calc", "a | b", "a > out.txt", "a < in", "a ^b", "%APPDATA%", "-novid\n-console", "a\rb", "a\u{0}b", "a\u{1b}b", "a\u{202E}b", "a\u{200B}b", r#"-name "solo"#] {
            assert!(clean(bad).is_err(), "{bad:?} aurait dû être refusé");
        }
        assert!(clean(&"a".repeat(MAX_LEN)).is_ok());
        assert!(clean(&"a".repeat(MAX_LEN + 1)).is_err());
        assert!(clean(&vec!["-a"; MAX_ARGS].join(" ")).is_ok());
        assert!(clean(&vec!["-a"; MAX_ARGS + 1].join(" ")).is_err());
    }

    #[test]
    fn decoupe_comme_windows() {
        let s = |t: &str| split(t).unwrap();
        assert_eq!(s(r#"-a "b c" d"#), ["-a", "b c", "d"]);
        assert_eq!(s(r#"a\"b"#), [r#"a"b"#]);
        assert_eq!(s(r#"a\\"b c""#), [r#"a\b c"#]);
        assert_eq!(s(r#"a\\\"b"#), [r#"a\"b"#]);
        assert_eq!(s(r"C:\Jeux\x"), [r"C:\Jeux\x"]);
        assert_eq!(s(r#"pre"fixe et"suite"#), ["prefixe etsuite"]);
        assert!(split(r#"a "b"#).is_err());
    }

    #[test]
    fn relecture_a_l_identique() {
        let cases: Vec<Vec<String>> = vec![
            vec!["-novid".into(), "+fps_max".into(), "240".into()],
            vec![r"D:\Mes jeux\".into(), "x".into()],
            vec![r#"say "salut""#.into()],
            vec![r#"a\"b"#.into(), r"fin\\".into()],
            vec!["tab\tici".into()],
        ];
        for args in cases {
            assert_eq!(split(&join(&args)).unwrap(), args, "{}", join(&args));
        }
    }

    #[test]
    fn uri_steam_encodee() {
        let args = split("-novid +fps_max 240").unwrap();
        assert_eq!(steam_uri("730", &args).unwrap(), "steam://run/730//-novid%20%2Bfps_max%20240/");
        let args = split(r#"-config "D:\a b/c" -é"#).unwrap();
        assert_eq!(steam_uri("10", &args).unwrap(), "steam://run/10//-config%20%22D%3A%5Ca%20b%2Fc%22%20-%C3%A9/");
        assert_eq!(steam_uri("73a", &args), None);
        assert_eq!(steam_uri("", &args), None);
    }

    #[test]
    fn refus_par_launcher() {
        assert_eq!(refusal(&steam()), None);
        for p in [Platform::Other, Platform::Ea, Platform::Battlenet, Platform::Rockstar, Platform::Gog, Platform::Itch] {
            assert_eq!(refusal(&exec(p)), None, "{p:?}");
        }
        let mut galaxy = exec(Platform::Gog);
        galaxy.launcher_tracks_playtime = true;
        assert!(refusal(&galaxy).is_some());
        let mut ankama = exec(Platform::Other);
        ankama.via_launcher = true;
        assert!(refusal(&ankama).is_some());
        for p in [Platform::Xbox, Platform::Riot, Platform::Amazon] {
            assert!(refusal(&exec(p)).is_some(), "{p:?}");
        }
        for (p, uri) in [
            (Platform::Epic, "com.epicgames.launcher://apps/Sugar?action=launch"),
            (Platform::Ubisoft, "uplay://launch/1/0"),
            (Platform::Battlenet, "battlenet://S2"),
            (Platform::Amazon, "amazon-games://play/amzn1.adg.product.x"),
            (Platform::Itch, "itch://install?game_id=1&launch"),
        ] {
            assert!(refusal(&Game::new(p, "x", "Jeu", Launch::Uri(uri.into()))).is_some(), "{p:?}");
        }
        let odd = Game::new(Platform::Steam, "abc", "Jeu", Launch::Uri("steam://rungameid/abc".into()));
        assert!(refusal(&odd).is_some());
    }

    #[test]
    fn lancement_avec_options() {
        let mut g = steam();
        assert!(matches!(launch_for(&g).unwrap(), Launch::Uri(u) if u == "steam://rungameid/730"));
        g.launch_args = Some("-novid -console".into());
        assert!(matches!(launch_for(&g).unwrap(), Launch::Uri(u) if u == "steam://run/730//-novid%20-console/"));

        let mut e = exec(Platform::Other);
        e.launch_args = Some(r#"-windowed -name "Joueur 1""#.into());
        let Launch::Exec { args, .. } = launch_for(&e).unwrap() else { panic!() };
        assert_eq!(args, ["-base", "-windowed", "-name", "Joueur 1"]);

        // Texte modifié à la main dans le fichier : refusé au lancement.
        e.launch_args = Some("-a & calc".into());
        assert!(launch_for(&e).is_err());

        // Launcher qui refuse : lancement inchangé.
        let mut epic = Game::new(Platform::Epic, "Sugar", "Rocket League", Launch::Uri("com.epicgames.launcher://x".into()));
        epic.launch_args = Some("-novid".into());
        assert!(matches!(launch_for(&epic).unwrap(), Launch::Uri(u) if u == "com.epicgames.launcher://x"));
    }
}
