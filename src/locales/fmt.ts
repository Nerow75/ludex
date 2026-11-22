// Petits outils des dictionnaires (`fr/*.ts`, `en/*.ts`) : nombres écrits dans la langue du fichier.
// Les dictionnaires n'importent jamais `src/lib/i18n.ts` (qui les importe : boucle au chargement).
const cache = new Map<string, Intl.NumberFormat>();

function format(locale: string, n: number, options: Intl.NumberFormatOptions = {}): string {
  const key = `${locale}|${JSON.stringify(options)}`;
  let f = cache.get(key);
  if (!f) {
    f = new Intl.NumberFormat(locale, options);
    cache.set(key, f);
  }
  return f.format(n);
}

/** « 1 240 », « 2,5 » : nombre en français. */
export const frNum = (n: number, options?: Intl.NumberFormatOptions) => format("fr-FR", n, options);
/** "1,240", "2.5": number in English. */
export const enNum = (n: number, options?: Intl.NumberFormatOptions) => format("en-US", n, options);
