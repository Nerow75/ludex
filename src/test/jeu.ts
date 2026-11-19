// Fabricant de `Game` pour les tests : un jeu Steam prêt, jamais lancé, sans fiche ni statut.
import type { Game, GameDetails, Mood } from "../lib/api";

let next = 0;

/** Jeu de test : chaque appel reçoit un id unique, sauf si `id` est donné. */
export function jeu(fields: Partial<Game> = {}): Game {
  next += 1;
  return {
    id: `steam:${next}`,
    title: `Jeu ${next}`,
    platform: "steam",
    platformId: String(next),
    installDir: null,
    coverUrl: null,
    iconUrl: null,
    heroUrl: null,
    logoUrl: null,
    lastPlayed: null,
    sizeOnDisk: null,
    playtimeMinutes: null,
    playtimeSource: null,
    achievements: null,
    installState: { kind: "ready" },
    isTool: false,
    manual: false,
    favorite: false,
    hidden: false,
    launchLabel: "Steam",
    details: null,
    customArt: [],
    status: null,
    note: null,
    hltb: null,
    launchArgs: null,
    launchArgsRefusal: null,
    ...fields,
  };
}

/** Fiche Steam de test : vide sauf les champs donnés. */
export function fiche(fields: Partial<GameDetails> = {}): GameDetails {
  return {
    genres: [],
    tags: [],
    moods: [],
    controller: null,
    developers: [],
    publishers: [],
    releaseDate: null,
    description: null,
    ...fields,
  };
}

/** Raccourci : jeu avec ces ambiances (et ces tags). */
export const jeuAmbiance = (moods: Mood[], fields: Partial<Game> = {}, tags: string[] = []) =>
  jeu({ details: fiche({ moods, tags }), ...fields });

/** Secondes Unix de l'heure figée par `vi.setSystemTime`. */
export const maintenant = () => Math.floor(Date.now() / 1000);

export const JOUR = 86_400;

/** Espace insécable fine (séparateur de milliers de `Intl` en français) remplacée par une espace simple. */
export const espaces = (s: string) => s.replace(/[  ]/g, " ");
