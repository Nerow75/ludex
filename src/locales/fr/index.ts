// Dictionnaire français, un fichier par espace de noms (`t.<espace>.<clé>`).
import activity from "./activity";
import app from "./app";
import common from "./common";
import extras from "./extras";
import fiche from "./fiche";
import library from "./library";
import settings from "./settings";

export const fr = { common, library, app, fiche, settings, activity, extras };

/** Forme de tout dictionnaire : mêmes espaces, mêmes clés, mêmes signatures que le français. */
export type Messages = typeof fr;
