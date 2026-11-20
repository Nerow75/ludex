// Bibliothèque factice pour mesurer la grille et la liste avec beaucoup de jeux, sans toucher aux
// vraies données : seulement si Ludex est construit avec `VITE_LUDEX_FAKE_GAMES=<nombre>` (jamais
// dans une build publiée). Les jeux factices n'existent que dans le front : Rust ne les voit jamais
// (aucun cache, aucune préférence, aucune recherche en ligne) ; les lancer échoue (« jeu inconnu »).
import type { Game } from "./api";

/** Nombre de jeux voulu, 0 sans la variable (cas normal : rien n'est ajouté). */
export const FAKE_GAMES = Math.min(20_000, Math.max(0, Math.floor(Number(import.meta.env.VITE_LUDEX_FAKE_GAMES ?? 0) || 0)));

/**
 * Complète la bibliothèque jusqu'à `count` jeux avec des copies numérotées des vrais (« Hades 2 »,
 * « Hades 3 »…, id `<id>~factice-<n>`) : mêmes jaquettes et mêmes données, donc un rendu réaliste.
 * Rien d'ajouté si `count` ne dépasse pas la bibliothèque ou si elle est vide.
 */
export function withFakeGames(games: Game[], count: number): Game[] {
  if (!games.length || count <= games.length) return games;
  const out = games.slice();
  for (let i = 0; out.length < count; i++) {
    const g = games[i % games.length];
    const n = Math.floor(i / games.length) + 2;
    out.push({ ...g, id: `${g.id}~factice-${n}`, title: `${g.title} ${n}` });
  }
  return out;
}

/**
 * Virtualisation coupée pour mesurer l'avant : `localStorage["ludex.virtual"] = "off"`, lu seulement
 * dans une build à bibliothèque factice.
 */
export function virtualDisabled(): boolean {
  if (!FAKE_GAMES) return false;
  try {
    return localStorage.getItem("ludex.virtual") === "off";
  } catch {
    return false;
  }
}
