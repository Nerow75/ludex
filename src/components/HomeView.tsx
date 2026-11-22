import { useEffect, useId, useMemo, useState, type ReactNode } from "react";
import { installDates, MOOD_LABEL, PLATFORM_LABEL, recap, STATUS_LABEL, type Game } from "../lib/api";
import { formatDuration, formatPlaytime, installBadge, relativeTime } from "../lib/format";
import { downloads, lastAchievement, moodIdeas, pickTonight, pileGames, tonightSettled, type Tonight } from "../lib/home";
import { t } from "../lib/i18n";
import { recentGames } from "../lib/library";
import type { SectionContext } from "../lib/sections";
import { Art, Logo } from "./Art";
import { Cover } from "./Cover";
import { lastSessionText, useLastSession } from "./GameStrip";
import { ChevronRightIcon, PlayIcon } from "./Icons";
import { ResumeRow } from "./ResumeRow";

/** Jeux par rangée d'ambiance et dans « Ta pile ». */
const PER_ROW = 10;
const PILE_ROW = 12;


/**
 * Section « Pour toi » : l'accueil. Bannière « Ce soir », quelques chiffres, la rangée Reprendre,
 * des idées par ambiance et la pile des jeux pas encore commencés. Tout est tiré des jeux déjà
 * chargés, sauf les dates d'installation et le temps de la semaine (commandes sans argument).
 * Les rangées se parcourent aux flèches : écran en deux dimensions (`data-nav-2d`).
 */
export function HomeView({ games, partial, playing, openGame, play, showMood }: SectionContext) {
  const home = t.library.home;
  const installed = useInstallDates(games);
  const pile = useMemo(() => pileGames(games, installed), [games, installed]);
  const tonight = useMemo(() => pickTonight(games, playing, pile), [games, playing, pile]);
  const tonightId = tonight?.game.id;
  const resume = useMemo(() => recentGames(games, 5).filter((g) => g.id !== tonightId).slice(0, 4), [games, tonightId]);
  const ideas = useMemo(() => moodIdeas(games, PER_ROW, new Set(tonightId ? [tonightId] : [])), [games, tonightId]);
  const pending = useMemo(() => downloads(games), [games]);
  const pileShown = pile.filter((g) => g.id !== tonightId).slice(0, PILE_ROW);

  return (
    <div className="home" data-nav-2d>
      {tonight && !tonightSettled(tonight, partial) && <section className="tonight" aria-hidden="true" />}
      {tonight && tonightSettled(tonight, partial) && <TonightBanner tonight={tonight} live={playing.has(tonight.game.id)} openGame={openGame} play={play} />}
      <Facts games={games} playing={playing} openGame={openGame} />
      <ResumeRow games={resume} playing={playing} onSelect={openGame} onPlay={play} nav />
      {pending.length > 0 && (
        <Shelf title={home.downloads} count={pending.length}>
          {pending.map((g) => (
            <ShelfItem key={g.id} game={g} meta={<span className="live is-pending">{installBadge(g.installState)}</span>} openGame={openGame} play={play} />
          ))}
        </Shelf>
      )}
      {ideas.map(({ mood, count, games: list }) => (
        <Shelf
          key={mood}
          title={MOOD_LABEL[mood]}
          count={count}
          onTitle={() => showMood(mood)}
          titleHint={home.moodHint(MOOD_LABEL[mood])}
        >
          {list.map((g) => (
            <ShelfItem key={g.id} game={g} meta={playing.has(g.id) ? <span className="live">{t.library.inGame}</span> : relativeTime(g.lastPlayed)} openGame={openGame} play={play} />
          ))}
        </Shelf>
      ))}
      {pileShown.length > 0 && (
        <Shelf title={home.pile} count={pile.length} note={home.pileNote}>
          {pileShown.map((g) => (
            <ShelfItem
              key={g.id}
              game={g}
              meta={g.status === "todo" ? STATUS_LABEL.todo : installed.has(g.id) ? home.installed(relativeTime(installed.get(g.id)!)) : PLATFORM_LABEL[g.platform]}
              openGame={openGame}
              play={play}
            />
          ))}
        </Shelf>
      )}
    </div>
  );
}

/** Dates d'installation, relues quand la liste des jeux change (scan, jeu ajouté ou retiré). */
function useInstallDates(games: Game[]): ReadonlyMap<string, number> {
  const [dates, setDates] = useState<ReadonlyMap<string, number>>(() => new Map());
  const ids = useMemo(() => games.map((g) => g.id).sort().join("\n"), [games]);
  useEffect(() => {
    let alive = true;
    installDates()
      .then((all) => alive && setDates(new Map(Object.entries(all))))
      .catch(() => {}); // sans dates, la pile est rangée par titre
    return () => {
      alive = false;
    };
  }, [ids]);
  return dates;
}

