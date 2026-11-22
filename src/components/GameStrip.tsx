import { useEffect, useState } from "react";
import { LauncherIcon } from "./LauncherIcon";
import {
  gameHistory,
  gameMods,
  gamePerformance,
  gameSaves,
  gameScreenshots,
  genreChoices,
  journal,
  MAX_ALIAS,
  MAX_ALIASES,
  MAX_COLLECTION_NAME,
  MAX_GENRES,
  MAX_LAUNCH_ARGS,
  MAX_NOTE,
  MAX_TITLE,
  MOOD_LABEL,
  PLATFORM_LABEL,
  STATUS_LABEL,
  STATUSES,
  type ArtKind,
  type Collection,
  type Game,
  type JournalDay,
  type Mood,
  type PastSession,
  type Screenshot,
  type SessionPerf,
  type Status,
  type WorkshopMods,
} from "../lib/api";
import {
  argsExample,
  formatAchievements,
  formatDate,
  formatDateTime,
  formatDay,
  formatDuration,
  formatPlaytime,
  formatSize,
  genreLabel,
  installNote,
  relativeDay,
  relativeTime,
  releaseYear,
  shortArgs,
} from "../lib/format";
import { downloadDetail } from "../lib/install";
import { live, t } from "../lib/i18n";
import { remember, remembered } from "../lib/library";
import { spatialNext, type Direction } from "../lib/spatial";
import { AchievementList } from "./AchievementList";
import { Art, Logo } from "./Art";
import { gameSummary } from "./JournalView";
import { ScreenshotViewer, ShotTile } from "./ScreenshotViewer";
import { MovePanel } from "./MovePanel";
import { CloseIcon, EyeOffIcon, FileIcon, FolderIcon, ImageIcon, JournalIcon, MoveIcon, NoteIcon, PencilIcon, PlayIcon, PlusIcon, SearchIcon, ShortcutIcon, SlidersIcon, StarIcon, TrashIcon, UndoIcon, WideImageIcon } from "./Icons";
import { InlineInput } from "./InlineInput";
import { NewsTab, useNews } from "./NewsPanel";
import { newNewsCount } from "../lib/news";
import { ModsPanel } from "./ModsPanel";
import { PerformancePanel } from "./PerformancePanel";
import { shownSessions } from "../lib/performance";
import { SavesPanel } from "./SavesPanel";
import { RequirementsPanel } from "./RequirementsPanel";
import { markFocus, restoreFocus } from "./focus";

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
  /** Raccourci sur le Bureau qui lance le jeu par `ludex://launch/<id>`. */
  onShortcut: (game: Game) => void;
  /** Jeux « Autres » : choisir un autre exécutable. */
  onChangeExe: (game: Game) => void;
  /** Titre vide : retour au titre d'origine. */
  onRename: (game: Game, title: string) => void;
  /** Choisir une jaquette ou une image de fond perso (sélecteur ouvert côté Rust). */
  onChooseArt: (game: Game, kind: ArtKind) => void;
  /** Retirer les images perso du jeu : retour aux images d'origine. */
  onResetArt: (game: Game) => void;
  /** Jeux ajoutés à la main seulement. */
  onRemove: (game: Game) => void;
  /** Montre les jeux d'un tag, ou d'une ambiance, dans « Tous les jeux ». */
  onTag: (tag: string) => void;
  onMood: (mood: Mood) => void;
  /** Journal de ce jeu (section « Journal » restreinte). */
  onJournal?: (game: Game) => void;
  onClose: () => void;
  /** Collections de l'utilisateur, pour y ajouter le jeu ou l'en retirer. */
  collections: Collection[];
  /** `null` : aucun statut. */
  onStatus: (game: Game, status: Status | null) => void;
  /** Note vide : note effacée. */
  onNote: (game: Game, note: string) => void;
  /** Alias de recherche séparés par des virgules (vide : effacés). Rejette avec le message de Rust. */
  onAliases: (game: Game, text: string) => Promise<void>;
  /** Genres choisis à la place de ceux de Steam (vide : genres d'origine). */
  onGenres: (game: Game, genres: string[]) => void;
  /** Options de lancement (texte vide : effacées). Rejette avec le message de Rust si refusées. */
  onLaunchArgs: (game: Game, text: string) => Promise<void>;
  onToggleCollection: (game: Game, collection: Collection, value: boolean) => void;
  /** Nouvelle collection qui reçoit tout de suite ce jeu. */
  onCreateCollection: (name: string, game: Game) => void;
  /** Champ à ouvrir tout de suite (« Renommer », « Nouvelle collection… » du menu contextuel). */
  edit?: FicheEdit | null;
  /** Le champ demandé par `edit` est ouvert : la demande peut être oubliée. */
  onEditStarted?: () => void;
}

/** Champ de la fiche qu'une autre partie de l'app peut ouvrir. */
export type FicheEdit = "rename" | "collection";

/** Onglets de la fiche, dans l'ordre de la barre. */
const TABS = ["overview", "achievements", "shots", "saves", "sessions", "performance", "journal", "news", "mods"] as const;
type Tab = (typeof TABS)[number];
const TAB_LABEL: Record<Tab, string> = live(() => t.fiche.tabs);
/** Onglet retenu entre deux fiches (stockage local, comme les autres préférences d'affichage). */
const TAB_KEY = "ficheTab";

/** Titre modifiable sur place : Entrée valide, Échap (ou un clic ailleurs) annule. */
function TitleEditor({ game, onDone, onRename }: { game: Game; onDone: () => void; onRename: Props["onRename"] }) {
  const [value, setValue] = useState(game.title);
  // Fermé au clavier, le champ rend le focus au bouton Renommer (sinon il tombait sur la page).
  const [opener] = useState(markFocus);
  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    e.stopPropagation(); // pas de raccourci global (Ctrl+D, flèches) pendant la saisie
    if (e.key === "Enter") {
      e.preventDefault();
      if (value.trim() !== game.title) onRename(game, value);
      onDone();
      restoreFocus(opener);
    } else if (e.key === "Escape") {
      e.preventDefault();
      onDone();
      restoreFocus(opener);
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
      aria-label={t.fiche.rename.label(game.title)}
      title={t.fiche.rename.hint}
      spellCheck={false}
    />
  );
}

