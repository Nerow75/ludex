import { Fragment, memo, type ReactNode } from "react";
import { PLATFORM_LABEL, type Game } from "../lib/api";
import { formatAchievements, formatDate, formatPlaytime, formatSize, installBadge, relativeTime, releaseYear } from "../lib/format";
import { achievementPercent, SORT_LABEL, storyMinutes, type Sort } from "../lib/library";
import { Cover } from "./Cover";
import { PlayIcon, StarIcon } from "./Icons";

/** Colonnes triables : en-tête cliquable, tri de la barre (même sens que dans la grille). */
const COLUMNS: { sort: Sort | null; label: string; className: string }[] = [
  { sort: "title", label: "Titre", className: "lc--title" },
  { sort: null, label: "Launcher", className: "lc--plat" },
  { sort: null, label: "Genres", className: "lc--genres" },
  { sort: "year", label: "Sortie", className: "lc--year" },
  { sort: "playtime", label: "Temps de jeu", className: "lc--num" },
  { sort: "recent", label: "Dernière partie", className: "lc--when" },
  { sort: "achievements", label: "Succès", className: "lc--num" },
  { sort: "lifespan", label: "Histoire", className: "lc--num" },
  { sort: "size", label: "Taille", className: "lc--num" },
];

/** Tris du plus petit au plus grand ; les autres vont du plus grand au plus petit. */
const ASCENDING: ReadonlySet<Sort> = new Set(["title", "lifespan"]);

/** Position de la flèche de la fiche : sous la petite jaquette. */
const NOTCH = 28;

/** Donnée absente : un point discret plutôt qu'un texte. */
const Missing = () => <span className="lrow__none" aria-label="Inconnu">·</span>;

interface Props {
  games: Game[];
  selectedId: string | null;
  open: boolean;
  sort: Sort;
  playing: ReadonlySet<string>;
  onSort: (sort: Sort) => void;
  onSelect: (game: Game) => void;
  onPlay: (game: Game) => void;
  /** Fiche à insérer sous la ligne du jeu sélectionné ; reçoit la position de la flèche. */
  renderStrip: (game: Game, notch: number) => ReactNode;
}

/** Bibliothèque en liste dense : une ligne par jeu, en-têtes qui trient, fiche dépliée sous la ligne. */
export function LibraryList({ games, selectedId, open, sort, playing, onSort, onSelect, onPlay, renderStrip }: Props) {
  return (
    <div className="lib-list">
      <div className="lhead" role="group" aria-label="Trier la liste">
        <span aria-hidden />
        {COLUMNS.map((c) =>
          c.sort ? (
            <button
              key={c.label}
              type="button"
              className={`lhead__sort ${c.className}`}
              aria-pressed={sort === c.sort}
              title={`Trier : ${SORT_LABEL[c.sort]}`}
              onClick={() => onSort(c.sort!)}
            >
              {c.label}
              {sort === c.sort && <span className="lhead__dir" aria-hidden>{ASCENDING.has(c.sort) ? "↑" : "↓"}</span>}
            </button>
          ) : (
            <span key={c.label} className={c.className}>{c.label}</span>
          ),
        )}
      </div>
      {games.map((g) => (
        <Fragment key={g.id}>
          <ListRow game={g} selected={g.id === selectedId} playing={playing.has(g.id)} onSelect={onSelect} onPlay={onPlay} />
          {open && g.id === selectedId && renderStrip(g, NOTCH)}
        </Fragment>
      ))}
    </div>
  );
}

interface RowProps {
  game: Game;
  selected: boolean;
  playing: boolean;
  onSelect: (game: Game) => void;
  onPlay: (game: Game) => void;
}

const ListRow = memo(function ListRow({ game, selected, playing, onSelect, onPlay }: RowProps) {
  const badge = installBadge(game.installState);
  const year = releaseYear(game.details?.releaseDate ?? null);
  const ach = game.achievements && game.achievements.total > 0 ? game.achievements : null;
  const percent = achievementPercent(game);
  const story = storyMinutes(game);
  return (
    <div className={`lrow ${selected ? "is-sel" : ""}`} data-game={game.id}>
      <button
        type="button"
        className="lrow__hit"
        data-nav
        aria-pressed={selected}
        aria-label={playing ? `${game.title}, en jeu` : badge ? `${game.title}, ${badge.toLowerCase()}` : game.title}
        onClick={() => onSelect(game)}
        onDoubleClick={() => onPlay(game)}
      >
        <Cover game={game} className="lrow__cover" />
        <span className="lc--title lrow__title">
          <span className="lrow__name">{game.title}</span>
          {game.favorite && <span className="lrow__fav" aria-label="Favori"><StarIcon filled /></span>}
          {playing ? <span className="live">En jeu</span> : badge && <span className="live is-pending">{badge}</span>}
        </span>
        <span className="lc--plat">{PLATFORM_LABEL[game.platform]}</span>
        <span className="lc--genres" title={game.details?.genres.join(", ")}>
          {game.details?.genres.length ? game.details.genres.join(", ") : <Missing />}
        </span>
        <span className="lc--year">{year ?? <Missing />}</span>
        <span className="lc--num" title={game.playtimeSource === "ludex" ? "Mesuré par Ludex" : undefined}>
          {game.playtimeMinutes === null ? <Missing /> : formatPlaytime(game.playtimeMinutes)}
        </span>
        <span className="lc--when" title={game.lastPlayed ? formatDate(game.lastPlayed) : undefined}>
          {game.lastPlayed ? relativeTime(game.lastPlayed) : <Missing />}
        </span>
        <span className={`lc--num ${percent === 100 ? "is-perfect" : ""}`} title={percent === null ? undefined : `${percent} % des succès`}>
          {ach ? formatAchievements(ach.unlocked, ach.total) : <Missing />}
        </span>
        <span className="lc--num" title={game.hltb ? `HowLongToBeat : ${game.hltb.name}` : undefined}>
          {story === null ? <Missing /> : formatPlaytime(story)}
        </span>
        <span className="lc--num">{game.sizeOnDisk ? formatSize(game.sizeOnDisk) : <Missing />}</span>
      </button>
      <button type="button" className="card__play" tabIndex={-1} aria-label={`Jouer à ${game.title}`} onClick={() => onPlay(game)}>
        <PlayIcon />
      </button>
    </div>
  );
});
