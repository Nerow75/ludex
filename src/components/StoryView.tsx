// Story du récap : jusqu'à 9 pages verticales (9:16, centrées dans la fenêtre, jamais de vrai plein
// écran) tirées du récap affiché. Pages et profil dans `src/lib/story.ts`, image exportée dans
// `storyImage.ts` (mêmes pages, mêmes proportions : 1 cqw ici = 10,8 px dans l'image).
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { recapRarest, saveStoryImage, type Game, type RareUnlock, type Recap } from "../lib/api";
import { titleColor } from "../lib/color";
import { rarityText, STORY_PAGE_MS, stepPage, storyAction, storyPages, type StoryPage } from "../lib/story";
import { Logo } from "./Art";
import { Cover } from "./Cover";
import { ChevronLeftIcon, ChevronRightIcon, LudexMark } from "./Icons";
import { drawStoryPage } from "./storyImage";

interface Props {
  recap: Recap;
  byId: Map<string, Game>;
  onClose: () => void;
}

const reducedMotion = () => typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;

/** Image de fond d'une page : image large du jeu, sinon sa jaquette (plus floutée), sinon une teinte. */
function backdrop(game: Game | undefined) {
  if (game?.heroUrl) return { url: game.heroUrl, kind: "hero" as const };
  if (game?.coverUrl) return { url: game.coverUrl, kind: "cover" as const };
  return { url: null, kind: "tint" as const };
}

function PageBody({ page, byId }: { page: StoryPage; byId: Map<string, Game> }) {
  const game = page.gameId ? byId.get(page.gameId) : undefined;
  return (
    <div className={`story__body story__body--${page.kind}`}>
      {page.logo && game && <Logo game={game} className="story__logo" />}
      <p className="story__eyebrow">{page.eyebrow}</p>
      {page.value && <p className="story__value">{page.value}</p>}
      <p className="story__title">{page.title}</p>
      {page.detail && <p className="story__detail">{page.detail}</p>}
      {page.items.length > 0 && (
        <ol className="story__items">
          {page.items.map((item, i) => {
            const g = byId.get(item.gameId);
            return (
              <li key={`${item.gameId}-${i}`}>
                {g ? <Cover game={g} className="story__cover" /> : <span className="story__cover story__cover--none" style={{ ["--art-tint" as string]: titleColor(item.label) }} />}
                <span className="story__item">
                  <span className="story__label">{item.label}</span>
                  {item.share > 0 && <span className="story__meter"><span style={{ width: `${Math.max(2, item.share * 100)}%` }} /></span>}
                </span>
                <span className="story__num">{item.value}</span>
              </li>
            );
          })}
        </ol>
      )}
      {page.bars.length > 0 && (
        <div className="story__bars" aria-hidden>
          {page.bars.map((b, i) => (
            <span key={i} className={b.active ? "is-on" : ""}>
              <i style={{ height: `${Math.max(3, b.share * 100)}%` }} />
              <b>{b.label}</b>
            </span>
          ))}
        </div>
      )}
      {page.achievement && <Rare rare={page.achievement} byId={byId} />}
    </div>
  );
}

function Rare({ rare, byId }: { rare: RareUnlock; byId: Map<string, Game> }) {
  const [failed, setFailed] = useState(false);
  return (
    <div className="story__rare">
      {rare.icon && !failed ? <img src={rare.icon} alt="" draggable={false} onError={() => setFailed(true)} /> : <span className="story__rare-icon" />}
      <span>
        <small>Le plus rare</small>
        <b>{rare.name}</b>
        <span>{rarityText(rare.percent)}</span>
        {byId.get(rare.gameId) && <span>{byId.get(rare.gameId)!.title}</span>}
      </span>
    </div>
  );
}

