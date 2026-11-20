import type { WorkshopMod, WorkshopMods } from "./api";
import { formatBytes, formatDate } from "./format";

/** Titre d'un mod, sinon son id Workshop. */
export const modTitle = (mod: WorkshopMod) => mod.title ?? `Mod ${mod.id}`;

/** « 15 mods installés, 3,3 Go », puis « dont 2 à mettre à jour ». */
export function modsSummary(list: WorkshopMods): string {
  const n = list.mods.length;
  const pending = list.mods.filter((m) => m.pending).length;
  const base = `${n} mod${n > 1 ? "s" : ""} installé${n > 1 ? "s" : ""}, ${formatBytes(list.total)}`;
  return pending > 0 ? `${base}, dont ${pending} à mettre à jour` : base;
}

/** Ligne sous le titre : « 312,6 Mo · mis à jour le 10 décembre 2026 », dossier absent signalé. */
export function modDetail(mod: WorkshopMod): string {
  const parts = [mod.present ? formatBytes(mod.size) : "Dossier absent"];
  if (mod.updated) parts.push(`mis à jour le ${formatDate(mod.updated)}`);
  return parts.join(" · ");
}