/** Bannière « Ce soir » : le jeu à reprendre, son temps de jeu, sa dernière session et Jouer. */
function TonightBanner({ tonight, live, openGame, play }: { tonight: Tonight; live: boolean } & Pick<SectionContext, "openGame" | "play">) {
  const { game, reason } = tonight;
  const last = lastSessionText(game, useLastSession(game.id, live));
  const home = t.library.home;
  const facts = [
    game.playtimeMinutes !== null && game.playtimeMinutes > 0 ? home.played(formatPlaytime(game.playtimeMinutes)) : null,
    game.lastPlayed ? home.lastSession(last.text) : null,
    game.status && reason !== "playing" ? STATUS_LABEL[game.status] : null,
  ].filter(Boolean);
  const install = installBadge(game.installState);
  return (
    <section className="tonight" aria-labelledby="tonight-title" data-game={game.id}>
      <Art game={game} className="tonight__art" />
      <div className="tonight__body">
        <p className="tonight__eyebrow">
          <span id="tonight-title">{home.tonight}</span>
          {live ? <span className="live">{t.library.inGame}</span> : <span>{home.reason[reason]}</span>}
        </p>
        <Logo game={game} className="tonight__logo" />
        {facts.length > 0 && <p className="tonight__facts" title={last.title}>{facts.join(" · ")}</p>}
        <div className="tonight__acts">
          <button type="button" className="btn-play" data-nav onClick={() => play(game)} aria-label={t.library.playGame(game.title)}><PlayIcon /> {t.library.play}</button>
          <button type="button" className="btn-ghost" data-nav onClick={() => openGame(game)}>{home.details}</button>
          {install && <span className="live is-pending">{install}</span>}
        </div>
      </div>
    </section>
  );
}

/** Ligne d'infos : temps de la semaine (récap) et dernier succès (vitrine), sans plus. */
function Facts({ games, playing, openGame }: Pick<SectionContext, "games" | "playing" | "openGame">) {
  const [week, setWeek] = useState<{ secs: number; sessions: number } | null>(null);
  // Relu à l'ouverture et quand une partie commence ou se termine.
  useEffect(() => {
    let alive = true;
    recap("week", 0)
      .then((r) => alive && setWeek({ secs: r.totalSecs, sessions: r.sessions }))
      .catch(() => {}); // sans historique, la ligne n'a que le dernier succès
    return () => {
      alive = false;
    };
  }, [playing]);
  const achievement = useMemo(() => lastAchievement(games), [games]);
  if (!(week && week.secs > 0) && !achievement) return null;
  const home = t.library.home;
  return (
    <ul className="facts" aria-label={home.facts}>
      {week && week.secs > 0 && (
        <li>
          {home.thisWeek} <b>{formatDuration(week.secs)}</b>
          <span>{home.sessions(week.sessions)}</span>
        </li>
      )}
      {achievement && (
        <li>
          <button type="button" data-nav onClick={() => openGame(achievement.game)} title={home.openDetails(achievement.game.title)}>
            {home.lastAchievement} <b>{achievement.game.title}</b>
            <span>{relativeTime(achievement.at)}</span>
          </button>
        </li>
      )}
    </ul>
  );
}

interface ShelfProps {
  title: string;
  count: number;
  note?: string;
  /** Titre cliquable (ambiance : ses jeux dans « Tous les jeux »). */
  onTitle?: () => void;
  titleHint?: string;
  children: ReactNode;
}

/** Rangée horizontale de jaquettes, avec son titre. */
function Shelf({ title, count, note, onTitle, titleHint, children }: ShelfProps) {
  const head = (
    <>
      {title}<em><span className="sr-only">, </span>{count}</em>
      {onTitle && <ChevronRightIcon />}
    </>
  );
  return (
    <section className="shelf" aria-label={title}>
      <div className="shelf__bar">
        {onTitle ? (
          // Titre de rangée cliquable : un bouton dans un titre, pour rester dans le plan des titres.
          <h3 className="shelf__heading">
            <button type="button" className="shelf__title is-link" data-nav onClick={onTitle} title={titleHint}>{head}</button>
          </h3>
        ) : (
          <h3 className="shelf__title">{head}</h3>
        )}
        {note && <span className="shelf__note">{note}</span>}
      </div>
      <ul className="shelf__row">{children}</ul>
    </section>
  );
}

/** Jaquette d'une rangée : clic = fiche, double clic = jouer. */
function ShelfItem({ game, meta, openGame, play }: { game: Game; meta: ReactNode } & Pick<SectionContext, "openGame" | "play">) {
  // Nom = titre, description = la ligne sous la jaquette (dernière partie, état d'installation…).
  const metaId = useId();
  return (
    <li className="shelf__item" data-game={game.id}>
      <button type="button" className="shelf__hit" data-nav onClick={() => openGame(game)} onDoubleClick={() => play(game)} aria-label={game.title} aria-describedby={metaId}>
        <Cover game={game} progress />
        <span className="shelf__name">{game.title}</span>
        <span className="shelf__meta" id={metaId}>{meta}</span>
      </button>
    </li>
  );
}