/** Options de lancement : champ sur place (Entrée valide, Échap annule), message de Rust si
 * refusées (le champ se rouvre avec le texte), champ désactivé avec l'explication si le launcher
 * ne transmet pas d'options. */
function LaunchArgs({ game, onLaunchArgs }: { game: Game; onLaunchArgs: Props["onLaunchArgs"] }) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    setEditing(false);
    setDraft(null);
    setError(null);
  }, [game.id]);
  const submit = (text: string) => {
    setError(null);
    onLaunchArgs(game, text).catch((e) => {
      setDraft(text);
      setError(String(e));
      setEditing(true);
    });
  };
  const refusal = game.launchArgsRefusal;
  return (
    <div className="fiche__block">
      <p className="fiche__label" role="heading" aria-level={3}>{t.fiche.args.title}</p>
      {refusal ? (
        <>
          <button type="button" className="strip__note strip__args is-empty" disabled aria-describedby={`args-why-${game.id}`}>
            <SlidersIcon />
            <span>{t.fiche.args.unavailable}</span>
          </button>
          <p className="strip__args-help" id={`args-why-${game.id}`}>{refusal}</p>
        </>
      ) : editing ? (
        <InlineInput
          key={draft ?? ""}
          className="strip__note-input strip__args-input"
          initial={draft ?? game.launchArgs ?? ""}
          maxLength={MAX_LAUNCH_ARGS}
          label={t.fiche.args.label(game.title)}
          placeholder={argsExample(game.platform)}
          onSubmit={submit}
          // Appelé juste après `onSubmit` : un refus arrive plus tard et rouvre le champ avec son message.
          onDone={() => {
            setEditing(false);
            setDraft(null);
            setError(null);
          }}
        />
      ) : (
        <button
          type="button"
          data-nav
          className={`strip__note strip__args ${game.launchArgs ? "" : "is-empty"}`}
          title={game.launchArgs ? `${game.launchArgs}\n${t.fiche.args.editHint}` : t.fiche.args.addTitle}
          onClick={() => {
            setError(null);
            setEditing(true);
          }}
        >
          <SlidersIcon />
          <span>{game.launchArgs ?? t.fiche.args.add}</span>
        </button>
      )}
      {error ? (
        <p className="strip__args-error" role="alert">{error}</p>
      ) : (
        !refusal &&
        editing && (
          <p className="strip__args-help">
            {t.fiche.args.help}
            {game.platform === "steam" && ` ${t.fiche.args.steamConfirm}`}
          </p>
        )
      )}
    </div>
  );
}

/** Alias de recherche : champ sur place comme les options (Entrée valide, Échap annule), message
 * de Rust sous le champ rouvert si l'enregistrement échoue. */
function Aliases({ game, onAliases }: { game: Game; onAliases: Props["onAliases"] }) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    setEditing(false);
    setDraft(null);
    setError(null);
  }, [game.id]);
  const submit = (text: string) => {
    setError(null);
    onAliases(game, text).catch((e) => {
      setDraft(text);
      setError(String(e));
      setEditing(true);
    });
  };
  const text = game.aliases.join(", ");
  return (
    <div className="fiche__block">
      <p className="fiche__label" role="heading" aria-level={3}>{t.fiche.aliases.title}</p>
      {editing ? (
        <InlineInput
          key={draft ?? ""}
          className="strip__note-input"
          initial={draft ?? text}
          // Huit alias, chacun avec sa virgule et son espace.
          maxLength={MAX_ALIASES * (MAX_ALIAS + 2)}
          label={t.fiche.aliases.label(game.title)}
          placeholder="cod, call of duty"
          onSubmit={submit}
          onDone={() => {
            setEditing(false);
            setDraft(null);
            setError(null);
          }}
        />
      ) : (
        <button
          type="button"
          data-nav
          className={`strip__note ${text ? "" : "is-empty"}`}
          title={text ? `${text}\n${t.fiche.aliases.editHint}` : t.fiche.aliases.addTitle}
          onClick={() => {
            setError(null);
            setEditing(true);
          }}
        >
          <SearchIcon />
          <span>{text || t.fiche.aliases.add}</span>
        </button>
      )}
      {error ? (
        <p className="strip__args-error" role="alert">{error}</p>
      ) : (
        editing && (
          <p className="strip__args-help">
            {t.fiche.aliases.help(MAX_ALIASES)}
          </p>
        )
      )}
    </div>
  );
}

// Liste des genres demandée une fois à Rust, à la première correction.
let genreList: Promise<string[]> | null = null;
const loadGenres = () => {
  genreList ??= genreChoices().catch((e) => {
    genreList = null;
    throw e;
  });
  return genreList;
};

/** Genres du jeu : ceux de la fiche Steam, ou ceux choisis par l'utilisateur. Pastilles à cocher
 * (liste de Ludex, trois au plus) et retour aux genres d'origine. */
