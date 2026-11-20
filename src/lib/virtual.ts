// Virtualisation de la grille et de la liste : au-delà de `VIRTUAL_MIN` jeux, seules les rangées
// proches de l'écran sont rendues, les autres sont remplacées par des cales de la même hauteur.
// Calculs purs (testés) ; la mesure du DOM est dans `useVirtualRows`.

/** Nombre de jeux à partir duquel la grille et la liste ne rendent que les rangées proches de l'écran. */
export const VIRTUAL_MIN = 300;

/**
 * Rangées de la grille (ou lignes de la liste) : toutes de même hauteur `size`, séparées par `gap`.
 * `extra` : élément inséré sous une rangée (la fiche dépliée), de hauteur `size`, suivi lui aussi de `gap`.
 */
export interface Rows {
  count: number;
  size: number;
  gap: number;
  extra: { row: number; size: number } | null;
}

/** Haut d'une rangée, depuis le haut de la première. */
export function rowTop(rows: Rows, i: number): number {
  const { size, gap, extra } = rows;
  return i * (size + gap) + (extra && i > extra.row ? extra.size + gap : 0);
}

/** Bas d'une rangée, fiche comprise si elle est dessous. */
export function rowEnd(rows: Rows, i: number): number {
  const { size, gap, extra } = rows;
  return rowTop(rows, i) + size + (extra && extra.row === i ? gap + extra.size : 0);
}

/** Hauteur totale des rangées. */
export function totalHeight(rows: Rows): number {
  return rows.count > 0 ? rowEnd(rows, rows.count - 1) : 0;
}

/** Rangée qui occupe la hauteur `y` (un écart compte pour la rangée qui le suit), bornée aux rangées existantes. */
export function rowAt(rows: Rows, y: number): number {
  const { count, size, gap, extra } = rows;
  if (count <= 0) return 0;
  const step = size + gap;
  if (step <= 0) return 0;
  const last = count - 1;
  if (extra && extra.row < last) {
    // Sous la fiche, les rangées sont décalées de sa hauteur.
    const after = rowTop(rows, extra.row + 1);
    if (y >= after - gap) return Math.min(last, extra.row + 1 + Math.max(0, Math.floor((y - after + gap) / step)));
    if (y >= rowTop(rows, extra.row)) return extra.row;
  }
  return Math.min(last, Math.max(0, Math.floor((y + gap) / step)));
}

/** Première et dernière rangées qui touchent l'intervalle `[top, bottom]` (coordonnées des rangées). */
export function rowsBetween(rows: Rows, top: number, bottom: number): [number, number] {
  if (rows.count <= 0) return [0, -1];
  const first = rowAt(rows, top);
  return [first, Math.max(first, rowAt(rows, bottom))];
}

/** Morceau à rendre : des rangées (bornes comprises) ou une cale de cette hauteur. */
export type Segment = { kind: "rows"; first: number; last: number } | { kind: "space"; size: number };

/**
 * Plan de rendu : rangées `[first, last]` plus les rangées `keep` (jeu choisi : son focus, sa fiche),
 * les rangées absentes remplacées par des cales. Une cale occupe la place d'une rangée de la grille,
 * donc elle a un écart de chaque côté : sa hauteur les déduit pour que tout tombe au pixel.
 */
export function plan(rows: Rows, [first, last]: [number, number], keep: readonly number[] = []): Segment[] {
  const shown = new Set<number>();
  for (let i = Math.max(0, first); i <= Math.min(rows.count - 1, last); i++) shown.add(i);
  for (const k of keep) if (k >= 0 && k < rows.count) shown.add(k);
  const sorted = [...shown].sort((a, b) => a - b);
  const out: Segment[] = [];
  if (!sorted.length) {
    if (rows.count > 0) out.push({ kind: "space", size: totalHeight(rows) });
    return out;
  }
  const { gap } = rows;
  if (sorted[0] > 0) out.push({ kind: "space", size: rowTop(rows, sorted[0]) - gap });
  let start = sorted[0];
  for (let j = 1; j <= sorted.length; j++) {
    const prev = sorted[j - 1];
    const cur = sorted[j];
    if (cur === prev + 1) continue;
    out.push({ kind: "rows", first: start, last: prev });
    if (cur !== undefined) {
      out.push({ kind: "space", size: rowTop(rows, cur) - rowEnd(rows, prev) - 2 * gap });
      start = cur;
    }
  }
  const end = sorted[sorted.length - 1];
  if (end < rows.count - 1) out.push({ kind: "space", size: totalHeight(rows) - rowEnd(rows, end) - gap });
  return out;
}
