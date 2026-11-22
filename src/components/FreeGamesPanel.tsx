import { useEffect, useState } from "react";
import { epicFreeGames, onEpicFreeChanged, openEpicFree, type FreeGames, type FreeOffer } from "../lib/api";
import { freeChecked, offerLabel, offerMeta } from "../lib/freeGames";
import { t } from "../lib/i18n";
import type { SectionContext } from "../lib/sections";

/** Vignette d'Epic ; cadre vide si elle ne se charge pas. */
function Thumb({ url }: { url: string | null }) {
  const [failed, setFailed] = useState(false);
  if (!url || failed) return <span className="freebie__img" aria-hidden="true" />;
  return <img className="freebie__img" src={url} alt="" loading="lazy" decoding="async" onError={() => setFailed(true)} />;
}

/**
 * Jeux gratuits de la semaine sur l'Epic Games Store, en tête de la section Souhaits : liste lue
 * par Rust (aucun compte), page ouverte dans le launcher Epic. Caché s'il n'y a rien à montrer.
 */
export function FreeGamesPanel({ ctx }: { ctx: SectionContext }) {
  const [free, setFree] = useState<FreeGames | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Les jeux de la bibliothèque changent : « Déjà à toi » relu.
  const ids = ctx.games.length;

  useEffect(() => {
    let alive = true;
    epicFreeGames()
      .then((f) => alive && setFree(f))
      .catch((e) => alive && setError(String(e)));
    const off = onEpicFreeChanged((f) => setFree(f));
    return () => {
      alive = false;
      off.then((stop) => stop()).catch(() => {});
    };
  }, [ids]);

  const f = t.library.freeGames;
  const items = free?.items ?? [];
  const open = (offer: FreeOffer) => {
    openEpicFree(offer.id).catch((err) => setError(String(err)));
  };
  if (!free || (items.length === 0 && !free.loading && !error && free.checked)) return null;

  return (
    <section className="freebies" aria-labelledby="freebies-title">
      <div className="freebies__head">
        <h3 id="freebies-title">{f.title}</h3>
        <p className="wishes__note">
          {f.help} {freeChecked(free.checked)}
        </p>
      </div>
      {error && <p className="wishes__msg" role="alert">{error}</p>}
      {items.length === 0 && (
        <p className="wishes__note" role="status">
          {free.loading ? f.loading : free.checked ? "" : f.unreachable}
        </p>
      )}
      {items.length > 0 && (
        <ul className="freebies__list">
          {items.map((o) => {
            const game = o.owned ? ctx.games.find((g) => g.id === o.owned) : undefined;
            return (
              <li key={`${o.id}-${o.start}`} className={o.current ? "freebie is-now" : "freebie"} role="group" aria-label={offerLabel(o)}>
                <Thumb url={o.imageUrl} />
                <span className="freebie__badge" aria-hidden="true">{o.current ? f.now : f.soon}</span>
                <div className="freebie__text" aria-hidden="true">
                  <span className="wish__title">{o.title}</span>
                  <span className="wish__meta">{offerMeta(o)}</span>
                </div>
                <div className="freebie__actions">
                  {o.owned &&
                    (game ? (
                      <button type="button" className="wish__owned is-link" data-nav onClick={() => ctx.openGame(game)} aria-label={t.library.wishlist.ownedOpen(game.title)}>
                        {t.library.wishlist.owned}
                      </button>
                    ) : (
                      <span className="wish__owned">{t.library.wishlist.owned}</span>
                    ))}
                  <button type="button" className="btn-ghost" data-nav onClick={() => open(o)} aria-label={f.openLabel(o.title)}>
                    {f.open}
                  </button>
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
