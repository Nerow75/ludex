// Menu contextuel d'un jeu : entrées et position, en fonctions pures (sans React ni DOM),
// pour qu'elles restent simples à relire et à vérifier.
import { STATUSES, STATUS_LABEL, type Collection, type Game, type Status } from "./api";

/** Action d'une entrée, traduite en appel par `App` (aucune ne prend de chemin). */
export type MenuAction =
  | { kind: "play" }
  | { kind: "open" }
  | { kind: "favorite" }
  | { kind: "status"; status: Status | null }
  | { kind: "collection"; id: string; value: boolean }
  | { kind: "newCollection" }
  | { kind: "folder" }
  | { kind: "rename" }
  | { kind: "hide" };

export type MenuEntry =
  /** `checked` : case (collection) ou choix unique (statut) ; absent = entrée simple. */
  | { kind: "item"; key: string; label: string; action: MenuAction; role?: "checkbox" | "radio"; checked?: boolean }
  | { kind: "submenu"; key: string; label: string; children: MenuEntry[] }
  | { kind: "separator"; key: string };

/**
 * Entrées du menu d'un jeu, dans l'ordre affiché. « Ouvrir le dossier » seulement si le jeu en a
 * un ; « Masquer » devient « Afficher » pour un jeu masqué ; le sous-menu des collections liste les
 * collections perso (cochées si le jeu y est) puis « Nouvelle collection… ».
 */
export function gameMenu(game: Game, collections: readonly Collection[]): MenuEntry[] {
  const statuses: MenuEntry[] = [...STATUSES, null].map((s) => ({
    kind: "item",
    key: `status-${s ?? "none"}`,
    label: s ? STATUS_LABEL[s] : "Aucun",
    action: { kind: "status", status: s },
    role: "radio",
    checked: game.status === s,
  }));
  const cols: MenuEntry[] = collections.map((c) => {
    const inside = c.games.includes(game.id);
    return { kind: "item", key: `col-${c.id}`, label: c.name, action: { kind: "collection", id: c.id, value: !inside }, role: "checkbox", checked: inside };
  });
  if (cols.length) cols.push({ kind: "separator", key: "col-sep" });
  cols.push({ kind: "item", key: "col-new", label: "Nouvelle collection…", action: { kind: "newCollection" } });

  const entries: MenuEntry[] = [
    { kind: "item", key: "play", label: "Jouer", action: { kind: "play" } },
    { kind: "item", key: "open", label: "Ouvrir la fiche", action: { kind: "open" } },
    { kind: "separator", key: "sep-1" },
    { kind: "item", key: "favorite", label: game.favorite ? "Retirer des favoris" : "Ajouter aux favoris", action: { kind: "favorite" } },
    { kind: "submenu", key: "status", label: "Statut", children: statuses },
    { kind: "submenu", key: "collections", label: "Ajouter à une collection", children: cols },
    { kind: "separator", key: "sep-2" },
  ];
  if (game.installDir) entries.push({ kind: "item", key: "folder", label: "Ouvrir le dossier", action: { kind: "folder" } });
  entries.push(
    { kind: "item", key: "rename", label: "Renommer", action: { kind: "rename" } },
    { kind: "item", key: "hide", label: game.hidden ? "Afficher" : "Masquer", action: { kind: "hide" } },
  );
  return entries;
}

export interface Point {
  x: number;
  y: number;
}
export interface Size {
  width: number;
  height: number;
}
export interface Rect {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/** Marge gardée entre le menu et le bord de la fenêtre. */
export const EDGE = 8;

const clamp = (v: number, min: number, max: number) => Math.max(min, Math.min(v, max));

/**
 * Coin haut-gauche du menu ouvert au point `at` (clic droit) : à droite et en dessous du point,
 * sinon de l'autre côté s'il déborde, et toujours dans la fenêtre (collé à la marge s'il est plus
 * grand qu'elle ; la liste défile alors).
 */
export function placeMenu(at: Point, menu: Size, view: Size): Point {
  const x = at.x + menu.width > view.width - EDGE ? at.x - menu.width : at.x;
  const y = at.y + menu.height > view.height - EDGE ? at.y - menu.height : at.y;
  return {
    x: clamp(x, EDGE, view.width - EDGE - menu.width),
    y: clamp(y, EDGE, view.height - EDGE - menu.height),
  };
}

/**
 * Coin haut-gauche d'un sous-menu ouvert depuis l'entrée `item` : à sa droite, aligné sur son haut,
 * sinon à sa gauche s'il déborde, et toujours dans la fenêtre.
 */
export function placeSubmenu(item: Rect, menu: Size, view: Size): Point {
  const x = item.right + menu.width > view.width - EDGE ? item.left - menu.width : item.right;
  return {
    x: clamp(x, EDGE, view.width - EDGE - menu.width),
    y: clamp(item.top, EDGE, view.height - EDGE - menu.height),
  };
}

/**
 * Entrée suivante (ou précédente) qu'on peut atteindre au clavier, en boucle, séparateurs sautés.
 * `from` = -1 : aucune entrée active (la première ou la dernière selon le sens).
 */
export function stepEntry(entries: readonly MenuEntry[], from: number, delta: 1 | -1): number {
  const n = entries.length;
  if (!entries.some((e) => e.kind !== "separator")) return -1;
  let i = from < 0 ? (delta > 0 ? -1 : n) : from;
  do i = (i + delta + n) % n;
  while (entries[i].kind === "separator");
  return i;
}
