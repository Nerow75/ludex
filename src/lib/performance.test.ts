import { afterEach, describe, expect, it } from "vitest";
import { setLang } from "./i18n";
import type { FpsSeries, PerfSeries, SessionPerf } from "./api";
import {
  areaPath,
  chartSegments,
  chartSpan,
  formatFps,
  formatMetric,
  fpsAverageText,
  fpsLowText,
  fpsNote,
  fpsScaleMax,
  fpsSummary,
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

const fps = (fields: Partial<FpsSeries> = {}): FpsSeries => ({
  avg: 62.4,
  low: 30.1,
  points: [60, 97.1, 30],
  lowPoints: [55, 25, null],
  ...fields,
});

describe("images par seconde", () => {
  it("écrit les images par seconde et l'échelle", () => {
    expect(formatFps(62.4)).toBe("62 i/s");
    expect(formatFps(7.46)).toBe("7,5 i/s");
    expect(fpsScaleMax(fps())).toBe(120);
    expect(fpsScaleMax(fps({ avg: 20, points: [20], lowPoints: [null] }))).toBe(60);
    expect(fpsScaleMax(fps({ avg: 144, points: [143.9, 144.2], lowPoints: [120] }))).toBe(150);
  });

  it("donne les chiffres et le résumé, 1 % low absent compris", () => {
    expect(fpsAverageText(fps())).toBe("62 i/s en moyenne");
    expect(fpsLowText(fps())).toBe("30 i/s");
    expect(fpsLowText(fps({ low: null }))).toBe("trop peu d'images");
    expect(espaces(fpsSummary(fps()))).toBe("Images par seconde : 62 i/s en moyenne ; 30 i/s pour le 1 % d'images le plus lent (1 % low).");
    expect(fpsSummary(fps({ low: null }))).toBe("Images par seconde : 62 i/s en moyenne.");
  });

  it("explique une mesure absente", () => {
    expect(fpsNote(perf({ fps: fps() }))).toBeNull();
    expect(fpsNote(perf({ fpsMissing: "denied" }))).toContain("Utilisateurs du journal de performances");
    expect(fpsNote(perf({ fpsMissing: "unavailable" }))).toContain("n'a pas ouvert leur lecture");
    expect(fpsNote(perf({ fpsMissing: "noFrames" }))).toContain("Aucune image vue");
    // Partie mesurée avant l'ajout des images par seconde.
    expect(fpsNote(perf())).toContain("relevé ajouté depuis");
  });

  it("trace la moyenne et le 1 % low sur les points de la session", () => {
    const segs = chartSegments(perf(), fps().lowPoints, 120, 600, 100);
    expect(segs.map((s) => s.length)).toEqual([2]);
  });
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

describe("textes en anglais", () => {
  afterEach(() => setLang("fr"));

  it("écrit les % et les mémoires à l'anglaise", () => {
    setLang("en");
    expect(formatMetric("cpu", 34.2)).toBe("34%");
    expect(formatMetric("gpu", 4.56)).toBe("4.6%");
    expect(formatMetric("memory", 850)).toBe("850 MB");
    expect(formatMetric("vram", 3584)).toBe("3.5 GB");
    expect(formatMetric("memory", 1_048_576)).toBe("1,024 GB");
  });

  it("résume une courbe et donne la phrase d'en-tête", () => {
    setLang("en");
    expect(seriesSummary(series(), perf())).toBe("Processor: game 34% on average, 78% at peak; whole PC 45% on average, 90% at peak.");
    const memory = series({ metric: "memory", game: { avg: 3584, peak: 4096 }, pc: { avg: 16384, peak: 20480 } });
    expect(seriesSummary(memory, perf())).toBe(
      "Memory: game 3.5 GB on average, 4 GB at peak; whole PC 16 GB on average, 20 GB at peak out of 64 GB installed.",
    );
    expect(perfHeadline([perf()])).toBe(
      "Measured during the last session launched from Ludex, every 5 seconds: the game compared with the whole PC.",
    );
    expect(perfHeadline([perf(), perf()])).toContain("during the last 2 sessions launched");
  });

  it("écrit les images par seconde à l'anglaise", () => {
    setLang("en");
    expect(formatFps(62.4)).toBe("62 fps");
    expect(formatFps(7.46)).toBe("7.5 fps");
    expect(fpsAverageText(fps())).toBe("62 fps on average");
    expect(fpsSummary(fps())).toBe("Frame rate: 62 fps on average; 30 fps for the slowest 1% of frames (1% low).");
    expect(fpsNote(perf({ fpsMissing: "denied" }))).toContain("Performance Log Users");
  });
});
