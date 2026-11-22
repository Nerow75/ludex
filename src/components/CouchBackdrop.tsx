// Fonds animés du mode canapé (réglage `couchBackdrop`) : dérive lente de l'image du jeu,
// particules discrètes, dégradé qui respire dans la couleur du jeu (`--lx-ambient`). Tout en
// animations CSS sur `transform` et `opacity` (composées par la carte graphique, aucune boucle JS),
// mises en pause quand la page est cachée et coupées par `prefers-reduced-motion` (règle commune
// d'`app.css` ; les particules disparaissent).
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
    const onVisibility = () => setPaused(document.hidden);
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      alive = false;
      document.removeEventListener("visibilitychange", onVisibility);
      void off.then((f) => f()).catch(() => {});
    };
  }, []);
  return { kind, paused };
}

const DUST = dustParticles(18);

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
