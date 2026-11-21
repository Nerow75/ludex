// Textes de la liste de souhaits (section « Souhaits ») : prix, promos, résumé, toast.
import type { WishHit, WishItem, WishPrice, WishSale } from "./api";
import { relativeTime } from "./format";

const formats = new Map<string, Intl.NumberFormat>();

/** « 7,99 € » ; monnaie inconnue de `Intl` : « 7,99 XYZ ». */
export function formatPrice(cents: number, currency: string): string {
  let format = formats.get(currency);
  if (!format) {
    try {
      format = new Intl.NumberFormat("fr-FR", { style: "currency", currency });
    } catch {
      return `${(cents / 100).toFixed(2).replace(".", ",")} ${currency}`;
    }
    formats.set(currency, format);
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
      discount: sale ? `-${p.discount} %` : null,
      was: sale ? formatPrice(p.initial, p.currency) : null,
      note: null,
    };
  }
  const note = item.offer === "noPrice" ? "Gratuit ou pas encore en vente" : item.offer === "unavailable" ? "Pas en vente en France" : "Prix pas encore lu";
  return { now: null, discount: null, was: null, note };
}

/** Prix d'un résultat de recherche : « 7,99 € (-80 %) », ou « Gratuit ou sans prix ». */
export function hitPrice(hit: Pick<WishHit, "price">): string {
  const p = hit.price;
  if (!p) return "Gratuit ou sans prix";
  return onSale(p) ? `${formatPrice(p.final, p.currency)} (-${p.discount} %)` : formatPrice(p.final, p.currency);
}

/** « Plus bas vu : 4,99 € » (seulement s'il est sous le prix actuel, ce que Rust a déjà vérifié). */
export function lowestText(item: Pick<WishItem, "lowest">): string | null {
  return item.lowest ? `Plus bas vu : ${formatPrice(item.lowest.final, item.lowest.currency)}` : null;
}

/** « Ajouté il y a 3 jours, à 24,50 € » (prix d'alors s'il diffère d'aujourd'hui). */
export function addedText(item: Pick<WishItem, "added" | "addedPrice" | "price">): string {
  const when = item.added > 0 ? `Ajouté ${relativeTime(item.added)}` : "Ajouté";
  const then = item.addedPrice;
  if (!then || (item.price && item.price.final === then.final && item.price.currency === then.currency)) return when;
  return `${when}, à ${formatPrice(then.final, then.currency)}`;
}

/** « 12 jeux, dont 3 en promo », « 1 jeu ». */
export function wishSummary(items: readonly Pick<WishItem, "price">[]): string {
  const n = items.length;
  const sales = items.filter((i) => onSale(i.price)).length;
  const base = `${n} jeu${n > 1 ? "x" : ""}`;
  return sales > 0 ? `${base}, dont ${sales} en promo` : base;
}

/** « Prix relus il y a 2 h » ; jamais relus en entier : explication de la relecture quotidienne. */
export function checkedText(checked: number | null): string {
  return checked ? `Prix relus ${relativeTime(checked)}, une fois par jour.` : "Prix relus une fois par jour, au premier scan de la journée.";
}

/** Nom complet d'une ligne pour les lecteurs d'écran. */
export function wishLabel(item: WishItem): string {
  const p = priceView(item);
  const parts = [item.title];
  if (p.now) parts.push(p.discount ? `${p.now}, ${p.discount}, au lieu de ${p.was}` : p.now);
  if (p.note) parts.push(p.note);
  const lowest = lowestText(item);
  if (lowest) parts.push(lowest);
  if (item.owned) parts.push("déjà à toi");
  return parts.join(", ");
}

/** Toast des promos nouvelles (mêmes textes que la notification Windows de `notify.rs`). */
export function saleToast(sales: readonly WishSale[]): { title: string; detail: string } | null {
  if (sales.length === 0) return null;
  if (sales.length === 1) {
    const s = sales[0];
    return {
      title: `${s.title} en promo : -${s.price.discount} %`,
      detail: `${formatPrice(s.price.final, s.price.currency)} au lieu de ${formatPrice(s.price.initial, s.price.currency)}.`,
    };
  }
  const list = sales.slice(0, 3).map((s) => `${s.title} -${s.price.discount} %`);
  const rest = sales.length - 3;
  return {
    title: `${sales.length} jeux de ta liste en promo`,
    detail: `${list.join(", ")}${rest > 0 ? ` et ${rest} autre${rest > 1 ? "s" : ""}` : ""}.`,
  };
}
