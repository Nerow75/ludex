import { afterEach, describe, expect, it, vi } from "vitest";
import { setLang } from "./i18n";
import {
  countGames,
  achievementPercent,
  hasFacets,
  libraryGenres,
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
  sortMeta,
  SORTS,
  SORT_LABEL,
  STATUS_FILTER_LABEL,
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
    expect(matchesFacets(coop, { tag: "Coop", mood: null, genre: null })).toBe(true);
    expect(matchesFacets(coop, { tag: "Coop", mood: "chill", genre: null })).toBe(false);
    expect(matchesFacets(calme, { tag: null, mood: "chill", genre: null })).toBe(true);
    expect(matchesFacets(sansFiche, { tag: "Coop", mood: null, genre: null })).toBe(false);
    expect(hasFacets({ tag: null, mood: "story", genre: null })).toBe(true);
  });
  it("genre : seulement les genres de la fiche, pas les tags", () => {
    const rpg = jeu({ details: fiche({ genres: ["RPG", "Action"], tags: ["Monde ouvert"] }) });
    const tagRpg = jeu({ details: fiche({ genres: ["Action"], tags: ["RPG"] }) });
    const genre = { tag: null, mood: null, genre: "RPG" };
    expect(hasFacets(genre)).toBe(true);
    expect([rpg, tagRpg, sansFiche].filter((g) => matchesFacets(g, genre))).toEqual([rpg]);
    expect(matchesFacets(rpg, { tag: "Monde ouvert", mood: null, genre: "Action" })).toBe(true);
    expect(matchesFacets(rpg, { tag: "Coop", mood: null, genre: "RPG" })).toBe(false);
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
  it("Année de sortie : la plus récente d'abord, inconnue en dernier", () => {
    const ancien = jeu({ title: "Ancien", details: fiche({ releaseDate: 1e9 }) });
    const recent = jeu({ title: "Récent", details: fiche({ releaseDate: 1.7e9 }) });
    const sansDate = jeu({ title: "A sans date", details: fiche() });
    const sansFiche = jeu({ title: "B sans fiche" });
    expect(titres(selectGames([sansFiche, ancien, sansDate, recent], "all", "year", ""))).toEqual(["Récent", "Ancien", "A sans date", "B sans fiche"]);
  });
  it("Avancement des succès : le plus avancé d'abord, plus de succès à égalité, sans succès en dernier", () => {
    const moitie = jeu({ title: "Moitié", achievements: { unlocked: 5, total: 10, lastUnlocked: null } });
    const moitieGrand = jeu({ title: "Moitié grand", achievements: { unlocked: 50, total: 100, lastUnlocked: null } });
    const fini = jeu({ title: "Fini", achievements: { unlocked: 12, total: 12, lastUnlocked: null } });
    const zero = jeu({ title: "Zéro", achievements: { unlocked: 0, total: 30, lastUnlocked: null } });
    const vide = jeu({ title: "A total nul", achievements: { unlocked: 0, total: 0, lastUnlocked: null } });
    const aucun = jeu({ title: "B aucun" });
    expect(titres(selectGames([aucun, zero, moitie, vide, fini, moitieGrand], "all", "achievements", ""))).toEqual([
      "Fini",
      "Moitié grand",
      "Moitié",
      "Zéro",
      "A total nul",
      "B aucun",
    ]);
  });
  it("Durée de vie : l'histoire la plus courte d'abord, inconnue en dernier", () => {
    const hltb = (main: number | null) => ({ name: "x", main, plus: null, full: 900 });
    const court = jeu({ title: "Court", hltb: hltb(120) });
    const long = jeu({ title: "Long", hltb: hltb(3000) });
    const sansHistoire = jeu({ title: "A sans histoire", hltb: hltb(null) });
    const inconnu = jeu({ title: "B inconnu" });
    expect(titres(selectGames([inconnu, long, sansHistoire, court], "all", "lifespan", ""))).toEqual(["Court", "Long", "A sans histoire", "B inconnu"]);
  });
  it("chaque tri a son libellé et finit par le titre à égalité", () => {
    const x = jeu({ title: "Xénon" });
    const y = jeu({ title: "Alpha" });
    for (const sort of SORTS) expect(titres(selectGames([x, y], "all", sort, ""))).toEqual(["Alpha", "Xénon"]);
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
    expect(titres(selectGames(tous, "all", "title", "", { tag: "Sport", mood: null, genre: null }))).toEqual(["EA SPORTS FC 24", "Rocket League"]);
    expect(titres(selectGames(tous, "all", "title", "rocket", { tag: "Football", mood: null, genre: null }))).toEqual([]);
  });
  it("recherche faite d'espaces : pas de recherche", () => {
    expect(selectGames(tous, "all", "title", "   ")).toHaveLength(3);
  });  it("countGames compte les mêmes jeux que selectGames sans recherche", () => {
    const facettes = [NO_FACETS, { tag: "Sport", mood: null, genre: null }, { tag: "Football", mood: null, genre: null }];
    for (const f of ["all", "favorites", "hidden", "riot", "ea", "steam"] as const)
      for (const fa of facettes) expect(countGames(tous, f, fa)).toBe(selectGames(tous, f, "recent", "", fa).length);
  });
});

