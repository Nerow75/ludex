import { useEffect, useState } from "react";
import { gameHistory, gameScreenshots, MAX_TITLE, PLATFORM_LABEL, type Game, type PastSession, type Screenshot } from "../lib/api";
import {
  formatAchievements,
  formatDate,
  formatDuration,
  formatPlaytime,
  formatSize,
  installNote,
  relativeTime,
  releaseYear,
} from "../lib/format";
import { AchievementList } from "./AchievementList";
import { Art, Logo } from "./Art";
import { ScreenshotViewer, ShotTile } from "./ScreenshotViewer";
import { CloseIcon, EyeOffIcon, FileIcon, FolderIcon, PencilIcon, PlayIcon, StarIcon, TrashIcon } from "./Icons";
import { SavesPanel } from "./SavesPanel";

interface Props {
  game: Game;
  /** Position horizontale (px) du centre de la jaquette, pour la flèche. */
  notch: number;
  /** Une session lancée depuis Ludex est en cours. */
  playing: boolean;
  onPlay: (game: Game) => void;
  onFavorite: (game: Game) => void;
  onHide: (game: Game) => void;
  onOpenFolder: (game: Game) => void;
  /** Jeux « Autres » : choisir un autre exécutable. */
  onChangeExe: (game: Game) => void;
  /** Titre vide : retour au titre d'origine. */
  onRename: (game: Game, title: string) => void;
  /** Jeux ajoutés à la main seulement. */
  onRemove: (game: Game) => void;
  onClose: () => void;
}

/** Titre modifiable sur place : Entrée valide, Échap (ou un clic ailleurs) annule. */
function TitleEditor({ game, onDone, onRename }: { game: Game; onDone: () => void; onRename: Props["onRename"] }) {
  const [value, setValue] = useState(game.title);
  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    e.stopPropagation(); // pas de raccourci global (Ctrl+D, flèches) pendant la saisie
    if (e.key === "Enter") {
      e.preventDefault();
      if (value.trim() !== game.title) onRename(game, value);
      onDone();
    } else if (e.key === "Escape") {
      e.preventDefault();
      onDone();
    }
  };
  return (
    <input
      className="strip__rename"
      value={value}
      maxLength={MAX_TITLE}
      autoFocus
      onFocus={(e) => e.currentTarget.select()}
      onChange={(e) => setValue(e.target.value)}
      onKeyDown={onKeyDown}
      onBlur={onDone}
      aria-label={`Nouveau titre de ${game.title}`}
      title="Entrée pour valider, Échap pour annuler. Vide : titre d'origine"
      spellCheck={false}
    />
  );
}

/** D'où vient le temps de jeu affiché, en une phrase. */
function playtimeOrigin(game: Game): string {
  if (game.playtimeSource === "launcher") return `Compté par ${PLATFORM_LABEL[game.platform]}, toutes sessions confondues`;
  if (game.playtimeSource === "ludex") return "Mesuré par Ludex, sur les parties lancées depuis Ludex";
  return "Ludex mesurera les parties lancées depuis Ludex";
}

/** « Action, RPG · CD PROJEKT RED · 2020 » et le détail complet au survol. */
function detailsLine(game: Game): { line: string; title: string } | null {
  const d = game.details;
  if (!d) return null;
  const year = releaseYear(d.releaseDate);
  const line = [d.genres.join(", "), d.developers[0], year].filter(Boolean).join(" · ");
  const title = [
    d.developers.length > 0 && `Développement : ${d.developers.join(", ")}`,
    d.publishers.length > 0 && `Édition : ${d.publishers.join(", ")}`,
    d.releaseDate && `Sortie : ${formatDate(d.releaseDate)}`,
  ]
    .filter(Boolean)
    .join("\n");
  return line ? { line, title } : null;
}

/** Dernier succès débloqué, au survol de la stat « Succès ». */
function lastAchievement(lastUnlocked: number | null): string {
  return lastUnlocked ? `Dernier succès débloqué ${relativeTime(lastUnlocked)}` : "Aucun succès débloqué";
}

/** Dernière session mesurée par Ludex, relue à l'ouverture de la fiche et à la fin d'une partie. */
function useLastSession(id: string, playing: boolean): PastSession | null {
  const [last, setLast] = useState<PastSession | null>(null);
  useEffect(() => {
    let alive = true;
    gameHistory(id)
      .then((sessions) => alive && setLast(sessions[0] ?? null))
      .catch(() => alive && setLast(null)); // sans historique, la date seule reste affichée
    return () => {
      alive = false;
    };
  }, [id, playing]);
  return last?.gameId === id ? last : null;
}

