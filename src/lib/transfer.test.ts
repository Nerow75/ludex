import { describe, expect, it } from "vitest";
import { previewOrigin, summaryParts, summaryText } from "./transfer";
import type { TransferSummary } from "./api";

const vide: TransferSummary = {
  favorites: 0,
  hidden: 0,
  statuses: 0,
  notes: 0,
  collections: 0,
  manualGames: 0,
  titles: 0,
  aliases: 0,
  genres: 0,
  launchArgs: 0,
  playtimeGames: 0,
  sessions: 0,
  covers: 0,
};

describe("summaryParts", () => {
  it("ne garde que ce qui n'est pas vide, au singulier ou au pluriel", () => {
    expect(summaryParts(vide)).toEqual([]);
    expect(summaryParts({ ...vide, favorites: 1, covers: 2, manualGames: 1 })).toEqual(["1 favori", "1 jeu ajouté à la main", "2 images perso"]);
  });

  it("sépare les milliers comme en français", () => {
    expect(summaryParts({ ...vide, sessions: 1234 })[0]).toMatch(/^1\s234 sessions$/);
  });
});

describe("summaryText", () => {
  it("rien d'autre que les réglages", () => {
    expect(summaryText(vide)).toBe("réglages seulement");
  });

  it("un, deux ou quatre éléments", () => {
    expect(summaryText({ ...vide, favorites: 3 })).toBe("3 favoris");
    expect(summaryText({ ...vide, favorites: 3, collections: 1 })).toBe("3 favoris et 1 collection");
    expect(summaryText({ ...vide, favorites: 3, collections: 2, statuses: 4, notes: 1 })).toBe("3 favoris, 2 collections, 4 statuts et 1 note");
  });

  it("au-delà de quatre, « et N autres »", () => {
    expect(summaryText({ ...vide, favorites: 3, collections: 2, statuses: 4, notes: 1, sessions: 9 })).toBe(
      "3 favoris, 2 collections, 4 statuts, 1 note et 1 autre",
    );
    expect(summaryText({ ...vide, favorites: 3, collections: 2, statuses: 4, notes: 1, sessions: 9, covers: 1 })).toBe(
      "3 favoris, 2 collections, 4 statuts, 1 note et 2 autres",
    );
  });
});

describe("previewOrigin", () => {
  const base = { file: "ludex-export-2026-11-19.zip", summary: vide };

  it("version et date de l'export", () => {
    // 19 novembre 2026 à midi, heure de Paris.
    expect(previewOrigin({ ...base, ludexVersion: "0.2.0", created: Date.UTC(2026, 10, 19, 11) / 1000 })).toBe(
      "Export de Ludex 0.2.0 du 19 novembre 2026",
    );
  });

  it("version ou date inconnues", () => {
    expect(previewOrigin({ ...base, ludexVersion: "", created: 0 })).toBe("Export de Ludex");
  });
});
