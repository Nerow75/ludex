// Mode canapé : la bibliothèque lisible à 3 mètres, pour la TV. Mêmes données, autre mise en page :
// grande image de fond du jeu choisi, rangées de grandes jaquettes, fiche simplifiée en grand.
// Manette d'abord (relayée par App), clavier aussi ; la souris n'est jamais obligatoire.
import { useEffect, useMemo, useRef, useState, type MutableRefObject } from "react";
import { PLATFORM_LABEL, recap, type Game } from "../lib/api";
import { ambientFromImage, titleColor } from "../lib/color";
import { couchKey, couchKeyAction, couchRows, initialFocus, stepRow, todaySecs, type CouchAction } from "../lib/couch";
import { formatAchievements, formatClock, formatDuration, formatPlaytime, installBadge, relativeTime, releaseYear } from "../lib/format";
import { spatialNext } from "../lib/spatial";
import { Art, Logo } from "./Art";
import { Cover } from "./Cover";
import { lastSessionText, storyLeft, useLastSession } from "./GameStrip";
import { CloseIcon, LudexMark, PlayIcon, StarIcon } from "./Icons";

interface Props {
  games: Game[];
  playing: ReadonlySet<string>;
  /** Indices de la manette plutôt que du clavier. */
  pad: boolean;
  onPlay: (game: Game) => void;
  onFavorite: (game: Game) => void;
  onExit: () => void;
  /** F11 : vrai plein écran si le réglage le permet (sinon App quitte le mode). */
  onFullscreen: () => void;
  /** Actions de la manette, relayées par App (une seule boucle de lecture des manettes). */
  control: MutableRefObject<((a: CouchAction) => void) | null>;
}

/** Image de fond changée seulement quand la sélection se pose : défiler vite ne fait pas clignoter. */
const BACKDROP_DELAY = 140;

const reducedMotion = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;
const scrollBehavior = (): ScrollBehavior => (reducedMotion() ? "auto" : "smooth");