/** « il y a 2 jours, 1 h 20 » : la durée n'est ajoutée que si la session mesurée est la dernière connue. */
export function lastSessionText(game: Game, last: PastSession | null): { text: string; title?: string } {
  const end = last ? last.start + last.secs : 0;
  const launcherDate = game.lastPlayed ?? 0;
  // Joué depuis hors de Ludex (date du launcher bien plus récente) : la durée mesurée serait trompeuse.
  // Marge de 10 min : un launcher peut dater la partie de sa fermeture, après le dernier relevé de Ludex.
  if (!last || end + 600 < launcherDate) return { text: relativeTime(game.lastPlayed) };
  const title = last.launcher
    ? `Session observée par Ludex. Le temps de jeu reste celui de ${PLATFORM_LABEL[game.platform]}.`
    : "Session mesurée par Ludex";
  return { text: `${relativeTime(Math.max(end, launcherDate))}, ${formatDuration(last.secs)}`, title };
}

/** Vignettes montrées dans la fiche ; la dernière annonce les suivantes (« +12 »). */
const STRIP_SHOTS = 6;

/** Captures du jeu (Steam, Xbox Game Bar), relues à l'ouverture de la fiche et à la fin d'une partie. */
function useScreenshots(id: string, playing: boolean): Screenshot[] {
  const [shots, setShots] = useState<{ id: string; list: Screenshot[] }>({ id, list: [] });
  useEffect(() => {
    let alive = true;
    gameScreenshots(id)
      .then((list) => alive && setShots({ id, list }))
      .catch(() => alive && setShots({ id, list: [] })); // sans captures lisibles, la section reste cachée
    return () => {
      alive = false;
    };
  }, [id, playing]);
  return shots.id === id ? shots.list : [];
}

/** Section « Captures » de la fiche : rien tant que le jeu n'en a pas. */
function StripScreenshots({ game, playing }: { game: Game; playing: boolean }) {
  const shots = useScreenshots(game.id, playing);
  const [open, setOpen] = useState<number | null>(null);
  if (shots.length === 0) return null;
  const shown = shots.slice(0, STRIP_SHOTS);
  const rest = shots.length - shown.length;
  return (
    <div className="strip__shots">
      <p className="strip__shots-head">Captures <span className="eyebrow">{shots.length}</span></p>
      <div className="shots-row shots-row--strip">
        {shown.map((s, i) => (
          <ShotTile
            key={s.key}
            shot={s}
            label={`Capture ${i + 1} de ${game.title}`}
            more={i === shown.length - 1 && rest > 0 ? rest : undefined}
            onOpen={() => setOpen(i)}
          />
        ))}
      </div>
      {open !== null && open < shots.length && (
        <ScreenshotViewer shots={shots} index={open} title={game.title} onIndex={setOpen} onClose={() => setOpen(null)} />
      )}
    </div>
  );
}

/** Temps restant pour l'histoire, d'après la moyenne HowLongToBeat et le temps de jeu connu. */
function storyLeft(main: number | null, played: number | null): string | null {
  if (main === null || played === null) return null;
  if (played >= main) return "Temps moyen de l'histoire atteint";
  return `Encore environ ${formatPlaytime(Math.max(main - played, 1))} pour l'histoire`;
}

/** Bloc « Durée de vie » : trois barres remplies selon le temps de jeu. Rien sans données. */
function Lifespan({ game }: { game: Game }) {
  const h = game.hltb;
  if (!h) return null;
  const played = game.playtimeMinutes;
  const rows: [string, number | null][] = [
    ["Histoire", h.main],
    ["+ Extras", h.plus],
    ["100 %", h.full],
  ];
  const left = storyLeft(h.main, played);
  return (
    <div className="lifespan" title={`Moyennes HowLongToBeat pour « ${h.name} »`}>
      <p className="lifespan__head">Durée de vie</p>
      {rows.map(([label, target]) => {
        const ratio = target && played ? Math.min(played / target, 1) : 0;
        return (
          <div className="lifespan__row" key={label}>
            <span className="lifespan__label">{label}</span>
            <span className="lifespan__track" aria-hidden="true">
              <span className="lifespan__fill" style={{ width: `${ratio * 100}%` }} />
            </span>
            <span className="lifespan__value">{target === null ? "inconnu" : formatPlaytime(target)}</span>
          </div>
        );
      })}
      {left && <p className="lifespan__left">{left}</p>}
    </div>
  );
}

/** Section « Sauvegardes » repliée : les dossiers ne sont lus qu'à l'ouverture. */
function SavesSection({ gameId, playing }: { gameId: string; playing: boolean }) {
  const [open, setOpen] = useState(false);
  return (
    <details className="strip__saves" onToggle={(e) => setOpen(e.currentTarget.open)}>
      <summary>Sauvegardes</summary>
      {open && <SavesPanel gameId={gameId} playing={playing} />}
    </details>
  );
}

