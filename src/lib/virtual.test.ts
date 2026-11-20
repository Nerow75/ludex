import { describe, expect, it } from "vitest";
import { plan, rowAt, rowEnd, rowsBetween, rowTop, totalHeight, type Rows, type Segment } from "./virtual";

// Grille : rangées de 250 px séparées de 24 px (pas de 274 px).
const grille: Rows = { count: 100, size: 250, gap: 24, extra: null };
// Même grille, fiche de 600 px sous la rangée 10.
const fiche: Rows = { ...grille, extra: { row: 10, size: 600 } };

/** Hauteur rendue d'un plan : rangées et cales, un écart entre deux éléments. */
function rendu(rows: Rows, segments: Segment[]): number {
  let h = 0;
  let items = 0;
  for (const s of segments) {
    if (s.kind === "space") {
      h += s.size;
      items += 1;
    } else {
      for (let i = s.first; i <= s.last; i++) {
        h += rows.size;
        items += 1;
        if (rows.extra?.row === i) {
          h += rows.extra.size;
          items += 1;
        }
      }
    }
  }
  return h + Math.max(0, items - 1) * rows.gap;
}

/** Haut réel de la rangée `row` dans le rendu d'un plan. */
function hautRendu(rows: Rows, segments: Segment[], row: number): number | null {
  let y = 0;
  for (const s of segments) {
    if (s.kind === "space") {
      y += s.size + rows.gap;
      continue;
    }
    for (let i = s.first; i <= s.last; i++) {
      if (i === row) return y;
      y += rows.size + rows.gap;
      if (rows.extra?.row === i) y += rows.extra.size + rows.gap;
    }
  }
  return null;
}

describe("positions des rangées", () => {
  it("place les rangées au pas de la grille", () => {
    expect(rowTop(grille, 0)).toBe(0);
    expect(rowTop(grille, 3)).toBe(822);
    expect(rowEnd(grille, 3)).toBe(1072);
    expect(totalHeight(grille)).toBe(99 * 274 + 250);
    expect(totalHeight({ ...grille, count: 0 })).toBe(0);
  });

  it("décale les rangées qui suivent la fiche", () => {
    expect(rowTop(fiche, 10)).toBe(2740);
    expect(rowEnd(fiche, 10)).toBe(2740 + 250 + 24 + 600);
    expect(rowTop(fiche, 11)).toBe(11 * 274 + 624);
    expect(totalHeight(fiche)).toBe(totalHeight(grille) + 624);
  });

  it("retrouve la rangée sous une hauteur", () => {
    expect(rowAt(grille, 0)).toBe(0);
    expect(rowAt(grille, 249)).toBe(0);
    // L'écart compte pour la rangée qui le suit.
    expect(rowAt(grille, 251)).toBe(1);
    expect(rowAt(grille, 274)).toBe(1);
    expect(rowAt(grille, -500)).toBe(0);
    expect(rowAt(grille, 1e9)).toBe(99);
  });

  it("compte la fiche avec sa rangée", () => {
    expect(rowAt(fiche, rowTop(fiche, 10) + 10)).toBe(10);
    expect(rowAt(fiche, rowTop(fiche, 10) + 700)).toBe(10);
    expect(rowAt(fiche, rowTop(fiche, 11) - 10)).toBe(11);
    expect(rowAt(fiche, rowTop(fiche, 11) + 5)).toBe(11);
    expect(rowAt(fiche, rowTop(fiche, 40) + 5)).toBe(40);
    for (let i = 0; i < 100; i++) expect(rowAt(fiche, rowTop(fiche, i))).toBe(i);
  });

  it("gère la fiche sous la dernière rangée", () => {
    const fin: Rows = { ...grille, extra: { row: 99, size: 600 } };
    expect(rowAt(fin, totalHeight(fin) - 1)).toBe(99);
  });

  it("donne les rangées d'un intervalle", () => {
    expect(rowsBetween(grille, 0, 790)).toEqual([0, 2]);
    expect(rowsBetween(grille, 0, 800)).toEqual([0, 3]); // dans l'écart avant la rangée 3
    expect(rowsBetween(grille, 1000, 1500)).toEqual([3, 5]);
    expect(rowsBetween({ ...grille, count: 0 }, 0, 800)).toEqual([0, -1]);
  });
});

describe("plan", () => {
  it("rend tout sans cale quand l'intervalle couvre tout", () => {
    expect(plan(grille, [0, 99])).toEqual([{ kind: "rows", first: 0, last: 99 }]);
  });

  it("remplace les rangées absentes par des cales de la bonne hauteur", () => {
    const p = plan(grille, [20, 30]);
    expect(p).toEqual([
      { kind: "space", size: 20 * 250 + 19 * 24 },
      { kind: "rows", first: 20, last: 30 },
      { kind: "space", size: 69 * 250 + 68 * 24 },
    ]);
    expect(rendu(grille, p)).toBe(totalHeight(grille));
    expect(hautRendu(grille, p, 20)).toBe(rowTop(grille, 20));
    expect(hautRendu(grille, p, 30)).toBe(rowTop(grille, 30));
  });

  it("garde la rangée choisie, loin de l'écran, avec sa fiche", () => {
    const p = plan(fiche, [60, 70], [10]);
    expect(p).toEqual([
      { kind: "space", size: 10 * 250 + 9 * 24 },
      { kind: "rows", first: 10, last: 10 },
      { kind: "space", size: 49 * 250 + 48 * 24 },
      { kind: "rows", first: 60, last: 70 },
      { kind: "space", size: 29 * 250 + 28 * 24 },
    ]);
    expect(rendu(fiche, p)).toBe(totalHeight(fiche));
    for (const r of [10, 60, 65, 70]) expect(hautRendu(fiche, p, r)).toBe(rowTop(fiche, r));
  });

  it("fond une rangée gardée voisine de l'intervalle", () => {
    expect(plan(grille, [5, 9], [10])).toEqual([
      { kind: "space", size: 5 * 250 + 4 * 24 },
      { kind: "rows", first: 5, last: 10 },
      { kind: "space", size: 89 * 250 + 88 * 24 },
    ]);
    expect(plan(grille, [0, 3], [3])[0]).toEqual({ kind: "rows", first: 0, last: 3 });
  });

  it("garde une rangée après l'intervalle", () => {
    const p = plan(fiche, [0, 4], [10]);
    expect(p.map((s) => s.kind)).toEqual(["rows", "space", "rows", "space"]);
    expect(rendu(fiche, p)).toBe(totalHeight(fiche));
    expect(hautRendu(fiche, p, 10)).toBe(rowTop(fiche, 10));
  });

  it("ignore les rangées hors bornes et une liste vide", () => {
    expect(plan(grille, [-5, 2], [-1, 500])).toEqual([
      { kind: "rows", first: 0, last: 2 },
      { kind: "space", size: 97 * 250 + 96 * 24 },
    ]);
    expect(plan({ ...grille, count: 0 }, [0, -1])).toEqual([]);
  });

  it("tombe juste pour n'importe quel intervalle", () => {
    for (const [a, b, k] of [[0, 0, 99], [99, 99, 0], [40, 41, 42], [12, 80, 11], [50, 52, 10]] as const) {
      const p = plan(fiche, [a, b], [k]);
      expect(rendu(fiche, p)).toBe(totalHeight(fiche));
      expect(hautRendu(fiche, p, k)).toBe(rowTop(fiche, k));
      expect(hautRendu(fiche, p, a)).toBe(rowTop(fiche, a));
    }
  });
});
