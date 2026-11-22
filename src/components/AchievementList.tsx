import { useEffect, useState } from "react";
import { gameAchievements, type Achievement, type Game } from "../lib/api";
import { formatDate, formatPercent, relativeTime } from "../lib/format";
import { t } from "../lib/i18n";
import { TrophyIcon } from "./Icons";

/** Icône d'un succès (CDN de Steam), coupe dessinée si elle manque ou ne charge pas. */
export function AchievementIcon({ src, className = "" }: { src: string | null; className?: string }) {
  const [failed, setFailed] = useState<string | null>(null);
  return (
    <span className={`ach-icon ${className}`}>
      {src && failed !== src ? (
        <img src={src} alt="" loading="lazy" draggable={false} onError={() => setFailed(src)} />
      ) : (
        <TrophyIcon />
      )}
    </span>
  );
}

/** Ligne d'un succès de la fiche. Caché et pas encore débloqué : rien ne le dévoile. */
function AchievementRow({ a }: { a: Achievement }) {
  const secret = a.hidden && !a.unlocked;
  const percent = a.globalPercent !== null ? t.fiche.achievements.players(formatPercent(a.globalPercent)) : null;
  return (
    <li className={`ach ${a.unlocked ? "is-unlocked" : ""}`}>
      <AchievementIcon src={a.unlocked ? a.icon : a.iconGray} />
      <span className="ach__text">
        <span className="ach__name">{secret ? t.fiche.achievements.hidden : a.name}</span>
        <span className="ach__desc">{secret ? t.fiche.achievements.hiddenDesc : a.description}</span>
      </span>
      <span className="ach__side">
        {percent && <span>{percent}</span>}
        {a.unlocked && (
          <span className="ach__when" title={a.unlockedAt ? formatDate(a.unlockedAt) : undefined}>
            {a.unlockedAt ? t.fiche.achievements.unlockedAgo(relativeTime(a.unlockedAt)) : t.fiche.achievements.unlocked}
          </span>
        )}
      </span>
    </li>
  );
}

type Load = { state: "idle" } | { state: "loading" } | { state: "ready"; list: Achievement[] } | { state: "error" };

/**
 * Liste complète des succès d'un jeu, repliée par défaut : lue à l'ouverture seulement (et relue
 * quand le nombre de succès débloqués change). Non débloqués les plus faciles d'abord, puis débloqués.
 * `expanded` : ouverte d'emblée, sans bouton (onglet « Succès » de la fiche, lu à son ouverture).
 */
export function AchievementList({ game, expanded = false }: { game: Game; expanded?: boolean }) {
  const [open, setOpen] = useState(expanded);
  const [load, setLoad] = useState<Load>({ state: "idle" });
  const unlocked = game.achievements?.unlocked ?? 0;
  const total = game.achievements?.total ?? 0;

  useEffect(() => {
    if (!open) return;
    let alive = true;
    setLoad((l) => (l.state === "ready" ? l : { state: "loading" }));
    gameAchievements(game.id)
      .then((list) => alive && setLoad({ state: "ready", list }))
      .catch(() => alive && setLoad({ state: "error" }));
    return () => {
      alive = false;
    };
  }, [open, game.id, unlocked]);

  if (!game.achievements || total === 0) return null;
  return (
    <div className="achs">
      {!expanded && (
        <button type="button" className="achs__toggle" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
          <TrophyIcon />
          {open ? t.fiche.achievements.hide : t.fiche.achievements.show(total)}
        </button>
      )}
      {open && load.state === "loading" && <p className="achs__note">{t.fiche.achievements.loading}</p>}
      {open && load.state === "error" && <p className="achs__note">{t.fiche.achievements.error}</p>}
      {open && load.state === "ready" && load.list.length === 0 && <p className="achs__note">{t.fiche.achievements.empty}</p>}
      {open && load.state === "ready" && load.list.length > 0 && (
        <ol className="achs__list" aria-label={t.fiche.achievements.label(game.title)}>
          {load.list.map((a) => <AchievementRow key={a.apiName} a={a} />)}
        </ol>
      )}
    </div>
  );
}