/** Fiche du jeu, dépliée sous la rangée de sa jaquette. */
export function GameStrip({ game, notch, playing, onPlay, onFavorite, onHide, onOpenFolder, onChangeExe, onRename, onRemove, onClose }: Props) {
  const [renaming, setRenaming] = useState(false);
  const details = detailsLine(game);
  const description = game.details?.description;
  const lastSession = lastSessionText(game, useLastSession(game.id, playing));
  const install = installNote(game.installState, PLATFORM_LABEL[game.platform]);
  return (
    <section className="strip" aria-label={`Fiche de ${game.title}`} style={{ ["--notch" as string]: `${notch}px` }}>
      <div className="strip__art">
        <Art game={game} />
        <Logo game={game} />
      </div>
      <div className="strip__info">
        <div className="strip__eyebrow">
          <span className="eyebrow">{PLATFORM_LABEL[game.platform]}{game.isTool ? " · Application" : ""}</span>
          {playing && <span className="live">En jeu</span>}
        </div>
        {renaming ? <TitleEditor game={game} onRename={onRename} onDone={() => setRenaming(false)} /> : <h2>{game.title}</h2>}
        {details && <p className="strip__meta" title={details.title}>{details.line}</p>}
        {description && <p className="strip__desc" title={description}>{description}</p>}
        <dl className="stats">
          <div title={playtimeOrigin(game)}><dt>Temps de jeu</dt><dd>{formatPlaytime(game.playtimeMinutes)}</dd></div>
          {game.achievements && (
            <div title={lastAchievement(game.achievements.lastUnlocked)}>
              <dt>Succès</dt><dd>{formatAchievements(game.achievements.unlocked, game.achievements.total)}</dd>
            </div>
          )}
          <div title={lastSession.title}><dt>Dernière session</dt><dd>{lastSession.text}</dd></div>
          <div><dt>Taille</dt><dd>{formatSize(game.sizeOnDisk)}</dd></div>
          <div><dt>Launcher</dt><dd>{PLATFORM_LABEL[game.platform]}</dd></div>
        </dl>
        <Lifespan game={game} />
        <div className="strip__acts">
          <button type="button" className="btn-play" onClick={() => onPlay(game)}><PlayIcon /> Jouer</button>
          {install && <span className="live is-pending">{install}</span>}
          <button
            type="button"
            className="btn-icon"
            aria-pressed={game.favorite}
            aria-label={game.favorite ? "Retirer des favoris" : "Ajouter aux favoris"}
            title={game.favorite ? "Retirer des favoris (Ctrl+D)" : "Ajouter aux favoris (Ctrl+D)"}
            onClick={() => onFavorite(game)}
          >
            <StarIcon filled={game.favorite} />
          </button>
          {game.installDir && (
            <button type="button" className="btn-icon" aria-label="Ouvrir le dossier" title="Ouvrir le dossier" onClick={() => onOpenFolder(game)}>
              <FolderIcon />
            </button>
          )}
          {game.platform === "other" && (
            <button type="button" className="btn-icon" aria-label="Changer l'exécutable" title="Changer l'exécutable" onClick={() => onChangeExe(game)}>
              <FileIcon />
            </button>
          )}
          <button
            type="button"
            className="btn-icon"
            aria-pressed={renaming}
            aria-label="Renommer"
            title="Renommer"
            // Sans ceci, le clic ferme d'abord l'éditeur (perte du focus) puis le rouvre.
            onMouseDown={(e) => renaming && e.preventDefault()}
            onClick={() => setRenaming((r) => !r)}
          >
            <PencilIcon />
          </button>
          <button
            type="button"
            className="btn-icon"
            aria-label={game.hidden ? "Ne plus masquer" : "Masquer de la bibliothèque"}
            title={game.hidden ? "Ne plus masquer" : "Masquer de la bibliothèque"}
            onClick={() => onHide(game)}
          >
            <EyeOffIcon />
          </button>
          {game.manual && (
            <button type="button" className="btn-icon" aria-label="Retirer de la bibliothèque" title="Retirer de la bibliothèque" onClick={() => onRemove(game)}>
              <TrashIcon />
            </button>
          )}
        </div>
        <StripScreenshots key={game.id} game={game} playing={playing} />
        {game.installDir && <p className="path" title={game.installDir}>{game.installDir}</p>}
        <AchievementList key={game.id} game={game} />
        <SavesSection key={game.id} gameId={game.id} playing={playing} />
      </div>
      <button type="button" className="strip__close" aria-label="Fermer la fiche" title="Fermer (Échap)" onClick={onClose}>
        <CloseIcon />
      </button>
    </section>
  );
}
