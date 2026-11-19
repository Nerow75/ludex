import { afterEach, describe, expect, it, vi } from "vitest";
import {
  hasFacets,
  libraryMoods,
  libraryTags,
  matchesFacets,
  matchesFilter,
  matchesStatus,
  NO_FACETS,
  recentGames,
  remember,
  remembered,
  rememberedFacets,
  rememberFacets,
  rememberedText,
  selectGames,
} from "./library";
import { fiche, jeu, jeuAmbiance } from "../test/jeu";

const titres = (games: { title: string }[]) => games.map((g) => g.title);

describe("matchesFilter", () => {
  const steam = jeu({ platform: "steam" });
  const epic = jeu({ platform: "epic", favorite: true });
  const masque = jeu({ platform: "steam", hidden: true, favorite: true });

  it("tous : jeux non masqués", () => {
    expect([steam, epic, masque].filter((g) => matchesFilter(g, "all"))).toEqual([steam, epic]);
  });
  it("masqués : seulement eux", () => {
    expect([steam, epic, masque].filter((g) => matchesFilter(g, "hidden"))).toEqual([masque]);
  });
  it("favoris : un favori masqué reste caché", () => {
    expect([steam, epic, masque].filter((g) => matchesFilter(g, "favorites"))).toEqual([epic]);
  });
  it("par launcher", () => {
    expect([steam, epic, masque].filter((g) => matchesFilter(g, "steam"))).toEqual([steam]);
    expect([steam, epic, masque].filter((g) => matchesFilter(g, "gog"))).toEqual([]);
  });
});

describe("matchesStatus", () => {
  const afaire = jeu({ status: "todo" });
  const aucun = jeu();
  it("tous, sans statut, un statut précis", () => {
    expect(matchesStatus(afaire, "any")).toBe(true);
    expect(matchesStatus(aucun, "any")).toBe(true);
    expect(matchesStatus(aucun, "none")).toBe(true);
    expect(matchesStatus(afaire, "none")).toBe(false);
    expect(matchesStatus(afaire, "todo")).toBe(true);
    expect(matchesStatus(afaire, "done")).toBe(false);
  });
});

describe("facettes tag et ambiance", () => {
  const coop = jeuAmbiance(["friends"], {}, ["Coop", "Survie"]);
  const calme = jeuAmbiance(["chill"], {}, ["Gestion"]);
  const sansFiche = jeu();

  it("sans facette, tout passe", () => {
    expect(hasFacets(NO_FACETS)).toBe(false);
    expect([coop, calme, sansFiche].every((g) => matchesFacets(g, NO_FACETS))).toBe(true);
  });
  it("tag et ambiance se cumulent ; un jeu sans fiche ne passe pas", () => {
    expect(matchesFacets(coop, { tag: "Coop", mood: null })).toBe(true);
    expect(matchesFacets(coop, { tag: "Coop", mood: "chill" })).toBe(false);
    expect(matchesFacets(calme, { tag: null, mood: "chill" })).toBe(true);
    expect(matchesFacets(sansFiche, { tag: "Coop", mood: null })).toBe(false);
    expect(hasFacets({ tag: null, mood: "story" })).toBe(true);
  });
});

describe("selectGames : tri", () => {
  const a = jeu({ title: "Alan Wake", lastPlayed: 100, playtimeMinutes: 0, sizeOnDisk: 50 });
  const b = jeu({ title: "Baldur's Gate 3", lastPlayed: 300, playtimeMinutes: 6000, sizeOnDisk: 150e9 });
  const c = jeu({ title: "Celeste", lastPlayed: null, playtimeMinutes: null, sizeOnDisk: null });
  const d = jeu({ title: "Ćamelot", lastPlayed: 300, playtimeMinutes: 30, sizeOnDisk: 2e9 });
  const tous = [c, a, d, b];

  it("Récents : le plus récent d'abord, titre à égalité, jamais joué en dernier", () => {
    expect(titres(selectGames(tous, "all", "recent", ""))).toEqual(["Baldur's Gate 3", "Ćamelot", "Alan Wake", "Celeste"]);
  });
  it("Plus joués : jamais joué (0) avant inconnu (null)", () => {
    expect(titres(selectGames(tous, "all", "playtime", ""))).toEqual(["Baldur's Gate 3", "Ćamelot", "Alan Wake", "Celeste"]);
  });
  it("A → Z : accents ignorés", () => {
    expect(titres(selectGames(tous, "all", "title", ""))).toEqual(["Alan Wake", "Baldur's Gate 3", "Ćamelot", "Celeste"]);
  });
  it("A → Z : ® et ™ ignorés", () => {
    const x = jeu({ title: "®Zed" });
    const y = jeu({ title: "Ymir" });
    expect(titres(selectGames([x, y], "all", "title", ""))).toEqual(["Ymir", "®Zed"]);
  });
  it("Taille : le plus gros d'abord, inconnue en dernier", () => {
    expect(titres(selectGames(tous, "all", "size", ""))).toEqual(["Baldur's Gate 3", "Ćamelot", "Alan Wake", "Celeste"]);
  });
  it("ne modifie pas la liste reçue", () => {
    const copie = [...tous];
    selectGames(tous, "all", "title", "");
    expect(tous).toEqual(copie);
  });
});

