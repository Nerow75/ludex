import { useEffect, useState } from "react";
import {
  gameHistory,
  gameSaves,
  gameScreenshots,
  journal,
  MAX_COLLECTION_NAME,
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
  type Status,
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
  installNote,
  relativeDay,
  relativeTime,
  releaseYear,
  shortArgs,
} from "../lib/format";
import { downloadDetail } from "../lib/install";
import { remember, remembered } from "../lib/library";
import { spatialNext, type Direction } from "../lib/spatial";
import { AchievementList } from "./AchievementList";
import { Art, Logo } from "./Art";
import { gameSummary } from "./JournalView";
import { ScreenshotViewer, ShotTile } from "./ScreenshotViewer";
import { CloseIcon, EyeOffIcon, FileIcon, FolderIcon, ImageIcon, JournalIcon, NoteIcon, PencilIcon, PlayIcon, PlusIcon, ShortcutIcon, SlidersIcon, StarIcon, TrashIcon, UndoIcon, WideImageIcon } from "./Icons";
import { InlineInput } from "./InlineInput";
import { SavesPanel } from "./SavesPanel";
import { RequirementsPanel } from "./RequirementsPanel";

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
const TABS = ["overview", "achievements", "shots", "saves", "sessions", "journal"] as const;
type Tab = (typeof TABS)[number];
const TAB_LABEL: Record<Tab, string> = {
  overview: "Aperçu",
  achievements: "Succès",
  shots: "Captures",
  saves: "Sauvegardes",
  sessions: "Sessions",
  journal: "Journal",
};
/** Onglet retenu entre deux fiches (stockage local, comme les autres préférences d'affichage). */
const TAB_KEY = "ficheTab";

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
      <p className="fiche__label">Options de lancement</p>
      {refusal ? (
        <>
          <button type="button" className="strip__note strip__args is-empty" disabled aria-describedby={`args-why-${game.id}`}>
            <SlidersIcon />
            <span>Indisponibles</span>
          </button>
          <p className="strip__args-help" id={`args-why-${game.id}`}>{refusal}</p>
        </>
      ) : editing ? (
        <InlineInput
          key={draft ?? ""}
          className="strip__note-input strip__args-input"
          initial={draft ?? game.launchArgs ?? ""}
          maxLength={MAX_LAUNCH_ARGS}
          label={`Options de lancement de ${game.title}`}
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
          title={game.launchArgs ? `${game.launchArgs}
Cliquer pour modifier. Vide : options effacées` : "Ajouter des options de lancement"}
          onClick={() => {
            setError(null);
            setEditing(true);
          }}
        >
          <SlidersIcon />
          <span>{game.launchArgs ?? "Ajouter des options"}</span>
        </button>
      )}
      {error ? (
        <p className="strip__args-error" role="alert">{error}</p>
      ) : (
        !refusal &&
        editing && (
          <p className="strip__args-help">
            Passées au jeu à chaque lancement depuis Ludex, séparées par des espaces, guillemets pour un texte avec espaces.
            {game.platform === "steam" && " Steam peut demander de confirmer."}
          </p>
        )
      )}
    </div>
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
  const title = last.launcher
    ? `Session observée par Ludex. Le temps de jeu reste celui de ${PLATFORM_LABEL[game.platform]}.`
    : "Session mesurée par Ludex";
  return { text: `${relativeTime(Math.max(end, launcherDate))}, ${formatDuration(last.secs)}`, title };
}

