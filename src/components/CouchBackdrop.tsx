// Fonds animés du mode canapé (réglage `couchBackdrop`) : dérive lente de l'image du jeu,
// particules discrètes, dégradé qui respire dans la couleur du jeu (`--lx-ambient`). Tout en
// animations CSS sur `transform` et `opacity` (composées par la carte graphique, aucune boucle JS),
// mises en pause quand la fenêtre est cachée ou réduite et coupées par `prefers-reduced-motion` (règle
// commune d'`app.css` ; les particules disparaissent). WebView2 garde `document.hidden` faux et continue
// de dessiner quand Tauri cache la fenêtre (zone de notification, `--minimized`) : l'état est donc lu
// par Tauri à chaque changement de focus ou de taille (une fenêtre cachée ou réduite perd le focus),
// relu 400 ms après (la fenêtre peut se cacher juste après) et toutes les 5 s (fenêtre cachée sans focus,
// par exemple après l'explication de la zone de notification) tant que le mode canapé est ouvert.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState, type CSSProperties } from "react";
import { onSettingsChanged, settings, type CouchBackdrop } from "../lib/api";
import { dustParticles } from "../lib/couch";

/** Fond choisi dans les Paramètres, et pause quand la fenêtre est cachée ou réduite. */
export function useCouchBackdrop(): { kind: CouchBackdrop; paused: boolean } {
  const [kind, setKind] = useState<CouchBackdrop>("still");
  const [paused, setPaused] = useState(() => document.hidden);
  useEffect(() => {
    let alive = true;
    settings()
      .then((s) => alive && setKind(s.couchBackdrop))
      .catch(() => {});
    const off = onSettingsChanged((s) => setKind(s.couchBackdrop));
    const win = getCurrentWindow();
    const check = () => {
      if (document.hidden) return setPaused(true);
      Promise.all([win.isVisible(), win.isMinimized()])
        .then(([visible, minimized]) => alive && setPaused(document.hidden || !visible || minimized))
        .catch(() => alive && setPaused(document.hidden));
    };
    let soon = 0;
    const later = () => {
      check();
      window.clearTimeout(soon);
      soon = window.setTimeout(check, 400);
    };
    check();
    const every = window.setInterval(check, 5000);
    document.addEventListener("visibilitychange", later);
    const focus = win.onFocusChanged(later);
    const resized = win.onResized(later);
    return () => {
      alive = false;
      window.clearTimeout(soon);
      window.clearInterval(every);
      document.removeEventListener("visibilitychange", later);
      for (const un of [off, focus, resized]) void un.then((f) => f()).catch(() => {});
    };
  }, []);
  return { kind, paused };
}

const DUST = dustParticles(26);

/** Couches posées dans `.couch__backdrop`, au-dessus des images (la dérive anime les images elles-mêmes). */
export function CouchFx({ kind }: { kind: CouchBackdrop }) {
  if (kind === "breathe") return <div className="couch__breathe" />;
  if (kind !== "particles") return null;
  return (
    <div className="couch__dust">
      {DUST.map((p, i) => (
        <i
          key={i}
          style={
            {
              left: `${p.left}%`,
              "--s": `${p.size}px`,
              "--d": `${p.duration}s`,
              "--delay": `${-p.delay}s`,
              "--x": `${p.drift}vw`,
              "--o": p.opacity,
            } as CSSProperties
          }
        />
      ))}
    </div>
  );
}
