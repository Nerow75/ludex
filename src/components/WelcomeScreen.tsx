// Écran de premier lancement : ce que Ludex a trouvé, trois raccourcis, puis la bibliothèque.
import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { ScanResult, WelcomeInfo } from "../lib/api";
import { summarize } from "../lib/welcome";
import { LudexMark } from "./Icons";

interface Props {
  scan: ScanResult;
  info: WelcomeInfo;
  /** Une manette est branchée : ses boutons accompagnent les touches. */
  pad: boolean;
  /** « C'est parti », Entrée, Échap ou B. */
  onDone: () => void;
  /** « Voir les applis masquées ». */
  onShowHidden: () => void;
}

/** Durée du fondu de sortie, avant de rendre la main à la bibliothèque. */
const LEAVE_MS = 160;

const plural = (n: number, one: string, many: string) => `${n.toLocaleString("fr-FR")} ${n > 1 ? many : one}`;

export default function WelcomeScreen({ scan, info, pad, onDone, onShowHidden }: Props) {
  const s = useMemo(() => summarize(scan), [scan]);
  const root = useRef<HTMLDivElement>(null);
  const start = useRef<HTMLButtonElement>(null);
  const [leaving, setLeaving] = useState(false);
  const live = useRef({ onDone, onShowHidden, leaving });
  live.current = { onDone, onShowHidden, leaving };

  const leave = (then: () => void) => {
    if (live.current.leaving) return;
    setLeaving(true);
    window.setTimeout(then, LEAVE_MS);
  };
  const leaveRef = useRef(leave);
  leaveRef.current = leave;

  // Toutes les touches s'arrêtent ici (phase de capture) : aucun raccourci de la bibliothèque
  // derrière, taper ne remplit pas la recherche. La manette arrive par des `keydown` relayés.
  useEffect(() => {
    start.current?.focus({ preventScroll: true });
    const onKey = (e: KeyboardEvent) => {
      e.stopPropagation();
      const items = [...(root.current?.querySelectorAll<HTMLElement>("[data-nav]") ?? [])];
      const current = document.activeElement as HTMLElement | null;
      const i = current ? items.indexOf(current) : -1;
      if (e.key === "Enter" || (e.key === " " && i >= 0)) {
        e.preventDefault();
        (i >= 0 ? items[i] : start.current)?.click();
      } else if (e.key === "Escape") {
        e.preventDefault();
        leaveRef.current(live.current.onDone);
      } else if (e.key === "Tab" || e.key.startsWith("Arrow")) {
        e.preventDefault(); // le focus reste dans l'écran
        const back = e.key === "ArrowUp" || e.key === "ArrowLeft" || (e.key === "Tab" && e.shiftKey);
        const next = i < 0 ? items.indexOf(start.current!) : (i + (back ? -1 : 1) + items.length) % items.length;
        items[next]?.focus();
      } else if (e.key === "F5" || e.ctrlKey || e.metaKey) {
        e.preventDefault(); // ni rechargement de la WebView, ni raccourci du navigateur
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  const others = [
    s.empty.length > 0 && `Installés sans jeu : ${s.empty.join(", ")}`,
    s.missing.length > 0 && `Pas sur ce PC : ${s.missing.join(", ")}`,
    s.failed.length > 0 && `Lecture impossible : ${s.failed.join(", ")}`,
  ].filter(Boolean) as string[];

  return createPortal(
    <div
      ref={root}
      className={`welcome ${leaving ? "is-leaving" : ""}`}
      role="dialog"
      aria-modal="true"
      aria-labelledby="welcome-title"
      // Un portail remonte les événements React à ses parents : ils s'arrêtent ici.
      onClick={(e) => e.stopPropagation()}
      onMouseDown={(e) => e.stopPropagation()}
    >
      <div className="welcome__panel">
        <span className="welcome__brand"><LudexMark size={32} />Ludex</span>

        <header className="welcome__head">
          <h1 id="welcome-title">
            {s.total ? <><b>{plural(s.total, "jeu", "jeux")}</b> {s.where}</> : "Aucun jeu trouvé pour l'instant"}
          </h1>
          {s.total === 0 && (
            <p className="welcome__lead">Installe un jeu avec ton launcher habituel, Ludex le verra tout seul. Un jeu rangé ailleurs s'ajoute avec son exécutable.</p>
          )}
        </header>

        {s.found.length > 0 && (
          <ul className="welcome__sources" aria-label="Jeux par source">
            {s.found.map((f) => (
              <li key={f.platform}>{f.label}<span>{f.count.toLocaleString("fr-FR")}</span></li>
            ))}
          </ul>
        )}

        <div className="welcome__facts">
          {others.map((line) => <p key={line}>{line}</p>)}
          {s.tools > 0 && (
            <p>
              {plural(s.tools, "appli masquée", "applis masquées")} : ce ne sont pas des jeux (fonds d'écran, éditeurs, serveurs).{" "}
              <button type="button" className="welcome__link" data-nav onClick={() => leave(onShowHidden)}>
                {s.tools > 1 ? "Les voir" : "La voir"}
              </button>
            </p>
          )}
          {info.folders.length > 0 ? (
            <p>
              {info.folders.length > 1 ? "Dossiers de jeux lus" : "Dossier de jeux lu"} :{" "}
              {info.folders.map((f, i) => <span key={f}>{i > 0 && ", "}<code>{f}</code></span>)}
            </p>
          ) : (
            <p>Aucun dossier « Jeux » ou « Games » à la racine de tes disques : un jeu rangé ailleurs s'ajoute avec <kbd>Ctrl</kbd><kbd>N</kbd>.</p>
          )}
        </div>

        <section className="welcome__keys" aria-labelledby="welcome-keys">
          <h2 id="welcome-keys">Trois raccourcis à retenir</h2>
          <dl>
            <div>
              <dt>Chercher un jeu</dt>
              <dd>Tape n'importe où, ou <kbd>Ctrl</kbd><kbd>K</kbd></dd>
            </div>
            <div>
              <dt>Jouer au jeu choisi</dt>
              <dd><kbd>Entrée</kbd>{pad && <> ou <kbd className="pad">A</kbd></>}</dd>
            </div>
            <div>
              <dt>Ouvrir sa fiche</dt>
              <dd><kbd>Espace</kbd>{pad && <> ou <kbd className="pad">X</kbd></>}</dd>
            </div>
          </dl>
          {pad && (
            <p className="welcome__pad">Manette branchée : la croix se déplace partout, <kbd>LB</kbd><kbd>RB</kbd> changent de section, <kbd className="pad">B</kbd> ferme.</p>
          )}
        </section>

        <div className="welcome__go">
          <button ref={start} type="button" className="btn-play" data-nav onClick={() => leave(onDone)}>C'est parti</button>
          <span className="welcome__hint"><kbd>Entrée</kbd>{pad && <kbd className="pad">A</kbd>}</span>
        </div>
      </div>
    </div>,
    document.body,
  );
}
