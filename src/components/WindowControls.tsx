import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { closeWindow, minimizeWindow, toggleMaximizeWindow } from "../lib/api";
import { t } from "../lib/i18n";

/* Glyphes des boutons de légende : dessinés comme ceux de Windows 11 (10 px, trait de 1 px),
   seule exception à la règle des icônes de 18 px, pour se fondre avec les autres fenêtres. */
const glyph = {
  width: 10,
  height: 10,
  viewBox: "0 0 10 10",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1,
  "aria-hidden": true,
  shapeRendering: "crispEdges" as const,
};

const MinimizeGlyph = () => <svg {...glyph}><path d="M0 5.5h10" /></svg>;
const MaximizeGlyph = () => <svg {...glyph}><rect x="0.5" y="0.5" width="9" height="9" rx="1" /></svg>;
const RestoreGlyph = () => (
  <svg {...glyph}>
    <rect x="0.5" y="2.5" width="7" height="7" rx="1" />
    <path d="M2.5 2.5v-1a1 1 0 0 1 1-1h5a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1h-1" />
  </svg>
);
const CloseGlyph = () => <svg {...glyph} shapeRendering="geometricPrecision"><path d="m.5.5 9 9m0-9-9 9" /></svg>;

/** Erreur d'un bouton : rien à montrer à l'écran (la fenêtre reste telle quelle). */
const report = (e: unknown) => console.error("Barre de titre :", e);

/**
 * Réduire, Agrandir ou Restaurer, Fermer : coin haut droit de la fenêtre principale, au-dessus de
 * tout (écran de bienvenue et visionneuse compris). Hors de l'ordre de tabulation, comme les
 * boutons de Windows (Alt+F4, Win+flèches restent au clavier).
 */
export function WindowControls() {
  const [maximized, setMaximized] = useState(false);
  const [focused, setFocused] = useState(true);

  useEffect(() => {
    const win = getCurrentWindow();
    let alive = true;
    const sync = () => win.isMaximized().then((m) => alive && setMaximized(m)).catch(() => {});
    sync();
    const resized = win.onResized(sync);
    const focus = win.onFocusChanged(({ payload }) => alive && setFocused(payload));
    return () => {
      alive = false;
      resized.then((off) => off()).catch(() => {});
      focus.then((off) => off()).catch(() => {});
    };
  }, []);

  const text = t.extras.window;
  // Portail : au-dessus des autres portails (bienvenue, visionneuse), hors du contexte isolé de `.app`.
  return createPortal(
    <div className={`caption ${focused ? "" : "is-inactive"}`} role="group" aria-label={text.group}>
      <button type="button" tabIndex={-1} aria-label={text.minimize} title={text.minimize} onClick={() => minimizeWindow().catch(report)}>
        <MinimizeGlyph />
      </button>
      <button
        type="button"
        tabIndex={-1}
        aria-label={maximized ? text.restore : text.maximize}
        title={maximized ? text.restore : text.maximize}
        onClick={() => toggleMaximizeWindow().catch(report)}
      >
        {maximized ? <RestoreGlyph /> : <MaximizeGlyph />}
      </button>
      <button type="button" tabIndex={-1} className="caption__close" aria-label={text.close} title={text.close} onClick={() => closeWindow().catch(report)}>
        <CloseGlyph />
      </button>
    </div>,
    document.body,
  );
}
