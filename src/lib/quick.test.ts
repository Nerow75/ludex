import { describe, expect, it } from "vitest";
import { quickResults } from "./quick";
import { jeu } from "../test/jeu";

const titres = (games: { title: string }[]) => games.map((g) => g.title);

describe("quickResults : recherche rapide", () => {
  const recent = jeu({ title: "Hades", lastPlayed: 300 });
  const ancien = jeu({ title: "Celeste", lastPlayed: 100 });
  const jamaisB = jeu({ title: "Baba Is You" });
  const jamaisA = jeu({ title: "Among Us" });
  const masque = jeu({ title: "Hollow Knight", lastPlayed: 999, hidden: true });
  const tous = [jamaisB, ancien, masque, jamaisA, recent];

  it("sans recherche : derniers jeux joués, puis les autres par titre, sans jeux masqués", () => {
    expect(titres(quickResults(tous, ""))).toEqual(["Hades", "Celeste", "Among Us", "Baba Is You"]);
    expect(titres(quickResults(tous, "  "))).toEqual(["Hades", "Celeste", "Among Us", "Baba Is You"]);
  });
  it("limite respectée, récents d'abord", () => {
    expect(titres(quickResults(tous, "", 1))).toEqual(["Hades"]);
    expect(titres(quickResults(tous, "", 3))).toEqual(["Hades", "Celeste", "Among Us"]);
  });
  it("avec recherche : même classement que la bibliothèque, jeux masqués exclus", () => {
    expect(titres(quickResults(tous, "h"))).toEqual(["Hades"]);
    expect(titres(quickResults(tous, "ba"))).toEqual(["Baba Is You"]);
  });
});
