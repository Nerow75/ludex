// Textes de la liste de souhaits (section « Souhaits ») : prix, promos, résumé, toast.
import type { WishHit, WishItem, WishPrice, WishSale } from "./api";
import { relativeTime } from "./format";
import { lang, locale, nf, t } from "./i18n";

const formats = new Map<string, Intl.NumberFormat>();

/**
 * « 7,99 € » (« €7.99 » en anglais) : montant écrit dans la langue de l'interface, monnaie telle que
 * Rust la donne ; monnaie inconnue de `Intl` : « 7,99 XYZ ».
 */
export function formatPrice(cents: number, currency: string): string {
  const key = `${locale()}|${currency}`;
  let format = formats.get(key);
  if (!format) {
    try {
      format = new Intl.NumberFormat(locale(), { style: "currency", currency });
    } catch {
      const amount = (cents / 100).toFixed(2);
      return `${lang() === "fr" ? amount.replace(".", ",") : nf({ minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(cents / 100)} ${currency}`;
    }
    formats.set(key, format);
  }
  return format.format(cents / 100);
}

export const onSale = (p: WishPrice | null): boolean => !!p && p.discount > 0 && p.final < p.initial;

/** Ce que la ligne d'un jeu affiche à droite : prix, remise, prix d'origine barré, ou une note. */
export interface PriceView {
  now: string | null;
  /** « -80 % », seulement en promo. */
  discount: string | null;
  /** Prix d'origine, barré, seulement en promo. */
  was: string | null;
  /** Texte à la place du prix (gratuit, indisponible, inconnu). */
  note: string | null;
}

export function priceView(item: Pick<WishItem, "price" | "offer">): PriceView {
  const p = item.price;
  if (item.offer === "price" && p) {
    const sale = onSale(p);
    return {
      now: formatPrice(p.final, p.currency),
      discount: sale ? t.library.wishlist.discount(p.discount) : null,
      was: sale ? formatPrice(p.initial, p.currency) : null,
      note: null,
    };
  }
  const w = t.library.wishlist;
  const note = item.offer === "noPrice" ? w.noPrice : item.offer === "unavailable" ? w.unavailable : w.unknownPrice;
  return { now: null, discount: null, was: null, note };
}

/** Prix d'un résultat de recherche : « 7,99 € (-80 %) », ou « Gratuit ou sans prix ». */
export function hitPrice(hit: Pick<WishHit, "price">): string {
  const p = hit.price;
  if (!p) return t.library.wishlist.hitNoPrice;
  return onSale(p) ? `${formatPrice(p.final, p.currency)} (${t.library.wishlist.discount(p.discount)})` : formatPrice(p.final, p.currency);
}

/** « Plus bas vu : 4,99 € » (seulement s'il est sous le prix actuel, ce que Rust a déjà vérifié). */
export function lowestText(item: Pick<WishItem, "lowest">): string | null {
  return item.lowest ? t.library.wishlist.lowest(formatPrice(item.lowest.final, item.lowest.currency)) : null;
}

/** « Ajouté il y a 3 jours, à 24,50 € » (prix d'alors s'il diffère d'aujourd'hui). */
export function addedText(item: Pick<WishItem, "added" | "addedPrice" | "price">): string {
  const w = t.library.wishlist;
  const when = item.added > 0 ? w.addedWhen(relativeTime(item.added)) : w.addedOn;
  const then = item.addedPrice;
  if (!then || (item.price && item.price.final === then.final && item.price.currency === then.currency)) return when;
  return w.addedAt(when, formatPrice(then.final, then.currency));
}

/** « 12 jeux, dont 3 en promo », « 1 jeu ». */
export function wishSummary(items: readonly Pick<WishItem, "price">[]): string {
  const n = items.length;
  const sales = items.filter((i) => onSale(i.price)).length;
  const base = t.library.wishlist.summary(n);
  return sales > 0 ? t.library.wishlist.summarySales(base, sales) : base;
}

/** « Prix relus il y a 2 h » ; jamais relus en entier : explication de la relecture quotidienne. */
export function checkedText(checked: number | null): string {
  return checked ? t.library.wishlist.checked(relativeTime(checked)) : t.library.wishlist.neverChecked;
}

/** Nom complet d'une ligne pour les lecteurs d'écran. */
export function wishLabel(item: WishItem): string {
  const p = priceView(item);
  const parts = [item.title];
  if (p.now) parts.push(p.discount && p.was ? t.library.wishlist.instead(p.now, p.discount, p.was) : p.now);
  if (p.note) parts.push(p.note);
  const lowest = lowestText(item);
  if (lowest) parts.push(lowest);
  if (item.owned) parts.push(t.library.wishlist.ownedLabel);
  return parts.join(", ");
}

/** Toast des promos nouvelles (mêmes textes que la notification Windows de `notify.rs`). */
export function saleToast(sales: readonly WishSale[]): { title: string; detail: string } | null {
  if (sales.length === 0) return null;
  const w = t.library.wishlist;
  if (sales.length === 1) {
    const s = sales[0];
    return {
      title: w.saleTitle(s.title, s.price.discount),
      detail: w.saleDetail(formatPrice(s.price.final, s.price.currency), formatPrice(s.price.initial, s.price.currency)),
    };
  }
  const list = sales.slice(0, 3).map((s) => w.saleItem(s.title, s.price.discount));
  const rest = sales.length - 3;
  return {
    title: w.salesTitle(sales.length),
    detail: `${list.join(", ")}${rest > 0 ? w.salesMore(rest) : ""}.`,
  };
}