function Genres({ game, onGenres }: { game: Game; onGenres: Props["onGenres"] }) {
  const [open, setOpen] = useState(false);
  const [choices, setChoices] = useState<string[] | null>(null);
  const [error, setError] = useState(false);
  useEffect(() => setOpen(false), [game.id]);
  useEffect(() => {
    if (!open || choices) return;
    let live = true;
    setError(false);
    loadGenres().then(
      (list) => live && setChoices(list),
      () => live && setError(true),
    );
    return () => {
      live = false;
    };
  }, [open, choices]);
  const current = game.details?.genres ?? [];
  const full = current.length >= MAX_GENRES;
  const toggle = (genre: string) =>
    onGenres(game, current.includes(genre) ? current.filter((g) => g !== genre) : [...current, genre]);
  return (
    <div className="fiche__block">
      <div className="fiche__row">
        <p className="fiche__label" role="heading" aria-level={3}>{t.fiche.genres.title}</p>
        <button type="button" className="fiche__link" data-nav aria-expanded={open} onClick={() => setOpen((v) => !v)}>
          {open ? t.fiche.close : t.fiche.genres.fix}
        </button>
      </div>
      {open ? (
        <>
          {error && <p className="strip__args-error" role="alert">{t.fiche.genres.error}</p>}
          {choices && (
            <div className="strip__cols" role="group" aria-label={t.fiche.genres.groupLabel(game.title, MAX_GENRES)}>
              {choices.map((g) => {
                const on = current.includes(g);
                return (
                  <button
                    key={g}
                    type="button"
                    className="chip"
                    data-nav
                    aria-pressed={on}
                    disabled={!on && full}
                    title={!on && full ? t.fiche.genres.full(MAX_GENRES) : undefined}
                    onClick={() => toggle(g)}
                  >
                    {genreLabel(g)}
                  </button>
                );
              })}
              {game.customGenres && (
                <button type="button" className="chip chip--add" data-nav onClick={() => onGenres(game, [])}>
                  <UndoIcon />{t.fiche.genres.reset}
                </button>
              )}
            </div>
          )}
          <p className="strip__args-help">
            {t.fiche.genres.max(MAX_GENRES)} {game.customGenres ? t.fiche.genres.customHelp : t.fiche.genres.steamHelp}
          </p>
        </>
      ) : (
        <p className="strip__member" title={game.customGenres ? t.fiche.genres.chosenByYou : undefined}>
          {current.length ? current.map(genreLabel).join(", ") : t.fiche.genres.none}
          {game.customGenres && ` ${t.fiche.genres.corrected}`}
        </p>
      )}
    </div>
  );
}

/** D'où vient le temps de jeu affiché, en une phrase. */
function playtimeOrigin(game: Game): string {
  if (game.playtimeSource === "launcher") return t.fiche.playtime.launcher(PLATFORM_LABEL[game.platform]);
  if (game.playtimeSource === "ludex") return t.fiche.playtime.ludex;
  return t.fiche.playtime.none;
}

/** « Action, RPG · CD PROJEKT RED · 2020 » et le détail complet au survol. */
function detailsLine(game: Game): { line: string; title: string } | null {
  const d = game.details;
  if (!d) return null;
  const year = releaseYear(d.releaseDate);
  const line = [d.genres.map(genreLabel).join(", "), d.developers[0], year].filter(Boolean).join(" · ");
  const title = [
    d.developers.length > 0 && t.fiche.details.developers(d.developers.join(", ")),
    d.publishers.length > 0 && t.fiche.details.publishers(d.publishers.join(", ")),
    d.releaseDate && t.fiche.details.release(formatDate(d.releaseDate)),
  ]
    .filter(Boolean)
    .join("\n");
  return line ? { line, title } : null;
}

/** Dernier succès débloqué, au survol de la stat « Succès ». */
function lastAchievement(lastUnlocked: number | null): string {
  return lastUnlocked ? t.fiche.stats.lastUnlocked(relativeTime(lastUnlocked)) : t.fiche.stats.noneUnlocked;
}

/** Dernière session mesurée par Ludex, relue à l'ouverture de la fiche et à la fin d'une partie. */
export function useLastSession(id: string, playing: boolean): PastSession | null {
  const history = useHistory(id, playing);
  return history?.[0] ?? null;
}

/** Historique du jeu (sessions terminées, la plus récente d'abord), `null` pendant la lecture. */
function useHistory(id: string, playing: boolean): PastSession[] | null {
  const [history, setHistory] = useState<{ id: string; list: PastSession[] } | null>(null);
  useEffect(() => {
    let alive = true;
    gameHistory(id)
      .then((list) => alive && setHistory({ id, list }))
      .catch(() => alive && setHistory({ id, list: [] })); // sans historique, la date seule reste affichée
    return () => {
      alive = false;
    };
  }, [id, playing]);
  return history?.id === id ? history.list.filter((s) => s.gameId === id) : null;
}

/** « il y a 2 jours, 1 h 20 » : la durée n'est ajoutée que si la session mesurée est la dernière connue. */
export function lastSessionText(game: Game, last: PastSession | null): { text: string; title?: string } {
  const end = last ? last.start + last.secs : 0;
  const launcherDate = game.lastPlayed ?? 0;
  // Joué depuis hors de Ludex (date du launcher bien plus récente) : la durée mesurée serait trompeuse.
  // Marge de 10 min : un launcher peut dater la partie de sa fermeture, après le dernier relevé de Ludex.
  if (!last || end + 600 < launcherDate) return { text: relativeTime(game.lastPlayed) };
  const title = last.launcher ? t.fiche.session.observed(PLATFORM_LABEL[game.platform]) : t.fiche.session.measured;
  return { text: `${relativeTime(Math.max(end, launcherDate))}, ${formatDuration(last.secs)}`, title };
}

/** Libellé du retour aux images d'origine, selon ce qui a été remplacé. */
function resetLabel(custom: ArtKind[]): string {
  if (custom.length > 1) return t.fiche.art.resetBoth;
  return custom[0] === "hero" ? t.fiche.art.resetHero : t.fiche.art.resetCover;
}

/** Captures du jeu (Steam, Xbox Game Bar), relues à l'ouverture de la fiche et à la fin d'une partie. `null` : lecture. */
function useScreenshots(id: string, playing: boolean): Screenshot[] | null {
  const [shots, setShots] = useState<{ id: string; list: Screenshot[] } | null>(null);
  useEffect(() => {
    let alive = true;
    gameScreenshots(id)
      .then((list) => alive && setShots({ id, list }))
      .catch(() => alive && setShots({ id, list: [] })); // sans captures lisibles, l'onglet reste caché
    return () => {
      alive = false;
    };
  }, [id, playing]);
  return shots?.id === id ? shots.list : null;
}

