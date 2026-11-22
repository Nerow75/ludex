import { useCallback, useEffect, useMemo, useRef, useState, type FocusEvent } from "react";
import { allScreenshots, type Screenshot, type ScreenshotGroup } from "../lib/api";
import { formatDate } from "../lib/format";
import { locale, t, useLang } from "../lib/i18n";
import { remember, remembered } from "../lib/library";
import {
  SHOT_KINDS,
  SHOT_SORTS,
  allShots,
  arrangeFolders,
  byMonth,
  kindCounts,
  shotCount,
  shotFolders,
  type ShotFolder,
  type ShotKind,
  type ShotSort,
} from "../lib/screenshots";
import { ImageIcon, PlayIcon, RefreshIcon } from "./Icons";
import { ScreenshotViewer, ShotTile } from "./ScreenshotViewer";

interface Props {
  /** Clic sur « Voir la fiche » d'un jeu : ouvrir sa fiche (facultatif). */
  onOpenGame?: (gameId: string) => void;
}

const shortFormats = new Map<string, Intl.DateTimeFormat>();
/** « 27 oct. 2026 », « Oct 27, 2026 » : date de la plus récente dans la liste, où la place manque. */
function shortDate(ts: number): string {
  const loc = locale();
  let f = shortFormats.get(loc);
  if (!f) {
    f = new Intl.DateTimeFormat(loc, { day: "numeric", month: "short", year: "numeric" });
    shortFormats.set(loc, f);
  }
  return f.format(new Date(ts * 1000));
}

/** Entrée « Toutes les captures » en tête de la liste. */
const ALL = "*";

/** Dernier dossier ouvert, gardé tant que Ludex tourne (on revient sur la page là où on l'a laissée). */
let lastFolder = ALL;

/**
 * Page « Captures » (Steam, Xbox Game Bar) : à gauche la liste des jeux (aperçu de la plus récente,
 * nombre, date), « Toutes les captures » en tête ; à droite la grille du dossier choisi, rangée par mois.
 * Tri de la liste (récentes, A → Z, nombre) et filtre images / vidéos quand il y a des deux.
 * Autonome : elle lit elle-même ses données. Clavier et manette : tout porte `data-nav` dans un
 * conteneur `data-nav-2d`, les flèches suivent l'écran ; une entrée de la liste qui prend le focus
 * s'ouvre aussitôt, Entrée (A) passe à sa première capture.
 */
