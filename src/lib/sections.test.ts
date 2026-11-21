import { afterEach, describe, expect, it } from "vitest";
import { setLang } from "./i18n";
import {
  ACTIVITY_GROUP,
  collectionSections,
  groupOpen,
  MEMORIES_GROUP,
  MINE_GROUP,
  OPEN_GROUPS_DEFAULT,
  SECTIONS,
  sideGroups,
  STEAM_GROUP,
  TOOLS_GROUP,
} from "./sections";
import type { CollectionsInfo } from "./api";
import { jeu } from "../test/jeu";

describe("SECTIONS", () => {
  const section = (id: string) => SECTIONS.find((s) => s.id === id)!;

  it("ids uniques, « Pour toi » en premier", () => {
    const ids = SECTIONS.map((s) => s.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids[0]).toBe("for-you");
  });
  it("quatre entrées hors catégorie, les autres rangées", () => {
    expect(SECTIONS.filter((s) => !s.group).map((s) => s.id)).toEqual(["for-you", "all", "favorites", "tags"]);
    expect(sideGroups(SECTIONS)).toEqual([ACTIVITY_GROUP, MEMORIES_GROUP, TOOLS_GROUP, MINE_GROUP]);
  });
  it("catégorie ouverte par l'utilisateur ou parce qu'elle contient la section affichée", () => {
    expect(groupOpen(ACTIVITY_GROUP, new Set(), SECTIONS, "journal")).toBe(true);
    expect(groupOpen(ACTIVITY_GROUP, new Set(), SECTIONS, "all")).toBe(false);
    expect(groupOpen(TOOLS_GROUP, new Set([TOOLS_GROUP]), SECTIONS, "all")).toBe(true);
    expect(OPEN_GROUPS_DEFAULT).toEqual([MINE_GROUP]);
  });
  it("« Favoris » : favoris seulement, sans filtres Favoris ni Masqués", () => {
    const fav = section("favorites");
    expect(fav.scope!(jeu({ favorite: true }))).toBe(true);
    expect(fav.scope!(jeu())).toBe(false);
    expect(fav.filters!("favorites")).toBe(false);
    expect(fav.filters!("hidden")).toBe(false);
    expect(fav.filters!("steam")).toBe(true);
  });
  it("« Tous les jeux » : pas de filtre Favoris", () => {
    const all = section("all");
    expect(all.scope).toBeUndefined();
    expect(all.filters!("favorites")).toBe(false);
    expect(all.filters!("hidden")).toBe(true);
  });
});

describe("collectionSections", () => {
  const a = jeu({ id: "steam:10" });
  const b = jeu({ id: "steam:20" });
  const masque = jeu({ id: "steam:30", hidden: true });
  const jeux = [a, b, masque];

  it("aucune info : aucune section", () => {
    expect(collectionSections(null, jeux)).toEqual([]);
  });
  it("collections perso dans l'ordre de création, même vides", () => {
    const info: CollectionsInfo = {
      mine: [
        { id: "c-2", name: "Zen", games: [a.id] },
        { id: "c-1", name: "Apéro", games: [] },
      ],
      steam: [],
    };
    const sections = collectionSections(info, jeux);
    expect(sections.map((s) => [s.id, s.label, s.group])).toEqual([
      ["col:c-2", "Zen", MINE_GROUP],
      ["col:c-1", "Apéro", MINE_GROUP],
    ]);
    expect(sections[0].collection).toEqual({ id: "c-2", kind: "mine" });
    expect(sections[0].scope!(a)).toBe(true);
    expect(sections[0].scope!(b)).toBe(false);
    expect(sections[0].filters!("favorites")).toBe(false);
  });
  it("collections Steam par nom, seulement si un jeu non masqué est dans la bibliothèque, après les perso", () => {
    const info: CollectionsInfo = {
      mine: [{ id: "c-1", name: "Perso", games: [] }],
      steam: [
        { id: "uc-3", name: "zombies", games: [b.id] },
        { id: "uc-1", name: "Favoris", games: [a.id, "steam:999"] },
        { id: "uc-2", name: "Vide", games: [] },
        { id: "uc-4", name: "Masqués seulement", games: [masque.id] },
        { id: "uc-5", name: "Absents", games: ["steam:999"] },
      ],
    };
    const sections = collectionSections(info, jeux);
    expect(sections.map((s) => s.id)).toEqual(["col:c-1", "steam-col:uc-1", "steam-col:uc-3"]);
    expect(sections[1]).toMatchObject({ group: STEAM_GROUP, collection: { id: "uc-1", kind: "steam" } });
    expect(sideGroups([...SECTIONS, ...sections])).toEqual([ACTIVITY_GROUP, MEMORIES_GROUP, TOOLS_GROUP, MINE_GROUP, STEAM_GROUP]);
  });
});

describe("en anglais", () => {
  afterEach(() => setLang("fr"));
  const section = (id: string) => SECTIONS.find((s) => s.id === id)!;

  it("libellés relus dans la langue actuelle, ids et catégories inchangés", () => {
    expect(section("for-you").label).toBe("Pour toi");
    setLang("en");
    expect(SECTIONS.map((s) => s.label)).toEqual([
      "For you",
      "All games",
      "Favorites",
      "Explore",
      "Progress",
      "History",
      "Journal",
      "Recap",
      "Stats",
      "Showcase",
      "Screenshots",
      "Saves",
      "Storage",
      "Wishlist",
    ]);
    expect(section("favorites").empty).toContain("Ctrl+D");
    expect(section("journal").group).toBe(ACTIVITY_GROUP);
    expect(ACTIVITY_GROUP).toBe("Activité");
  });
  it("textes des collections vides", () => {
    const info: CollectionsInfo = { mine: [{ id: "c-1", name: "Zen", games: [] }], steam: [] };
    const [mine] = collectionSections(info, []);
    expect(mine.empty).toContain("Collections");
    setLang("en");
    expect(mine.empty).toBe('Open a game\'s details and pick "Collections" to add it here');
    expect(mine.label).toBe("Zen");
  });
});
