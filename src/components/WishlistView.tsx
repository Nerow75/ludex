import { useEffect, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import {
  onWishlistChanged,
  openStorePage,
  wishlist as readWishlist,
  wishlistAdd,
  wishlistRemove,
  wishlistSearch,
  type WishHit,
  type WishItem,
  type Wishlist,
} from "../lib/api";
import { t } from "../lib/i18n";
import type { SectionContext } from "../lib/sections";
import { addedText, checkedText, hitPrice, lowestText, priceView, wishLabel, wishSummary } from "../lib/wishlist";
import { SearchIcon } from "./Icons";
import { EmptyState } from "./States";

/** Image de la boutique ; cadre vide si elle ne se charge pas. */
function Header({ url, small }: { url: string; small?: boolean }) {
  const [failed, setFailed] = useState(false);
  const cls = small ? "wish__img wish__img--small" : "wish__img";
  if (failed) return <span className={cls} aria-hidden="true" />;
  return <img className={cls} src={url} alt="" loading="lazy" decoding="async" onError={() => setFailed(true)} />;
}

/** Bloc « Déjà à toi » : ouvre la fiche du jeu de la bibliothèque. */
function Owned({ id, ctx }: { id: string; ctx: SectionContext }) {
  const game = ctx.games.find((g) => g.id === id);
  const w = t.library.wishlist;
  if (!game) return <span className="wish__owned">{w.owned}</span>;
  return (
    <button type="button" className="wish__owned is-link" data-nav onClick={() => ctx.openGame(game)} aria-label={w.ownedOpen(game.title)}>
      {w.owned}
    </button>
  );
}

/**
 * Section « Souhaits » : liste de souhaits locale (aucun compte Steam). Recherche dans la boutique
 * Steam et ajout sur place, prix relus par Ludex une fois par jour, promos signalées.
 */
export function WishlistView(ctx: SectionContext) {
  const [list, setList] = useState<Wishlist | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<WishHit[] | null>(null);
  const [searching, setSearching] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  const field = useRef<HTMLInputElement>(null);
  const root = useRef<HTMLElement>(null);
  // Les jeux de la bibliothèque changent : « Déjà à toi » relu.
  const ids = ctx.games.length;

  useEffect(() => {
    let alive = true;
    readWishlist()
      .then((l) => alive && setList(l))
      .catch((e) => alive && setError(String(e)));
    const off = onWishlistChanged((l) => setList(l));
    return () => {
      alive = false;
      off.then((stop) => stop()).catch(() => {});
    };
  }, [ids]);

  const wished = new Set(list?.items.map((i) => i.appid) ?? []);

  const search = async (e: FormEvent) => {
    e.preventDefault();
    if (!query.trim() || searching) return;
    setSearching(true);
    setSearchError(null);
    try {
      setHits(await wishlistSearch(query));
    } catch (err) {
      setHits(null);
      setSearchError(String(err));
    } finally {
      setSearching(false);
    }
  };

  const add = async (hit: WishHit) => {
    try {
      setList(await wishlistAdd(hit.appid));
      setError(null);
      ctx.showUndo(t.library.wishlist.added(hit.title), () => void remove(hit.appid, hit.title, false));
    } catch (err) {
      setError(String(err));
    }
  };

  async function remove(appid: number, title: string, undo = true) {
    try {
      setList(await wishlistRemove(appid));
      setError(null);
      if (undo) {
        ctx.showUndo(t.library.wishlist.removed(title), () => {
          wishlistAdd(appid)
            .then((l) => setList(l))
            .catch((err) => setError(String(err)));
        });
      }
    } catch (err) {
      setError(String(err));
    }
  }

  const store = (appid: number) => {
    openStorePage(appid).catch((err) => setError(String(err)));
  };

  // Flèche bas depuis le champ : premier élément sous lui ; Échap vide la recherche.
  const onFieldKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown") {
      const all = [...(root.current?.querySelectorAll<HTMLElement>("[data-nav]") ?? [])].filter((el) => !(el as HTMLButtonElement).disabled);
      const next = all[all.indexOf(e.currentTarget) + 1];
      if (next) {
        e.preventDefault();
        e.stopPropagation();
        next.focus();
      }
    } else if (e.key === "Escape" && (query || hits)) {
      e.preventDefault();
      e.stopPropagation();
      setQuery("");
      setHits(null);
      setSearchError(null);
    }
  };

  const items = list?.items ?? [];
  const w = t.library.wishlist;
  return (
    <section className="wishes" aria-label={w.title} ref={root}>
      <div className="bar">
        <h2>
          {w.title}
          {list && (
            <span>
              <span className="sr-only">, </span>
              {items.length}
            </span>
          )}
        </h2>
      </div>

      <form className="wishes__search" role="search" onSubmit={search}>
        <label className="search wishes__field">
          <SearchIcon />
          <span className="sr-only">{w.search}</span>
          <input
            ref={field}
            data-nav
            type="search"
            value={query}
            maxLength={100}
            placeholder={w.search}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onFieldKey}
          />
        </label>
        <button type="submit" className="btn-ghost" data-nav disabled={searching || !query.trim()}>
          {searching ? w.searching : w.searchButton}
        </button>
      </form>

      {searchError && <p className="wishes__msg" role="alert">{searchError}</p>}
      {hits && (
        <div className="wishes__hits">
          <p className="wishes__note" role="status">
            {hits.length === 0 ? w.noHit : w.hits(hits.length)}
          </p>
          {hits.length > 0 && (
            <ul className="wishes__list">
              {hits.map((h) => {
                const inList = h.wished || wished.has(h.appid);
                return (
                  <li key={h.appid} className="wish wish--hit">
                    <Header url={h.imageUrl} small />
                    <div className="wish__text">
                      <span className="wish__title">{h.title}</span>
                      <span className="wish__meta">{hitPrice(h)}</span>
                    </div>
                    {h.owned && <Owned id={h.owned} ctx={ctx} />}
                    <button type="button" className="btn-ghost" data-nav onClick={() => store(h.appid)} aria-label={w.viewInSteamLabel(h.title)}>
                      {w.viewInSteam}
                    </button>
                    <button type="button" className="btn-ghost" data-nav disabled={inList} onClick={() => add(h)} aria-label={inList ? w.inListLabel(h.title) : w.addLabel(h.title)}>
                      {inList ? w.inList : w.add}
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      )}

      {error && <p className="wishes__msg" role="alert">{error}</p>}
      {list && items.length > 0 && (
        <p className="wishes__note">
          {wishSummary(items)}. {checkedText(list.checked)}
        </p>
      )}
      {list && items.length === 0 && !hits && (
        <EmptyState title={w.emptyTitle}>{w.emptyBody}</EmptyState>
      )}
      {items.length > 0 && (
        <ul className="wishes__list" aria-label={w.yours}>
          {items.map((item) => (
            <WishRow key={item.appid} item={item} ctx={ctx} onStore={store} onRemove={remove} />
          ))}
        </ul>
      )}
    </section>
  );
}

function WishRow({ item, ctx, onStore, onRemove }: { item: WishItem; ctx: SectionContext; onStore: (appid: number) => void; onRemove: (appid: number, title: string) => void }) {
  const p = priceView(item);
  const lowest = lowestText(item);
  const w = t.library.wishlist;
  return (
    <li className={p.discount ? "wish is-sale" : "wish"}>
      <div className="wish__main" role="group" aria-label={wishLabel(item)}>
        <Header url={item.imageUrl} />
        <div className="wish__text">
          <span className="wish__title">{item.title}</span>
          <span className="wish__meta">
            {addedText(item)}
            {lowest && ` · ${lowest}`}
          </span>
        </div>
        <div className="wish__price" aria-hidden="true">
          {p.discount && <span className="wish__discount">{p.discount}</span>}
          {p.now && (
            <span className="wish__amounts">
              {p.was && <s className="wish__was">{p.was}</s>}
              <span className="wish__now">{p.now}</span>
            </span>
          )}
          {p.note && <span className="wish__note">{p.note}</span>}
        </div>
      </div>
      <div className="wish__actions">
        {item.owned && <Owned id={item.owned} ctx={ctx} />}
        <button type="button" className="btn-ghost" data-nav onClick={() => onStore(item.appid)} aria-label={w.viewInSteamLabel(item.title)}>
          {w.viewInSteam}
        </button>
        <button type="button" className="btn-ghost" data-nav onClick={() => onRemove(item.appid, item.title)} aria-label={w.removeLabel(item.title)}>
          {w.remove}
        </button>
      </div>
    </li>
  );
}
