// Progression d'un téléchargement ou d'une mise à jour Steam reçue en direct (`game-install`).
import type { DownloadRate, Game, InstallEvent, InstallState } from "./api";
import { formatBytes, formatDuration } from "./format";
import { t } from "./i18n";

/** Changements à poser sur le jeu pour un événement `game-install`, sans rescan. */
export function installPatch(event: InstallEvent): Partial<Game> {
  const patch: Partial<Game> = { installState: event.installState };
  if (event.sizeOnDisk !== null) patch.sizeOnDisk = event.sizeOnDisk;
  const downloading = event.installState.kind === "updating" && (event.speed !== null || event.remaining !== null);
  patch.download = downloading ? { speed: event.speed, remaining: event.remaining } : undefined;
  return patch;
}

/** Remplissage de la barre sur la jaquette, en % : téléchargement ou pause dont le % est connu, sinon `null`. */
export function progressBar(state: InstallState): number | null {
  if (state.kind !== "updating" && state.kind !== "paused") return null;
  if (state.progress === null) return null;
  return Math.min(100, Math.max(0, state.progress));
}

/** « 12,4 Mo/s · 3 min restantes » (« 12.4 MB/s · 3 min left »), `null` si ni débit ni temps restant. */
export function downloadDetail(rate: DownloadRate | undefined): string | null {
  if (!rate) return null;
  const parts: string[] = [];
  if (rate.speed !== null && rate.speed > 0) parts.push(`${formatBytes(rate.speed)}/s`);
  if (rate.remaining !== null) parts.push(rate.remaining < 60 ? t.common.install.underAMinute : t.common.install.remaining(formatDuration(rate.remaining)));
  return parts.length ? parts.join(" · ") : null;
}
