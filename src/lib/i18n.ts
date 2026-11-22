// Langue de l'interface : français (langue de référence, par défaut) ou anglais.
//
// Dictionnaires typés dans `src/locales/<langue>/<espace>.ts` : `t.<espace>.<clé>` donne une
// chaîne, ou une fonction pour un texte qui prend des valeurs (`t.common.gameCount(3)`). Le
// dictionnaire anglais a exactement les mêmes clés que le français (vérifié par TypeScript et par
// `i18n.test.ts`).
//
// `t` est une liaison vivante : `setLang` la remplace et les modules qui l'importent voient le
// nouveau dictionnaire. Lire `t` au moment du rendu ou de l'appel, jamais au chargement d'un module
// (une constante calculée au chargement resterait dans la langue du démarrage) : pour une table de
// libellés, `live(() => …)`. Les composants qui doivent se redessiner au changement de langue
// appellent `useLang()` (l'`App` et la recherche rapide le font : leurs enfants suivent, sauf ceux
// en `memo`, qui l'appellent aussi).
//
// Module pur (aucun appel à Rust) : le réglage est suivi par `language.ts`.
import { useSyncExternalStore } from "react";
import { en } from "../locales/en";
import { fr, type Messages } from "../locales/fr";

export type { Messages };

/** Langue effective de l'interface. */
export type Lang = "fr" | "en";
export const LANGS: readonly Lang[] = ["fr", "en"];

/** Réglage `language` (`system` : la langue d'affichage de Windows, résolue côté Rust). */
export type LanguageSetting = "system" | "fr" | "en";
export const LANGUAGE_SETTINGS: readonly LanguageSetting[] = ["system", "fr", "en"];

const DICTS: Record<Lang, Messages> = { fr, en };
const LOCALES: Record<Lang, string> = { fr: "fr-FR", en: "en-US" };

let current: Lang = "fr";

/** Dictionnaire de la langue actuelle (liaison vivante, voir l'en-tête). */
export let t: Messages = fr;

const listeners = new Set<() => void>();

/** Langue actuelle. */
export function lang(): Lang {
  return current;
}

/** Locale de `Intl` et `toLocale…` de la langue actuelle (« fr-FR », « en-US »). */
export function locale(): string {
  return LOCALES[current];
}

/** Langue relue (localStorage, réponse de Rust, version future…) : une langue connue, sinon le français. */
export function knownLang(value: unknown): Lang {
  return value === "en" ? "en" : "fr";
}

/** Réglage relu : un réglage connu, sinon `system`. */
export function knownLanguageSetting(value: unknown): LanguageSetting {
  return typeof value === "string" && (LANGUAGE_SETTINGS as readonly string[]).includes(value) ? (value as LanguageSetting) : "system";
}

/** Langue d'un navigateur (`navigator.language`), à défaut de réponse de Rust : français s'il l'est, anglais sinon. */
export function langOf(tag: string | undefined | null): Lang {
  return tag && tag.toLowerCase().startsWith("fr") ? "fr" : "en";
}

/** Change la langue de toute l'interface (sans effet si c'est déjà elle). */
export function setLang(next: Lang) {
  if (next === current) return;
  current = next;
  t = DICTS[next];
  if (typeof document !== "undefined") document.documentElement.lang = next;
  for (const listener of listeners) listener();
}

/** Abonnement aux changements de langue (hors React). Renvoie la fonction de désabonnement. */
export function onLangChange(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Langue actuelle, et nouveau rendu du composant quand elle change. */
export function useLang(): Lang {
  return useSyncExternalStore(onLangChange, lang, lang);
}

const numberFormats = new Map<string, Intl.NumberFormat>();

/** `Intl.NumberFormat` de la langue actuelle, gardé par locale et options. */
export function nf(options: Intl.NumberFormatOptions = {}): Intl.NumberFormat {
  const key = `${locale()}|${JSON.stringify(options)}`;
  let format = numberFormats.get(key);
  if (!format) {
    format = new Intl.NumberFormat(locale(), options);
    numberFormats.set(key, format);
  }
  return format;
}

/** Nombre écrit dans la langue actuelle : « 1 240 » en français, « 1,240 » en anglais. */
export function formatNumber(n: number, options?: Intl.NumberFormatOptions): string {
  return nf(options).format(n);
}

/**
 * Accord simple selon la langue : `plural(n, "jeu", "jeux")`. Français : singulier pour 0 et 1 ;
 * anglais : singulier pour 1 seulement (`Intl.PluralRules`).
 */
export function plural(n: number, one: string, other: string): string {
  return new Intl.PluralRules(locale()).select(n) === "one" ? one : other;
}

/**
 * Table de libellés relue dans la langue actuelle à chaque accès : `live(() => t.common.status)`
 * s'utilise comme l'objet lui-même (`STATUS_LABEL[s]`, `Object.entries(…)`), sans figer la langue
 * du chargement.
 */
export function live<T extends object>(source: () => T): T {
  return new Proxy({} as T, {
    get: (_, key) => Reflect.get(source(), key),
    has: (_, key) => Reflect.has(source(), key),
    ownKeys: () => Reflect.ownKeys(source()),
    getOwnPropertyDescriptor: (_, key) => {
      const desc = Reflect.getOwnPropertyDescriptor(source(), key);
      return desc ? { ...desc, configurable: true } : undefined;
    },
  });
}