/**
 * Le jeu a-t-il quelque chose à montrer dans « Sauvegardes » (jeu connu de la base, ou copies) ?
 * `null` : lecture. Le panneau, lui, relit tout à l'ouverture de l'onglet.
 */
function useHasSaves(id: string, playing: boolean): boolean | null {
  const [has, setHas] = useState<{ id: string; value: boolean } | null>(null);
  useEffect(() => {
    let alive = true;
    gameSaves(id)
      .then((s) => alive && setHas({ id, value: s.name !== null || s.copies.length > 0 }))
      .catch(() => alive && setHas({ id, value: false }));
    return () => {
      alive = false;
    };
  }, [id, playing]);
  return has?.id === id ? has.value : null;
}

/**
 * Performances des dernières parties lancées depuis Ludex, lues à l'ouverture de la fiche (pour
 * savoir s'il faut montrer l'onglet) et à la fin d'une partie. `null` : lecture.
 */
function usePerformance(id: string, playing: boolean): SessionPerf[] | null {
  const [perf, setPerf] = useState<{ id: string; list: SessionPerf[] } | null>(null);
  useEffect(() => {
    let alive = true;
    gamePerformance(id)
      .then((list) => alive && setPerf({ id, list: shownSessions(list) }))
      .catch(() => alive && setPerf({ id, list: [] }));
    return () => {
      alive = false;
    };
  }, [id, playing]);
  return perf?.id === id ? perf.list : null;
}

/**
 * Mods Steam Workshop installés (jeux Steam seulement), lus à l'ouverture de la fiche et à la fin
 * d'une partie (le jeu peut en télécharger). `null` : lecture ; liste vide : rien à montrer.
 */
function useMods(game: Game, playing: boolean): WorkshopMods | null {
  const steam = game.platform === "steam";
  const [mods, setMods] = useState<{ id: string; list: WorkshopMods | null } | null>(null);
  useEffect(() => {
    if (!steam) return;
    let alive = true;
    gameMods(game.id)
      .then((list) => alive && setMods({ id: game.id, list }))
      .catch(() => alive && setMods({ id: game.id, list: null }));
    return () => {
      alive = false;
    };
  }, [game.id, steam, playing]);
  if (!steam) return EMPTY_MODS;
  return mods?.id === game.id ? (mods.list ?? EMPTY_MODS) : null;
}
const EMPTY_MODS: WorkshopMods = { mods: [], total: 0, folder: false, stale: false };

/** Vignettes montrées dans l'onglet ; la dernière annonce les suivantes (« +12 »). */
const TAB_SHOTS = 12;

/** Onglet « Captures » : vignettes et visionneuse. */
function ShotsTab({ game, shots }: { game: Game; shots: Screenshot[] | null }) {
  const [open, setOpen] = useState<number | null>(null);
  if (!shots) return <p className="fiche__note">{t.fiche.shots.loading}</p>;
  const shown = shots.slice(0, TAB_SHOTS);
  const rest = shots.length - shown.length;
  return (
    <>
      <div className="shots-row shots-row--fiche">
        {shown.map((s, i) => (
          <ShotTile
            key={s.key}
            shot={s}
            nav
            label={t.fiche.shots.label(i + 1, game.title)}
            more={i === shown.length - 1 && rest > 0 ? rest : undefined}
            onOpen={() => setOpen(i)}
          />
        ))}
      </div>
      {open !== null && open < shots.length && (
        <ScreenshotViewer shots={shots} index={open} title={game.title} onIndex={setOpen} onClose={() => setOpen(null)} />
      )}
    </>
  );
}

/** Sessions montrées dans l'onglet ; les plus anciennes restent dans le journal. */
const TAB_SESSIONS = 50;

/** Onglet « Sessions » : historique mesuré par Ludex, la plus récente d'abord. */
function SessionsTab({ game, history }: { game: Game; history: PastSession[] | null }) {
  if (!history) return <p className="fiche__note">{t.fiche.sessions.loading}</p>;
  const total = history.reduce((n, s) => n + s.secs, 0);
  return (
    <>
      <p className="fiche__note">{t.fiche.sessions.summary(history.length, formatDuration(total))}</p>
      <ol className="sessions" aria-label={t.fiche.sessions.label(game.title)}>
        {history.slice(0, TAB_SESSIONS).map((s) => (
          <li
            key={`${s.start}-${s.secs}`}
            className="sessions__row"
            title={s.launcher ? t.fiche.session.observed(PLATFORM_LABEL[game.platform]) : undefined}
          >
            <span>{s.merged ? t.fiche.sessions.merged(formatDate(s.start)) : formatDateTime(s.start)}</span>
            <span className="sessions__secs">{formatDuration(s.secs)}</span>
          </li>
        ))}
      </ol>
      {history.length > TAB_SESSIONS && <p className="fiche__note">{t.fiche.sessions.older(history.length - TAB_SESSIONS)}</p>}
    </>
  );
}

/** Jours montrés dans l'onglet « Journal » avant le lien vers le journal complet. */
const TAB_DAYS = 5;

