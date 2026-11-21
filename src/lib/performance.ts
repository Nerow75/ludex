// Onglet « Performances » de la fiche : textes, échelles et tracés des courbes (fonctions pures).
import type { PerfMetric, PerfSeries, PerfStat, SessionPerf } from "./api";
import { formatDateTime, formatDuration } from "./format";

export const METRIC_LABEL: Record<PerfMetric, string> = {
  cpu: "Processeur",
  memory: "Mémoire vive",
  gpu: "Carte graphique",
  vram: "Mémoire vidéo",
};

const oneDecimal = new Intl.NumberFormat("fr-FR", { maximumFractionDigits: 1 });
const integer = new Intl.NumberFormat("fr-FR", { maximumFractionDigits: 0 });

/** Mesures en % ; les autres sont en Mo. */
const isPercent = (metric: PerfMetric) => metric === "cpu" || metric === "gpu";

/** « 34 % », « 4,5 % », « 850 Mo », « 3,5 Go ». */
export function formatMetric(metric: PerfMetric, value: number): string {
  if (isPercent(metric)) return `${(value < 10 ? oneDecimal : integer).format(value)} %`;
  if (value < 1024) return `${integer.format(value)} Mo`;
  return `${oneDecimal.format(value / 1024)} Go`;
}

/** Haut de l'échelle : 100 % ; mémoire installée ; mémoire vidéo arrondie au Go supérieur. */
export function scaleMax(series: PerfSeries, perf: SessionPerf): number {
  if (isPercent(series.metric)) return 100;
  if (series.metric === "memory" && perf.totalMemory > 0) return perf.totalMemory;
  const peak = Math.max(series.game.peak, series.pc.peak, ...series.pcPoints.map((v) => v ?? 0));
  return Math.max(1024, Math.ceil(peak / 1024) * 1024);
}

/** Durée couverte par les courbes, en secondes (un point au moins). */
export const chartSpan = (perf: SessionPerf) => Math.max(perf.end - perf.start, perf.step, 1);

/** Points d'une courbe, coupée là où rien n'a été lu (pause, veille) : un tableau par morceau. */
export function chartSegments(
  perf: SessionPerf,
  values: (number | null)[],
  max: number,
  width: number,
  height: number,
): [number, number][][] {
  const span = chartSpan(perf);
  const segments: [number, number][][] = [];
  let current: [number, number][] = [];
  let previous: number | null = null;
  perf.times.forEach((t, i) => {
    const v = values[i];
    const gap = previous !== null && t - previous > perf.step;
    if (v === null || v === undefined || gap) {
      if (current.length) segments.push(current);
      current = [];
    }
    previous = t;
    if (v === null || v === undefined) return;
    // Milieu du point, et valeur bornée pour qu'une ligne à 0 ou au plafond reste visible.
    const x = Math.min(width, ((t + perf.step / 2) / span) * width);
    const y = height - Math.min(1, Math.max(0, v / max)) * (height - 2) - 1;
    current.push([round(x), round(y)]);
  });
  if (current.length) segments.push(current);
  return segments;
}

const round = (n: number) => Math.round(n * 10) / 10;

/** Tracé SVG d'une ligne ; un point seul devient un petit trait. */
export function linePath(segments: [number, number][][]): string {
  return segments
    .map((seg) => {
      if (seg.length === 1) {
        const [x, y] = seg[0];
        return `M${round(x - 2)} ${y}H${round(x + 2)}`;
      }
      return seg.map(([x, y], i) => `${i ? "L" : "M"}${x} ${y}`).join("");
    })
    .join("");
}

/** Tracé SVG de la surface sous la ligne, jusqu'au bas du graphique. */
export function areaPath(segments: [number, number][][], height: number): string {
  return segments
    .filter((seg) => seg.length > 1)
    .map((seg) => `M${seg[0][0]} ${height}${seg.map(([x, y]) => `L${x} ${y}`).join("")}L${seg[seg.length - 1][0]} ${height}Z`)
    .join("");
}

/** « 34 % en moyenne, 78 % au plus ». */
export const statText = (metric: PerfMetric, stat: PerfStat) =>
  `${formatMetric(metric, stat.avg)} en moyenne, ${formatMetric(metric, stat.peak)} au plus`;

/** Résumé d'une courbe pour les lecteurs d'écran. */
export function seriesSummary(series: PerfSeries, perf: SessionPerf): string {
  const scale = series.metric === "memory" && perf.totalMemory > 0 ? ` sur ${formatMetric("memory", perf.totalMemory)} installés` : "";
  return `${METRIC_LABEL[series.metric]} : le jeu ${statText(series.metric, series.game)} ; tout le PC ${statText(series.metric, series.pc)}${scale}.`;
}

/** Libellé d'une session dans la liste : « 21 novembre 2026 à 14:05 », durée à part. */
export const sessionDate = (perf: SessionPerf) => formatDateTime(perf.start);
export const sessionLength = (perf: SessionPerf) => formatDuration(perf.end - perf.start);

/** Phrase d'en-tête de l'onglet. */
export function perfHeadline(perfs: SessionPerf[]): string {
  const n = perfs.length;
  const what = n > 1 ? `des ${n} dernières parties lancées` : "de la dernière partie lancée";
  return `Mesures ${what} depuis Ludex, toutes les 5 secondes : le jeu comparé à tout le PC.`;
}

/** La carte graphique manque-t-elle à la session (Windows ne donne pas ses compteurs) ? */
export const lacksGpu = (perf: SessionPerf) => !perf.series.some((s) => s.metric === "gpu");

/** Sessions qui ont quelque chose à montrer. */
export const shownSessions = (perfs: SessionPerf[]) => perfs.filter((p) => p.series.length > 0 && p.times.length > 0);
