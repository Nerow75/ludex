import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { placeMenu, placeSubmenu, stepEntry, type MenuAction, type MenuEntry, type Point } from "../lib/contextMenu";
import { CheckIcon, ChevronRightIcon } from "./Icons";

interface Props {
  entries: MenuEntry[];
  /** Point d'ouverture (clic droit, ou milieu de l'élément au clavier et à la manette). */
  at: Point;
  label: string;
  /** Ouvert au clavier ou à la manette : la première entrée est active d'emblée. */
  keyboard: boolean;
  onAction: (action: MenuAction) => void;
  /** `restore` : rendre le focus à l'élément d'origine (fermé au clavier ou par une action). */
  onClose: (restore: boolean) => void;
}

/** Sous-menu ouvert : entrée qui l'ouvre, entrée active dedans, et si le focus y est. */
interface Sub {
  index: number;
  active: number;
  focus: boolean;
}

const roleOf = (e: Extract<MenuEntry, { kind: "item" }>) =>
  e.role === "checkbox" ? "menuitemcheckbox" : e.role === "radio" ? "menuitemradio" : "menuitem";

/**
 * Menu contextuel maison (`role="menu"`), rendu dans un portail et placé dans la fenêtre.
 * Clavier : ↑ ↓ Début Fin, → ou Entrée ouvrent un sous-menu, ← ou Échap en sortent, Échap ou Tab
 * ferment le menu. Aucune touche ne remonte aux raccourcis globaux pendant qu'il est ouvert.
 */
