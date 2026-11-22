import { useEffect, useState } from "react";
import { MAX_COLLECTION_NAME, recentPlaytime, type Game, type RecentPlaytime } from "../lib/api";
import { formatPlaytime, relativeTime } from "../lib/format";
import { t } from "../lib/i18n";
import { ACTIVITY_GROUP, groupOpen, MEMORIES_GROUP, MINE_GROUP, OPEN_GROUPS_DEFAULT, sideGroups, STEAM_GROUP, TOOLS_GROUP, type Section } from "../lib/sections";
import { ChevronRightIcon, PlusIcon, SidebarIcon, SlidersIcon, TvIcon } from "./Icons";
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
  /** Change quand l'historique est remplacé (import) : le temps récent est relu aussi. */
  historyKey: number;
  /** Nouvelle collection, nommée sur place sous « Mes collections ». */
  onCreateCollection: (name: string) => void;
  /** Paramètres affichés, et leur ouverture (dernier élément de la barre, Ctrl+,). */
  settingsActive: boolean;
  onSettings: () => void;
  /** Passe en mode canapé (plein écran, pour la TV). */
  onCouch: () => void;
}

/** « 12,5 h », « 40 min », « Aucune partie ». */
const recentText = (r: RecentPlaytime | null) => (!r ? "…" : r.secs < 60 ? t.library.sidebar.noSession : formatPlaytime(r.secs / 60));

/**
 * Nom affiché d'une catégorie : le nom de `sections.tsx` sert d'identifiant (retenu dans
 * `ludex.sideGroups`), traduit ici ; une catégorie inconnue garde son nom.
 */
function groupLabel(group: string): string {
  const names = t.library.sidebar.groups;
  const known: Record<string, string> = {
    [ACTIVITY_GROUP]: names.activity,
    [MEMORIES_GROUP]: names.memories,
    [TOOLS_GROUP]: names.tools,
    [MINE_GROUP]: names.mine,
    [STEAM_GROUP]: names.steam,
  };
  return Object.prototype.hasOwnProperty.call(known, group) ? known[group] : group;
}

const GROUPS_KEY = "ludex.sideGroups";

/** Catégories dépliées, retenues d'une ouverture à l'autre (localStorage, absent ou illisible = défaut). */
function readOpenGroups(): Set<string> {
  try {
    const raw = localStorage.getItem(GROUPS_KEY);
    const list: unknown = raw ? JSON.parse(raw) : null;
    if (Array.isArray(list)) return new Set(list.filter((g): g is string => typeof g === "string"));
  } catch {
    // Stockage indisponible : catégories par défaut.
  }
  return new Set(OPEN_GROUPS_DEFAULT);
}

function saveOpenGroups(groups: ReadonlySet<string>) {
  try {
    localStorage.setItem(GROUPS_KEY, JSON.stringify([...groups]));
  } catch {
    // Rien de grave : la barre reviendra au défaut.
  }
}

