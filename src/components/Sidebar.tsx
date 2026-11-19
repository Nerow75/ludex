import { useEffect, useState } from "react";
import { recentPlaytime, type Game, type RecentPlaytime } from "../lib/api";
import { formatPlaytime, relativeTime } from "../lib/format";
import type { Section } from "../lib/sections";
import { Cover } from "./Cover";
import { SidebarIcon } from "./Icons";

interface Props {
  sections: readonly Section[];
  active: string;
  onSelect: (id: string) => void;
  /** Icônes seules. */
  collapsed: boolean;
  onToggle: () => void;
  /** Dernier jeu lancé, ouvert dans sa fiche au clic. */
  lastGame: Game | null;
  onOpenGame: (game: Game) => void;
  /** Jeux en cours : le temps récent est relu quand une partie se termine. */
  playing: ReadonlySet<string>;
}

/** « 12,5 h », « 40 min », « Aucune partie ». */
const recentText = (r: RecentPlaytime | null) => (!r ? "…" : r.secs < 60 ? "Aucune partie" : formatPlaytime(r.secs / 60));

/** Barre de navigation à gauche : sections, puis temps de jeu récent et dernier jeu lancé. */
export function Sidebar({ sections, active, onSelect, collapsed, onToggle, lastGame, onOpenGame, playing }: Props) {
  const [recent, setRecent] = useState<RecentPlaytime | null>(null);
  useEffect(() => {
    let alive = true;
    recentPlaytime()
      .then((r) => alive && setRecent(r))
      .catch(() => alive && setRecent(null));
    return () => {
      alive = false;
    };
  }, [playing]);

  const recentLabel = `${recent?.days ?? 14} derniers jours`;
  const recentTitle = recent
    ? `${recentLabel} : ${recentText(recent)}${recent.sessions ? `, ${recent.sessions} session${recent.sessions > 1 ? "s" : ""}` : ""}. Parties lancées depuis Ludex.`
    : undefined;

  return (
    <nav className="side" aria-label="Sections">
      <button
        type="button"
        className="side__toggle"
        onClick={onToggle}
        aria-expanded={!collapsed}
        aria-label={collapsed ? "Déplier la barre latérale" : "Replier la barre latérale"}
        title={`${collapsed ? "Déplier" : "Replier"} la barre (Ctrl+B)`}
      >
        <SidebarIcon />
      </button>
      <ul className="side__list">
        {sections.map((s, i) => (
          <li key={s.id}>
            <button
              type="button"
              className="side__item"
              aria-current={s.id === active ? "page" : undefined}
              aria-label={s.label}
              title={i < 9 ? `${s.label} (Ctrl+${i + 1})` : s.label}
              onClick={() => onSelect(s.id)}
            >
              {s.icon()}
              <span>{s.label}</span>
            </button>
          </li>
        ))}
      </ul>
      <div className="side__foot">
        <p className="side__stat" title={recentTitle}>
          <small>{recentLabel}</small>
          <b>{recentText(recent)}</b>
        </p>
        {lastGame && (
          <button
            type="button"
            className="side__last"
            onClick={() => onOpenGame(lastGame)}
            aria-label={`Dernier jeu lancé : ${lastGame.title}, ouvrir sa fiche`}
            title={`${lastGame.title}, ${relativeTime(lastGame.lastPlayed)}`}
          >
            <Cover game={lastGame} className="side__cover" />
            <span>
              <small>Dernier lancé</small>
              <b>{lastGame.title}</b>
            </span>
          </button>
        )}
      </div>
    </nav>
  );
}
