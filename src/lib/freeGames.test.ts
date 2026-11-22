import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import type { FreeOffer } from "./api";
import { freeChecked, freeDay, freeDeadline, freeToast, offerLabel, offerMeta } from "./freeGames";
import { setLang } from "./i18n";
import { espaces, maintenant } from "../test/jeu";

beforeAll(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2026-11-22T17:47:00+01:00"));
});
afterAll(() => vi.useRealTimers());
afterEach(() => setLang("fr"));

// Relevé réel du 22 novembre 2026 : Mark of the Deep gratuit, Bread and Fred la semaine suivante.
const FIN = Date.parse("2026-11-26T16:00:00Z") / 1000;
const SUIVANTE = Date.parse("2026-12-03T16:00:00Z") / 1000;
const offre = (champs: Partial<FreeOffer> = {}): FreeOffer => ({
  id: "1f9a12b06b4f437790b6913ad136a08c",
  title: "Mark of the Deep",
  start: Date.parse("2026-11-19T16:00:00Z") / 1000,
  end: FIN,
  current: true,
  imageUrl: null,
  owned: null,
  ...champs,
});
const bientot = offre({ title: "Bread and Fred", start: FIN, end: SUIVANTE, current: false });

describe("dates des offres", () => {
  it("écrit la fin d'une offre en cours avec le jour et l'heure de Paris", () => {
    expect(freeDay(FIN)).toBe("26 novembre");
    expect(freeDeadline(FIN)).toBe("jeudi 26 novembre, 17:00");
    expect(offerMeta(offre())).toBe("Gratuit jusqu'au jeudi 26 novembre, 17:00");
  });

  it("donne la période d'une offre à venir", () => {
    expect(offerMeta(bientot)).toBe("Gratuit du 26 novembre au 3 décembre");
  });

  it("nomme la carte en entier", () => {
    expect(offerLabel(offre({ owned: "epic:x" }))).toBe("Mark of the Deep, Gratuit, Gratuit jusqu'au jeudi 26 novembre, 17:00, déjà à toi");
    expect(offerLabel(bientot)).toBe("Bread and Fred, Bientôt, Gratuit du 26 novembre au 3 décembre");
  });

  it("dit quand la liste a été relue", () => {
    expect(freeChecked(null)).toBe("");
    expect(espaces(freeChecked(maintenant() - 2 * 3600))).toBe("Liste relue il y a 2 h.");
  });
});

describe("toast d'une nouvelle fournée", () => {
  it("un jeu, puis plusieurs", () => {
    expect(freeToast([])).toBeNull();
    expect(freeToast(["Mark of the Deep"])).toEqual({ title: "Gratuit sur Epic : Mark of the Deep", detail: "À récupérer dans la section Souhaits." });
    expect(freeToast(["A", "B", "C", "D", "E"])).toEqual({ title: "5 jeux gratuits sur Epic", detail: "A, B, C et 2 autres. À récupérer dans la section Souhaits." });
  });
});

describe("en anglais", () => {
  it("dates et textes", () => {
    setLang("en");
    expect(freeDay(FIN)).toBe("November 26");
    expect(espaces(offerMeta(offre()))).toBe("Free until Thursday, November 26, 05:00 PM");
    expect(offerMeta(bientot)).toBe("Free from November 26 to December 3");
    expect(freeToast(["A", "B"])).toEqual({ title: "2 free games on Epic", detail: "A, B. Claim them from the Wishlist section." });
  });
});