describe("achievementPercent et sortMeta", () => {
  const succes = (unlocked: number, total: number) => ({ unlocked, total, lastUnlocked: null });

  it("pourcentage entier arrondi vers le bas, 100 seulement si tout est débloqué", () => {
    expect(achievementPercent(jeu({ achievements: succes(1, 3) }))).toBe(33);
    expect(achievementPercent(jeu({ achievements: succes(999, 1000) }))).toBe(99);
    expect(achievementPercent(jeu({ achievements: succes(12, 12) }))).toBe(100);
    expect(achievementPercent(jeu({ achievements: succes(0, 40) }))).toBe(0);
  });
  it("sans succès connus : null", () => {
    expect(achievementPercent(jeu())).toBeNull();
    expect(achievementPercent(jeu({ achievements: succes(0, 0) }))).toBeNull();
  });
  it("information sous la jaquette selon le tri", () => {
    const g = jeu({
      sizeOnDisk: 2.5e9,
      playtimeMinutes: 90,
      achievements: succes(3, 10),
      hltb: { name: "x", main: 600, plus: null, full: null },
      details: fiche({ releaseDate: Date.UTC(2020, 5, 1) / 1000 }),
    });
    expect(sortMeta(g, "size")).toBe("2,5 Go");
    expect(sortMeta(g, "playtime")).toBe("1,5 h");
    expect(sortMeta(g, "year")).toBe("2020");
    expect(sortMeta(g, "achievements")).toBe("3 / 10 succès");
    expect(sortMeta(g, "lifespan")).toBe("Histoire : 10 h");
    expect(sortMeta(g, "recent")).toBe("Jamais lancé");
    const vide = jeu();
    expect(sortMeta(vide, "year")).toBe("Année inconnue");
    expect(sortMeta(vide, "achievements")).toBe("Sans succès");
    expect(sortMeta(vide, "lifespan")).toBe("Durée inconnue");
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
  it("genres comptés une fois par jeu, masqués exclus", () => {
    const g = (genres: string[], hidden = false) => jeu({ hidden, details: fiche({ genres }) });
    expect(libraryGenres([g(["RPG", "Action"]), g(["Action"]), g(["Course"], true), jeu()])).toEqual([
      { name: "Action", count: 2 },
      { name: "RPG", count: 1 },
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
    rememberFacets({ tag: "Coop", mood: "chill", genre: null });
    expect(rememberedFacets()).toEqual({ tag: "Coop", mood: "chill", genre: null });
    rememberFacets(NO_FACETS);
    expect(rememberedFacets()).toEqual(NO_FACETS);
    rememberFacets({ tag: null, mood: null, genre: "RPG" });
    expect(data.get("ludex.genre")).toBe("RPG");
    expect(rememberedFacets()).toEqual({ tag: null, mood: null, genre: "RPG" });
    data.set("ludex.genre", "");
    data.set("ludex.mood", "joyeux");
    data.set("ludex.tag", "Tag disparu");
    expect(rememberedFacets()).toEqual({ tag: "Tag disparu", mood: null, genre: null });
  });
});

describe("en anglais", () => {
  afterEach(() => setLang("fr"));

  it("texte sous la jaquette selon le tri", () => {
    setLang("en");
    const g = jeu({
      sizeOnDisk: 2.5e9,
      playtimeMinutes: 90,
      achievements: { unlocked: 3, total: 10, lastUnlocked: null },
      hltb: { name: "x", main: 600, plus: null, full: null },
      details: fiche({ releaseDate: Date.UTC(2020, 5, 1) / 1000 }),
    });
    expect(sortMeta(g, "size")).toBe("2.5 GB");
    expect(sortMeta(g, "playtime")).toBe("1.5 h");
    expect(sortMeta(g, "year")).toBe("2020");
    expect(sortMeta(g, "achievements")).toBe("3 / 10 achievements");
    expect(sortMeta(g, "lifespan")).toBe("Main story: 10 h");
    expect(sortMeta(g, "recent")).toBe("Never launched");
    const vide = jeu();
    expect(sortMeta(vide, "year")).toBe("Unknown year");
    expect(sortMeta(vide, "achievements")).toBe("No achievements");
    expect(sortMeta(vide, "lifespan")).toBe("Unknown length");
  });
  it("libellés des tris et des statuts relus dans la langue actuelle", () => {
    expect(SORT_LABEL.recent).toBe("Récents");
    expect(STATUS_FILTER_LABEL.any).toBe("Tous les statuts");
    setLang("en");
    expect(SORTS.map((s) => SORT_LABEL[s])).toEqual(["Recent", "Most played", "A → Z", "Size", "Release year", "Achievement progress", "Time to beat"]);
    expect(STATUS_FILTER_LABEL.any).toBe("All statuses");
    expect(STATUS_FILTER_LABEL.none).toBe("No status");
    expect(STATUS_FILTER_LABEL.todo).toBe("Backlog");
  });
});
