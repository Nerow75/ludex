import { PLATFORM_LABEL, type AchievementStore } from "./api";

/** Nom de la boutique d'un succès (« Steam », « GOG »), dans la langue affichée. */
export const storeName = (store: AchievementStore) => PLATFORM_LABEL[store];

/** Texte suivi de sa boutique : « Hades · GOG ». Sert dans la Vitrine, où Steam et GOG se mêlent. */
export const withStore = (text: string, store: AchievementStore) => `${text} · ${storeName(store)}`;
