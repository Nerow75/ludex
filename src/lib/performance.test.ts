import { describe, expect, it } from "vitest";
import type { PerfSeries, SessionPerf } from "./api";
import {
  areaPath,
  chartSegments,
  chartSpan,
  formatMetric,
  lacksGpu,
  linePath,
  perfHeadline,
  scaleMax,
  seriesSummary,
  sessionDate,
  sessionLength,
  shownSessions,
} from "./performance";
import { espaces } from "../test/jeu";

const series = (fields: Partial<PerfSeries> = {}): PerfSeries => ({
  metric: "cpu",
  game: { avg: 34.2, peak: 78 },
  pc: { avg: 45, peak: 90.4 },
  gamePoints: [10, 30, 25.8],
  pcPoints: [40, 50, 60],
  ...fields,
});

// 10 décembre 2020 à 14:23 (heure de Paris).
const START = 1607606580;

const perf = (fields: Partial<SessionPerf> = {}): SessionPerf => ({
  start: START,
  end: START + 180,
  step: 60,
  totalMemory: 65536,
  times: [0, 60, 120],
  series: [series()],
  ...fields,
});

describe("formatMetric", () => {
  it("écrit les % et les mémoires", () => {
    expect(formatMetric("cpu", 34.2)).toBe("34 %");
    expect(formatMetric("gpu", 4.56)).toBe("4,6 %");
    expect(formatMetric("memory", 850)).toBe("850 Mo");
    expect(formatMetric("vram", 3584)).toBe("3,5 Go");
    expect(formatMetric("memory", 65536)).toBe("64 Go");
  });
});

describe("scaleMax", () => {
  it("prend 100 %, la mémoire installée, ou la mémoire vidéo arrondie au Go", () => {
    expect(scaleMax(series(), perf())).toBe(100);
    expect(scaleMax(series({ metric: "memory" }), perf())).toBe(65536);
    expect(scaleMax(series({ metric: "memory", pc: { avg: 9000, peak: 12000 } }), perf({ totalMemory: 0 }))).toBe(12288);
    expect(scaleMax(series({ metric: "vram", game: { avg: 2000, peak: 3000 }, pc: { avg: 3500, peak: 4100 } }), perf())).toBe(5120);
    expect(scaleMax(series({ metric: "vram", game: { avg: 0, peak: 0 }, pc: { avg: 0, peak: 0 }, pcPoints: [0] }), perf())).toBe(1024);
  });
});

describe("chartSegments", () => {
  it("place chaque point au milieu de sa minute, bas = 0, haut = le maximum", () => {
    const segs = chartSegments(perf(), [0, 50, 100], 100, 600, 100);
    expect(segs).toEqual([[[100, 99], [300, 50], [500, 1]]]);
  });

  it("coupe la courbe sur une valeur absente ou une pause", () => {
    const p = perf({ end: START + 600, times: [0, 60, 120, 420, 480] });
    expect(chartSpan(p)).toBe(600);
    const segs = chartSegments(p, [10, null, 10, 20, 20], 100, 600, 100);
    expect(segs.map((s) => s.length)).toEqual([1, 1, 2]);
  });

  it("borne les valeurs hors échelle", () => {
    expect(chartSegments(perf({ times: [0] }), [140], 100, 600, 100)).toEqual([[[100, 1]]]);
    expect(chartSegments(perf({ times: [0] }), [-5], 100, 600, 100)).toEqual([[[100, 99]]]);
  });
});

describe("linePath et areaPath", () => {
  it("trace une ligne, un trait pour un point seul et la surface dessous", () => {
    const segs: [number, number][][] = [[[10, 50]], [[100, 20], [200, 40]]];
    expect(linePath(segs)).toBe("M8 50H12M100 20L200 40");
    expect(areaPath(segs, 100)).toBe("M100 100L100 20L200 40L200 100Z");
    expect(linePath([])).toBe("");
  });
});

describe("textes", () => {
  it("résume une courbe pour les lecteurs d'écran", () => {
    expect(espaces(seriesSummary(series(), perf()))).toBe(
      "Processeur : le jeu 34 % en moyenne, 78 % au plus ; tout le PC 45 % en moyenne, 90 % au plus.",
    );
    const memory = series({ metric: "memory", game: { avg: 3584, peak: 4096 }, pc: { avg: 16384, peak: 20480 } });
    expect(espaces(seriesSummary(memory, perf()))).toBe(
      "Mémoire vive : le jeu 3,5 Go en moyenne, 4 Go au plus ; tout le PC 16 Go en moyenne, 20 Go au plus sur 64 Go installés.",
    );
  });

  it("date et durée de la session, phrase d'en-tête", () => {
    expect(sessionDate(perf())).toBe("10 décembre 2020 à 14:23");
    expect(sessionLength(perf({ end: START + 4800 }))).toBe("1 h 20");
    expect(perfHeadline([perf()])).toBe("Mesures de la dernière partie lancée depuis Ludex, toutes les 5 secondes : le jeu comparé à tout le PC.");
    expect(perfHeadline([perf(), perf()])).toContain("Mesures des 2 dernières parties lancées");
  });

  it("repère une session sans carte graphique et écarte les sessions vides", () => {
    expect(lacksGpu(perf())).toBe(true);
    expect(lacksGpu(perf({ series: [series({ metric: "gpu" })] }))).toBe(false);
    expect(shownSessions([perf(), perf({ series: [] }), perf({ times: [] })])).toHaveLength(1);
  });
});