/** Onglet « Journal » : derniers jours actifs du jeu, lus à l'ouverture, puis le journal complet. */
function JournalTab({ game, playing, onJournal }: { game: Game; playing: boolean; onJournal: (game: Game) => void }) {
  const [days, setDays] = useState<JournalDay[] | null | "error">(null);
  useEffect(() => {
    let alive = true;
    journal(null, game.id)
      .then((page) => alive && setDays(page.days.slice(0, TAB_DAYS)))
      .catch(() => alive && setDays("error"));
    return () => {
      alive = false;
    };
  }, [game.id, playing, game.achievements?.unlocked]);
  return (
    <>
      {days === null && <p className="fiche__note">{t.fiche.journal.loading}</p>}
      {days === "error" && <p className="fiche__note">{t.fiche.journal.error}</p>}
      {Array.isArray(days) && days.length === 0 && <p className="fiche__note">{t.fiche.journal.empty}</p>}
      {Array.isArray(days) && days.length > 0 && (
        <ol className="sessions" aria-label={t.fiche.journal.label(game.title)}>
          {days.map((d) => {
            const entry = d.games.find((g) => g.gameId === game.id);
            return (
              <li key={d.date} className="sessions__row">
                <span title={formatDay(d.date)}>{relativeDay(d.date)}</span>
                <span className="sessions__what">{entry ? gameSummary(entry) : ""}</span>
                <span className="sessions__secs">{d.secs > 0 ? formatDuration(d.secs) : ""}</span>
              </li>
            );
          })}
        </ol>
      )}
      <button type="button" className="btn-ghost fiche__more" data-nav onClick={() => onJournal(game)}>
        <JournalIcon /> {t.fiche.journal.all}
      </button>
    </>
  );
}

/** Temps restant pour l'histoire, d'après la moyenne HowLongToBeat et le temps de jeu connu. */
export function storyLeft(main: number | null, played: number | null): string | null {
  if (main === null || played === null) return null;
  if (played >= main) return t.fiche.lifespan.reached;
  return t.fiche.lifespan.left(formatPlaytime(Math.max(main - played, 1)));
}

/** Bloc « Durée de vie » : trois barres remplies selon le temps de jeu. Rien sans données. */
function Lifespan({ game }: { game: Game }) {
  const h = game.hltb;
  if (!h) return null;
  const played = game.playtimeMinutes;
  const rows: [string, number | null][] = [
    [t.fiche.lifespan.main, h.main],
    [t.fiche.lifespan.plus, h.plus],
    [t.fiche.lifespan.full, h.full],
  ];
  const left = storyLeft(h.main, played);
  return (
    <div className="lifespan" title={t.fiche.lifespan.source(h.name)}>
      <p className="lifespan__head" role="heading" aria-level={3}>{t.fiche.lifespan.title}</p>
      {rows.map(([label, target]) => {
        const ratio = target && played ? Math.min(played / target, 1) : 0;
        return (
          <div className="lifespan__row" key={label}>
            <span className="lifespan__label">{label}</span>
            <span className="lifespan__track" aria-hidden="true">
              <span className="lifespan__fill" style={{ width: `${ratio * 100}%` }} />
            </span>
            <span className="lifespan__value">{target === null ? t.fiche.lifespan.unknown : formatPlaytime(target)}</span>
          </div>
        );
      })}
      {left && <p className="lifespan__left">{left}</p>}
    </div>
  );
}

/** Chiffre « Durée de vie » de la ligne du haut : l'histoire, sinon la première durée connue. */
function lifespanStat(game: Game): string | null {
  const h = game.hltb;
  const minutes = h ? (h.main ?? h.plus ?? h.full) : null;
  return minutes === null ? null : formatPlaytime(minutes);
}

/**
 * Élément suivant de la fiche pour les flèches et la croix de la manette. Onglets : gauche et droite
 * passent à l'onglet voisin (qui s'affiche au focus). Ailleurs : l'élément voisin à l'écran, haut et
 * bas restant dans la même zone (haut de fiche, onglets, colonne de droite). `"card"` : retour à la
 * jaquette (haut depuis le haut de la fiche) ; `"out"` : sortie par le bas, vers la rangée suivante.
 */
export function ficheNext(fiche: HTMLElement, from: HTMLElement, dir: Direction): HTMLElement | "card" | "out" | null {
  const items = [...fiche.querySelectorAll<HTMLElement>("[data-nav]")];
  if (from.getAttribute("role") === "tab" && (dir === "left" || dir === "right")) {
    const tabs = items.filter((el) => el.getAttribute("role") === "tab");
    return tabs[tabs.indexOf(from) + (dir === "right" ? 1 : -1)] ?? null;
  }
  // Arrivée dans la barre d'onglets depuis ailleurs : l'onglet affiché, pas le plus proche (qui changerait d'onglet).
  const settle = (el: HTMLElement | null) =>
    el?.getAttribute("role") === "tab" && from.getAttribute("role") !== "tab"
      ? (el.parentElement?.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]') ?? el)
      : el;
  if (dir === "left" || dir === "right") return settle(spatialNext(items, from, dir));
  const zone = from.closest("[data-nav-zone]");
  const inZone = spatialNext(items.filter((el) => el.closest("[data-nav-zone]") === zone), from, dir);
  if (inZone) return settle(inZone);
  // Bas d'une colonne (onglets ou colonne de droite) : rangée suivante, sans sauter dans l'autre colonne.
  if (dir === "down" && zone?.getAttribute("data-nav-zone") !== "hero") return "out";
  // Bas du haut de fiche : les onglets ; haut d'une colonne : le haut de la fiche, puis la jaquette.
  const other = settle(spatialNext(items.filter((el) => el.closest("[data-nav-zone]") !== zone), from, dir));
  return other ?? (dir === "down" ? "out" : "card");
}

