// Textes et choix du panneau « Déplacer vers un autre disque » (`MovePanel.tsx`). Le déplacement
// lui-même est fait côté Rust (`game_move.rs`).
import { PLATFORM_LABEL, type MoveProgress, type MoveTarget, type Platform } from "./api";
import { formatBytes, formatDuration, formatSize } from "./format";
import { t } from "./i18n";

/** Comment déplacer un jeu que Ludex laisse à son launcher. */
export function launcherHelp(platform: Platform): string {
  const m = t.fiche.move;
  if (platform === "ubisoft") return m.ubisoft;
  if (platform === "battlenet") return m.battlenet;
  if (platform === "itch") return m.itch;
  return m.launcher(PLATFORM_LABEL[platform]);
}

/** Destination choisissable : assez de place, pas de dossier du même nom déjà rempli. */
export const targetUsable = (target: MoveTarget) => target.enough && !target.taken;

/** Ce qu'on sait d'une destination : refus d'abord, puis disque, place et création du dossier. */
export function targetNote(target: MoveTarget): string {
  const m = t.fiche.move;
  if (target.taken) return m.taken;
  if (!target.enough) return `${m.notEnough} · ${m.free(formatSize(target.free))}`;
  const parts = [target.sameDrive ? m.sameDrive : m.free(formatSize(target.free))];
  if (target.create) parts.push(m.create(target.key));
  return parts.join(" · ");
}

/** Première destination choisissable, pour le choix par défaut. */
export const firstUsable = (targets: MoveTarget[]) => targets.find(targetUsable) ?? null;

/** Déplacement pas encore fini (un autre ne peut pas commencer, le jeu ne se lance pas). */
export const moveActive = (job: MoveProgress | null) =>
  !!job && ["preparing", "copying", "verifying", "ready", "finishing"].includes(job.phase);

/** Avancement de la copie en %, `null` hors copie ou taille inconnue. */
export function movePercent(job: MoveProgress): number | null {
  if (job.phase === "verifying" || job.phase === "ready" || job.phase === "finishing" || job.phase === "done") return 100;
  if (job.phase !== "copying" || job.total <= 0) return null;
  return Math.min(100, Math.floor((job.copied / job.total) * 100));
}

/** Ligne d'état d'un déplacement. */
export function moveStatus(job: MoveProgress): string {
  const m = t.fiche.move;
  switch (job.phase) {
    case "preparing":
      return m.preparing;
    case "copying": {
      const parts = [m.copying(formatBytes(job.copied), formatBytes(job.total))];
      if (job.speed !== null && job.speed > 0) parts.push(`${formatBytes(job.speed)}/s`);
      if (job.remaining !== null) {
        parts.push(job.remaining < 60 ? t.common.install.underAMinute : t.common.install.remaining(formatDuration(job.remaining)));
      }
      return parts.join(" · ");
    }
    case "verifying":
      return m.verifying;
    case "ready":
      return m.ready(job.from);
    case "finishing":
      return m.finishing;
    case "done":
      return m.done(job.to);
    case "cancelled":
      return m.cancelled;
    case "failed":
      return m.failed(job.error ?? "");
  }
}
