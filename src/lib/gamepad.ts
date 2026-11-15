import { useEffect, useRef, useState } from "react";

/** Action de navigation produite par la manette. */
export type PadAction =
  | "up"
  | "down"
  | "left"
  | "right"
  | "confirm"
  | "back"
  | "details"
  | "favorite"
  | "prevFilter"
  | "nextFilter"
  | "prevSort"
  | "nextSort";

type Direction = "up" | "down" | "left" | "right";

/** Boutons du « standard mapping » de la Gamepad API (manettes Xbox, PlayStation via XInput, etc.). */
const BUTTONS: [number, PadAction][] = [
  [0, "confirm"], // A
  [1, "back"], // B
  [2, "details"], // X
  [3, "favorite"], // Y
  [4, "prevFilter"], // LB
  [5, "nextFilter"], // RB
  [6, "prevSort"], // LT
  [7, "nextSort"], // RT
];
const DPAD: [number, Direction][] = [
  [12, "up"],
  [13, "down"],
  [14, "left"],
  [15, "right"],
];

/** Inclinaison du stick à partir de laquelle il compte comme une direction. */
const DEADZONE = 0.5;
/** Direction maintenue : premier pas immédiat, puis répétition après ce délai… */
const REPEAT_DELAY = 380;
/** … à ce rythme. */
const REPEAT_EVERY = 110;

/** État lu sur une manette à un instant donné. */
export interface PadSnapshot {
  pressed: boolean[];
  axes: number[];
}

export function snapshot(pad: Gamepad): PadSnapshot {
  return { pressed: pad.buttons.map((b) => b.pressed), axes: [...pad.axes] };
}

/** Direction demandée : la croix d'abord, sinon l'axe dominant du stick gauche. */
function direction(s: PadSnapshot): Direction | null {
  for (const [i, dir] of DPAD) if (s.pressed[i]) return dir;
  const [x = 0, y = 0] = s.axes;
  if (Math.max(Math.abs(x), Math.abs(y)) < DEADZONE) return null;
  if (Math.abs(x) > Math.abs(y)) return x < 0 ? "left" : "right";
  return y < 0 ? "up" : "down";
}

/**
 * Transforme des relevés successifs en actions : un bouton agit à l'appui (pas tant qu'il reste
 * enfoncé), une direction maintenue se répète comme une touche fléchée.
 */
export class PadReader {
  private pressed = new Set<number>();
  private held: { dir: Direction; next: number } | null = null;

  update(s: PadSnapshot, now: number): PadAction[] {
    const actions: PadAction[] = [];
    for (const [i, action] of BUTTONS) {
      const down = s.pressed[i] === true;
      if (down && !this.pressed.has(i)) actions.push(action);
      if (down) this.pressed.add(i);
      else this.pressed.delete(i);
    }
    const dir = direction(s);
    if (!dir) {
      this.held = null;
    } else if (this.held?.dir !== dir) {
      this.held = { dir, next: now + REPEAT_DELAY };
      actions.push(dir);
    } else if (now >= this.held.next) {
      this.held.next = now + REPEAT_EVERY;
      actions.push(dir);
    }
    return actions;
  }
}

/**
 * Écoute les manettes branchées et appelle `onAction` à chaque action. Rien n'est lu tant
 * qu'aucune manette n'a été détectée, et rien n'agit quand Ludex n'a pas le focus (un jeu
 * lancé au premier plan ne doit pas piloter la bibliothèque). Renvoie vrai si une manette est branchée.
 */
export function useGamepad(onAction: (action: PadAction) => void): boolean {
  const [connected, setConnected] = useState(false);
  const handler = useRef(onAction);
  handler.current = onAction;

  useEffect(() => {
    if (!("getGamepads" in navigator)) return;
    const readers = new Map<number, PadReader>();
    let frame = 0;

    const pads = () => (navigator.getGamepads?.() ?? []).filter((p): p is Gamepad => !!p && p.connected);
    const loop = (now: number) => {
      const list = pads();
      if (list.length === 0) {
        frame = 0;
        readers.clear();
        setConnected(false);
        return;
      }
      // Sans le focus, les relevés sont lus mais ignorés : un bouton tenu au retour n'agit pas.
      const active = document.hasFocus() && !document.hidden;
      for (const pad of list) {
        let reader = readers.get(pad.index);
        if (!reader) readers.set(pad.index, (reader = new PadReader()));
        const actions = reader.update(snapshot(pad), now);
        if (active) actions.forEach((a) => handler.current(a));
      }
      frame = requestAnimationFrame(loop);
    };
    const start = () => {
      setConnected(true);
      if (!frame) frame = requestAnimationFrame(loop);
    };

    window.addEventListener("gamepadconnected", start);
    if (pads().length) start();
    return () => {
      window.removeEventListener("gamepadconnected", start);
      cancelAnimationFrame(frame);
    };
  }, []);

  return connected;
}
