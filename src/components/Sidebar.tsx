import { useEffect, useState } from "react";
import { MAX_COLLECTION_NAME, recentPlaytime, type Game, type RecentPlaytime } from "../lib/api";
import { formatPlaytime, relativeTime } from "../lib/format";
import { MINE_GROUP, type Section } from "../lib/sections";
import { Cover } from "./Cover";
import { PlusIcon, SidebarIcon, SlidersIcon } from "./Icons";
import { InlineInput } from "./InlineInput";

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
  /** Nouvelle collection, nommée sur place sous « Mes collections ». */
  onCreateCollection: (name: string) => void;
  /** Paramètres affichés, et leur ouverture (dernier élément de la barre, Ctrl+,). */
  settingsActive: boolean;
  onSettings: () => void;
}

/** « 12,5 h », « 40 min », « Aucune partie ». */
const recentText = (r: RecentPlaytime | null) => (!r ? "…" : r.secs < 60 ? "Aucune partie" : formatPlaytime(r.secs / 60));

/** Barre de navigation à gauche : sections, collections, puis temps de jeu récent et dernier jeu lancé. */
export function Sidebar({ sections, active, onSelect, collapsed, onToggle, lastGame, onOpenGame, playing, onCreateCollection, settingsActive, onSettings }: Props) {
  const [recent, setRecent] = useState<RecentPlaytime | null>(null);
  const [creating, setCreating] = useState(false);
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

  const item = (s: Section) => {
    const i = sections.indexOf(s);
    return (
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
    );
  };
  // Sections fixes, puis un groupe par titre (« Mes collections » toujours présent : on y crée).
  const fixed = sections.filter((s) => !s.group);
  const groups = [MINE_GROUP, ...new Set(sections.map((s) => s.group).filter((g): g is string => !!g && g !== MINE_GROUP))];

  return (
    <nav className="side" aria-label="Sections">
      {/* Haut de la barre : fait partie de la barre de titre (déplace la fenêtre). Pas toute la
          barre : un clic sur sa barre de défilement déplacerait la fenêtre au lieu de défiler. */}
      <div className="side__top" data-tauri-drag-region>
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
      </div>
      <ul className="side__list">{fixed.map(item)}</ul>
      {groups.map((group) => (
        <div key={group} className="side__group" role="group" aria-label={group}>
          <div className="side__head">
            <small>{group}</small>
            {group === MINE_GROUP && !collapsed && (
              <button
                type="button"
                className="side__add"
                aria-label="Nouvelle collection"
                title="Nouvelle collection"
                // Sans ceci, le clic ferme d'abord le champ (perte du focus) puis le rouvre.
                onMouseDown={(e) => creating && e.preventDefault()}
                onClick={() => setCreating((c) => !c)}
              >
                <PlusIcon />
              </button>
            )}
          </div>
          <ul className="side__list">
            {sections.filter((s) => s.group === group).map(item)}
            {group === MINE_GROUP && creating && !collapsed && (
              <li>
                <InlineInput
                  className="side__new"
                  maxLength={MAX_COLLECTION_NAME}
                  label="Nom de la nouvelle collection"
                  placeholder="Nom de la collection"
                  onSubmit={onCreateCollection}
                  onDone={() => setCreating(false)}
                />
              </li>
            )}
          </ul>
        </div>
      ))}
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
        {/* Dernier `.side__item` de la barre : les sections gardent leurs indices pour haut et bas. */}
        <button
          type="button"
          className="side__item"
          aria-current={settingsActive ? "page" : undefined}
          aria-label="Paramètres"
          title="Paramètres (Ctrl+,)"
          onClick={onSettings}
        >
          <SlidersIcon />
          <span>Paramètres</span>
        </button>
      </div>
    </nav>
  );
}
