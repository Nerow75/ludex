// Textes des jeux gratuits d'Epic (bloc « Gratuits sur Epic » de la section Souhaits) et du toast
// d'une nouvelle fournée.
import type { FreeOffer } from "./api";
import { relativeTime } from "./format";
import { locale, t } from "./i18n";

const formats = new Map<string, Intl.DateTimeFormat>();
function df(options: Intl.DateTimeFormatOptions): Intl.DateTimeFormat {
  const key = `${locale()}|${JSON.stringify(options)}`;
  let format = formats.get(key);
  if (!format) {
    format = new Intl.DateTimeFormat(locale(), options);
    formats.set(key, format);
  }
  return format;
}

/** « 26 novembre » (« November 26 »). */
export const freeDay = (ts: number): string => df({ day: "numeric", month: "long" }).format(new Date(ts * 1000));

/** « jeudi 26 novembre, 17:00 » : fin d'une offre en cours, heure comprise (Epic change le jeudi). */
export const freeDeadline = (ts: number): string => {
  const date = new Date(ts * 1000);
  return `${df({ weekday: "long", day: "numeric", month: "long" }).format(date)}, ${df({ hour: "2-digit", minute: "2-digit" }).format(date)}`;
};

/** Ligne sous le titre : « Gratuit jusqu'au … » ou « Gratuit du 26 novembre au 3 décembre ». */
export function offerMeta(offer: Pick<FreeOffer, "current" | "start" | "end">): string {
  const f = t.library.freeGames;
  return offer.current ? f.until(freeDeadline(offer.end)) : f.from(freeDay(offer.start), freeDay(offer.end));
}

/** Nom complet d'une carte pour les lecteurs d'écran. */
export function offerLabel(offer: FreeOffer): string {
  const f = t.library.freeGames;
  const parts = [offer.title, offer.current ? f.now : f.soon, offerMeta(offer)];
  if (offer.owned) parts.push(f.ownedLabel);
  return parts.join(", ");
}

/** « Liste relue il y a 2 h. », vide si jamais lue. */
export const freeChecked = (checked: number | null): string => (checked ? t.library.freeGames.checked(relativeTime(checked)) : "");

/** Toast d'une nouvelle fournée (mêmes titres que la notification Windows de `notify.rs`). */
export function freeToast(titles: readonly string[]): { title: string; detail: string } | null {
  if (titles.length === 0) return null;
  const f = t.library.freeGames;
  if (titles.length === 1) return { title: f.toastOne(titles[0]), detail: f.toastDetail };
  return { title: f.toastMany(titles.length), detail: `${titles.slice(0, 3).join(", ")}${titles.length > 3 ? t.library.wishlist.salesMore(titles.length - 3) : ""}. ${f.toastDetail}` };
}
