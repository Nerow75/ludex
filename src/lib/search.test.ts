import { describe, expect, it } from "vitest";
import { maxTypos, prefixDistance, prepareQuery, RANK_ALIAS, RANK_META, searchRank, titleRank } from "./search";
import { fiche, jeu } from "../test/jeu";

describe("titleRank : rang du titre", () => {
  it("0 : préfixe, article de tête ignoré", () => {
    expect(titleRank("Rocket League", "rocket")).toBe(0);
    expect(titleRank("The Witcher 3: Wild Hunt", "the witcher")).toBe(0);
    expect(titleRank("The Witcher 3: Wild Hunt", "witcher")).toBe(0);
    expect(titleRank("Rocket League", "rocketleague")).toBe(0);
  });
  it("1 : initiales, nombres et chiffres romains", () => {
    expect(titleRank("Call of Duty: Black Ops III", "cod")).toBe(1);
    expect(titleRank("Grand Theft Auto V", "gta")).toBe(1);
    expect(titleRank("Grand Theft Auto V", "gta5")).toBe(1);
    expect(titleRank("Football Manager 2020", "fm20")).toBe(1);
    expect(titleRank("Football Manager 2020", "fm2020")).toBe(1);
    expect(titleRank("Red Dead Redemption 2", "rdr2")).toBe(1);
    expect(titleRank("StarCraft II", "sc2")).toBe(1);
  });
  it("un seul chiffre ne remplace pas un nombre à plusieurs chiffres", () => {
    expect(titleRank("Football Manager 2020", "fm0")).toBeNull();
  });
  it("2 : début d'un mot", () => {
    expect(titleRank("Rocket League", "league")).toBe(2);
  });
  it("3 : sous-chaîne ou mots dans le désordre", () => {
    expect(titleRank("Rocket League", "ague")).toBe(3);
    expect(titleRank("Football Manager 2020", "manager football")).toBe(3);
  });
  it("4 et 5 : une ou deux fautes", () => {
    expect(titleRank("Rocket League", "rocet")).toBe(4);
    expect(titleRank("Rocket League", "rocketleage")).toBe(4);
    expect(titleRank("Football Manager 2020", "footbal manger")).toBe(4);
    expect(titleRank("Rocket League", "rokcet leage")).toBe(5);
  });
  it("pas de faute sous 4 lettres, première lettre juste, nombres intacts", () => {
    expect(titleRank("Rocket League", "rkt")).toBeNull();
    expect(titleRank("Rocket League", "pocket")).toBeNull();
    expect(titleRank("Football Manager 2020", "football manager 2021")).toBeNull();
  });
  it("faux positifs écartés", () => {
    expect(titleRank("Far Cry 5", "fifa")).toBeNull();
    expect(titleRank("Counter-Strike 2", "star")).toBeNull();
  });
  it("accents, casse, ®/™, apostrophes et & ignorés", () => {
    expect(titleRank("Pokémon Épée", "pokemon epee")).toBe(0);
    expect(titleRank("Assassin’s Creed® Odyssey", "assassins creed")).toBe(0);
    expect(titleRank("Tom Clancy's Rainbow Six® Siege", "TOM CLANCYS")).toBe(0);
    expect(titleRank("Dungeons & Dragons", "dungeons and")).toBe(0);
  });
  it("recherche vide ou sans lettre : rien", () => {
    expect(titleRank("Rocket League", "")).toBeNull();
    expect(titleRank("Rocket League", " ... ")).toBeNull();
  });
  it("accepte une recherche déjà préparée", () => {
    expect(titleRank("Rocket League", prepareQuery("rocet"))).toBe(4);
  });
});

describe("searchRank", () => {
  const farCry = jeu({ title: "Far Cry 5", details: fiche({ developers: ["Ubisoft Montréal"], tags: ["Coop", "Monde ouvert"] }) });

  it("titre d'abord, puis studio, éditeur ou tag", () => {
    expect(searchRank(farCry, "far")).toBe(0);
    expect(searchRank(farCry, "montreal")).toBe(RANK_META);
    expect(searchRank(farCry, "coop")).toBe(RANK_META);
    expect(searchRank(farCry, "ouvert")).toBe(RANK_META);
  });
  it("genre de la fiche, même absent des tags (« rpg », « course »)", () => {
    const forza = jeu({ title: "Forza Horizon 5", details: fiche({ genres: ["Course", "RPG"], tags: ["Monde ouvert"] }) });
    expect(searchRank(forza, "course")).toBe(RANK_META);
    expect(searchRank(forza, "rpg")).toBe(RANK_META);
    expect(searchRank(forza, "strategie")).toBeNull();
  });
  it("méta : début de mot seulement, sans fiche rien", () => {
    expect(searchRank(farCry, "soft")).toBeNull();
    expect(searchRank(jeu({ title: "Far Cry 5" }), "coop")).toBeNull();
  });
  it("« cod » classe Code Vein avant Call of Duty", () => {
    const code = searchRank(jeu({ title: "Code Vein" }), "cod")!;
    const cod = searchRank(jeu({ title: "Call of Duty" }), "cod")!;
    expect(code).toBeLessThan(cod);
  });
});

describe("prefixDistance et maxTypos", () => {
  it("distance au début le plus proche, transpositions comprises", () => {
    expect(prefixDistance("rocket", "rocketleague")).toBe(0);
    expect(prefixDistance("rokcet", "rocket")).toBe(1);
    expect(prefixDistance("rocet", "rocket", 5)).toBe(1);
  });
  it("début comparé d'au moins minLen lettres", () => {
    // « star » contre « strike » : le début « st » serait à 2, mais au moins 4 lettres comparées.
    expect(prefixDistance("star", "strike", 4)).toBe(2);
  });
  it("fautes tolérées selon la longueur", () => {
    expect([3, 4, 6, 7, 12].map(maxTypos)).toEqual([0, 1, 1, 2, 2]);
  });
});

describe("searchRank : alias choisis par l'utilisateur", () => {
  const cod = jeu({ title: "Call of Duty®", aliases: ["cod", "Modern Warfare"] });
  it("un alias est classé comme le titre", () => {
    expect(searchRank(cod, "modern")).toBe(0);
    expect(searchRank(cod, "warfare")).toBe(2);
    expect(searchRank(cod, "modrn warfare")).toBe(4);
    expect(searchRank(jeu({ title: "Gold Rush: The Game", aliases: ["Gold Mining Simulator"] }), "mining")).toBe(2);
  });
  it("le meilleur rang du titre et des alias l'emporte", () => {
    expect(searchRank(cod, "call")).toBe(0);
    expect(searchRank(jeu({ title: "Wallpaper Engine", aliases: ["fond d'écran"] }), "fond")).toBe(0);
  });
  it("un alias tapé en entier passe devant tout, casse et accents ignorés", () => {
    expect(searchRank(cod, "COD")).toBe(RANK_ALIAS);
    expect(searchRank(cod, "modern warfare")).toBe(RANK_ALIAS);
    const vein = jeu({ title: "Code Vein", lastPlayed: 2_000_000_000 });
    const q = prepareQuery("cod");
    const ranks = [vein, cod].map((g) => searchRank(g, q) ?? 99);
    expect(ranks[1]).toBeLessThan(ranks[0]);
  });
  it("sans alias ni correspondance, rien ne change", () => {
    expect(searchRank(jeu({ title: "Hades", aliases: [] }), "cod")).toBeNull();
    expect(searchRank(jeu({ title: "Hades", aliases: ["roguelike"] }), "zelda")).toBeNull();
  });
});
