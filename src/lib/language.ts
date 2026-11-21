// Suivi du réglage de langue (`language` de `settings.rs`) pour la fenêtre principale et la
// recherche rapide. Le réglage et la langue de Windows sont résolus côté Rust
// (`SettingsInfo.languageResolved`) ; une copie dans localStorage applique la langue dès le premier
// rendu, sans passer par le français le temps de la réponse.
import { onSettingsChanged, settings } from "./api";
import { knownLang, langOf, setLang, type Lang } from "./i18n";

const KEY = "ludex.lang";

function remember(value: Lang) {
  try {
    localStorage.setItem(KEY, value);
  } catch {
    // stockage indisponible : la langue revient par les réglages
  }
}

function recall(): string | null {
  try {
    return localStorage.getItem(KEY);
  } catch {
    return null;
  }
}

/** Applique une langue et la retient pour le prochain démarrage. */
export function applyLang(value: Lang) {
  remember(value);
  setLang(value);
}

/** À appeler avant le premier rendu : langue retenue (sinon celle du navigateur), puis celle des réglages et ses changements. */
export function startLanguage() {
  const saved = recall();
  setLang(saved ? knownLang(saved) : langOf(typeof navigator === "undefined" ? null : navigator.language));
  const apply = (s: { languageResolved: Lang }) => applyLang(knownLang(s.languageResolved));
  settings()
    .then(apply)
    .catch(() => {}); // sans réponse, la langue retenue reste
  onSettingsChanged(apply).catch(() => {});
}
