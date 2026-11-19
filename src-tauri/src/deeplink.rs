//! Lien profond `ludex://launch/<id>` : lance un jeu de la bibliothèque depuis un raccourci du
//! Bureau, la Jump List de la barre des tâches ou n'importe quel lien. Seule cette forme est
//! comprise ; l'identifiant n'est jamais interprété (ni chemin ni URI) : il est seulement cherché
//! parmi les jeux de l'état, et le lancement est reconstruit côté Rust comme pour `launch_game`.

use std::fmt;

/// Schéma déclaré dans `tauri.conf.json` (`plugins.deep-link.desktop.schemes`).
pub const SCHEME: &str = "ludex";
const LAUNCH: &str = "launch";
/// Au-delà, l'identifiant ne peut pas être celui d'un jeu.
const MAX_ID: usize = 512;

/// Lien refusé.
#[derive(Debug, PartialEq, Eq)]
pub enum LinkError {
    /// Autre schéma que `ludex://`.
    NotLudex,
    /// Autre action que `launch`.
    UnknownAction,
    /// Identifiant absent, illisible ou trop long.
    BadId,
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            LinkError::NotLudex => "Ce lien n'est pas un lien Ludex.",
            LinkError::UnknownAction => "Lien Ludex inconnu : seul « ludex://launch/<jeu> » est compris.",
            LinkError::BadId => "Lien Ludex illisible : identifiant du jeu absent ou invalide.",
        })
    }
}

/// Caractères gardés tels quels dans le lien (les autres en `%XX`, octet par octet en UTF-8).
fn is_kept(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b':')
}

/// Lien qui lance le jeu `id` : `ludex://launch/steam:730`.
pub fn launch_url(id: &str) -> String {
    let mut out = format!("{SCHEME}://{LAUNCH}/");
    for b in id.bytes() {
        if is_kept(b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn hex(b: u8) -> Option<u8> {
    (b as char).to_digit(16).map(|d| d as u8)
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hi = hex(*bytes.get(i + 1)?)?;
            let lo = hex(*bytes.get(i + 2)?)?;
            out.push(hi << 4 | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Préfixe `prefix` sans tenir compte de la casse (ASCII).
fn strip_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then(|| &text[prefix.len()..])
}

/// Identifiant du jeu d'un lien `ludex://launch/<id>`. Une barre finale (ajoutée par certains
/// navigateurs) est tolérée ; requête, fragment, sous-chemin et caractères de contrôle sont refusés.
pub fn parse(link: &str) -> Result<String, LinkError> {
    let link = link.trim();
    let rest = strip_prefix_ci(link, SCHEME).and_then(|r| r.strip_prefix("://")).ok_or(LinkError::NotLudex)?;
    let (action, raw) = rest.split_once('/').unwrap_or((rest, ""));
    if !action.eq_ignore_ascii_case(LAUNCH) {
        return Err(LinkError::UnknownAction);
    }
    let raw = raw.strip_suffix('/').unwrap_or(raw);
    if raw.is_empty() || raw.contains(['/', '?', '#']) {
        return Err(LinkError::BadId);
    }
    let id = percent_decode(raw).ok_or(LinkError::BadId)?;
    if id.trim().is_empty() || id.len() > MAX_ID || id.chars().any(char::is_control) {
        return Err(LinkError::BadId);
    }
    Ok(id)
}

/// Arguments d'une instance de Ludex lancée par un lien (`ludex.exe ludex://…`, comme le
/// transmet Windows) : elle ne doit pas afficher la fenêtre, seulement lancer le jeu.
pub fn is_link_launch(args: &[String]) -> bool {
    args.len() == 2 && strip_prefix_ci(&args[1], SCHEME).is_some_and(|r| r.starts_with(':'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_game_id() {
        assert_eq!(parse("ludex://launch/steam:730"), Ok("steam:730".into()));
        assert_eq!(parse("LUDEX://Launch/steam:730/"), Ok("steam:730".into()));
        assert_eq!(parse("  ludex://launch/riot:league_of_legends.live \n"), Ok("riot:league_of_legends.live".into()));
        assert_eq!(parse("ludex://launch/other:Jeu%20%C3%A9t%C3%A9"), Ok("other:Jeu été".into()));
    }

    #[test]
    fn link_round_trips_any_id() {
        for id in ["steam:730", "other:manual-00ff", "epic:Fortnite&co", "other:Jeux/Été 100%", "xbox:Microsoft.Minecraft_8wekyb3d8bbwe"] {
            let url = launch_url(id);
            assert!(url.is_ascii() && !url.contains(' ') && !url[8..].contains("//"), "{url}");
            assert_eq!(parse(&url), Ok(id.to_string()));
        }
        assert_eq!(launch_url("steam:730"), "ludex://launch/steam:730");
        assert_eq!(launch_url("other:a b/c"), "ludex://launch/other:a%20b%2Fc");
    }

    #[test]
    fn refuses_other_links() {
        assert_eq!(parse("steam://rungameid/730"), Err(LinkError::NotLudex));
        assert_eq!(parse("ludex:launch/steam:730"), Err(LinkError::NotLudex));
        assert_eq!(parse("ludexx://launch/steam:730"), Err(LinkError::NotLudex));
        assert_eq!(parse(""), Err(LinkError::NotLudex));
        assert_eq!(parse("ludex://open/steam:730"), Err(LinkError::UnknownAction));
        assert_eq!(parse("ludex://launch"), Err(LinkError::BadId));
        assert_eq!(parse("ludex://launch/"), Err(LinkError::BadId));
        assert_eq!(parse("ludex://launch/%20"), Err(LinkError::BadId));
        assert_eq!(parse("ludex://launch/steam:730?x=1"), Err(LinkError::BadId));
        assert_eq!(parse("ludex://launch/steam:730#x"), Err(LinkError::BadId));
        assert_eq!(parse("ludex://launch/a/b"), Err(LinkError::BadId));
        assert_eq!(parse("ludex://launch/a%0Ab"), Err(LinkError::BadId));
        assert_eq!(parse("ludex://launch/a%2"), Err(LinkError::BadId));
        assert_eq!(parse("ludex://launch/a%zz"), Err(LinkError::BadId));
        assert_eq!(parse("ludex://launch/%FF"), Err(LinkError::BadId));
        assert_eq!(parse(&format!("ludex://launch/{}", "a".repeat(MAX_ID + 1))), Err(LinkError::BadId));
    }

    #[test]
    fn spots_an_instance_started_by_a_link() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(is_link_launch(&args(&["ludex.exe", "ludex://launch/steam:730"])));
        assert!(is_link_launch(&args(&["ludex.exe", "LUDEX://x"])));
        assert!(!is_link_launch(&args(&["ludex.exe"])));
        assert!(!is_link_launch(&args(&["ludex.exe", "--minimized"])));
        assert!(!is_link_launch(&args(&["ludex.exe", "ludex://launch/a", "autre"])));
    }
}