export default function CouchMode({ games, playing, pad, onPlay, onFavorite, onExit, onFullscreen, control }: Props) {
  const rows = useMemo(() => couchRows(games, playing), [games, playing]);
  const byId = useMemo(() => new Map(games.map((g) => [g.id, g])), [games]);
  const [focusKey, setFocusKey] = useState<string | null>(() => initialFocus(rows, games, playing));
  const [ficheId, setFicheId] = useState<string | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const shelves = useRef<HTMLDivElement>(null);

  // Jaquette choisie : celle retenue si elle existe encore (rescan, jeu masqué), sinon le même jeu
  // dans une autre rangée, sinon la jaquette de départ.
  const current = useMemo(() => {
    const keys = rows.flatMap((r) => r.games.map((g) => ({ key: couchKey(r.id, g.id), row: r.id, game: g })));
    const id = focusKey?.slice(focusKey.indexOf("/") + 1);
    return (
      keys.find((k) => k.key === focusKey) ??
      keys.find((k) => k.game.id === id) ??
      keys.find((k) => k.key === initialFocus(rows, games, playing)) ??
      null
    );
  }, [rows, focusKey, games, playing]);
  const game = current?.game ?? null;
  const fiche = ficheId ? byId.get(ficheId) ?? null : null;

  // Image de fond et couleur du contour : posées un instant après la sélection.
  // Couches numérotées : revenir au jeu d'avant refait le fondu au lieu de déplacer l'ancienne couche.
  const [backdrop, setBackdrop] = useState<{ game: Game; n: number }[]>(() => (game ? [{ game, n: 0 }] : []));
  const [ambient, setAmbient] = useState<string | null>(null);
  const shown = fiche ?? game;
  useEffect(() => {
    if (!shown) return;
    let alive = true;
    const t = window.setTimeout(() => {
      // Deux couches : la nouvelle image arrive en fondu par-dessus la précédente.
      // Même jeu (image trouvée entre-temps) : la couche est mise à jour sans nouveau fondu.
      setBackdrop((list) => {
        const top = list[list.length - 1];
        if (top?.game.id === shown.id) return [...list.slice(0, -1), { game: shown, n: top.n }];
        return [...list.slice(-1), { game: shown, n: (top?.n ?? 0) + 1 }];
      });
      setAmbient(titleColor(shown.title));
      const image = shown.coverUrl ?? shown.iconUrl;
      if (image) ambientFromImage(image).then((c) => alive && c && setAmbient(c));
    }, BACKDROP_DELAY);
    return () => {
      alive = false;
      window.clearTimeout(t);
    };
  }, [shown?.id, shown?.heroUrl, shown?.coverUrl]);

  // Focus réel sur la jaquette choisie, rangée remontée en haut de la zone, jaquette gardée visible.
  useEffect(() => {
    if (fiche || !current) return;
    const el = root.current?.querySelector<HTMLElement>(`[data-key="${CSS.escape(couchKey(current.row, current.game.id))}"]`);
    if (!el) return;
    el.focus({ preventScroll: true });
    const behavior = scrollBehavior();
    const item = el.closest("li");
    const list = item?.parentElement;
    if (item && list) {
      const gutter = parseFloat(getComputedStyle(list).paddingLeft) || 0;
      const left = item.offsetLeft - gutter;
      const right = item.offsetLeft + item.offsetWidth + gutter - list.clientWidth;
      const x = left < list.scrollLeft ? left : right > list.scrollLeft ? right : list.scrollLeft;
      if (x !== list.scrollLeft) list.scrollTo({ left: x, behavior });
    }
    const row = el.closest<HTMLElement>(".couch__row");
    const area = shelves.current;
    if (row && area) {
      const top = row.offsetTop - (parseFloat(getComputedStyle(area).paddingTop) || 0);
      if (Math.abs(area.scrollTop - top) > 1) area.scrollTo({ top, behavior });
    }
  }, [current?.row, current?.game.id, fiche, rows]);

  // Fiche : Jouer a le focus à l'ouverture ; fermée, le focus revient à la jaquette (effet ci-dessus).
  useEffect(() => {
    if (fiche) root.current?.querySelector<HTMLElement>(".couch__fiche [data-couch-btn]")?.focus({ preventScroll: true });
  }, [fiche?.id]);
  // Jeu de la fiche disparu (rescan) : retour aux rangées.
  useEffect(() => {
    if (ficheId && !byId.has(ficheId)) setFicheId(null);
  }, [ficheId, byId]);

  const cards = () => [...(root.current?.querySelectorAll<HTMLElement>(".couch__card") ?? [])];
  const focusEl = (el: HTMLElement | null | undefined) => el?.dataset.key && setFocusKey(el.dataset.key);

  const act = (a: CouchAction) => {
    if (fiche) {
      const buttons = [...(root.current?.querySelectorAll<HTMLElement>(".couch__fiche [data-couch-btn]") ?? [])];
      const i = buttons.indexOf(document.activeElement as HTMLElement);
      switch (a) {
        case "back":
        case "fiche":
          return setFicheId(null);
        case "confirm":
          return void (i >= 0 ? buttons[i].click() : onPlay(fiche));
        case "favorite":
          return onFavorite(fiche);
        case "left":
        case "right":
          return buttons[Math.min(buttons.length - 1, Math.max(0, (i < 0 ? 0 : i) + (a === "right" ? 1 : -1)))]?.focus();
        case "exit":
          return onExit();
        case "fullscreen":
          return onFullscreen();
        default:
          return;
      }
    }
    if (a === "fullscreen") return onFullscreen();
    if (!current) return a === "back" || a === "exit" ? onExit() : undefined;
    const from = root.current?.querySelector<HTMLElement>(`[data-key="${CSS.escape(couchKey(current.row, current.game.id))}"]`);
    const rowIndex = rows.findIndex((r) => r.id === current.row);
    switch (a) {
      case "up":
      case "down":
      case "left":
      case "right":
        return void (from && focusEl(spatialNext(cards(), from, a)));
      case "prevRow":
      case "nextRow": {
        const next = stepRow(rows.length, rowIndex, a === "nextRow" ? 1 : -1);
        if (next !== null) setFocusKey(couchKey(rows[next].id, rows[next].games[0].id));
        return;
      }
      case "first":
      case "last": {
        const list = rows[rowIndex].games;
        return setFocusKey(couchKey(current.row, (a === "first" ? list[0] : list[list.length - 1]).id));
      }
      case "confirm":
        return onPlay(current.game);
      case "fiche":
        return setFicheId(current.game.id);
      case "favorite":
        return onFavorite(current.game);
      case "back":
      case "exit":
        return onExit();
    }
  };
  const actRef = useRef(act);
  actRef.current = act;

  // Manette (par App) et clavier : toutes les touches s'arrêtent ici (phase de capture), aucun
  // raccourci de la bibliothèque derrière, taper ne remplit pas la recherche.
  useEffect(() => {
    control.current = (a) => actRef.current(a);
    const onKey = (e: KeyboardEvent) => {
      e.stopPropagation();
      const a = couchKeyAction(e);
      if (a) {
        e.preventDefault();
        actRef.current(a);
      } else if (e.key === "Tab" || e.key === "F5" || e.ctrlKey || e.metaKey) {
        e.preventDefault(); // le focus reste dans le mode, ni rechargement ni raccourci du navigateur
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => {
      control.current = null;
      window.removeEventListener("keydown", onKey, true);
    };
  }, [control]);

  return (
    <div
      ref={root}
      className="couch"
      role="dialog"
      aria-modal="true"
      aria-label="Mode canapé"
      style={ambient ? ({ ["--lx-ambient" as string]: ambient } as React.CSSProperties) : undefined}
    >
      <div className="couch__backdrop" aria-hidden>
        {backdrop.map(({ game: g, n }) => <Art key={n} game={g} className="couch__art" />)}
      </div>

      <header className="couch__top">
        <span className="couch__brand"><LudexMark size={28} />Ludex</span>
        <TopInfo playing={playing} />
        <button type="button" className="couch__quit" tabIndex={-1} onClick={onExit} aria-label="Quitter le mode canapé" title="Quitter le mode canapé (Échap)">
          <CloseIcon />
        </button>
      </header>

      {fiche ? (
        <CouchFiche game={fiche} live={playing.has(fiche.id)} onPlay={onPlay} onFavorite={onFavorite} />
      ) : (
        <>
          <section className="couch__info" aria-live="polite">{game && <GameInfo game={game} live={playing.has(game.id)} />}</section>
          <div className="couch__shelves" ref={shelves}>
            {rows.length === 0 && <p className="couch__empty">Aucun jeu à afficher. Quitte le mode canapé pour ajouter un jeu ou relancer un scan.</p>}
            {rows.map((row) => (
              <section key={row.id} className="couch__row" aria-label={row.title}>
                <h2>{row.title}<em>{row.games.length}</em></h2>
                <ul className="couch__list">
                  {row.games.map((g) => {
                    const key = couchKey(row.id, g.id);
                    const on = current?.row === row.id && current.game.id === g.id;
                    return (
                      <li key={g.id}>
                        <button
                          type="button"
                          className={`couch__card ${on ? "is-focus" : ""}`}
                          data-key={key}
                          data-game={g.id}
                          tabIndex={on ? 0 : -1}
                          aria-label={g.title}
                          // Souris facultative : un clic choisit, un second ouvre la fiche, double clic = jouer.
                          onClick={() => (on ? setFicheId(g.id) : setFocusKey(key))}
                          onDoubleClick={() => onPlay(g)}
                        >
                          <Cover game={g} progress />
                          {playing.has(g.id) && <span className="couch__live live">En jeu</span>}
                        </button>
                      </li>
                    );
                  })}
                </ul>
              </section>
            ))}
          </div>
        </>
      )}

      <footer className="couch__keys" aria-hidden>
        {pad ? (
          <>
            <span><kbd className="pad">A</kbd> {fiche ? "Valider" : "Jouer"}</span>
            <span><kbd className="pad">X</kbd> {fiche ? "Fermer la fiche" : "Fiche"}</span>
            <span><kbd className="pad">Y</kbd> Favori</span>
            {!fiche && <span><kbd>LB</kbd><kbd>RB</kbd> Rangée</span>}
            <span><kbd className="pad">B</kbd> {fiche ? "Retour" : "Quitter"}</span>
          </>
        ) : (
          <>
            <span><kbd>Entrée</kbd> {fiche ? "Valider" : "Jouer"}</span>
            <span><kbd>Espace</kbd> {fiche ? "Fermer la fiche" : "Fiche"}</span>
            <span><kbd>Ctrl</kbd><kbd>D</kbd> Favori</span>
            {!fiche && <span><kbd>Pg↑</kbd><kbd>Pg↓</kbd> Rangée</span>}
            <span><kbd>Échap</kbd> {fiche ? "Retour" : "Quitter"}</span>
          </>
        )}
      </footer>
    </div>
  );
}

/** Heure et temps de jeu du jour, discrets en haut à droite. */
function TopInfo({ playing }: { playing: ReadonlySet<string> }) {
  const now = useClock();
  const [today, setToday] = useState(0);
  // Relu à l'ouverture, quand une partie commence ou se termine, et au changement de jour.
  const day = now.toDateString();
  useEffect(() => {
    let alive = true;
    recap("week", 0)
      .then((r) => alive && setToday(todaySecs(r, new Date())))
      .catch(() => {}); // sans historique, l'heure seule
    return () => {
      alive = false;
    };
  }, [playing, day]);
  return (
    <p className="couch__now">
      {today >= 60 && <span title="Parties lancées depuis Ludex">Aujourd'hui <b>{formatDuration(today)}</b></span>}
      <time>{formatClock(now.getTime() / 1000)}</time>
    </p>
  );
}

/** Heure courante, mise à jour à chaque changement de minute. */
function useClock(): Date {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const t = window.setTimeout(() => setNow(new Date()), 60_000 - (now.getTime() % 60_000) + 50);
    return () => window.clearTimeout(t);
  }, [now]);
  return now;
}

/** Jeu choisi, au-dessus des rangées : logo, launcher, genres, temps de jeu, dernière partie. */
function GameInfo({ game, live }: { game: Game; live: boolean }) {
  const install = installBadge(game.installState);
  const year = releaseYear(game.details?.releaseDate ?? null);
  const meta = [PLATFORM_LABEL[game.platform], ...(game.details?.genres.slice(0, 2) ?? []), year].filter(Boolean).join(" · ");
  const facts = [
    game.playtimeMinutes !== null && game.playtimeMinutes > 0 ? `${formatPlaytime(game.playtimeMinutes)} de jeu` : null,
    game.lastPlayed ? `Joué ${relativeTime(game.lastPlayed)}` : "Jamais lancé",
    game.achievements ? `${formatAchievements(game.achievements.unlocked, game.achievements.total)} succès` : null,
  ].filter(Boolean);
  return (
    <div className="couch__game" key={game.id}>
      <Logo game={game} className="couch__logo" />
      <p className="couch__meta">
        {live && <span className="live">En jeu</span>}
        {install && <span className="live is-pending">{install}</span>}
        {game.favorite && <span className="couch__fav"><StarIcon filled /> Favori</span>}
        <span>{meta}</span>
      </p>
      <p className="couch__facts">{facts.join(" · ")}</p>
    </div>
  );
}

/** Fiche simplifiée en grand : Jouer, favori, temps de jeu, dernière session, succès, durée de vie. */
function CouchFiche({ game, live, onPlay, onFavorite }: { game: Game; live: boolean; onPlay: (g: Game) => void; onFavorite: (g: Game) => void }) {
  const last = lastSessionText(game, useLastSession(game.id, live));
  const install = installBadge(game.installState);
  const a = game.achievements;
  const h = game.hltb;
  const left = h ? storyLeft(h.main, game.playtimeMinutes) : null;
  const stats: [string, string, string | null][] = [
    ["Temps de jeu", formatPlaytime(game.playtimeMinutes), game.playtimeSource === "ludex" ? "Mesuré par Ludex" : null],
    ["Dernière session", game.lastPlayed ? last.text : "Jamais lancé", null],
  ];
  if (a) stats.push(["Succès", formatAchievements(a.unlocked, a.total), a.total ? `${Math.round((a.unlocked / a.total) * 100)} %` : null]);
  if (h && h.main !== null) stats.push(["Durée de vie", `${formatPlaytime(h.main)} pour l'histoire`, left]);
  return (
    <section className="couch__fiche" aria-label={`Fiche de ${game.title}`}>
      <p className="couch__meta">
        {live && <span className="live">En jeu</span>}
        <span>{PLATFORM_LABEL[game.platform]}{game.details?.developers[0] ? ` · ${game.details.developers[0]}` : ""}</span>
      </p>
      <Logo game={game} className="couch__logo couch__logo--big" />
      {game.details?.description && <p className="couch__desc">{game.details.description}</p>}
      <div className="couch__acts">
        <button type="button" className="btn-play" data-couch-btn onClick={() => onPlay(game)}><PlayIcon /> Jouer</button>
        <button type="button" className="btn-ghost" data-couch-btn aria-pressed={game.favorite} onClick={() => onFavorite(game)}>
          <StarIcon filled={game.favorite} /> {game.favorite ? "Dans les favoris" : "Ajouter aux favoris"}
        </button>
        {install && <span className="live is-pending">{install}</span>}
      </div>
      <dl className="couch__stats">
        {stats.map(([label, value, note]) => (
          <div key={label}>
            <dt>{label}</dt>
            <dd>{value}</dd>
            {note && <dd className="couch__note">{note}</dd>}
          </div>
        ))}
      </dl>
    </section>
  );
}