export function ScreenshotsPage({ onOpenGame }: Props) {
  // Groupes reçus de Rust ; les dossiers (« Jeu Steam 218 », « Autres captures ») sont refaits au changement de langue.
  const lang = useLang();
  const [groups, setGroups] = useState<ScreenshotGroup[] | null>(null);
  const folders = useMemo<ShotFolder[] | null>(() => (groups ? shotFolders(groups) : null), [groups, lang]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [sort, setSort] = useState<ShotSort>(() => remembered("shotsSort", SHOT_SORTS.map((s) => s.id), "recent"));
  const [kind, setKind] = useState<ShotKind>(() => remembered("shotsKind", SHOT_KINDS.map((k) => k.id), "all"));
  const [current, setCurrent] = useState(lastFolder);
  const [open, setOpen] = useState<number | null>(null);
  const mainRef = useRef<HTMLDivElement>(null);
  const gridRef = useRef<HTMLDivElement>(null);
  const listRef = useRef<HTMLElement>(null);

  const load = useCallback(() => {
    setLoading(true);
    setError(null);
    allScreenshots()
      .then((g) => {
        setGroups(g);
        setOpen(null);
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, []);

  useEffect(load, [load]);

  const counts = useMemo(() => kindCounts(folders ?? []), [folders]);
  // Filtre proposé seulement s'il y a des images et des vidéos ; sinon il reste sur « Tout ».
  const showKinds = counts.images > 0 && counts.videos > 0;
  const activeKind: ShotKind = showKinds ? kind : "all";
  const list = useMemo(() => arrangeFolders(folders ?? [], sort, activeKind), [folders, sort, activeKind]);
  const every = useMemo(() => allShots(list), [list]);
  const folder = current === ALL ? null : list.find((f) => f.key === current);
  // Dossier disparu (relu, filtré) : retour à « Toutes les captures ».
  const key = folder ? current : ALL;
  const shots = folder ? folder.shots : every;
  const months = useMemo(() => byMonth(shots), [shots]);
  const titleOf = useMemo(() => {
    const m = new Map<string, string>();
    for (const f of list) for (const s of f.shots) m.set(s.key, f.title);
    return m;
  }, [list]);
  const text = t.extras.shots;
  const label = (s: Screenshot) => titleOf.get(s.key) ?? text.title;

  const choose = (next: string) => {
    lastFolder = next;
    setCurrent(next);
  };
  // Dossier ouvert toujours visible dans la liste (colonne collante ou bandeau horizontal), sans
  // faire défiler la page : seule la liste bouge.
  useEffect(() => {
    const nav = listRef.current;
    const entry = nav?.querySelector<HTMLElement>(`[data-folder="${CSS.escape(key)}"]`);
    if (!nav || !entry) return;
    const n = nav.getBoundingClientRect();
    const e = entry.getBoundingClientRect();
    if (e.top < n.top) nav.scrollTop -= n.top - e.top + 8;
    else if (e.bottom > n.bottom) nav.scrollTop += e.bottom - n.bottom + 8;
    if (e.left < n.left) nav.scrollLeft -= n.left - e.left + 8;
    else if (e.right > n.right) nav.scrollLeft += e.right - n.right + 8;
  }, [key, list]);
  // Nouveau dossier : la grille repart de son début si on l'avait fait défiler.
  useEffect(() => {
    const main = mainRef.current;
    const scroller = main?.closest<HTMLElement>(".scroll");
    if (main && scroller && main.getBoundingClientRect().top < scroller.getBoundingClientRect().top) {
      scroller.scrollTop += main.getBoundingClientRect().top - scroller.getBoundingClientRect().top - 16;
    }
  }, [key]);

  /**
   * Haut et bas dans la liste ouvrent l'entrée qui prend le focus, comme la barre latérale. Y entrer
   * depuis ailleurs (gauche depuis la grille, haut ou bas depuis la barre du haut) ramène sur l'entrée
   * ouverte au lieu d'en ouvrir une autre au passage.
   */
  const onListFocus = (e: FocusEvent<HTMLElement>) => {
    const entry = (e.target as HTMLElement).closest<HTMLElement>(".shots-folder");
    if (!entry?.dataset.folder) return;
    const from = e.relatedTarget as Node | null;
    if (from && e.currentTarget.contains(from)) choose(entry.dataset.folder);
    else if (entry.dataset.folder !== key) e.currentTarget.querySelector<HTMLElement>(`[data-folder="${CSS.escape(key)}"]`)?.focus();
  };

  const chooseSort = (v: ShotSort) => {
    setSort(v);
    remember("shotsSort", v);
  };
  const chooseKind = (v: ShotKind) => {
    setKind(v);
    remember("shotsKind", v);
  };

  const total = folders ? counts.all : 0;
  const viewed = open !== null ? shots[open] : undefined;

  return (
    <section className="shots-page" aria-label={text.title} aria-busy={loading} data-nav-2d>
      <div className="bar">
        <h2>
          {text.title}{folders && <span><span className="sr-only">, </span>{shotCount(total)}</span>}
        </h2>
        {showKinds && (
          <div className="seg" role="group" aria-label={text.kindGroup}>
            {SHOT_KINDS.map((k) => (
              <button key={k.id} type="button" data-nav aria-pressed={k.id === kind} onClick={() => chooseKind(k.id)}>
                {k.label} <em>{counts[k.id]}</em>
              </button>
            ))}
          </div>
        )}
        {list.length > 1 && (
          <div className="seg" role="group" aria-label={text.sortGroup}>
            {SHOT_SORTS.map((s) => (
              <button key={s.id} type="button" data-nav aria-pressed={s.id === sort} onClick={() => chooseSort(s.id)}>
                {s.label}
              </button>
            ))}
          </div>
        )}
        <button type="button" className="btn-icon" data-nav aria-label={text.reload} title={text.reload} disabled={loading} onClick={load}>
          <RefreshIcon />
        </button>
      </div>

      {error && <p className="shots-page__error" role="alert">{error}</p>}

      {folders && total === 0 && (
        <div className="empty" role="status">
          <span className="empty__icon"><ImageIcon /></span>
          <h3>{text.none}</h3>
          <p>{text.noneText}</p>
        </div>
      )}

      {total > 0 && (
        <div className="shots-layout">
          <nav className="shots-list" ref={listRef} aria-label={text.games} onFocus={onListFocus}>
            <FolderButton
              id={ALL}
              title={text.all}
              hint={text.all}
              meta={shotCount(every.length, activeKind)}
              shot={every[0]}
              current={key === ALL}
              onChoose={() => choose(ALL)}
              onEnter={() => gridRef.current?.querySelector<HTMLElement>("[data-nav]")?.focus()}
            />
            <div className="shots-list__sep" role="presentation" />
            {list.map((f) => (
              <FolderButton
                key={f.key}
                id={f.key}
                title={f.title}
                meta={`${shotCount(f.shots.length, activeKind)} · ${shortDate(f.shots[0].taken)}`}
                hint={f.note ? `${f.title} (${f.note.toLowerCase()})` : f.title}
                shot={f.shots[0]}
                current={key === f.key}
                onChoose={() => choose(f.key)}
                onEnter={() => gridRef.current?.querySelector<HTMLElement>("[data-nav]")?.focus()}
              />
            ))}
          </nav>

          <div className="shots-main" ref={mainRef}>
            <header className="shots-main__head">
              <h3>{folder ? folder.title : text.all}</h3>
              <span className="eyebrow">
                {[folder?.note, shotCount(shots.length, activeKind), shots[0] && text.latest(formatDate(shots[0].taken))].filter(Boolean).join(" · ")}
              </span>
              {folder?.gameId && onOpenGame && (
                <button type="button" className="btn-ghost shots-main__open" data-nav onClick={() => onOpenGame(folder.gameId!)}>
                  {text.viewDetails}
                </button>
              )}
            </header>
            <div ref={gridRef} className="shots-months">
              {months.map((m) => (
                <section key={m.key} className="shots-month" aria-label={m.label}>
                  <h4>
                    {m.label}
                    <span>{m.shots.length}</span>
                  </h4>
                  <div className="shots-row">
                    {m.shots.map((s, i) => (
                      <ShotTile key={s.key} shot={s} nav label={t.extras.viewer.shotOf(label(s), formatDate(s.taken))} onOpen={() => setOpen(m.start + i)} />
                    ))}
                  </div>
                </section>
              ))}
            </div>
          </div>
        </div>
      )}

      {viewed && open !== null && (
        <ScreenshotViewer
          shots={shots}
          index={open}
          // « Toutes les captures » mêle les jeux : le titre suit la capture affichée.
          title={folder ? folder.title : label(viewed)}
          onIndex={setOpen}
          onClose={() => setOpen(null)}
        />
      )}
    </section>
  );
}

interface FolderProps {
  /** Clé du dossier (`data-folder`). */
  id: string;
  title: string;
  /** Info-bulle : le titre en entier, avec la précision (« désinstallé »). */
  hint: string;
  meta: string;
  /** Capture la plus récente, en aperçu. */
  shot: Screenshot | undefined;
  current: boolean;
  onChoose: () => void;
  /** Entrée ou A sur l'entrée déjà ouverte : passer à la grille. */
  onEnter: () => void;
}

/**
 * Entrée de la liste (le focus est suivi par la liste, voir `onListFocus`). Un clic de souris ouvre
 * seulement ; Entrée, Espace ou A (clic sans pointeur) passent en plus à la première capture.
 */
function FolderButton({ id, title, hint, meta, shot, current, onChoose, onEnter }: FolderProps) {
  return (
    <button
      type="button"
      className="shots-folder"
      data-nav
      data-folder={id}
      aria-current={current || undefined}
      title={hint}
      onClick={(e) => {
        onChoose();
        e.currentTarget.focus();
        // `detail` à 0 : clic venu du clavier ou de la manette, pas de la souris.
        if (e.detail === 0) requestAnimationFrame(onEnter);
      }}
    >
      <span className="shots-folder__thumb">
        {shot && (shot.video ? <PlayIcon /> : <img src={shot.thumbUrl} alt="" loading="lazy" decoding="async" draggable={false} />)}
      </span>
      <span className="shots-folder__text">
        <span className="shots-folder__title">{title}</span>
        <span className="shots-folder__meta">{meta}</span>
      </span>
    </button>
  );
}
