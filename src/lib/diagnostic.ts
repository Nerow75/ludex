// Textes de la page de diagnostic (Paramètres), tirés du diagnostic envoyé par Rust.
import type { DiagService, DiagSource, Diagnostic } from "./api";
import { relativeTime } from "./format";
import { formatNumber, t } from "./i18n";

/** État d'une source en une ligne, comme dans le rapport. */
export function sourceState(s: DiagSource): string {
  const d = t.settings.diagnostic;
  switch (s.state) {
    case "unknown":
      return d.state.unknown;
    case "absent":
      return d.state.absent;
    case "empty":
      return s.id === "manual" ? d.state.none : d.state.empty;
    case "games":
      return d.state.games(s.count);
    case "disabled":
      return s.count ? d.state.disabledCount(s.count) : d.state.disabled;
    case "error":
      return d.state.error(s.error ?? d.unknownError);
  }
}

/** Ton de la ligne : erreur, quelque chose à regarder (écarts, emplacement attendu absent), ou rien. */
export type Tone = "error" | "warn" | "ok" | "off";

export function sourceTone(s: DiagSource): Tone {
  if (s.state === "error") return "error";
  if (s.state === "absent" || s.state === "disabled" || s.state === "unknown") return "off";
  if (s.id === "manual") return s.skipped.length ? "warn" : s.state === "games" ? "ok" : "off";
  return s.skipped.length ? "warn" : "ok";
}

/** Détails courts sous le nom : écarts, applis masquées, durée de lecture. */
export function sourceDetail(s: DiagSource): string {
  const d = t.settings.diagnostic;
  const parts: string[] = [];
  if (s.skipped.length) parts.push(s.id === "manual" ? d.missingExes(s.skipped.length) : d.skipped(s.skipped.length));
  if (s.tools.length) parts.push(d.tools(s.tools.length));
  const ms = durationText(s.millis);
  if (ms) parts.push(d.readIn(ms));
  return parts.join(" · ");
}

/** « 12 ms », « 1,6 s » (« 1.6 s » en anglais) ; vide si inconnue. */
export function durationText(ms: number | null): string {
  if (ms === null) return "";
  if (ms < 1000) return `${formatNumber(ms)} ms`;
  return `${formatNumber(ms / 1000, { maximumFractionDigits: 1 })} s`;
}

/** État d'un service en ligne depuis le démarrage. Une erreur plus récente que la dernière réussite compte. */
export function serviceState(s: DiagService): { text: string; tone: Tone } {
  const d = t.settings.diagnostic;
  const failing = s.errorAt !== null && (s.lastOk === null || s.errorAt >= s.lastOk);
  if (failing) return { text: d.serviceError(relativeTime(s.errorAt), s.lastError ?? d.unknownError), tone: "error" };
  if (s.busy) return { text: d.serviceBusy, tone: "ok" };
  if (s.lastOk !== null) return { text: d.serviceOk(relativeTime(s.lastOk)), tone: "ok" };
  return { text: d.serviceIdle, tone: "off" };
}

/** Nombre de problèmes à signaler en tête : sources en erreur, services en erreur. */
export function problems(d: Diagnostic): number {
  return d.sources.filter((s) => s.state === "error").length + d.services.filter((s) => serviceState(s).tone === "error").length;
}
