import { afterEach, describe, expect, it } from "vitest";
import { cleanTitle } from "./format";
import { setLang } from "./i18n";
import { byTitle } from "./library";
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

describe("byTitle : ordre partagé des titres", () => {
  afterEach(() => setLang("fr"));
  const noms = ["Élite Dangerous", "elden ring", "Ori", "œuvre", "Zelda", "ÆON", "DOOM®", "Doom 3", "Doom Eternal", "Assassin’s Creed", "assassin's creed", "Ábzu", "Baba Is You", "2048", "Ys VIII", "Éclipse", "eclipse"];
  const jeux = noms.map((title) => jeu({ title }));
  // Ordre d'avant (`localeCompare` avec options, un collateur recréé à chaque comparaison) : inchangé.
  const avant = (lang: string) => [...jeux].sort((a, b) => cleanTitle(a.title).localeCompare(cleanTitle(b.title), lang, { sensitivity: "base" }));

  it("même ordre que localeCompare, en français et en anglais", () => {
    expect(titres([...jeux].sort(byTitle))).toEqual(titres(avant("fr-FR")));
    setLang("en");
    expect(titres([...jeux].sort(byTitle))).toEqual(titres(avant("en-US")));
  });
  it("ignore accents, casse et ®", () => {
    expect(byTitle(jeu({ title: "Éclipse" }), jeu({ title: "eclipse" }))).toBe(0);
    expect(byTitle(jeu({ title: "DOOM®" }), jeu({ title: "doom" }))).toBe(0);
  });
  it("trie aussi les jeux jamais joués de la recherche rapide", () => {
    const sans = quickResults(jeux, "");
    expect(titres(sans)).toEqual(titres(avant("fr-FR")).slice(0, sans.length));
  });
});