/** Libellé du retour aux images d'origine, selon ce qui a été remplacé. */
function resetLabel(custom: ArtKind[]): string {
  if (custom.length > 1) return "Jaquette et image de fond d'origine";
  return custom[0] === "hero" ? "Image de fond d'origine" : "Jaquette d'origine";
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

/** Vignettes montrées dans l'onglet ; la dernière annonce les suivantes (« +12 »). */
const TAB_SHOTS = 12;

/** Onglet « Captures » : vignettes et visionneuse. */
function ShotsTab({ game, shots }: { game: Game; shots: Screenshot[] | null }) {
  const [open, setOpen] = useState<number | null>(null);
  if (!shots) return <p className="fiche__note">Lecture des captures…</p>;
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
            label={`Capture ${i + 1} de ${game.title}`}
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
  if (!history) return <p className="fiche__note">Lecture de l'historique…</p>;
  const total = history.reduce((n, s) => n + s.secs, 0);
  const plural = history.length > 1 ? "s" : "";
  return (
    <>
      <p className="fiche__note">
        {history.length} session{plural} lancée{plural} depuis Ludex, {formatDuration(total)} au total
      </p>
      <ol className="sessions" aria-label={`Sessions de ${game.title}`}>
        {history.slice(0, TAB_SESSIONS).map((s) => (
          <li
            key={`${s.start}-${s.secs}`}
            className="sessions__row"
            title={s.launcher ? `Session observée par Ludex. Le temps de jeu reste celui de ${PLATFORM_LABEL[game.platform]}.` : undefined}
          >
            <span>{s.merged ? `${formatDate(s.start)}, plusieurs sessions` : formatDateTime(s.start)}</span>
            <span className="sessions__secs">{formatDuration(s.secs)}</span>
          </li>
        ))}
      </ol>
      {history.length > TAB_SESSIONS && <p className="fiche__note">Les {history.length - TAB_SESSIONS} plus anciennes sont dans son journal.</p>}
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
      {days === null && <p className="fiche__note">Lecture du journal…</p>}
      {days === "error" && <p className="fiche__note">Journal illisible pour l'instant</p>}
      {Array.isArray(days) && days.length === 0 && <p className="fiche__note">Rien dans son journal pour l'instant</p>}
      {Array.isArray(days) && days.length > 0 && (
        <ol className="sessions" aria-label={`Derniers jours de ${game.title}`}>
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
        <JournalIcon /> Voir tout son journal
      </button>
    </>
  );
}

/** Temps restant pour l'histoire, d'après la moyenne HowLongToBeat et le temps de jeu connu. */
export function storyLeft(main: number | null, played: number | null): string | null {
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
  onLaunchArgs,
  onToggleCollection,
  onCreateCollection,
  edit,
  onEditStarted,
}: Props) {
  const [renaming, setRenaming] = useState(false);
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
  const member = collections.filter((c) => c.games.includes(game.id));
  const details = detailsLine(game);
  const d = game.details;
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
    journal: !onJournal ? false : journalData ? true : history === null || shots === null ? null : false,
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
  };
  const tabId = (t: Tab) => `fiche-${t}`;

  return (
    <section className="strip" aria-label={`Fiche de ${game.title}`} style={{ ["--notch" as string]: `${notch}px` }}>
      <header className="strip__hero" data-nav-zone="hero">
        <Art game={game} className="strip__art" />
        <div className="strip__head">
          <div className="strip__eyebrow">
            <span className="eyebrow">{PLATFORM_LABEL[game.platform]}{game.isTool ? " · Application" : ""}</span>
            {playing && <span className="live">En jeu</span>}
          </div>
          {renaming ? (
            <TitleEditor game={game} onRename={onRename} onDone={() => setRenaming(false)} />
          ) : (
            <h2 className="strip__title"><Logo game={game} /></h2>
          )}
          {details && <p className="strip__meta" title={details.title}>{details.line}</p>}
          <div className="strip__acts">
            <button type="button" className="btn-play" data-nav onClick={() => onPlay(game)}><PlayIcon /> Jouer</button>
            <button
              type="button"
              className="btn-icon"
              data-nav
              aria-pressed={game.favorite}
              aria-label={game.favorite ? "Retirer des favoris" : "Ajouter aux favoris"}
              title={game.favorite ? "Retirer des favoris (Ctrl+D)" : "Ajouter aux favoris (Ctrl+D)"}
              onClick={() => onFavorite(game)}
            >
              <StarIcon filled={game.favorite} />
            </button>
            {install && <span className="live is-pending">{install}{rate && ` · ${rate}`}</span>}
          </div>
        </div>
      </header>

      <dl className="stats strip__stats">
        <div title={playtimeOrigin(game)}><dt>Temps de jeu</dt><dd>{formatPlaytime(game.playtimeMinutes)}</dd></div>
        <div title={lastSession.title}><dt>Dernière session</dt><dd>{lastSession.text}</dd></div>
        {achievements && (
          <div title={lastAchievement(achievements.lastUnlocked)}>
            <dt>Succès</dt><dd>{formatAchievements(achievements.unlocked, achievements.total)}</dd>
          </div>
        )}
        {lifespan && (
          <div title={`Histoire, moyenne HowLongToBeat pour « ${game.hltb!.name} »`}>
            <dt>Durée de vie</dt><dd>{lifespan}</dd>
          </div>
        )}
        <div><dt>Taille</dt><dd>{formatSize(game.sizeOnDisk)}</dd></div>
      </dl>

      <div className="strip__body">
        <div className="strip__main" data-nav-zone="main">
          <div className="fiche__tabs" role="tablist" aria-label="Onglets de la fiche">
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
                  <ul className="chips" aria-label="Ambiances et tags Steam">
                    {moods.map((m) => (
                      <li key={m}>
                        <button type="button" className="chip is-mood" data-nav title={`Jeux de l'ambiance « ${MOOD_LABEL[m]} »`} onClick={() => onMood(m)}>{MOOD_LABEL[m]}</button>
                      </li>
                    ))}
                    {tags.map((t) => (
                      <li key={t}>
                        <button type="button" className="chip" data-nav title={`Jeux du tag « ${t} »`} onClick={() => onTag(t)}>{t}</button>
                      </li>
                    ))}
                  </ul>
                )}
                <dl className="fiche__facts">
                  {d && d.genres.length > 0 && <div><dt>Genres</dt><dd>{d.genres.join(", ")}</dd></div>}
                  {d && d.developers.length > 0 && <div><dt>Studio</dt><dd>{d.developers.join(", ")}</dd></div>}
                  {d && d.publishers.length > 0 && <div><dt>Édition</dt><dd>{d.publishers.join(", ")}</dd></div>}
                  {d?.releaseDate && <div><dt>Sortie</dt><dd title={formatDate(d.releaseDate)}>{releaseYear(d.releaseDate)}</dd></div>}
                  {d?.controller && (
                    <div title="Prise en charge annoncée par la boutique Steam">
                      <dt>Manette</dt><dd>{d.controller === "full" ? "Complète" : "Partielle"}</dd>
                    </div>
                  )}
                  <div><dt>Launcher</dt><dd>{PLATFORM_LABEL[game.platform]}</dd></div>
                  {game.launchArgs && (
                    <div className="is-wide" title={game.launchArgs}>
                      <dt>Options</dt><dd className="args">{shortArgs(game.launchArgs, 60)}</dd>
                    </div>
                  )}
                  {game.installDir && <div className="is-wide"><dt>Dossier</dt><dd className="path" title={game.installDir}>{game.installDir}</dd></div>}
                </dl>
                {!d && <p className="fiche__note">Pas de fiche Steam pour ce jeu : ni description, ni genres, ni tags.</p>}
                <RequirementsPanel key={`reqs-${game.id}`} gameId={game.id} />
              </>
            )}
            {active === "achievements" && <AchievementList key={`achs-${game.id}`} game={game} expanded />}
            {active === "shots" && <ShotsTab key={`shots-${game.id}`} game={game} shots={shots} />}
            {active === "saves" && <SavesPanel key={`saves-${game.id}`} gameId={game.id} playing={playing} />}
            {active === "sessions" && <SessionsTab key={`sessions-${game.id}`} game={game} history={history} />}
            {active === "journal" && onJournal && <JournalTab key={`journal-${game.id}`} game={game} playing={playing} onJournal={onJournal} />}
          </div>
        </div>

        <aside className="strip__side" data-nav-zone="side" aria-label="Suivi et actions">
          <div className="fiche__block">
            <p className="fiche__label">Progression</p>
            <div className="seg strip__status" role="group" aria-label="Progression">
              {STATUSES.map((s) => (
                <button
                  key={s}
                  type="button"
                  data-nav
                  aria-pressed={game.status === s}
                  title={game.status === s ? "Cliquer de nouveau retire le statut" : undefined}
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
                label={`Note sur ${game.title}`}
                placeholder="Une note pour toi : où tu en es, ce qu'il reste à faire…"
                onSubmit={(note) => onNote(game, note)}
                onDone={() => setEditingNote(false)}
              />
            ) : (
              <button
                type="button"
                data-nav
                className={`strip__note ${game.note ? "" : "is-empty"}`}
                title={game.note ? `${game.note}\nCliquer pour modifier. Vide : note effacée` : "Ajouter une note perso"}
                onClick={() => setEditingNote(true)}
              >
                <NoteIcon />
                <span>{game.note ?? "Ajouter une note"}</span>
              </button>
            )}
          </div>

          <div className="fiche__block">
            <div className="fiche__row">
              <p className="fiche__label">Collections</p>
              <button
                type="button"
                className="fiche__link"
                data-nav
                aria-expanded={showCollections}
                onClick={() => setShowCollections((v) => !v)}
              >
                {showCollections ? "Fermer" : member.length ? "Modifier" : "Ajouter"}
              </button>
            </div>
            {showCollections ? (
              <div className="strip__cols" role="group" aria-label="Collections">
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
                    label="Nom de la nouvelle collection"
                    placeholder="Nom de la collection"
                    onSubmit={(name) => onCreateCollection(name, game)}
                    onDone={() => setCreating(false)}
                  />
                ) : (
                  <button type="button" className="chip chip--add" data-nav onClick={() => setCreating(true)}>
                    <PlusIcon />Nouvelle collection
                  </button>
                )}
              </div>
            ) : (
              <p className="strip__member">{member.length ? member.map((c) => `« ${c.name} »`).join(", ") : "Dans aucune collection"}</p>
            )}
          </div>

          {game.hltb && (
            <div className="fiche__block">
              <Lifespan game={game} />
            </div>
          )}

          <LaunchArgs game={game} onLaunchArgs={onLaunchArgs} />

          <div className="fiche__block">
            <p className="fiche__label">Actions</p>
            <div className="strip__tools">
              {game.installDir && (
                <button type="button" className="btn-icon" data-nav aria-label="Ouvrir le dossier" title="Ouvrir le dossier" onClick={() => onOpenFolder(game)}>
                  <FolderIcon />
                </button>
              )}
              <button type="button" className="btn-icon" data-nav aria-label="Créer un raccourci sur le Bureau" title="Créer un raccourci sur le Bureau" onClick={() => onShortcut(game)}>
                <ShortcutIcon />
              </button>
              {game.platform === "other" && (
                <button type="button" className="btn-icon" data-nav aria-label="Changer l'exécutable" title="Changer l'exécutable" onClick={() => onChangeExe(game)}>
                  <FileIcon />
                </button>
              )}
              <button
                type="button"
                className="btn-icon"
                data-nav
                aria-pressed={renaming}
                aria-label="Renommer"
                title="Renommer"
                // Sans ceci, le clic ferme d'abord l'éditeur (perte du focus) puis le rouvre.
                onMouseDown={(e) => renaming && e.preventDefault()}
                onClick={() => setRenaming((r) => !r)}
              >
                <PencilIcon />
              </button>
              <button type="button" className="btn-icon" data-nav aria-label="Changer la jaquette" title="Changer la jaquette" onClick={() => onChooseArt(game, "cover")}>
                <ImageIcon />
              </button>
              <button type="button" className="btn-icon" data-nav aria-label="Changer l'image de fond" title="Changer l'image de fond" onClick={() => onChooseArt(game, "hero")}>
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
                aria-label={game.hidden ? "Ne plus masquer" : "Masquer de la bibliothèque"}
                title={game.hidden ? "Ne plus masquer" : "Masquer de la bibliothèque"}
                onClick={() => onHide(game)}
              >
                <EyeOffIcon />
              </button>
              {game.manual && (
                <button type="button" className="btn-icon" data-nav aria-label="Retirer de la bibliothèque" title="Retirer de la bibliothèque" onClick={() => onRemove(game)}>
                  <TrashIcon />
                </button>
              )}
            </div>
          </div>
        </aside>
      </div>

      <button type="button" className="strip__close" aria-label="Fermer la fiche" title="Fermer (Échap)" onClick={onClose}>
        <CloseIcon />
      </button>
    </section>
  );
}
