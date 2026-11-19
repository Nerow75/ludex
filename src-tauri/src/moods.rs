//! Ambiances d'un jeu (Chill, Intense, Histoire, Entre amis), tirées de ses tags Steam et des
//! catégories « joueurs » de la boutique.
//!
//! Calculées côté Rust plutôt que dans le front : la table se teste avec `cargo test` (le front n'a
//! pas d'outil de test), elle travaille sur les identifiants numériques des tags (indépendants de la
//! langue des noms) et le résultat arrive tout prêt dans `Details.moods`, comme les genres. Le cache
//! des fiches garde les données brutes : changer cette table ne demande pas de tout relire.
//!
//! Règles :
//! - seuls les `MOOD_RANK` tags les plus votés comptent : un tag en queue de liste décrit mal le jeu ;
//! - un jeu peut avoir plusieurs ambiances (Cyberpunk : Intense et Histoire) ;
//! - Chill et Intense s'excluent : un jeu à la fois mignon et impitoyable n'est pas reposant ; la
//!   survie (`NOT_CHILL`) écarte aussi Chill ;
//! - Entre amis vient aussi des catégories déclarées par l'éditeur (coop, écran partagé), quel que
//!   soit le rang des tags.

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mood {
    Chill,
    Intense,
    Story,
    Friends,
}

/// Tags les plus votés pris en compte.
pub const MOOD_RANK: usize = 12;

/// Détente, construction, vie paisible.
const CHILL: &[u32] = &[
    1654,    // Détente
    97376,   // Réconfortant
    552282,  // Bienveillant
    597,     // Casual
    4726,    // Mignon
    5350,    // Familial
    87918,   // Simulation de ferme
    4520,    // Ferme
    10235,   // Simulation de vie
    1220528, // Simulation de loisirs
    4328,    // Construction de villes
    1643,    // Construction
    220585,  // Simulation de colonies
    1664,    // Casse-tête
    15564,   // Pêche
    1738,    // Objets cachés
];

/// Réflexes, tension, difficulté, compétition.
const INTENSE: &[u32] = &[
    1663,    // FPS
    1774,    // Tir
    3814,    // Tir à la 3ᵉ personne
    5547,    // Tir en arène
    620519,  // Tir avec héros et héroïnes
    1199779, // Tir et extraction
    176981,  // Battle Royale
    1734,    // Intense
    4026,    // Impitoyable
    29482,   // De type Souls
    3878,    // Compétitif
    1775,    // JcJ
    1667,    // Horreur
    3978,    // Horreur et survie
    1721,    // Horreur psychologique
    4885,    // Bullet Hell
    1646,    // Hack 'n' Slash
    1743,    // Combat
    42804,   // Action et roguelike
    3877,    // Plateforme et précision
];

/// Récit, choix, enquête.
const STORY: &[u32] = &[
    1742,  // Scénario riche
    7702,  // Narratif
    6426,  // Choix multiples
    6971,  // Fins multiples
    9592,  // Narration dynamique
    3799,  // Roman graphique
    11014, // Fiction interactive
    42152, // Riche en dialogues
    3854,  // Univers riche
    5613,  // Détective
    5716,  // Mystère
    1698,  // Pointer et cliquer
    5900,  // Simulation de marche
    5608,  // Émotions
    4145,  // Cinématographique
    3796,  // Inspiré d'un roman
];

/// À plusieurs, avec des proches : coop, jeu de groupe, multijoueur local. « Multijoueur » et
/// « Jeu d'équipe » seuls n'y sont pas : sur ce PC, la moitié des jeux portent le premier (Football
/// Manager, GTA V…), et le second désigne surtout le compétitif en ligne, déjà Intense.
const FRIENDS: &[u32] = &[
    1685,   // Coop
    3843,   // Coop en ligne
    3841,   // Coop locale
    4508,   // Campagne en coop
    7368,   // Multijoueur local
    4840,   // 4 personnes en local
    7178,   // En groupe
    7108,   // Groupe
    745697, // Déduction en groupe
];

