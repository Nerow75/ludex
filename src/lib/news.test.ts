import { afterEach, describe, expect, it } from "vitest";
import { setLang } from "./i18n";
import type { NewsAlert } from "./api";
import { isNewNews, newNewsCount, newsToast } from "./news";

const alerte = (fields: Partial<NewsAlert>): NewsAlert => ({
  id: "steam:252950",
  game: "Rocket League",
  title: "Rocket League S24 Patch Notes v2.76",
  gid: "1844751498216898",
  update: true,
  ...fields,
});

describe("isNewNews", () => {
  it("marque une annonce publiée après la dernière partie", () => {
    expect(isNewNews({ date: 200 }, 100)).toBe(true);
    expect(isNewNews({ date: 100 }, 100)).toBe(false);
    expect(isNewNews({ date: 50 }, 100)).toBe(false);
  });

  it("ne marque rien pour un jeu jamais joué", () => {
    expect(isNewNews({ date: 200 }, null)).toBe(false);
    expect(isNewNews({ date: 200 }, 0)).toBe(false);
  });

  it("compte les annonces nouvelles", () => {
    expect(newNewsCount([{ date: 300 }, { date: 200 }, { date: 50 }], 100)).toBe(2);
    expect(newNewsCount([], 100)).toBe(0);
  });
});

describe("newsToast", () => {
  it("rien sans annonce", () => {
    expect(newsToast([])).toBeNull();
  });

  it("une mise à jour", () => {
    expect(newsToast([alerte({})])).toEqual({ title: "Mise à jour de Rocket League", detail: "Rocket League S24 Patch Notes v2.76" });
  });

  it("une annonce qui n'est pas une mise à jour", () => {
    expect(newsToast([alerte({ update: false, title: "Saison 25" })])).toEqual({ title: "Nouvelle annonce pour Rocket League", detail: "Saison 25" });
  });

  it("plusieurs jeux", () => {
    expect(newsToast([alerte({}), alerte({ id: "steam:105600", game: "Terraria" })])?.detail).toBe("Rocket League et Terraria");
    const quatre = ["A", "B", "C", "D"].map((game, i) => alerte({ id: String(i), game }));
    expect(newsToast(quatre)).toEqual({ title: "Actualités de 4 jeux en cours", detail: "A, B et 2 autres" });
    expect(newsToast(quatre.slice(0, 3))?.detail).toBe("A, B et 1 autre");
  });
});

describe("newsToast en anglais", () => {
  afterEach(() => setLang("fr"));

  it("une mise à jour, une annonce, plusieurs jeux", () => {
    setLang("en");
    expect(newsToast([alerte({})])?.title).toBe("Rocket League update");
    expect(newsToast([alerte({ update: false })])?.title).toBe("New announcement for Rocket League");
    const quatre = ["A", "B", "C", "D"].map((game, i) => alerte({ id: String(i), game }));
    expect(newsToast(quatre)).toEqual({ title: "News for 4 games you're playing", detail: "A, B and 2 others" });
    expect(newsToast(quatre.slice(0, 3))?.detail).toBe("A, B and 1 other");
    expect(newsToast(quatre.slice(0, 2))?.detail).toBe("A and B");
  });
});
