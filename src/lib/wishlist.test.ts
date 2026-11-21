import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import type { WishItem, WishPrice, WishSale } from "./api";
import { addedText, checkedText, formatPrice, hitPrice, lowestText, priceView, saleToast, wishLabel, wishSummary } from "./wishlist";
import { espaces, JOUR, maintenant } from "../test/jeu";

beforeAll(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2026-11-21T12:00:00+01:00"));
});
afterAll(() => vi.useRealTimers());

const prix = (initial: number, final: number, discount: number, currency = "EUR"): WishPrice => ({ currency, initial, final, discount });

const souhait = (champs: Partial<WishItem> = {}): WishItem => ({
  appid: 1426210,
  title: "It Takes Two",
  added: maintenant() - 3 * JOUR,
  addedPrice: prix(3999, 3999, 0),
  price: prix(3999, 799, 80),
  offer: "price",
  lowest: null,
  checked: maintenant(),
  imageUrl: "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/1426210/header.jpg",
  owned: null,
  ...champs,
});

describe("formatPrice", () => {
  it("écrit les euros à la française", () => {
    expect(espaces(formatPrice(799, "EUR"))).toBe("7,99 €");
    expect(espaces(formatPrice(123456, "EUR"))).toBe("1 234,56 €");
  });

  it("garde une monnaie inconnue en texte", () => {
    expect(formatPrice(799, "€€€")).toBe("7,99 €€€");
  });
});

describe("priceView", () => {
  it("montre la remise et le prix d'origine en promo", () => {
    const v = priceView(souhait());
    expect(espaces(v.now!)).toBe("7,99 €");
    expect(v.discount).toBe("-80 %");
    expect(espaces(v.was!)).toBe("39,99 €");
    expect(v.note).toBeNull();
  });

  it("sans promo : le prix seul", () => {
    const v = priceView(souhait({ price: prix(2450, 2450, 0) }));
    expect([v.discount, v.was]).toEqual([null, null]);
  });

  it("explique l'absence de prix", () => {
    expect(priceView(souhait({ offer: "noPrice", price: null })).note).toBe("Gratuit ou pas encore en vente");
    expect(priceView(souhait({ offer: "unavailable", price: null })).note).toBe("Pas en vente en France");
    expect(priceView(souhait({ offer: "unknown", price: null })).note).toBe("Prix pas encore lu");
  });
});

describe("textes des lignes", () => {
  it("prix d'un résultat de recherche", () => {
    expect(espaces(hitPrice({ price: prix(3999, 799, 80) }))).toBe("7,99 € (-80 %)");
    expect(espaces(hitPrice({ price: prix(2450, 2450, 0) }))).toBe("24,50 €");
    expect(hitPrice({ price: null })).toBe("Gratuit ou sans prix");
  });

  it("plus bas vu", () => {
    expect(lowestText(souhait())).toBeNull();
    expect(espaces(lowestText(souhait({ lowest: prix(3999, 599, 85) }))!)).toBe("Plus bas vu : 5,99 €");
  });

  it("date d'ajout et prix d'alors s'il a changé", () => {
    expect(espaces(addedText(souhait()))).toBe("Ajouté il y a 3 jours, à 39,99 €");
    expect(addedText(souhait({ price: prix(3999, 3999, 0) }))).toBe("Ajouté il y a 3 jours");
    expect(addedText(souhait({ addedPrice: null }))).toBe("Ajouté il y a 3 jours");
  });

  it("résumé de la liste", () => {
    expect(wishSummary([souhait()])).toBe("1 jeu, dont 1 en promo");
    expect(wishSummary([souhait({ price: null }), souhait({ price: prix(100, 100, 0) })])).toBe("2 jeux");
  });

  it("date de la dernière relecture", () => {
    expect(checkedText(maintenant() - 7200)).toBe("Prix relus il y a 2 h, une fois par jour.");
    expect(checkedText(null)).toMatch(/une fois par jour/);
  });

  it("nom complet pour les lecteurs d'écran", () => {
    expect(espaces(wishLabel(souhait({ owned: "steam:1426210", lowest: prix(3999, 599, 85) })))).toBe(
      "It Takes Two, 7,99 €, -80 %, au lieu de 39,99 €, Plus bas vu : 5,99 €, déjà à toi",
    );
    expect(wishLabel(souhait({ offer: "unavailable", price: null }))).toBe("It Takes Two, Pas en vente en France");
  });
});

describe("saleToast", () => {
  const promo = (title: string, discount: number): WishSale => ({ appid: 1, title, price: prix(3999, 799, discount) });

  it("une promo : titre et prix", () => {
    const t = saleToast([promo("It Takes Two", 80)])!;
    expect(t.title).toBe("It Takes Two en promo : -80 %");
    expect(espaces(t.detail)).toBe("7,99 € au lieu de 39,99 €.");
  });

  it("plusieurs promos : trois noms puis le reste", () => {
    expect(saleToast([promo("A", 10), promo("B", 20)])!.detail).toBe("A -10 %, B -20 %.");
    const t = saleToast([promo("A", 10), promo("B", 20), promo("C", 30), promo("D", 40), promo("E", 50)])!;
    expect(t.title).toBe("5 jeux de ta liste en promo");
    expect(t.detail).toBe("A -10 %, B -20 %, C -30 % et 2 autres.");
    expect(saleToast([])).toBeNull();
  });
});
