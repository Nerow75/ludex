import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import type { EpicLibrary, GogAccount, SteamOwnedLibrary } from "./api";
import { setLang } from "./i18n";
import {
  accountNote,
  epicAccountText,
  epicDiagState,
  epicSummary,
  readStoreFilter,
  saveStoreFilter,
  steamKeyDiagState,
  steamKeyText,
  steamNote,
  steamSummary,
  storeName,
  visibleStores,
} from "./notInstalled";
import { maintenant } from "../test/jeu";

beforeAll(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2026-11-22T12:00:00+01:00"));
});
afterAll(() => vi.useRealTimers());
afterEach(() => {
  setLang("fr");
  vi.unstubAllGlobals();
});

const compte = (champs: Partial<GogAccount> = {}): GogAccount => ({ state: "connected", username: "JoueurEssai", loginOpen: false, error: null, ...champs });
const liste = (champs: Partial<EpicLibrary> = {}): EpicLibrary => ({
  account: compte(),
  games: [
    { id: "Sugar", title: "Rocket League", imageUrl: null },
    { id: "Fortnite", title: "Fortnite", imageUrl: null },
  ],
  owned: 40,
  checked: maintenant() - 2 * 3600,
  error: null,
  launcher: true,
  ...champs,
});

describe("filtre par boutique", () => {
  const stockage = () => {
    const data = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (k: string) => data.get(k) ?? null,
      setItem: (k: string, v: string) => void data.set(k, v),
      removeItem: (k: string) => void data.delete(k),
    });
    return data;
  };

  it("sans stockage (Node), toutes les boutiques", () => {
    expect(readStoreFilter()).toBe("all");
    expect(() => saveStoreFilter("epic")).not.toThrow();
  });

  it("retenu, puis effacé pour « Toutes »", () => {
    const data = stockage();
    saveStoreFilter("epic");
    expect(data.get("ludex.notInstalledStore")).toBe("epic");
    expect(readStoreFilter()).toBe("epic");
    saveStoreFilter("all");
    expect(data.has("ludex.notInstalledStore")).toBe(false);
    expect(readStoreFilter()).toBe("all");
  });

  it("valeur inconnue = toutes", () => {
    const data = stockage();
    data.set("ludex.notInstalledStore", "steam-future");
    expect(readStoreFilter()).toBe("all");
  });

  it("boutiques montrées", () => {
    expect(visibleStores("all")).toEqual(["steam", "gog", "epic"]);
    expect(visibleStores("steam")).toEqual(["steam"]);
    expect(storeName("steam")).toBe("Steam");
    expect(visibleStores("gog")).toEqual(["gog"]);
    expect(storeName("epic")).toBe("Epic Games");
  });
});

describe("accountNote", () => {
  it("rien si connecté, sinon la raison", () => {
    expect(accountNote("connected", "epic")).toBeNull();
    expect(accountNote("off", "epic")).toBe("Compte Epic Games non connecté.");
    expect(accountNote("expired", "gog")).toBe("GOG a refusé la connexion enregistrée.");
    setLang("en");
    expect(accountNote("off", "gog")).toBe("GOG account not signed in.");
  });
});

describe("epicSummary", () => {
  it("jeux à installer, éléments de la bibliothèque et date", () => {
    expect(epicSummary(liste())).toBe("2 jeux à installer sur 40 éléments de ta bibliothèque Epic. Liste relue il y a 2 h.");
    expect(epicSummary(liste({ games: [], owned: 1, checked: null }))).toBe("1 élément dans ta bibliothèque Epic, rien à installer.");
    setLang("en");
    expect(epicSummary(liste({ checked: null }))).toBe("2 games to install out of 40 items in your Epic library.");
  });
});

describe("compte Epic", () => {
  it("état dans les Paramètres", () => {
    expect(epicAccountText(compte())).toBe("Connecté en tant que JoueurEssai");
    expect(epicAccountText(compte({ username: null }))).toBe("Connecté");
    expect(epicAccountText(compte({ state: "off", loginOpen: true }))).toBe("Connexion en cours dans la fenêtre d'Epic…");
    expect(epicAccountText(compte({ state: "expired" }))).toBe("À reconnecter : Epic a refusé la connexion enregistrée");
  });

  it("Diagnostic : jamais d'identifiant, seulement l'état", () => {
    expect(epicDiagState("connected")).toEqual({ text: "Connecté", tone: "ok" });
    expect(epicDiagState("off")).toEqual({ text: "Non connecté", tone: "off" });
    expect(epicDiagState("expired").tone).toBe("warn");
  });
});

const steam = (champs: Partial<SteamOwnedLibrary> = {}): SteamOwnedLibrary => ({
  key: { state: "off", error: null },
  source: "local",
  games: [
    { id: 1145360, title: "Hades", imageUrl: "https://example.test/h.jpg" },
    { id: 620, title: "Portal 2", imageUrl: "https://example.test/p.jpg" },
  ],
  owned: 30,
  checked: null,
  error: null,
  steam: true,
  ...champs,
});

describe("groupe Steam", () => {
  it("liste locale : jeux déjà lancés sur ce compte", () => {
    expect(steamSummary(steam())).toBe("2 jeux à installer sur 30 déjà lancés sur ce compte.");
    expect(steamSummary(steam({ games: [], owned: 1 }))).toBe("1 jeu déjà lancé sur ce compte, tous installés.");
    expect(steamNote(steam())).toBe("local");
    setLang("en");
    expect(steamSummary(steam())).toBe("2 games to install out of 30 already launched on this account.");
  });

  it("liste complète avec la clé : possédés et date", () => {
    const complete = steam({ source: "key", key: { state: "connected", error: null }, owned: 120, checked: maintenant() - 2 * 3600 });
    expect(steamSummary(complete)).toBe("2 jeux à installer sur 120 possédés. Liste relue il y a 2 h.");
    expect(steamSummary({ ...complete, games: [], checked: null })).toBe("120 jeux possédés, tous installés.");
    expect(steamNote(complete)).toBeNull();
  });

  it("clé refusée : liste locale et avertissement", () => {
    expect(steamNote(steam({ key: { state: "expired", error: null } }))).toBe("refused");
  });

  it("état de la clé, jamais la clé", () => {
    expect(steamKeyText("off")).toBe("Aucune clé : liste locale");
    expect(steamKeyText("connected")).toBe("Clé enregistrée : liste complète");
    expect(steamKeyText("expired")).toBe("Steam refuse la clé enregistrée");
    expect(steamKeyDiagState("connected")).toEqual({ text: "Enregistrée", tone: "ok" });
    expect(steamKeyDiagState("off")).toEqual({ text: "Absente (liste locale)", tone: "off" });
    expect(steamKeyDiagState("expired").tone).toBe("warn");
    setLang("en");
    expect(steamKeyText("off")).toBe("No key: local list");
  });
});
