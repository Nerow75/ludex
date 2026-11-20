//! Corrections par jeu choisies dans la fiche, gardées dans `user-data.json` comme les titres :
//! alias de recherche (« cod » pour Call of Duty) et genres corrigés (Wallpaper Engine n'est pas
//! un jeu d'action). Tout est nettoyé ici : le front n'envoie que du texte ou des noms de genre.

use crate::library::Game;
use crate::library::manual;
use crate::metadata::{self, Details};

/// Caractères d'un alias, après nettoyage.
pub const MAX_ALIAS: usize = 40;
/// Alias gardés par jeu.
pub const MAX_ALIASES: usize = 8;
/// Genres reçus du front au plus avant tout tri (la liste de Ludex en a moins).
const MAX_GENRES_INPUT: usize = 16;

/// Alias nettoyés depuis des morceaux saisis : contrôles et invisibles retirés, espaces regroupés,
/// `MAX_ALIAS` caractères chacun, doublons écartés sans tenir compte de la casse, `MAX_ALIASES`
/// au plus. Liste vide : aucun alias.
pub fn clean_alias_list<'a>(pieces: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for piece in pieces {
        let Some(alias) = manual::clean_text(piece, MAX_ALIAS) else { continue };
        let lower = alias.to_lowercase();
        if !out.iter().any(|a| a.to_lowercase() == lower) {
            out.push(alias);
        }
        if out.len() == MAX_ALIASES {
            break;
        }
    }
    out
}

/// Alias saisis d'un bloc, séparés par des virgules (« cod, call of duty »).
pub fn clean_aliases(text: &str) -> Vec<String> {
    clean_alias_list(text.split(','))
}

/// Nom d'un genre de la liste de Ludex, casse et espaces autour ignorés. `None` : genre inconnu.
fn known_genre(raw: &str) -> Option<&'static str> {
    let wanted = raw.trim().to_lowercase();
    metadata::genre_names().find(|name| name.to_lowercase() == wanted)
}

/// Genres choisis dans la fiche : seulement ceux de la liste de Ludex, sans doublon, trois au
/// plus. Liste vide : retour aux genres de Steam.
pub fn clean_genres(raw: &[String]) -> Result<Vec<String>, String> {
    if raw.len() > MAX_GENRES_INPUT {
        return Err(format!("{} genres au plus.", metadata::MAX_GENRES));
    }
    let mut out: Vec<String> = Vec::new();
    for g in raw {
        let name = known_genre(g).ok_or_else(|| format!("Genre inconnu : « {} ».", manual::clean_text(g, 40).unwrap_or_default()))?;
        if !out.iter().any(|k| k == name) {
            out.push(name.to_string());
        }
    }
    if out.len() > metadata::MAX_GENRES {
        return Err(format!("{} genres au plus.", metadata::MAX_GENRES));
    }
    Ok(out)
}

/// Genres relus d'un fichier (import, `user-data.json` modifié à la main) : genres inconnus
/// écartés plutôt que tout refuser, doublons écartés, trois au plus.
pub fn known_genres(raw: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in raw.iter().filter_map(|g| known_genre(g)) {
        if !out.iter().any(|k| k == name) && out.len() < metadata::MAX_GENRES {
            out.push(name.to_string());
        }
    }
    out
}

/// Pose les genres corrigés sur un jeu, après sa fiche. Un jeu sans fiche Steam reçoit une fiche
/// vide qui ne porte que ces genres. `None` ou liste vide : genres de la fiche laissés tels quels.
pub fn apply_genres(game: &mut Game, genres: Option<&Vec<String>>) {
    match genres.filter(|g| !g.is_empty()) {
        Some(list) => {
            game.details.get_or_insert_with(Details::default).genres = list.clone();
            game.custom_genres = true;
        }
        None => game.custom_genres = false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{Launch, Platform};

    #[test]
    fn aliases_are_cleaned_bounded_and_unique() {
        assert_eq!(clean_aliases(" cod ,  Call\tof   Duty , ,COD, cod\u{200B} "), vec!["cod", "Call of Duty"]);
        assert!(clean_aliases(" , \u{FEFF} ,\n").is_empty());
        let long = "a".repeat(80);
        assert_eq!(clean_aliases(&long)[0].chars().count(), MAX_ALIAS);
        let many: Vec<String> = (0..20).map(|i| format!("alias {i}")).collect();
        let list = clean_aliases(&many.join(","));
        assert_eq!(list.len(), MAX_ALIASES);
        assert_eq!(list[0], "alias 0");
        assert_eq!(clean_aliases("Jeu\u{0}\u{7}2"), vec!["Jeu 2"]);
    }

    #[test]
    fn only_known_genres_are_accepted() {
        let g = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(clean_genres(&g(&[" rpg", "Stratégie", "RPG"])).unwrap(), vec!["RPG", "Stratégie"]);
        assert!(clean_genres(&[]).unwrap().is_empty());
        assert!(clean_genres(&g(&["Fantaisie"])).unwrap_err().contains("Genre inconnu"));
        assert!(clean_genres(&g(&["Action", "RPG", "Sport", "Course"])).unwrap_err().contains("3 genres"));
        assert!(clean_genres(&g(&["RPG"; 17])).is_err());
        // Relecture d'un fichier : l'inconnu et le surplus sont écartés sans tout perdre.
        assert_eq!(known_genres(&g(&["Fantaisie", "simulation", "Action", "Action", "Sport", "Course"])), vec!["Simulation", "Action", "Sport"]);
    }

    #[test]
    fn corrected_genres_replace_steam_ones() {
        let mut game = Game::new(Platform::Steam, "431960", "Wallpaper Engine", Launch::Uri(String::new()));
        game.details = Some(Details { genres: vec!["Action".into()], tags: vec!["Utilitaires".into()], ..Details::default() });
        apply_genres(&mut game, Some(&vec!["Occasionnel".into()]));
        let d = game.details.as_ref().unwrap();
        assert_eq!(d.genres, vec!["Occasionnel"]);
        assert_eq!(d.tags, vec!["Utilitaires"], "le reste de la fiche ne bouge pas");
        assert!(game.custom_genres);

        apply_genres(&mut game, Some(&Vec::new()));
        assert!(!game.custom_genres);

        // Jeu sans fiche : une fiche vide qui ne porte que les genres.
        let mut riot = Game::new(Platform::Riot, "lol", "League of Legends", Launch::Uri(String::new()));
        apply_genres(&mut riot, None);
        assert!(riot.details.is_none() && !riot.custom_genres);
        apply_genres(&mut riot, Some(&vec!["Stratégie".into()]));
        assert_eq!(riot.details.as_ref().unwrap().genres, vec!["Stratégie"]);
        assert!(riot.details.as_ref().unwrap().description.is_none());
    }
}
