import type { WorkshopMod, WorkshopMods } from "./api";
import { formatBytes, formatDate } from "./format";
import { t } from "./i18n";

/** Titre d'un mod, sinon son id Workshop. */
export const modTitle = (mod: WorkshopMod) => mod.title ?? t.fiche.mods.title(mod.id);

/** « 15 mods installés, 3,3 Go », puis « dont 2 à mettre à jour ». */
export function modsSummary(list: WorkshopMods): string {
  const n = list.mods.length;
  const pending = list.mods.filter((m) => m.pending).length;
  const base = t.fiche.mods.summary(n, formatBytes(list.total));
  return pending > 0 ? `${base}, ${t.fiche.mods.pending(pending)}` : base;
}

/** Ligne sous le titre : « 312,6 Mo · mis à jour le 10 décembre 2026 », dossier absent signalé. */
export function modDetail(mod: WorkshopMod): string {
  const parts = [mod.present ? formatBytes(mod.size) : t.fiche.mods.missing];
  if (mod.updated) parts.push(t.fiche.mods.updated(formatDate(mod.updated)));
  return parts.join(" · ");
}
