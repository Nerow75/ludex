import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { historyGroups } from "./history";
import { jeu, JOUR, maintenant } from "../test/jeu";

beforeEach(() => {
  vi.useFakeTimers();
  // 19 novembre 2026, 21 h à Paris.
  vi.setSystemTime(new Date("2026-11-19T20:00:00Z"));
});
afterEach(() => vi.useRealTimers());

const ilYa = (secs: number) => maintenant() - secs;
const resume = (games: ReturnType<typeof jeu>[]) => historyGroups(games).map((g) => [g.label, g.games.map((x) => x.title)]);

describe("historyGroups", () => {
  it("range chaque jeu dans sa période, sans période vide", () => {
    const games = [
      jeu({ title: "Ce soir", lastPlayed: ilYa(3600) }),
      jeu({ title: "Hier soir", lastPlayed: ilYa(JOUR) }),
      jeu({ title: "Lundi", lastPlayed: ilYa(3 * JOUR) }),
      jeu({ title: "Il y a 3 mois", lastPlayed: ilYa(90 * JOUR) }),
      jeu({ title: "Il y a 2 ans", lastPlayed: ilYa(730 * JOUR) }),
    ];
    expect(resume(games)).toEqual([
      ["Aujourd'hui", ["Ce soir"]],
      ["Hier", ["Hier soir"]],
      ["7 derniers jours", ["Lundi"]],
      ["12 derniers mois", ["Il y a 3 mois"]],
      ["Plus ancien", ["Il y a 2 ans"]],
    ]);
  });
  it("aujourd'hui commence à minuit, heure locale", () => {
    // 21 h 30 avant : 23 h 30 la veille à Paris.
    expect(resume([jeu({ title: "Veille", lastPlayed: ilYa(21.5 * 3600) })])).toEqual([["Hier", ["Veille"]]]);
    expect(resume([jeu({ title: "Matin", lastPlayed: ilYa(20 * 3600) })])).toEqual([["Aujourd'hui", ["Matin"]]]);
  });
  it("garde l'ordre reçu dans une période", () => {
    const games = [jeu({ title: "A", lastPlayed: ilYa(8 * JOUR) }), jeu({ title: "B", lastPlayed: ilYa(20 * JOUR) })];
    expect(resume(games)).toEqual([["30 derniers jours", ["A", "B"]]]);
  });
});
