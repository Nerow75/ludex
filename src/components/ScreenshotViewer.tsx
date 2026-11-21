import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { showScreenshot, type Screenshot } from "../lib/api";
import { positionText } from "../lib/a11y";
import { formatDateTime, formatSize } from "../lib/format";
import { trapTab } from "./focus";
import { ChevronLeftIcon, ChevronRightIcon, CloseIcon, FolderIcon, PlayIcon } from "./Icons";

/** Origine lisible d'une capture : « Steam », « Steam, appid 244210 » (jeu désinstallé), « Xbox Game Bar ». */
function sourceLabel(shot: Screenshot): string {
  if (shot.source === "gamebar") return "Xbox Game Bar";
  return shot.gameId || !shot.steamAppid ? "Steam" : `Steam, appid ${shot.steamAppid}`;
}

/** « 12 mars 2025 à 14:23 · 2560 × 1440 · 3 Mo · Steam ». */
export function shotMeta(shot: Screenshot): string {
  const dims = shot.width && shot.height ? `${shot.width} × ${shot.height}` : null;
  return [formatDateTime(shot.taken), dims, formatSize(shot.size), sourceLabel(shot)].filter(Boolean).join(" · ");
}

interface TileProps {
  shot: Screenshot;
  label: string;
  onOpen: () => void;
  /** Captures restantes, affichées sur la dernière vignette (« +12 »). */
  more?: number;
  /** Atteinte par les flèches et la manette (`data-nav`) dans un écran de section. */
  nav?: boolean;
}

/** Vignette d'une capture : la miniature (Steam) ou l'image, jamais la pleine taille d'une vidéo. */
export function ShotTile({ shot, label, onOpen, more, nav }: TileProps) {
  return (
    <button type="button" className="shot" data-nav={nav || undefined} onClick={onOpen} aria-label={more ? `${label}, et ${more} autres` : label} title={shotMeta(shot)}>
      {shot.video ? (
        <span className="shot__video"><PlayIcon /></span>
      ) : (
        <img src={shot.thumbUrl} alt="" loading="lazy" decoding="async" draggable={false} />
      )}
      {more ? <span className="shot__more">+{more}</span> : null}
    </button>
  );
}

interface ViewerProps {
  shots: Screenshot[];
  index: number;
  /** Jeu des captures, en tête de la visionneuse. */
  title: string;
  onIndex: (index: number) => void;
  onClose: () => void;
}

/**
 * Visionneuse plein écran : flèches gauche et droite, Échap pour fermer. Tant qu'elle est ouverte,
 * aucune touche n'atteint les raccourcis de l'écran principal.
 */
export function ScreenshotViewer({ shots, index, title, onIndex, onClose }: ViewerProps) {
  const shot = shots[index];
  const closeRef = useRef<HTMLButtonElement>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const [error, setError] = useState<string | null>(null);
  const live = useRef({ index, count: shots.length, onIndex, onClose });
  live.current = { index, count: shots.length, onIndex, onClose };

  useEffect(() => {
    const before = document.activeElement as HTMLElement | null;
    closeRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      const s = live.current;
      // Phase de capture sur window : rien ne passe aux raccourcis globaux (recherche, F5, Ctrl+D…).
      e.stopPropagation();
      // Dialogue modal : Tab boucle sur ses boutons au lieu de passer à la barre latérale dessous.
      if (trapTab(e, rootRef.current)) return;
      if (e.key === "Escape") {
        e.preventDefault();
        s.onClose();
      } else if (e.key === "ArrowLeft" && s.index > 0) {
        e.preventDefault();
        s.onIndex(s.index - 1);
      } else if (e.key === "ArrowRight" && s.index < s.count - 1) {
        e.preventDefault();
        s.onIndex(s.index + 1);
      } else if (e.key === "F5") {
        e.preventDefault(); // pas de rechargement de la WebView
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      before?.focus?.();
    };
  }, []);

  // Visionneuse ouverte : boutons de la fenêtre retirés, son « Fermer » prend le coin haut droit
  // (deux croix côte à côte prêtaient à confusion).
  useEffect(() => {
    document.documentElement.classList.add("has-viewer");
    return () => document.documentElement.classList.remove("has-viewer");
  }, []);

  // Images voisines chargées d'avance : la flèche suivante affiche tout de suite.
  useEffect(() => {
    setError(null);
    for (const near of [shots[index - 1], shots[index + 1]]) {
      if (near && !near.video) new Image().src = near.url;
    }
  }, [shots, index]);

  if (!shot) return null;

  const reveal = () => showScreenshot(shot).catch((e) => setError(String(e)));

  return createPortal(
    <div
      className="viewer"
      ref={rootRef}
      role="dialog"
      aria-modal="true"
      aria-label={`Captures de ${title}`}
      // Un portail remonte les événements React à ses parents (fiche, grille) : ils s'arrêtent ici.
      onClick={(e) => e.stopPropagation()}
      onMouseDown={(e) => e.stopPropagation()}
      onPointerDown={(e) => e.stopPropagation()}
    >
      <header className="viewer__bar" data-tauri-drag-region="deep">
        <span className="viewer__title">{title}</span>
        <span className="viewer__count" aria-hidden="true">{index + 1} / {shots.length}</span>
        {/* Annonce de la capture affichée à chaque flèche (le compteur visible « 1 / 128 » se lit mal). */}
        <span className="sr-only" aria-live="polite">{positionText("Capture", index, shots.length)}</span>
        <button type="button" className="btn-icon" aria-label="Ouvrir le dossier" title="Ouvrir le dossier de la capture" onClick={reveal}>
          <FolderIcon />
        </button>
        <button type="button" className="btn-icon" ref={closeRef} aria-label="Fermer" title="Fermer (Échap)" onClick={onClose}>
          <CloseIcon />
        </button>
      </header>
      <div className="viewer__stage" onClick={(e) => e.target === e.currentTarget && onClose()}>
        <button
          type="button"
          className="viewer__nav"
          aria-label="Capture précédente"
          title="Précédente (←)"
          disabled={index === 0}
          onClick={() => onIndex(index - 1)}
        >
          <ChevronLeftIcon />
        </button>
        {shot.video ? (
          <video key={shot.key} className="viewer__media" src={shot.url} controls autoPlay />
        ) : (
          <img key={shot.key} className="viewer__media" src={shot.url} alt={`Capture de ${title}, ${formatDateTime(shot.taken)}`} draggable={false} />
        )}
        <button
          type="button"
          className="viewer__nav"
          aria-label="Capture suivante"
          title="Suivante (→)"
          disabled={index === shots.length - 1}
          onClick={() => onIndex(index + 1)}
        >
          <ChevronRightIcon />
        </button>
      </div>
      <footer className="viewer__meta">
        {error ? <span className="viewer__error" role="alert">{error}</span> : shotMeta(shot)}
      </footer>
    </div>,
    document.body,
  );
}