export function ContextMenu({ entries, at, label, keyboard, onAction, onClose }: Props) {
  const [active, setActive] = useState(() => (keyboard ? stepEntry(entries, -1, 1) : -1));
  const [sub, setSub] = useState<Sub | null>(null);
  const [pos, setPos] = useState<Point | null>(null);
  const [subPos, setSubPos] = useState<Point | null>(null);
  const layer = useRef<HTMLDivElement>(null);
  const root = useRef<HTMLUListElement>(null);
  const subRoot = useRef<HTMLUListElement>(null);
  const close = useRef(onClose);
  close.current = onClose;

  const subEntry = sub ? entries[sub.index] : null;
  const children = subEntry?.kind === "submenu" ? subEntry.children : [];
  const view = () => ({ width: window.innerWidth, height: window.innerHeight });

  // Placé après une première mesure (invisible), puis gardé dans la fenêtre.
  useLayoutEffect(() => {
    const r = root.current?.getBoundingClientRect();
    if (r) setPos(placeMenu(at, { width: r.width, height: r.height }, view()));
  }, [at, entries]);

  useLayoutEffect(() => {
    setSubPos(null);
    if (!sub) return;
    const item = root.current?.children[sub.index]?.getBoundingClientRect();
    const r = subRoot.current?.getBoundingClientRect();
    if (item && r) setSubPos(placeSubmenu(item, { width: r.width, height: r.height }, view()));
  }, [sub?.index, pos]);

  // Le focus suit l'entrée active (sans faire défiler la page derrière).
  useEffect(() => {
    if (!pos) return;
    const target =
      sub?.focus && sub.active >= 0
        ? subRoot.current?.children[sub.active]?.firstElementChild
        : active >= 0
          ? root.current?.children[active]?.firstElementChild
          : root.current;
    (target as HTMLElement | null | undefined)?.focus({ preventScroll: true });
  }, [pos, subPos, active, sub?.active, sub?.focus]);

  // Clic ailleurs, fenêtre qui perd le focus, redimensionnée ou défilée : le menu se ferme.
  useEffect(() => {
    const outside = (e: Event) => {
      if (!layer.current?.contains(e.target as Node)) close.current(false);
    };
    const away = () => close.current(false);
    document.addEventListener("pointerdown", outside, true);
    document.addEventListener("scroll", outside, true);
    window.addEventListener("blur", away);
    window.addEventListener("resize", away);
    return () => {
      document.removeEventListener("pointerdown", outside, true);
      document.removeEventListener("scroll", outside, true);
      window.removeEventListener("blur", away);
      window.removeEventListener("resize", away);
    };
  }, []);

  const run = (action: MenuAction) => {
    close.current(true);
    onAction(action);
  };

  /** Entrée choisie (Entrée, Espace, clic) : action, ou sous-menu ouvert avec le focus dedans. */
  const choose = (index: number, inSub: boolean) => {
    const entry = inSub ? children[index] : entries[index];
    if (entry?.kind === "item") run(entry.action);
    else if (entry?.kind === "submenu") {
      setActive(index);
      setSub({ index, active: stepEntry(entry.children, -1, 1), focus: true });
    }
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    e.stopPropagation(); // ni recherche, ni Ctrl+D, ni flèches de la grille pendant le menu
    const inSub = !!sub?.focus;
    const list = inSub ? children : entries;
    const current = inSub ? sub!.active : active;
    const move = (i: number) => (inSub ? setSub({ ...sub!, active: i }) : (setActive(i), setSub(null)));
    switch (e.key) {
      case "ArrowDown":
      case "ArrowUp":
        move(stepEntry(list, current, e.key === "ArrowDown" ? 1 : -1));
        break;
      case "Home":
        move(stepEntry(list, -1, 1));
        break;
      case "End":
        move(stepEntry(list, -1, -1));
        break;
      case "ArrowRight":
        if (!inSub && entries[current]?.kind === "submenu") choose(current, false);
        break;
      case "ArrowLeft":
        if (inSub) setSub(null);
        break;
      case "Enter":
      case " ":
        if (current >= 0) choose(current, inSub);
        break;
      case "Escape":
        if (inSub) setSub(null);
        else close.current(true);
        break;
      case "Tab":
        close.current(true);
        break;
      default:
        return; // touche sans effet : rien d'empêché (Alt+F4 reste possible)
    }
    e.preventDefault();
  };

  const renderEntry = (entry: MenuEntry, i: number, inSub: boolean) => {
    if (entry.kind === "separator") return <li key={entry.key} role="separator" className="cmenu__sep" />;
    const isActive = inSub ? sub?.active === i : active === i;
    const hover = () => {
      if (inSub) setSub((s) => s && { ...s, active: i, focus: true });
      else {
        setActive(i);
        setSub(entry.kind === "submenu" ? { index: i, active: -1, focus: false } : null);
      }
    };
    if (entry.kind === "submenu") {
      const open = sub?.index === i;
      return (
        <li key={entry.key} role="none">
          <button
            type="button"
            role="menuitem"
            tabIndex={-1}
            aria-haspopup="menu"
            aria-expanded={open}
            className={`cmenu__item ${isActive || open ? "is-active" : ""}`}
            onPointerMove={() => !open && hover()}
            onClick={() => choose(i, false)}
          >
            <span className="cmenu__check" />
            <span className="cmenu__label">{entry.label}</span>
            <ChevronRightIcon />
          </button>
        </li>
      );
    }
    return (
      <li key={entry.key} role="none">
        <button
          type="button"
          role={roleOf(entry)}
          tabIndex={-1}
          aria-checked={entry.role ? !!entry.checked : undefined}
          className={`cmenu__item ${isActive ? "is-active" : ""}`}
          onPointerMove={() => !isActive && hover()}
          onClick={() => choose(i, inSub)}
        >
          <span className="cmenu__check">{entry.checked && <CheckIcon />}</span>
          <span className="cmenu__label">{entry.label}</span>
        </button>
      </li>
    );
  };

  return createPortal(
    <div
      ref={layer}
      className="cmenu-layer"
      onKeyDown={onKeyDown}
      // Un portail remonte les événements React à ses parents : ils s'arrêtent ici.
      onClick={(e) => e.stopPropagation()}
      onDoubleClick={(e) => e.stopPropagation()}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
      }}
    >
      <ul
        ref={root}
        role="menu"
        aria-label={label}
        tabIndex={-1}
        className="cmenu"
        style={pos ? { left: pos.x, top: pos.y } : { left: 0, top: 0, visibility: "hidden" }}
      >
        {entries.map((e, i) => renderEntry(e, i, false))}
      </ul>
      {subEntry?.kind === "submenu" && (
        <ul
          ref={subRoot}
          role="menu"
          aria-label={subEntry.label}
          tabIndex={-1}
          className="cmenu"
          style={subPos ? { left: subPos.x, top: subPos.y } : { left: 0, top: 0, visibility: "hidden" }}
        >
          {children.map((e, i) => renderEntry(e, i, true))}
        </ul>
      )}
    </div>,
    document.body,
  );
}
