import { PLATFORM_LABEL, type Footprint, type FootprintFolder } from "./api";
import { formatSize } from "./format";
import { t } from "./i18n";

/** Taille ou état d'un dossier de la vue avancée. */
export function folderSize(f: FootprintFolder): string {
  const l = t.fiche.footprint;
  if (!f.exists) return l.deleted;
  if (f.bytes === null) return f.kind === "install" ? l.measuring : l.unknownSize;
  return f.bytes === 0 ? l.empty : formatSize(f.bytes);
}

/** Où se trouve un élément rangé dans un autre dossier de la liste, `null` : à part. */
export function insideLabel(f: Footprint, x: FootprintFolder): string | null {
  if (!x.inside) return null;
  if (x.inside === "install") return t.fiche.footprint.insideGame;
  const parent = f.folders.find((p) => p.item === x.inside);
  return parent ? t.fiche.footprint.inside(parent.label) : null;
}

/**
 * Place des emplacements présents, sans compter deux fois ce qui est rangé dans un autre dossier
 * présent de la liste. `unknown` : au moins un dossier présent de taille inconnue.
 */
export function presentBytes(f: Footprint): { bytes: number; unknown: boolean } {
  let bytes = 0;
  let unknown = false;
  const present = new Set(f.folders.filter((x) => x.exists).map((x) => x.item));
  for (const x of f.folders) {
    if (!x.exists || (x.inside && present.has(x.inside))) continue;
    if (x.bytes === null) unknown = true;
    else bytes += x.bytes;
  }
  return { bytes, unknown };
}

/** Ligne d'état sous le titre. */
export function footprintStatus(f: Footprint): string {
  const l = t.fiche.footprint;
  if (f.installed) {
    const { bytes, unknown } = presentBytes(f);
    if (bytes === 0) return unknown ? l.installedMeasuring : l.installed;
    const extra = f.folders.some((x) => x.kind !== "install" && x.exists);
    return l.installedSize(formatSize(bytes), unknown, extra);
  }
  if (f.leftovers === 0) return l.clean;
  return l.left(f.leftovers);
}

/** Bouton principal de la vue avancée. */
export type MainAction =
  | { kind: "steam" }
  | { kind: "run"; item: string }
  | { kind: "settings" }
  | { kind: "folder" }
  | null;

export function mainAction(f: Footprint): MainAction {
  if (!f.installed) return null;
  if (f.steam) return { kind: "steam" };
  const run = f.uninstall.find((u) => u.exists && u.action === "run");
  if (run) return { kind: "run", item: run.item };
  if (f.storeApp) return { kind: "settings" };
  return f.folders.some((x) => x.kind === "install" && x.exists) ? { kind: "folder" } : null;
}

/** Ce que fait le bouton principal, en une phrase. */
export function actionHint(f: Footprint, action: MainAction): string {
  const l = t.fiche.footprint;
  switch (action?.kind) {
    case "steam":
      return l.hintSteam;
    case "run":
      return l.hintRun;
    case "settings":
      return l.hintSettings;
    case "folder":
      return f.platform === "other" ? l.hintFolderOther : l.hintFolder(PLATFORM_LABEL[f.platform]);
    default:
      return f.installed ? l.notFound : "";
  }
}

/** « 2024-01-15 » → « 15/01/2024 » (en anglais : « 01/15/2024 »). */
export function installDay(date: string | null): string | null {
  if (!date) return null;
  const [y, m, d] = date.split("-");
  // Forme inattendue : à l'envers, comme avant.
  if (m === undefined || d === undefined) return date.split("-").reverse().join("/");
  return t.fiche.footprint.day(y, m, d);
}
