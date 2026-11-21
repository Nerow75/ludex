import { useEffect, useState } from "react";
import { gameNews, openNews, type Game, type News, type NewsBlock, type NewsSpan } from "../lib/api";
import { formatDateTime, relativeTime } from "../lib/format";
import { t } from "../lib/i18n";
import { isNewNews } from "../lib/news";
import { ChevronRightIcon, ExternalIcon } from "./Icons";

/**
 * Annonces officielles du jeu (Steam), lues à l'ouverture de la fiche (cache de 6 h côté Rust).
 * `null` : lecture ; liste vide : pas d'appid, pas d'annonce ou Steam injoignable (onglet caché).
 */
export function useNews(id: string): News[] | null {
  const [news, setNews] = useState<{ id: string; list: News[] } | null>(null);
  useEffect(() => {
    let alive = true;
    gameNews(id, false)
      .then((list) => alive && setNews({ id, list }))
      .catch(() => alive && setNews({ id, list: [] }));
    return () => {
      alive = false;
    };
  }, [id]);
  return news?.id === id ? news.list : null;
}

/** Texte d'un bloc : gras, italique, souligné, et liens Steam ouverts par Rust (numéro seulement). */
function Spans({ spans, open }: { spans: NewsSpan[]; open: (link: number) => void }) {
  return (
    <>
      {spans.map((s, i) => {
        const cls = [s.bold && "is-bold", s.italic && "is-italic", s.underline && "is-underline"].filter(Boolean).join(" ") || undefined;
        if (s.link === undefined) return <span key={i} className={cls}>{s.text}</span>;
        const link = s.link;
        return (
          <button key={i} type="button" className={`news__link${cls ? ` ${cls}` : ""}`} data-nav title={t.fiche.news.openOnSteam} onClick={() => open(link)}>
            {s.text}
          </button>
        );
      })}
    </>
  );
}

function Block({ block, open }: { block: NewsBlock; open: (link: number) => void }) {
  switch (block.kind) {
    case "heading": {
      const H = block.level === 1 ? "h4" : block.level === 2 ? "h5" : "h6";
      return <H className={`news__h news__h--${block.level}`}><Spans spans={block.spans} open={open} /></H>;
    }
    case "paragraph":
      return <p className="news__p"><Spans spans={block.spans} open={open} /></p>;
    case "list": {
      const L = block.ordered ? "ol" : "ul";
      return (
        <L className="news__list">
          {block.items.map((item, i) => <li key={i}><Spans spans={item} open={open} /></li>)}
        </L>
      );
    }
    case "image":
      return <img className="news__img" src={block.url} alt="" loading="lazy" referrerPolicy="no-referrer" />;
    case "rule":
      return <hr className="news__rule" />;
  }
}

/** Une annonce : en-tête cliquable (image, titre, date, extrait), contenu complet une fois dépliée. */
function NewsItem({ game, news, open, onToggle }: { game: Game; news: News; open: boolean; onToggle: () => void }) {
  const [error, setError] = useState<string | null>(null);
  const fresh = isNewNews(news, game.lastPlayed);
  const go = (link: number | null) => {
    setError(null);
    openNews(game.id, news.gid, link).catch((e) => setError(String(e)));
  };
  const bodyId = `news-${news.gid}`;
  return (
    <li className={`news__item${open ? " is-open" : ""}`}>
      <button type="button" className="news__head" data-nav aria-expanded={open} aria-controls={bodyId} onClick={onToggle}>
        {news.image && !open ? <img className="news__thumb" src={news.image} alt="" loading="lazy" referrerPolicy="no-referrer" /> : null}
        <span className="news__text">
          <span className="news__title">{news.title}</span>
          <span className="news__meta">
            <span title={formatDateTime(news.date)}>{relativeTime(news.date)}</span>
            {news.update && <span>{t.fiche.news.patchNotes}</span>}
            {fresh && <span className="news__new">{t.fiche.news.fresh}</span>}
          </span>
          {!open && news.excerpt && <span className="news__excerpt">{news.excerpt}</span>}
        </span>
        <span className="news__chev" aria-hidden="true"><ChevronRightIcon /></span>
      </button>
      {open && (
        <div className="news__body" id={bodyId}>
          {news.blocks.map((b, i) => <Block key={i} block={b} open={(link) => go(link)} />)}
          <div className="news__foot">
            {news.author && <span className="news__author">{news.author}</span>}
            <button type="button" className="btn-ghost news__steam" data-nav onClick={() => go(null)}>
              <ExternalIcon /> {t.fiche.news.readOnSteam}
            </button>
          </div>
          {error && <p className="news__error" role="alert">{error}</p>}
        </div>
      )}
    </li>
  );
}

/**
 * Onglet « Actualités » : annonces officielles du studio, la plus récente d'abord, dépliées une à la
 * fois. À l'affichage, Rust note les annonces comme vues (plus de notification pour elles).
 */
export function NewsTab({ game, news }: { game: Game; news: News[] | null }) {
  const [list, setList] = useState(news);
  const [open, setOpen] = useState<string | null>(null);
  useEffect(() => setList(news), [news]);
  useEffect(() => {
    let alive = true;
    gameNews(game.id, true)
      .then((l) => alive && l.length > 0 && setList(l))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [game.id]);
  if (!list) return <p className="fiche__note">{t.fiche.news.loading}</p>;
  if (list.length === 0) return <p className="fiche__note">{t.fiche.news.empty}</p>;
  return (
    <>
      <p className="fiche__note">{t.fiche.news.intro}</p>
      <ol className="news" aria-label={t.fiche.news.label(game.title)}>
        {list.map((n) => (
          <NewsItem key={n.gid} game={game} news={n} open={open === n.gid} onToggle={() => setOpen((o) => (o === n.gid ? null : n.gid))} />
        ))}
      </ol>
    </>
  );
}
