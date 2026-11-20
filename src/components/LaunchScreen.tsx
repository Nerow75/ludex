// Écran de lancement « cinéma » : au clic sur Jouer, l'image du jeu floutée occupe la fenêtre (jamais
// de vrai plein écran), avec son logo, son launcher et l'état de la partie suivi par `game-session`.
// Calculs (états, délais, textes) dans `src/lib/launch.ts`.
import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { onGameSession, type Game } from "../lib/api";
import { titleColor } from "../lib/color";
import {
  afterError,
  afterSession,
  autoClose,
  FADE_MS,
  launchBackdrop,
  launcherLine,
  launchText,
  RELAY_HINT_MS,
  SOFT_FADE_MS,
  startLaunch,
  type LaunchView,
} from "../lib/launch";
import { Logo } from "./Art";
import { LauncherIcon } from "./LauncherIcon";

interface Props {
  game: Game;
  /** Le jeu tournait déjà au clic : « Bonne partie » tout de suite. */
  alreadyPlaying: boolean;
  /** Erreur de `launch_game`, arrivée après l'ouverture de l'écran. */
  error: string | null;
  /** Fondu de sortie terminé : l'écran peut être retiré. */
  onClosed: () => void;
  /** Fermeture demandée de l'extérieur (manette), posée par l'écran. */
  closeRef: React.RefObject<(() => void) | null>;
}

/** Touches seules qui ne ferment pas l'écran (Alt+Tab, Ctrl…). */
const MODIFIERS = new Set(["Alt", "AltGraph", "Control", "Shift", "Meta", "OS", "CapsLock", "NumLock"]);

export function LaunchScreen({ game, alreadyPlaying, error, onClosed, closeRef }: Props) {
  const [view, setView] = useState<LaunchView>(() => startLaunch(game, alreadyPlaying, Date.now()));
  /** Durée du fondu de sortie en cours, `null` tant que l'écran reste. */
  const [leaving, setLeaving] = useState<number | null>(null);
  /** Heure relue pour faire apparaître « Steam prend le relais… ». */
  const [now, setNow] = useState(() => Date.now());
  const [artFailed, setArtFailed] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const closeBtn = useRef<HTMLButtonElement>(null);
  const viewRef = useRef(view);
  viewRef.current = view;
  const done = useRef(onClosed);
  done.current = onClosed;

  const leave = useCallback((soft = false) => setLeaving((l) => l ?? (soft ? SOFT_FADE_MS : FADE_MS)), []);
  closeRef.current = leave;

  // Fondu terminé : l'écran est retiré par App.
  useEffect(() => {
    if (leaving === null) return;
    const t = window.setTimeout(() => done.current(), leaving);
    return () => window.clearTimeout(t);
  }, [leaving]);

  // État de la partie : jeu vu, ou session finie (lanceur intermédiaire refermé sans jeu).
  useEffect(() => {
    const unlisten = onGameSession((e) => {
      const next = afterSession(viewRef.current, e, Date.now());
      if (next) setView(next);
      else leave();
    });
    return () => void unlisten.then((stop) => stop()).catch(() => {});
  }, [leave]);

  // Erreur de `launch_game` : message et bouton Fermer (focus dessus).
  useEffect(() => {
    if (error !== null) setView((v) => afterError(v, game.id, error, Date.now()));
  }, [error, game.id]);
  useEffect(() => {
    if (view.phase === "error") closeBtn.current?.focus();
  }, [view.phase]);

  // Fermeture seule : 2 s après « Bonne partie », ou en douceur après 20 s sans jeu vu.
  useEffect(() => {
    const auto = autoClose(view, Date.now());
    if (!auto || leaving !== null) return;
    const t = window.setTimeout(() => leave(auto.soft), auto.ms);
    return () => window.clearTimeout(t);
  }, [view, leaving, leave]);

  // Ligne « … prend le relais » après quelques secondes de lancement.
  useEffect(() => {
    if (view.phase !== "launching") return;
    const t = window.setTimeout(() => setNow(Date.now()), Math.max(0, view.since + RELAY_HINT_MS - Date.now()));
    return () => window.clearTimeout(t);
  }, [view]);

  // Clavier : toutes les touches arrêtées en phase de capture (aucun raccourci de la bibliothèque),
  // n'importe laquelle ferme l'écran. Focus pris à l'ouverture, rendu à la fermeture.
  useEffect(() => {
    const before = document.activeElement as HTMLElement | null;
    root.current?.focus({ preventScroll: true });
    const onKey = (e: KeyboardEvent) => {
      e.stopPropagation();
      if (e.altKey && e.key === "F4") return; // Alt+F4 garde son effet (fermer la fenêtre)
      if (MODIFIERS.has(e.key)) return;
      e.preventDefault();
      // Touche maintenue (Entrée qui a lancé le jeu) : l'écran reste.
      if (!e.repeat) leave();
    };
    window.addEventListener("keydown", onKey, true);
    document.documentElement.classList.add("has-launch");
    return () => {
      window.removeEventListener("keydown", onKey, true);
      document.documentElement.classList.remove("has-launch");
      before?.focus?.({ preventScroll: true });
    };
  }, [leave]);

  const { status, detail } = launchText(view, game, now);
  const backdrop = launchBackdrop(game);
  const showArt = backdrop.url && !artFailed;
  const stop = (e: React.SyntheticEvent) => e.stopPropagation();

  return createPortal(
    <div
      ref={root}
      className={`launch is-${view.phase} ${leaving !== null ? "is-leaving" : ""}`}
      style={{ ["--art-tint" as string]: titleColor(game.title), ["--launch-fade" as string]: `${leaving ?? FADE_MS}ms` }}
      role="dialog"
      aria-modal="true"
      aria-label={`Lancement de ${game.title}`}
      tabIndex={-1}
      // Un portail remonte les événements React à ses parents (grille, fiche) : ils s'arrêtent ici.
      onMouseDown={stop}
      onPointerDown={stop}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
      }}
      onClick={(e) => {
        e.stopPropagation();
        leave();
      }}
    >
      <div className={`launch__art launch__art--${showArt ? backdrop.kind : "tint"}`} aria-hidden>
        {showArt && <img src={backdrop.url!} alt="" draggable={false} onError={() => setArtFailed(true)} />}
      </div>
      <div className="launch__scrim" aria-hidden />
      <div className="launch__body">
        <Logo game={game} className="launch__logo" />
        <p className="launch__launcher">
          <LauncherIcon platform={game.platform} size={16} />
          {launcherLine(game)}
        </p>
        <div className="launch__status" role="status" aria-live="polite">
          <span className="launch__state">{status}</span>
          {view.phase !== "error" && <span className="launch__line" aria-hidden />}
          <span className={`launch__detail ${detail ? "is-on" : ""}`}>{detail ?? " "}</span>
        </div>
        {view.phase === "error" && (
          <button ref={closeBtn} type="button" className="launch__close" onClick={(e) => {
            e.stopPropagation();
            leave();
          }}>
            Fermer
          </button>
        )}
      </div>
      <p className="launch__hint" aria-hidden>
        N'importe quelle touche, B ou un clic pour revenir à Ludex
      </p>
    </div>,
    document.body,
  );
}
