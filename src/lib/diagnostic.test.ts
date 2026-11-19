import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { durationText, problems, serviceState, sourceDetail, sourceState, sourceTone } from "./diagnostic";
import type { DiagService, DiagSource, Diagnostic } from "./api";

const NOW = new Date("2026-11-19T14:00:00+01:00");
const t = (minutesAgo: number) => Math.floor(NOW.getTime() / 1000) - minutesAgo * 60;

beforeAll(() => {
  vi.useFakeTimers();
  vi.setSystemTime(NOW);
});
afterAll(() => {
  vi.useRealTimers();
});

const source = (champs: Partial<DiagSource>): DiagSource => ({
  id: "steam",
  label: "Steam",
  state: "games",
  count: 0,
  error: null,
  millis: null,
  locations: [],
  skipped: [],
  tools: [],
  ...champs,
});

const service = (champs: Partial<DiagService>): DiagService => ({
  id: "hltb",
  label: "Durées de vie",
  host: "HowLongToBeat",
  busy: false,
  lastOk: null,
  lastError: null,
  errorAt: null,
  ...champs,
});

describe("sourceState", () => {
  it("une ligne par état", () => {
    expect(sourceState(source({ state: "unknown" }))).toBe("Pas encore lu");
    expect(sourceState(source({ state: "absent" }))).toBe("Pas sur ce PC");
    expect(sourceState(source({ state: "empty" }))).toBe("Installé, aucun jeu");
    expect(sourceState(source({ id: "manual", state: "empty" }))).toBe("Aucun");
    expect(sourceState(source({ count: 1 }))).toBe("1 jeu");
    expect(sourceState(source({ count: 1240 }))).toBe(`1 240 jeux`);
    expect(sourceState(source({ state: "disabled", count: 2 }))).toBe("Désactivé, 2 jeux écartés");
    expect(sourceState(source({ state: "disabled" }))).toBe("Désactivé");
    expect(sourceState(source({ state: "error", error: "Accès refusé" }))).toBe("Erreur : Accès refusé");
  });
});

describe("sourceTone", () => {
  it("erreur, écarts à regarder, rien, ou éteint", () => {
    expect(sourceTone(source({ state: "error" }))).toBe("error");
    expect(sourceTone(source({ skipped: [{ title: "x", reason: "y" }] }))).toBe("warn");
    expect(sourceTone(source({ count: 3 }))).toBe("ok");
    expect(sourceTone(source({ state: "absent" }))).toBe("off");
    expect(sourceTone(source({ id: "manual", state: "empty" }))).toBe("off");
    expect(sourceTone(source({ id: "manual", state: "empty", skipped: [{ title: "x", reason: "y" }] }))).toBe("warn");
  });
});

describe("sourceDetail", () => {
  it("écarts, applis masquées et durée", () => {
    expect(sourceDetail(source({}))).toBe("");
    expect(sourceDetail(source({ skipped: [{ title: "a", reason: "b" }], tools: ["Wallpaper Engine", "SDK"], millis: 220 }))).toBe(
      "1 écarté · 2 applis masquées · lu en 220 ms",
    );
    expect(sourceDetail(source({ id: "manual", skipped: [{ title: "a", reason: "b" }] }))).toBe("1 exécutable introuvable");
  });
});

describe("durationText", () => {
  it("millisecondes puis secondes", () => {
    expect(durationText(null)).toBe("");
    expect(durationText(0)).toBe("0 ms");
    expect(durationText(999)).toBe("999 ms");
    expect(durationText(1600)).toBe("1,6 s");
    expect(durationText(2000)).toBe("2 s");
  });
});

describe("serviceState", () => {
  it("une erreur plus récente que la dernière réussite l'emporte", () => {
    expect(serviceState(service({}))).toEqual({ text: "Rien demandé depuis le démarrage", tone: "off" });
    expect(serviceState(service({ lastOk: t(5) }))).toEqual({ text: "Joint il y a 5 min", tone: "ok" });
    expect(serviceState(service({ busy: true }))).toEqual({ text: "Lecture en cours", tone: "ok" });
    expect(serviceState(service({ lastOk: t(30), lastError: "réponse 429", errorAt: t(2) }))).toEqual({
      text: "Erreur il y a 2 min : réponse 429",
      tone: "error",
    });
    // Réussite revenue depuis l'erreur.
    expect(serviceState(service({ lastOk: t(1), lastError: "réponse 429", errorAt: t(2) })).tone).toBe("ok");
  });
});

describe("problems", () => {
  it("sources et services en erreur", () => {
    const d = {
      sources: [source({ state: "error" }), source({ count: 2 })],
      services: [service({ lastError: "hors ligne", errorAt: t(1) }), service({ lastOk: t(1) })],
    } as Diagnostic;
    expect(problems(d)).toBe(2);
  });
});
