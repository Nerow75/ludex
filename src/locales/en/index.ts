// English dictionary: each namespace file is checked against its French twin.
import type { Messages } from "../fr";
import activity from "./activity";
import app from "./app";
import common from "./common";
import extras from "./extras";
import fiche from "./fiche";
import library from "./library";
import settings from "./settings";

export const en: Messages = { common, library, app, fiche, settings, activity, extras };