/** Barre de navigation à gauche : sections, catégories repliables, collections, puis petites stats. */
export function Sidebar({ sections, active, onSelect, collapsed, onToggle, lastGame, onOpenGame, playing, historyKey, onCreateCollection, settingsActive, onSettings, onCouch }: Props) {
  const [recent, setRecent] = useState<RecentPlaytime | null>(null);
  const [creating, setCreating] = useState(false);
  const [opened, setOpened] = useState(readOpenGroups);
  useEffect(() => {
    let alive = true;
    recentPlaytime()
      .then((r) => alive && setRecent(r))
      .catch(() => alive && setRecent(null));
    return () => {
      alive = false;
    };
  }, [playing, historyKey]);

  const toggleGroup = (group: string) => {
    setOpened((prev) => {
      const next = new Set(prev);
      if (next.has(group)) next.delete(group);
      else next.add(group);
      saveOpenGroups(next);
      return next;
    });
  };

  const days = recent?.days ?? 14;
  const side = t.library.sidebar;
  const recentTitle = recent ? side.recentTitle(days, recentText(recent), recent.sessions) : undefined;

  const item = (s: Section) => {
    const i = sections.indexOf(s);
    return (
      <li key={s.id}>
        <button
          type="button"
          className="side__item"
          data-section={s.id}
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
  const fixed = sections.filter((s) => !s.group);

  return (
    <nav className="side" aria-label={side.nav}>
      {/* Haut de la barre : fait partie de la barre de titre (déplace la fenêtre). Pas toute la
          barre : un clic sur sa barre de défilement déplacerait la fenêtre au lieu de défiler. */}
      <div className="side__top" data-tauri-drag-region>
        <button
          type="button"
          className="side__toggle"
          onClick={onToggle}
          aria-expanded={!collapsed}
          aria-label={collapsed ? side.expand : side.collapse}
          title={collapsed ? side.expandHint : side.collapseHint}
        >
          <SidebarIcon />
        </button>
      </div>
      <ul className="side__list">{fixed.map(item)}</ul>
      {sideGroups(sections).map((group) => {
        // Barre repliée : toutes les icônes, séparées par un trait (pas de titre à cliquer).
        const open = collapsed || groupOpen(group, opened, sections, active);
        const members = sections.filter((s) => s.group === group);
        return (
          <div key={group} className="side__group" role="group" aria-label={groupLabel(group)}>
            {!collapsed && (
              <div className="side__head">
                {/* Titre de la catégorie : un `.side__item` de plus pour haut et bas, Entrée l'ouvre ou la ferme. */}
                <button
                  type="button"
                  className="side__item side__item--group"
                  data-group={group}
                  aria-expanded={open}
                  onClick={() => toggleGroup(group)}
                >
                  <ChevronRightIcon />
                  <span>{groupLabel(group)}</span>
                  {!open && members.length > 0 && <em>{members.length}</em>}
                </button>
                {group === MINE_GROUP && (
                  <button
                    type="button"
                    className="side__add"
                    aria-label={side.newCollection}
                    title={side.newCollection}
                    // Sans ceci, le clic ferme d'abord le champ (perte du focus) puis le rouvre.
                    onMouseDown={(e) => creating && e.preventDefault()}
                    onClick={() => {
                      if (!opened.has(group)) toggleGroup(group);
                      setCreating((c) => !c);
                    }}
                  >
                    <PlusIcon />
                  </button>
                )}
              </div>
            )}
            {open && (
              <ul className="side__list">
                {members.map(item)}
                {group === MINE_GROUP && creating && !collapsed && (
                  <li>
                    <InlineInput
                      className="side__new"
                      maxLength={MAX_COLLECTION_NAME}
                      label={side.newCollectionLabel}
                      placeholder={side.newCollectionPlaceholder}
                      onSubmit={onCreateCollection}
                      onDone={() => setCreating(false)}
                    />
                  </li>
                )}
              </ul>
            )}
          </div>
        );
      })}
      <div className="side__foot">
        <button type="button" className="side__couch" onClick={onCouch} aria-label={side.couch} title={side.couchHint}>
          <TvIcon />
          <span>{side.couch}</span>
        </button>
        {/* Petites stats : temps des derniers jours, sessions et jeux, puis le dernier jeu lancé en une ligne. */}
        <div className="side__stats" title={recentTitle}>
          <small>{side.lastDays(days)}</small>
          <b>{recentText(recent)}</b>
          {recent && recent.sessions > 0 && (
            <span>
              {side.sessions(recent.sessions)} · {t.library.gameCount(recent.games)}
            </span>
          )}
        </div>
        {lastGame && (
          <button
            type="button"
            className="side__last"
            onClick={() => onOpenGame(lastGame)}
            aria-label={side.lastLaunchedLabel(lastGame.title)}
            title={`${lastGame.title}, ${relativeTime(lastGame.lastPlayed)}`}
          >
            <small>{side.lastLaunched}</small>
            <b>{lastGame.title}</b>
          </button>
        )}
        {/* Dernier `.side__item` de la barre : joignable par haut et bas, et par LB/RB. */}
        <button
          type="button"
          className="side__item"
          data-section="settings"
          aria-current={settingsActive ? "page" : undefined}
          aria-label={side.settings}
          title={side.settingsHint}
          onClick={onSettings}
        >
          <SlidersIcon />
          <span>{side.settings}</span>
        </button>
      </div>
    </nav>
  );
}
