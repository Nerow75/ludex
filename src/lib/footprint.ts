import { PLATFORM_LABEL, type Footprint, type FootprintFolder } from "./api";
import { formatSize } from "./format";

/** Taille ou état d'un dossier de la vue avancée. */
export function folderSize(f: FootprintFolder): string {
  if (!f.exists) return "Supprimé";
  if (f.bytes === null) return f.kind === "install" ? "Mesure en cours" : "Taille inconnue";
  return f.bytes === 0 ? "Vide" : formatSize(f.bytes);
}

/**
 * Place des emplacements présents, sans compter deux fois ce qui est dans le dossier du jeu.
 * `unknown` : au moins un dossier présent de taille inconnue.
 */
export function presentBytes(f: Footprint): { bytes: number; unknown: boolean } {
  let bytes = 0;
  let unknown = false;
  for (const x of f.folders) {
    if (!x.exists || x.inInstall) continue;
    if (x.bytes === null) unknown = true;
    else bytes += x.bytes;
  }
  return { bytes, unknown };
}

/** Ligne d'état sous le titre. */
export function footprintStatus(f: Footprint): string {
  if (f.installed) {
    const { bytes, unknown } = presentBytes(f);
    if (bytes === 0) return unknown ? "Installé, taille en cours de mesure" : "Installé";
    return `Installé : ${formatSize(bytes)} sur le PC${unknown ? " au moins" : ""}, sauvegardes et caches compris`;
  }
  if (f.leftovers === 0) return "Désinstallé : plus rien aux emplacements connus de Ludex";
  return `Désinstallé : ${f.leftovers} reste${f.leftovers > 1 ? "s" : ""} encore sur le PC`;
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
  switch (action?.kind) {
    case "steam":
      return "Steam demande confirmation puis supprime le dossier du jeu. Les autres emplacements restent : la vue les montre ensuite.";
    case "run":
      return "Le désinstalleur officiel du jeu demande sa propre confirmation. Ludex ne supprime rien lui-même.";
    case "settings":
      return "Jeu du Microsoft Store : il se désinstalle depuis Paramètres > Applications de Windows.";
    case "folder":
      return f.platform === "other"
        ? "Aucun désinstalleur trouvé pour ce jeu : supprime son dossier à la main."
        : `Aucun désinstalleur trouvé : désinstalle-le depuis ${PLATFORM_LABEL[f.platform]}, ou supprime son dossier à la main.`;
    default:
      return f.installed ? "Dossier du jeu introuvable." : "";
  }
}

/** « 2024-01-15 » → « 15/01/2024 ». */
export const installDay = (date: string | null) => (date ? date.split("-").reverse().join("/") : null);