describe("selectGames : filtre, facettes et recherche", () => {
  const rocket = jeu({ title: "Rocket League", platform: "epic", lastPlayed: 10, details: fiche({ tags: ["Sport"] }) });
  const rocketMasque = jeu({ title: "Rocket Racing", hidden: true });
  const league = jeu({ title: "League of Legends", platform: "riot", lastPlayed: 500 });
  const fifa = jeu({ title: "EA SPORTS FC 24", platform: "ea", details: fiche({ tags: ["Sport", "Football"] }) });
  const tous = [fifa, league, rocketMasque, rocket];

  it("classe par rang de recherche avant le tri", () => {
    // « league » : début de mot pour Rocket League (2), préfixe pour League of Legends (0).
    expect(titres(selectGames(tous, "all", "title", "league"))).toEqual(["League of Legends", "Rocket League"]);
  });
  it("à rang égal, le plus récemment joué d'abord", () => {
    const vieux = jeu({ title: "Rocket Arena", lastPlayed: 1 });
    expect(titres(selectGames([vieux, rocket], "all", "title", "rocket"))).toEqual(["Rocket League", "Rocket Arena"]);
  });
  it("le titre passe avant le tag", () => {
    const sportTitre = jeu({ title: "Sports Story", lastPlayed: null });
    expect(titres(selectGames([fifa, sportTitre, rocket], "all", "title", "sport"))).toEqual(["Sports Story", "EA SPORTS FC 24", "Rocket League"]);
  });
  it("respecte le filtre : jeux masqués exclus, launcher choisi", () => {
    expect(titres(selectGames(tous, "all", "title", "rocket"))).toEqual(["Rocket League"]);
    expect(titres(selectGames(tous, "hidden", "title", "rocket"))).toEqual(["Rocket Racing"]);
    expect(titres(selectGames(tous, "riot", "title", ""))).toEqual(["League of Legends"]);
  });
  it("recherche et facettes se cumulent", () => {
    expect(titres(selectGames(tous, "all", "title", "", { tag: "Sport", mood: null }))).toEqual(["EA SPORTS FC 24", "Rocket League"]);
    expect(titres(selectGames(tous, "all", "title", "rocket", { tag: "Football", mood: null }))).toEqual([]);
  });
  it("recherche faite d'espaces : pas de recherche", () => {
    expect(selectGames(tous, "all", "title", "   ")).toHaveLength(3);
  });
});

describe("recentGames", () => {
  it("jeux joués non masqués, du plus récent au plus ancien, n au plus", () => {
    const a = jeu({ title: "A", lastPlayed: 10 });
    const b = jeu({ title: "B", lastPlayed: 30 });
    const c = jeu({ title: "C", lastPlayed: 20 });
    const masque = jeu({ title: "M", lastPlayed: 99, hidden: true });
    const jamais = jeu({ title: "J" });
    expect(titres(recentGames([a, b, c, masque, jamais], 2))).toEqual(["B", "C"]);
    expect(titres(recentGames([a, b, c, masque, jamais], 10))).toEqual(["B", "C", "A"]);
  });
});

describe("libraryTags et libraryMoods", () => {
  const jeux = [
    jeuAmbiance(["friends", "intense"], {}, ["Coop", "Action"]),
    jeuAmbiance(["friends"], {}, ["Coop", "Coop"]),
    jeuAmbiance(["chill"], {}, ["Action", "Gestion"]),
    jeuAmbiance(["chill"], { hidden: true }, ["Gestion", "Zen"]),
    jeu(),
  ];

  it("tags comptés une fois par jeu, masqués exclus, du plus fréquent au moins fréquent puis par nom", () => {
    expect(libraryTags(jeux)).toEqual([
      { name: "Action", count: 2 },
      { name: "Coop", count: 2 },
      { name: "Gestion", count: 1 },
    ]);
  });
  it("ambiances dans l'ordre fixe, zéro compris", () => {
    expect(libraryMoods(jeux)).toEqual([
      { mood: "chill", count: 1 },
      { mood: "intense", count: 1 },
      { mood: "story", count: 0 },
      { mood: "friends", count: 2 },
    ]);
  });
});

describe("préférences retenues", () => {
  afterEach(() => vi.unstubAllGlobals());

  const stockage = () => {
    const data = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (k: string) => data.get(k) ?? null,
      setItem: (k: string, v: string) => void data.set(k, v),
    });
    return data;
  };

  it("sans stockage (Node), la valeur par défaut", () => {
    expect(remembered("sort", ["recent", "title"], "recent")).toBe("recent");
    expect(rememberedText("section")).toBeNull();
    expect(rememberedFacets()).toEqual(NO_FACETS);
    expect(() => remember("sort", "title")).not.toThrow();
  });
  it("valeur gardée avec le préfixe ludex., valeur inconnue ignorée", () => {
    const data = stockage();
    remember("sort", "title");
    expect(data.get("ludex.sort")).toBe("title");
    expect(remembered("sort", ["recent", "title"], "recent")).toBe("title");
    data.set("ludex.sort", "inconnu");
    expect(remembered("sort", ["recent", "title"], "recent")).toBe("recent");
  });
  it("facettes : aller-retour, ambiance inconnue écartée, tag gardé tel quel", () => {
    const data = stockage();
    rememberFacets({ tag: "Coop", mood: "chill" });
    expect(rememberedFacets()).toEqual({ tag: "Coop", mood: "chill" });
    rememberFacets(NO_FACETS);
    expect(rememberedFacets()).toEqual(NO_FACETS);
    data.set("ludex.mood", "joyeux");
    data.set("ludex.tag", "Tag disparu");
    expect(rememberedFacets()).toEqual({ tag: "Tag disparu", mood: null });
  });
});