/// Pas reposant malgré une construction ou une gestion : la survie (Frostpunk, Terraria).
const NOT_CHILL: &[u32] = &[
    1662,    // Survie
    1100689, // Survie et fabrication en monde ouvert
];

/// Catégories « joueurs » de la boutique qui suffisent pour Entre amis : coop (en ligne, en LAN,
/// en écran partagé) et écran partagé. Le multijoueur compétitif seul (1, 36, 49) n'y est pas.
const FRIENDS_CATEGORIES: &[u32] = &[9, 38, 39, 48, 24, 37];

/// Ambiances d'un jeu. `tags` : du plus voté au moins voté ; `players` : catégories « joueurs ».
/// Ordre fixe : Chill, Intense, Histoire, Entre amis.
pub fn moods(tags: &[u32], players: &[u32]) -> Vec<Mood> {
    let top = &tags[..tags.len().min(MOOD_RANK)];
    let has = |table: &[u32]| top.iter().any(|t| table.contains(t));
    let intense = has(INTENSE);
    let mut out = Vec::new();
    if has(CHILL) && !intense && !has(NOT_CHILL) {
        out.push(Mood::Chill);
    }
    if intense {
        out.push(Mood::Intense);
    }
    if has(STORY) {
        out.push(Mood::Story);
    }
    if has(FRIENDS) || players.iter().any(|p| FRIENDS_CATEGORIES.contains(p)) {
        out.push(Mood::Friends);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use Mood::*;

    #[test]
    fn tables_do_not_overlap() {
        let all = [CHILL, INTENSE, STORY, FRIENDS, NOT_CHILL];
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert!(a.iter().all(|t| !b.contains(t)), "tag présent dans deux ambiances");
            }
        }
    }

    #[test]
    fn real_games() {
        // Stardew Valley : Simulation de ferme, Simulation de vie, Mignon… et coop en ligne.
        assert_eq!(moods(&[87918, 10235, 1654, 4726, 3843], &[2, 1, 9, 38]), vec![Chill, Friends]);
        // Cyberpunk 2077 : Monde ouvert, RPG, Scénario riche, FPS… solo.
        assert_eq!(moods(&[4115, 1695, 6650, 122, 4182, 1742, 1663, 3942], &[2]), vec![Intense, Story]);
        // Counter-Strike 2 : FPS, Tir, Multijoueur, Compétitif ; multijoueur compétitif seulement.
        assert_eq!(moods(&[1663, 1774, 3859, 3878, 5711], &[1, 36]), vec![Intense]);
        // Football Manager : Simulation, Sport, Gestion, Multijoueur.
        assert!(moods(&[599, 701, 12472, 3859], &[2, 1]).is_empty());
        // Frostpunk : Construction de villes, Survie.
        assert!(moods(&[4328, 1662, 8945], &[2]).is_empty());
        // Portal 2 : Casse-tête, Coop, Scénario riche.
        assert_eq!(moods(&[1664, 1685, 1742], &[2, 9, 38]), vec![Chill, Story, Friends]);
        // Wallpaper Engine : aucun tag d'ambiance.
        assert!(moods(&[87, 84], &[]).is_empty());
    }

    #[test]
    fn intense_wins_over_chill() {
        // Mignon mais impitoyable (Cuphead) : pas Chill.
        assert_eq!(moods(&[4726, 4026, 3877], &[]), vec![Intense]);
    }

    #[test]
    fn only_top_tags_count() {
        let mut tags = vec![19; MOOD_RANK]; // Action, sans ambiance
        tags.push(1742); // Scénario riche, 13ᵉ : trop loin
        assert!(moods(&tags, &[]).is_empty());
        tags.insert(0, 1742);
        assert_eq!(moods(&tags, &[]), vec![Story]);
    }

    #[test]
    fn friends_from_categories_alone() {
        assert_eq!(moods(&[], &[39]), vec![Friends]); // écran partagé en coop
        assert!(moods(&[], &[1, 36, 49]).is_empty()); // multijoueur compétitif seul
    }
}