/** Fiche du jeu, dépliée sous la rangée de sa jaquette : en-tête, chiffres, onglets et colonne de droite. */
export function GameStrip({
  game,
  notch,
  playing,
  onPlay,
  onFavorite,
  onHide,
  onOpenFolder,
  onShortcut,
  onChangeExe,
  onRename,
  onChooseArt,
  onResetArt,
  onRemove,
  onTag,
  onMood,
  onJournal,
  onClose,
  collections,
  onStatus,
  onNote,
  onAliases,
  onGenres,
  onLaunchArgs,
  onToggleCollection,
  onCreateCollection,
  edit,
  onEditStarted,
}: Props) {
  const [renaming, setRenaming] = useState(false);
  // Panneau « Déplacer vers un autre disque » ouvert sous les actions.
  const [moving, setMoving] = useState(false);
  const [editingNote, setEditingNote] = useState(false);
  const [showCollections, setShowCollections] = useState(false);
  const [creating, setCreating] = useState(false);
  const [chosen, setChosen] = useState<Tab>(() => remembered(TAB_KEY, TABS, "overview"));
  // Demande venue du menu contextuel : le champ s'ouvre (et prend le focus) dès que la fiche est là.
  useEffect(() => {
    if (!edit) return;
    if (edit === "rename") setRenaming(true);
    else {
      setShowCollections(true);
      setCreating(true);
    }
    onEditStarted?.();
  }, [edit]);
  const history = useHistory(game.id, playing);
  const shots = useScreenshots(game.id, playing);
  const hasSaves = useHasSaves(game.id, playing);
  const news = useNews(game.id);
  const mods = useMods(game, playing);
  const perf = usePerformance(game.id, playing);
  const member = collections.filter((c) => c.games.includes(game.id));
  const details = detailsLine(game);
  const d = game.details;
  // Un jeu sans fiche Steam dont l'utilisateur a choisi les genres reçoit une fiche qui ne porte que ceux-ci.
  const steamFiche = !!d && !(game.customGenres && !d.description && d.tags.length === 0 && d.developers.length === 0 && d.publishers.length === 0 && !d.releaseDate);
  const moods = d?.moods ?? [];
  const tags = d?.tags ?? [];
  const lastSession = lastSessionText(game, history?.[0] ?? null);
  const install = installNote(game.installState, PLATFORM_LABEL[game.platform]);
  const rate = downloadDetail(game.download);
  const achievements = game.achievements && game.achievements.total > 0 ? game.achievements : null;
  const lifespan = lifespanStat(game);

  // Le journal a de quoi montrer dès qu'il y a une session, une capture ou un succès daté.
  const journalData = (history?.length ?? 0) > 0 || (shots?.length ?? 0) > 0 || (achievements?.unlocked ?? 0) > 0;
  // Ce que chaque onglet a à montrer : `null` pendant une lecture (onglet montré seulement s'il est retenu).
  const has: Record<Tab, boolean | null> = {
    overview: true,
    achievements: achievements !== null,
    shots: shots && shots.length > 0,
    saves: hasSaves,
    sessions: history && history.length > 0,
    performance: perf && perf.length > 0,
    journal: !onJournal ? false : journalData ? true : history === null || shots === null ? null : false,
    news: news && news.length > 0,
    mods: mods && mods.mods.length > 0,
  };
  const tabs = TABS.filter((t) => has[t] === true || (has[t] === null && t === chosen));
  const active: Tab = tabs.includes(chosen) ? chosen : "overview";
  const choose = (t: Tab) => {
    if (t === chosen) return;
    setChosen(t);
    remember(TAB_KEY, t);
  };
  const count: Partial<Record<Tab, string>> = {
    achievements: achievements ? `${achievements.unlocked}/${achievements.total}` : undefined,
    shots: shots?.length ? String(shots.length) : undefined,
    sessions: history?.length ? String(history.length) : undefined,
    performance: perf?.length ? String(perf.length) : undefined,
    // Annonces publiées depuis la dernière partie.
    news: news && newNewsCount(news, game.lastPlayed) > 0 ? String(newNewsCount(news, game.lastPlayed)) : undefined,
    mods: mods?.mods.length ? String(mods.mods.length) : undefined,
  };
  const tabId = (t: Tab) => `fiche-${t}`;

  return (
    <section className="strip" aria-label={t.fiche.ariaLabel(game.title)} style={{ ["--notch" as string]: `${notch}px` }}>
      <header className="strip__hero" data-nav-zone="hero">
        <Art game={game} className="strip__art" />
        <div className="strip__head">
          <div className="strip__eyebrow">
            <span className="eyebrow"><LauncherIcon platform={game.platform} />{PLATFORM_LABEL[game.platform]}{game.emulation ? ` · ${game.emulation.systemName}` : ""}{game.isTool ? ` · ${t.fiche.application}` : ""}</span>
            {playing && <span className="live">{t.fiche.inGame}</span>}
          </div>
          {renaming ? (
            <TitleEditor game={game} onRename={onRename} onDone={() => setRenaming(false)} />
          ) : (
            <h2 className="strip__title"><Logo game={game} /></h2>
          )}
          {details && <p className="strip__meta" title={details.title}>{details.line}</p>}
          <div className="strip__acts">
            <button type="button" className="btn-play" data-nav onClick={() => onPlay(game)}><PlayIcon /> {t.fiche.play}</button>
            <button
              type="button"
              className="btn-icon"
              data-nav
              aria-pressed={game.favorite}
              aria-label={game.favorite ? t.fiche.favoriteRemove : t.fiche.favoriteAdd}
              title={game.favorite ? t.fiche.favoriteRemoveTitle : t.fiche.favoriteAddTitle}
              onClick={() => onFavorite(game)}
            >
              <StarIcon filled={game.favorite} />
            </button>
            {install && <span className="live is-pending">{install}{rate && ` · ${rate}`}</span>}
          </div>
        </div>
      </header>

      <dl className="stats strip__stats">
        <div title={playtimeOrigin(game)}><dt>{t.fiche.stats.playtime}</dt><dd>{formatPlaytime(game.playtimeMinutes)}</dd></div>
        <div title={lastSession.title}><dt>{t.fiche.stats.lastSession}</dt><dd>{lastSession.text}</dd></div>
        {achievements && (
          <div title={lastAchievement(achievements.lastUnlocked)}>
            <dt>{t.fiche.stats.achievements}</dt><dd>{formatAchievements(achievements.unlocked, achievements.total)}</dd>
          </div>
        )}
        {lifespan && (
          <div title={t.fiche.stats.lifespanTitle(game.hltb!.name)}>
            <dt>{t.fiche.stats.lifespan}</dt><dd>{lifespan}</dd>
          </div>
        )}
        <div><dt>{t.fiche.stats.size}</dt><dd>{formatSize(game.sizeOnDisk)}</dd></div>
      </dl>

      <div className="strip__body">
        <div className="strip__main" data-nav-zone="main">
          <div className="fiche__tabs" role="tablist" aria-label={t.fiche.tabsLabel}>
            {tabs.map((t) => (
              <button
                key={t}
                type="button"
                role="tab"
                id={`${tabId(t)}-tab`}
                aria-selected={t === active}
                aria-controls={tabId(t)}
                tabIndex={t === active ? 0 : -1}
                data-nav
                className="fiche__tab"
                // Le focus suffit (flèches, croix de la manette) : l'onglet s'affiche dès qu'il est atteint.
                onFocus={() => choose(t)}
                onClick={() => choose(t)}
              >
                {TAB_LABEL[t]}
                {count[t] && <em>{count[t]}</em>}
              </button>
            ))}
          </div>
          <div className="fiche__panel" role="tabpanel" id={tabId(active)} aria-labelledby={`${tabId(active)}-tab`}>
            {active === "overview" && (
              <>
                {d?.description && <p className="fiche__desc">{d.description}</p>}
                {(moods.length > 0 || tags.length > 0) && (
                  <ul className="chips" aria-label={t.fiche.overview.chips}>
                    {moods.map((m) => (
                      <li key={m}>
                        <button type="button" className="chip is-mood" data-nav title={t.fiche.overview.moodTitle(MOOD_LABEL[m])} onClick={() => onMood(m)}>{MOOD_LABEL[m]}</button>
                      </li>
                    ))}
                    {tags.map((tag) => (
                      <li key={tag}>
                        <button type="button" className="chip" data-nav title={t.fiche.overview.tagTitle(tag)} onClick={() => onTag(tag)}>{tag}</button>
                      </li>
                    ))}
                  </ul>
                )}
                <dl className="fiche__facts">
                  {d && d.genres.length > 0 && (
                    <div title={game.customGenres ? t.fiche.overview.genresCustom : undefined}>
                      <dt>{t.fiche.overview.genres}</dt><dd>{d.genres.map(genreLabel).join(", ")}</dd>
                    </div>
                  )}
                  {d && d.developers.length > 0 && <div><dt>{t.fiche.overview.studio}</dt><dd>{d.developers.join(", ")}</dd></div>}
                  {d && d.publishers.length > 0 && <div><dt>{t.fiche.overview.publisher}</dt><dd>{d.publishers.join(", ")}</dd></div>}
                  {d?.releaseDate && <div><dt>{t.fiche.overview.release}</dt><dd title={formatDate(d.releaseDate)}>{releaseYear(d.releaseDate)}</dd></div>}
                  {d?.controller && (
                    <div title={t.fiche.overview.controllerTitle}>
                      <dt>{t.fiche.overview.controller}</dt><dd>{d.controller === "full" ? t.fiche.overview.controllerFull : t.fiche.overview.controllerPartial}</dd>
                    </div>
                  )}
                  <div><dt>{t.fiche.overview.launcher}</dt><dd className="with-icon"><LauncherIcon platform={game.platform} />{PLATFORM_LABEL[game.platform]}</dd></div>
                  {game.emulation && (
                    <>
                      <div><dt>{t.fiche.overview.console}</dt><dd>{game.emulation.systemName}</dd></div>
                      <div><dt>{t.fiche.overview.emulator}</dt><dd>{game.emulation.emulator}</dd></div>
                    </>
                  )}
                  {game.launchArgs && (
                    <div className="is-wide" title={game.launchArgs}>
                      <dt>{t.fiche.overview.options}</dt><dd className="args">{shortArgs(game.launchArgs, 60)}</dd>
                    </div>
                  )}
                  {game.installDir && <div className="is-wide"><dt>{t.fiche.overview.folder}</dt><dd className="path" title={game.installDir}>{game.installDir}</dd></div>}
                </dl>
                {!steamFiche && <p className="fiche__note">{t.fiche.overview.noFiche(!!game.customGenres)}</p>}
                <RequirementsPanel key={`reqs-${game.id}`} gameId={game.id} />
              </>
            )}
            {active === "achievements" && <AchievementList key={`achs-${game.id}`} game={game} expanded />}
            {active === "shots" && <ShotsTab key={`shots-${game.id}`} game={game} shots={shots} />}
            {active === "saves" && <SavesPanel key={`saves-${game.id}`} gameId={game.id} playing={playing} />}
            {active === "sessions" && <SessionsTab key={`sessions-${game.id}`} game={game} history={history} />}
            {active === "performance" && perf && <PerformancePanel key={`perf-${game.id}`} sessions={perf} />}
            {active === "journal" && onJournal && <JournalTab key={`journal-${game.id}`} game={game} playing={playing} onJournal={onJournal} />}
            {active === "news" && <NewsTab key={`news-${game.id}`} game={game} news={news} />}
            {active === "mods" && mods && <ModsPanel key={`mods-${game.id}`} gameId={game.id} initial={mods} />}
          </div>
        </div>

        <aside className="strip__side" data-nav-zone="side" aria-label={t.fiche.side.label}>
          <div className="fiche__block">
            <p className="fiche__label" role="heading" aria-level={3}>{t.fiche.side.progress}</p>
            <div className="seg strip__status" role="group" aria-label={t.fiche.side.progress}>
              {STATUSES.map((s) => (
                <button
                  key={s}
                  type="button"
                  data-nav
                  aria-pressed={game.status === s}
                  title={game.status === s ? t.fiche.side.statusAgain : undefined}
                  onClick={() => onStatus(game, game.status === s ? null : s)}
                >
                  {STATUS_LABEL[s]}
                </button>
              ))}
            </div>
            {editingNote ? (
              <InlineInput
                className="strip__note-input"
                initial={game.note ?? ""}
                maxLength={MAX_NOTE}
                label={t.fiche.note.label(game.title)}
                placeholder={t.fiche.note.placeholder}
                onSubmit={(note) => onNote(game, note)}
                onDone={() => setEditingNote(false)}
              />
            ) : (
              <button
                type="button"
                data-nav
                className={`strip__note ${game.note ? "" : "is-empty"}`}
                title={game.note ? `${game.note}\n${t.fiche.note.editHint}` : t.fiche.note.addTitle}
                onClick={() => setEditingNote(true)}
              >
                <NoteIcon />
                <span>{game.note ?? t.fiche.note.add}</span>
              </button>
            )}
          </div>

          <div className="fiche__block">
            <div className="fiche__row">
              <p className="fiche__label" role="heading" aria-level={3}>{t.fiche.collections.title}</p>
              <button
                type="button"
                className="fiche__link"
                data-nav
                aria-expanded={showCollections}
                onClick={() => setShowCollections((v) => !v)}
              >
                {showCollections ? t.fiche.close : member.length ? t.fiche.collections.edit : t.fiche.collections.add}
              </button>
            </div>
            {showCollections ? (
              <div className="strip__cols" role="group" aria-label={t.fiche.collections.title}>
                {collections.map((c) => {
                  const inside = c.games.includes(game.id);
                  return (
                    <button key={c.id} type="button" className="chip" data-nav aria-pressed={inside} onClick={() => onToggleCollection(game, c, !inside)}>
                      {c.name}
                    </button>
                  );
                })}
                {creating ? (
                  <InlineInput
                    className="chip chip--input"
                    maxLength={MAX_COLLECTION_NAME}
                    label={t.fiche.collections.newLabel}
                    placeholder={t.fiche.collections.newPlaceholder}
                    onSubmit={(name) => onCreateCollection(name, game)}
                    onDone={() => setCreating(false)}
                  />
                ) : (
                  <button type="button" className="chip chip--add" data-nav onClick={() => setCreating(true)}>
                    <PlusIcon />{t.fiche.collections.create}
                  </button>
                )}
              </div>
            ) : (
              <p className="strip__member">{member.length ? member.map((c) => t.fiche.collections.quoted(c.name)).join(", ") : t.fiche.collections.none}</p>
            )}
          </div>

          <Genres game={game} onGenres={onGenres} />

          <Aliases game={game} onAliases={onAliases} />

          {game.hltb && (
            <div className="fiche__block">
              <Lifespan game={game} />
            </div>
          )}

          <LaunchArgs game={game} onLaunchArgs={onLaunchArgs} />

          <div className="fiche__block">
            <p className="fiche__label" role="heading" aria-level={3}>{t.fiche.actions.title}</p>
            <div className="strip__tools">
              {game.installDir && (
                <button type="button" className="btn-icon" data-nav aria-label={t.fiche.actions.openFolder} title={t.fiche.actions.openFolder} onClick={() => onOpenFolder(game)}>
                  <FolderIcon />
                </button>
              )}
              <button type="button" className="btn-icon" data-nav aria-label={t.fiche.actions.shortcut} title={t.fiche.actions.shortcut} onClick={() => onShortcut(game)}>
                <ShortcutIcon />
              </button>
              {game.installDir && (
                <button
                  type="button"
                  className="btn-icon"
                  data-nav
                  aria-pressed={moving}
                  aria-label={t.fiche.move.action}
                  title={t.fiche.move.action}
                  onClick={() => setMoving((v) => !v)}
                >
                  <MoveIcon />
                </button>
              )}
              {game.platform === "other" && (
                <button type="button" className="btn-icon" data-nav aria-label={t.fiche.actions.changeExe} title={t.fiche.actions.changeExe} onClick={() => onChangeExe(game)}>
                  <FileIcon />
                </button>
              )}
              <button
                type="button"
                className="btn-icon"
                data-nav
                aria-pressed={renaming}
                aria-label={t.fiche.actions.rename}
                title={t.fiche.actions.rename}
                // Sans ceci, le clic ferme d'abord l'éditeur (perte du focus) puis le rouvre.
                onMouseDown={(e) => renaming && e.preventDefault()}
                onClick={() => setRenaming((r) => !r)}
              >
                <PencilIcon />
              </button>
              <button type="button" className="btn-icon" data-nav aria-label={t.fiche.actions.cover} title={t.fiche.actions.cover} onClick={() => onChooseArt(game, "cover")}>
                <ImageIcon />
              </button>
              <button type="button" className="btn-icon" data-nav aria-label={t.fiche.actions.hero} title={t.fiche.actions.hero} onClick={() => onChooseArt(game, "hero")}>
                <WideImageIcon />
              </button>
              {game.customArt.length > 0 && (
                <button type="button" className="btn-icon" data-nav aria-label={resetLabel(game.customArt)} title={resetLabel(game.customArt)} onClick={() => onResetArt(game)}>
                  <UndoIcon />
                </button>
              )}
              <button
                type="button"
                className="btn-icon"
                data-nav
                aria-label={game.hidden ? t.fiche.actions.unhide : t.fiche.actions.hide}
                title={game.hidden ? t.fiche.actions.unhide : t.fiche.actions.hide}
                onClick={() => onHide(game)}
              >
                <EyeOffIcon />
              </button>
              {game.manual && (
                <button type="button" className="btn-icon" data-nav aria-label={t.fiche.actions.remove} title={t.fiche.actions.remove} onClick={() => onRemove(game)}>
                  <TrashIcon />
                </button>
              )}
            </div>
            {moving && game.installDir && <MovePanel key={`move-${game.id}`} id={game.id} title={game.title} />}
          </div>
        </aside>
      </div>

      <button type="button" className="strip__close" aria-label={t.fiche.closeFiche} title={t.fiche.closeTitle} onClick={onClose}>
        <CloseIcon />
      </button>
    </section>
  );
}