export function StoryView({ recap, byId, onClose }: Props) {
  const [rarest, setRarest] = useState<RareUnlock | null>(null);
  const [index, setIndex] = useState(0);
  const [paused, setPaused] = useState(false);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [failedArt, setFailedArt] = useState<Set<string>>(() => new Set());
  const reduced = useMemo(reducedMotion, []);
  const root = useRef<HTMLDivElement>(null);

  const pages = useMemo(
    () =>
      storyPages(recap, {
        title: (id) => byId.get(id)?.title ?? "Jeu retiré de la bibliothèque",
        known: (id) => byId.has(id),
        rarest,
      }),
    [recap, byId, rarest],
  );
  const count = pages.length;
  const current = pages[Math.min(index, count - 1)];

  // Succès le plus rare : lu à l'ouverture, depuis le cache des pourcentages seulement.
  useEffect(() => {
    if (recap.achievements === 0) return;
    let alive = true;
    recapRarest(recap.period, recap.back)
      .then((r) => alive && setRarest(r))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [recap.period, recap.back, recap.achievements]);

  const move = useCallback(
    (action: "next" | "prev") => {
      setMessage(null);
      const next = stepPage(index, count, action);
      if (next === null) onClose();
      else setIndex(next);
    },
    [index, count, onClose],
  );
  const moveRef = useRef(move);
  moveRef.current = move;
  const closeRef = useRef(onClose);
  closeRef.current = onClose;

  const save = useCallback(async () => {
    if (!current || saving) return;
    setSaving(true);
    setMessage(null);
    try {
      const png = await drawStoryPage(current, byId);
      const file = await saveStoryImage(png);
      if (file) setMessage(`Image enregistrée : ${file}`);
    } catch (e) {
      setMessage(`Enregistrement impossible : ${String(e)}`);
    } finally {
      setSaving(false);
    }
  }, [current, byId, saving]);

  // Clavier : toutes les touches arrêtées en phase de capture (aucun raccourci de la bibliothèque).
  useEffect(() => {
    const before = document.activeElement as HTMLElement | null;
    root.current?.focus({ preventScroll: true });
    const onKey = (e: KeyboardEvent) => {
      e.stopPropagation();
      if (e.altKey && e.key === "F4") return;
      const onButton = document.activeElement instanceof HTMLButtonElement && root.current?.contains(document.activeElement);
      if (e.key === "Tab") {
        // Focus gardé dans la story : boutons du bas seulement.
        e.preventDefault();
        const buttons = Array.from(root.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? []);
        if (!buttons.length) return;
        const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
        const next = at < 0 ? (e.shiftKey ? buttons.length - 1 : 0) : (at + (e.shiftKey ? -1 : 1) + buttons.length) % buttons.length;
        buttons[next].focus();
        return;
      }
      // Entrée sur un bouton qui a le focus : le bouton (Enregistrer, Fermer…).
      if (e.key === "Enter" && onButton) return;
      e.preventDefault();
      if (e.repeat && e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
      const action = storyAction(e.key);
      if (action === "close") closeRef.current();
      else if (action === "pause") setPaused((p) => !p);
      else if (action) moveRef.current(action);
    };
    window.addEventListener("keydown", onKey, true);
    document.documentElement.classList.add("has-story");
    return () => {
      window.removeEventListener("keydown", onKey, true);
      document.documentElement.classList.remove("has-story");
      before?.focus?.({ preventScroll: true });
    };
  }, []);

  if (!current) return null;
  const game = current.gameId ? byId.get(current.gameId) : undefined;
  const art = backdrop(game);
  const showArt = art.url && !failedArt.has(art.url);
  const artFailed = (url: string) => setFailedArt((s) => new Set(s).add(url));
  // Avance seule sauf mouvement réduit, pause, enregistrement en cours ou dernière page.
  const running = !reduced && !paused && !saving && index < count - 1;
  const tint = titleColor(game?.title ?? recap.start);

  return createPortal(
    <div
      ref={root}
      className={`story${reduced ? " is-reduced" : ""}`}
      role="dialog"
      aria-modal="true"
      aria-label="Story du récap"
      tabIndex={-1}
      style={{ ["--art-tint" as string]: tint, ["--story-ms" as string]: `${STORY_PAGE_MS}ms` }}
      onMouseDown={(e) => e.stopPropagation()}
      onPointerDown={(e) => e.stopPropagation()}
      onClick={(e) => e.stopPropagation()}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
      }}
    >
      <div className={`story__backdrop story__backdrop--${showArt ? art.kind : "tint"}`} aria-hidden>
        {showArt && <img key={art.url} src={art.url!} alt="" draggable={false} />}
      </div>

      <button type="button" className="story__side story__side--prev" aria-label="Page précédente" disabled={index === 0} onClick={() => move("prev")}>
        <ChevronLeftIcon />
      </button>

      <div className="story__frame" onClick={(e) => {
          const box = e.currentTarget.getBoundingClientRect();
          move(e.clientX - box.left < box.width * 0.3 ? "prev" : "next");
        }}>
        <div className={`story__art story__art--${showArt ? art.kind : "tint"}`} aria-hidden>
          {showArt && <img key={art.url} src={art.url!} alt="" draggable={false} onError={() => artFailed(art.url!)} />}
        </div>
        <div className="story__scrim" aria-hidden />
        <div className="story__progress" aria-hidden>
          {pages.map((p, i) => (
            <span key={p.kind} className={i < index ? "is-done" : i === index ? "is-current" : ""}>
              {i === index && (
                <i
                  key={`${index}-${reduced}`}
                  className={running ? "is-running" : reduced || index === count - 1 ? "is-full" : "is-paused"}
                  onAnimationEnd={() => move("next")}
                />
              )}
            </span>
          ))}
        </div>
        <div className="story__brand" aria-hidden>
          <LudexMark size={28} />
          <span>Ludex</span>
          <small>Récap</small>
        </div>
        <div className="story__page" key={index} aria-live="polite" aria-label={`Page ${index + 1} sur ${count}`}>
          <PageBody page={current} byId={byId} />
        </div>
      </div>

      <button type="button" className="story__side story__side--next" aria-label={index === count - 1 ? "Fermer la story" : "Page suivante"} onClick={() => move("next")}>
        <ChevronRightIcon />
      </button>

      <div className="story__tools">
        <span className="story__count">
          {index + 1} / {count}
        </span>
        {!reduced && (
          <button type="button" onClick={() => setPaused((p) => !p)} aria-pressed={paused}>
            {paused ? "Reprendre" : "Pause"}
          </button>
        )}
        <button type="button" onClick={save} disabled={saving}>
          {saving ? "Enregistrement…" : "Enregistrer en image"}
        </button>
        <button type="button" onClick={onClose}>
          Fermer
        </button>
      </div>
      <p className="story__message" role="status">
        {message ?? (reduced ? "Flèches ou clic pour changer de page, Échap pour fermer" : "Espace : pause · flèches : pages · Échap : fermer")}
      </p>
    </div>,
    document.body,
  );
}
