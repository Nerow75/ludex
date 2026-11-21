import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { jeu } from "../test/jeu";
import { cardLabel, gamesCount, libraryAnnouncement, positionText, rowLabel, trapNext } from "./a11y";

const MAINTENANT = new Date("2026-11-21T15:00:00+01:00");
const now = Math.floor(MAINTENANT.getTime() / 1000);

beforeAll(() => {
  vi.useFakeTimers();
  vi.setSystemTime(MAINTENANT);
});
afterAll(() => vi.useRealTimers());

describe("gamesCount", () => {
  it("accorde le pluriel", () => {
    expect(gamesCount(0)).toBe("0 jeu");
    expect(gamesCount(1)).toBe("1 jeu");
    expect(gamesCount(12)).toBe("12 jeux");
  });
});

describe("cardLabel", () => {
  it("lit titre, launcher et la ligne sous le titre", () => {
    expect(cardLabel(jeu({ title: "Hades" }), false, "il y a 2 jours")).toBe("Hades, Steam, il y a 2 jours");
  });

  it("ajoute favori et pourcentage des succès", () => {
    const g = jeu({ title: "Hades", favorite: true, achievements: { unlocked: 40, total: 50, lastUnlocked: null } });
    expect(cardLabel(g, false, "12 h")).toBe("Hades, Steam, favori, 12 h, succès 80 %");
  });

  it("« en jeu » remplace la ligne sous le titre, comme à l'écran", () => {
    expect(cardLabel(jeu({ title: "Hades", platform: "epic" }), true, "il y a 2 jours")).toBe("Hades, Epic Games, en jeu");
  });

  it("la mention d'installation passe avant la ligne sous le titre", () => {
    const g = jeu({ title: "Apex", installState: { kind: "updating", progress: 42 } });
    expect(cardLabel(g, false, "il y a 2 jours")).toBe("Apex, Steam, téléchargement 42 %");
  });

  it("ignore une ligne vide", () => {
    expect(cardLabel(jeu({ title: "Hades" }), false, "  ")).toBe("Hades, Steam");
  });
});

describe("rowLabel", () => {
  it("reprend les colonnes utiles de la ligne", () => {
    const g = jeu({
      title: "Hades",
      favorite: true,
      playtimeMinutes: 720,
      lastPlayed: now - 2 * 86400,
      achievements: { unlocked: 40, total: 49, lastUnlocked: null },
    });
    expect(rowLabel(g, false)).toBe("Hades, favori, Steam, temps de jeu 12 h, joué il y a 2 jours, succès 40 / 49");
  });

  it("jeu jamais lancé, sans temps ni succès", () => {
    expect(rowLabel(jeu({ title: "Portal" }), false)).toBe("Portal, Steam, jamais lancé");
  });

  it("partie en cours et succès vides écartés", () => {
    const g = jeu({ title: "Portal", playtimeMinutes: 0, achievements: { unlocked: 0, total: 0, lastUnlocked: null } });
    expect(rowLabel(g, true)).toBe("Portal, en jeu, Steam, jamais lancé");
  });
});

describe("libraryAnnouncement", () => {
  it("recherche : nombre de résultats ou aucun", () => {
    expect(libraryAnnouncement({ section: "Tous les jeux", count: 3, query: " hades ", filter: null })).toBe("3 jeux pour « hades »");
    expect(libraryAnnouncement({ section: "Tous les jeux", count: 0, query: "zzz", filter: "Steam" })).toBe("Aucun jeu ne correspond à « zzz »");
  });

  it("section seule", () => {
    expect(libraryAnnouncement({ section: "Favoris", count: 1, query: "", filter: null })).toBe("Favoris : 1 jeu");
  });

  it("filtres de launcher et autres filtres", () => {
    expect(libraryAnnouncement({ section: "Tous les jeux", count: 69, query: "", filter: "Steam", extra: ["genre RPG"] })).toBe(
      "Tous les jeux, filtre Steam, genre RPG : 69 jeux",
    );
    expect(libraryAnnouncement({ section: "Tous les jeux", count: 0, query: "", filter: null, extra: ["tag Coop"] })).toBe("Tous les jeux, tag Coop : aucun jeu");
  });
});

describe("positionText", () => {
  it("compte à partir de 1", () => {
    expect(positionText("Capture", 0, 128)).toBe("Capture 1 sur 128");
  });
});

describe("trapNext", () => {
  it("boucle dans les deux sens", () => {
    expect(trapNext(3, 2, false)).toBe(0);
    expect(trapNext(3, 0, true)).toBe(2);
    expect(trapNext(3, 1, false)).toBe(2);
  });

  it("focus hors de la liste : premier ou dernier élément", () => {
    expect(trapNext(3, -1, false)).toBe(0);
    expect(trapNext(3, -1, true)).toBe(2);
  });

  it("liste vide", () => {
    expect(trapNext(0, -1, false)).toBeNull();
  });
});
