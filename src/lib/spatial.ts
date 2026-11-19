export type Direction = "up" | "down" | "left" | "right";

/**
 * Élément voisin à l'écran dans une direction, pour les écrans en plusieurs rangées (« Pour toi ») :
 * gauche et droite restent dans la même rangée (éléments qui se chevauchent en hauteur), haut et bas
 * passent à la rangée la plus proche puis à l'élément le plus proche en largeur. `null` : aucun.
 */
export function spatialNext(items: readonly HTMLElement[], from: HTMLElement, dir: Direction): HTMLElement | null {
  const r = from.getBoundingClientRect();
  const cx = r.left + r.width / 2;
  let best: HTMLElement | null = null;
  let bestScore = [Infinity, Infinity];
  for (const el of items) {
    if (el === from) continue;
    const b = el.getBoundingClientRect();
    if (b.width === 0 && b.height === 0) continue; // caché
    const bx = b.left + b.width / 2;
    let score: [number, number];
    if (dir === "left" || dir === "right") {
      const overlap = Math.min(r.bottom, b.bottom) - Math.max(r.top, b.top);
      if (overlap <= 0) continue;
      const gap = dir === "right" ? bx - cx : cx - bx;
      if (gap <= 1) continue;
      score = [gap, 0];
    } else {
      const gap = dir === "down" ? b.top - r.bottom : r.top - b.bottom;
      if (gap < -1) continue; // pas plus bas (ou plus haut)
      // Rangées arrondies à 8 px : deux éléments d'une même rangée comptent pareil, le plus proche en largeur gagne.
      score = [Math.round(Math.max(0, gap) / 8), Math.abs(bx - cx)];
    }
    if (score[0] < bestScore[0] || (score[0] === bestScore[0] && score[1] < bestScore[1])) {
      best = el;
      bestScore = score;
    }
  }
  return best;
}
