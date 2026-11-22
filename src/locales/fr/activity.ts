// Textes français (langue de référence) de l'espace « activity » : Récap, story, Statistiques,
// Journal. Voir `src/lib/i18n.ts`.
import { frNum } from "../fmt";

/** « 3 jours » : nombre puis mot, pluriel au-delà de 1 (règle française du code d'origine). */
const pl = (n: number, one: string, many: string) => `${frNum(n)} ${n > 1 ? many : one}`;
const dayMonth = new Intl.DateTimeFormat("fr-FR", { day: "numeric", month: "long" });

export default {
  /** Mots de chaque période (Récap et story). */
  period: {
    week: {
      label: "Semaine",
      currentTitle: "Cette semaine",
      current: "cette semaine",
      past: "cette semaine-là",
      previous: "la semaine précédente",
      versus: "à la semaine précédente",
      next: "À la semaine prochaine",
    },
    month: {
      label: "Mois",
      currentTitle: "Ce mois-ci",
      current: "ce mois-ci",
      past: "ce mois-là",
      previous: "le mois précédent",
      versus: "au mois précédent",
      next: "Au mois prochain",
    },
    year: {
      label: "Année",
      currentTitle: "Cette année",
      current: "cette année",
      past: "cette année-là",
      previous: "l'année précédente",
      versus: "à l'année précédente",
      next: "À l'année prochaine",
    },
  },
  /** Jours du lundi au dimanche, en minuscules (mis en majuscule par le code au besoin). */
  weekdays: ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"],
  weekdaysShort: ["lun", "mar", "mer", "jeu", "ven", "sam", "dim"],
  weekdayLetters: ["L", "M", "M", "J", "V", "S", "D"],
  /** Moments de la journée (nuit, matin, après-midi, soir) : dans une phrase, « Et surtout le soir ». */
  moments: ["la nuit", "le matin", "l'après-midi", "le soir"],
  /** Les mêmes, seuls (chiffre « Moment préféré » du Récap). */
  momentNames: ["La nuit", "Le matin", "L'après-midi", "Le soir"],
  /** Heures de chaque moment, courtes (Récap). */
  momentHours: ["0 h à 6 h", "6 h à 12 h", "12 h à 18 h", "18 h à 24 h"],
  /** Heures de chaque moment, dans une phrase (story). */
  momentSpan: ["de minuit à 6 h", "de 6 h à midi", "de midi à 18 h", "de 18 h à minuit"],
  /** « 16 au 22 novembre 2026 » : une semaine. */
  weekRange: (start: Date, end: Date) => {
    if (start.getFullYear() !== end.getFullYear()) return `${dayMonth.format(start)} ${start.getFullYear()} au ${dayMonth.format(end)} ${end.getFullYear()}`;
    if (start.getMonth() !== end.getMonth()) return `${dayMonth.format(start)} au ${dayMonth.format(end)} ${end.getFullYear()}`;
    return `${start.getDate()} au ${dayMonth.format(end)} ${end.getFullYear()}`;
  },
  /** Comparaison avec la période précédente : `previous` / `versus` viennent de `period`. */
  compare: {
    sameStage: " au même stade",
    nothing: (previous: string, when: string) => `Rien de mesuré ${previous}${when}`,
    same: (previous: string, when: string) => `Autant que ${previous}${when}`,
    /** `signed` : « +35 », « −25 ». */
    change: (signed: string, versus: string, when: string) => `${signed} % par rapport ${versus}${when}`,
  },
  days: (n: number) => pl(n, "jour", "jours"),
  sessions: (n: number) => pl(n, "session", "sessions"),
  games: (n: number) => pl(n, "jeu", "jeux"),
  achievements: (n: number) => pl(n, "succès", "succès"),
  playedDays: (n: number) => pl(n, "jour joué", "jours joués"),
  removedGame: "Jeu retiré de la bibliothèque",
  zero: "0 min",
  /** Infobulle d'une barre d'histogramme ; `value` nul : rien ce jour-là ou ce mois-là. */
  barTitle: (title: string, value: string | null) => `${title} : ${value ?? "rien"}`,

  recap: {
    title: "Récap",
    periodGroup: "Période",
    prevPeriod: "Période précédente",
    nextPeriod: "Période suivante",
    viewStory: "Voir en story",
    unavailable: "Récap indisponible",
    emptyTitle: "Pas encore de récap",
    emptyBody: "Ludex mesure les parties lancées depuis Ludex : lance un jeu ici et ton récap se remplira au fil des sessions",
    /** `since` : date déjà écrite, ou `null`. */
    shortHistory: (since: string | null) => `Historique encore court : Ludex ne mesure que les parties lancées depuis Ludex${since ? `, depuis le ${since}` : ""}.`,
    noneMeasured: "Aucune partie mesurée sur cette période",
    chart: "Temps de jeu par jour",
    statSessions: "Sessions",
    statStreak: "Série de jours",
    streakRunning: (days: string) => `${days} en cours`,
    noStreak: "Aucune série en cours",
    longestOfPeriod: "La plus longue de la période",
    statDay: "Jour préféré",
    statMoment: "Moment préféré",
    statLongest: "Plus longue session",
    statAchievements: "Succès Steam",
    none: "Aucun",
    noneFem: "Aucune",
    noneUnlocked: "Aucun débloqué",
    mostPlayed: "Les plus joués",
    newGames: "Nouveaux jeux",
    firstPlayed: (date: string) => `Première partie le ${date}`,
  },

  story: {
    dialog: "Story du récap",
    brand: "Récap",
    prevPage: "Page précédente",
    nextPage: "Page suivante",
    close: "Fermer la story",
    pageOf: (index: number, count: number) => `Page ${index} sur ${count}`,
    pause: "Pause",
    resume: "Reprendre",
    save: "Enregistrer en image",
    saving: "Enregistrement…",
    closeButton: "Fermer",
    saved: (file: string) => `Image enregistrée : ${file}`,
    saveFailed: (error: string) => `Enregistrement impossible : ${error}`,
    hintReduced: "Flèches ou clic pour changer de page, Échap pour fermer",
    hint: "Espace : pause · flèches : pages · Échap : fermer",
    rarest: "Le plus rare",
    /** « Débloqué par 1,2 % des joueurs Steam » ; `orLess` : sous 0,1 %. */
    rarity: (percent: string, orLess: boolean) => `Débloqué par ${percent} % des joueurs Steam${orLess ? " ou moins" : ""}`,
    canvasUnavailable: "Canvas indisponible",
    imageFailed: "Image impossible à produire",
    /** Pages. `when` : « cette semaine » ou « cette semaine-là ». */
    totalTitle: (when: string) => `de jeu ${when}`,
    topEyebrow: "Les plus joués",
    topOne: (when: string) => `Ton jeu ${when}`,
    topMany: (n: number) => `Ton top ${n}`,
    rhythmEyebrow: "Ton rythme",
    rhythmTitle: "Ton jour de jeu préféré",
    /** « Et surtout le soir, de 18 h à minuit ». */
    rhythmDetail: (moment: string, span: string) => `Et surtout ${moment}, ${span}`,
    longestEyebrow: "Ta plus longue session",
    /** « Samedi 14 novembre, à partir de 20:30 ». */
    longestDetail: (day: string, time: string) => `${day}, à partir de ${time}`,
    streakEyebrow: "Ta série",
    streakTitle: "de jeu d'affilée",
    streakRunning: (days: string) => `Série en cours : ${days}`,
    streakLongest: (when: string) => `Ta plus longue série ${when}`,
    newEyebrow: "Nouveautés",
    newTitle: (n: number): string => (n > 1 ? "nouveaux jeux lancés" : "nouveau jeu lancé"),
    newMore: (n: number) => `Les 5 premiers, et ${pl(n, "autre", "autres")}`,
    achEyebrow: "Succès Steam",
    achTitle: (n: number): string => (n > 1 ? "succès débloqués" : "succès débloqué"),
    achSome: (n: number, game: string) => `dont ${frNum(n)} dans ${game}`,
    achIn: (game: string) => `dans ${game}`,
    profileEyebrow: "Ton profil de joueur",
    endPast: "Fin du récap",
  },

  /** Profils de joueur de la story. */
  profile: {
    night: { title: "Le noctambule", line: (pct: number) => `${pct} % de ton temps de jeu entre minuit et 6 h` },
    early: { title: "Le lève-tôt", line: (pct: number) => `${pct} % de ton temps de jeu entre 6 h et midi` },
    marathon: { title: "Le marathonien", line: (average: string, longest: string) => `${average} par session en moyenne, ${longest} pour la plus longue` },
    explorer: { title: "L'explorateur", line: (n: number, when: string) => `${pl(n, "nouveau jeu lancé", "nouveaux jeux lancés")} ${when}` },
    hunter: { title: "Le chasseur de succès", line: (n: number, when: string) => `${pl(n, "succès débloqué", "succès débloqués")} ${when}` },
    regular: { title: "Le régulier", line: (n: number) => `${pl(n, "jour", "jours")} de jeu d'affilée, sans en sauter un` },
    loyal: { title: "Le fidèle", line: (pct: number, game: string) => `${pct} % de ton temps sur ${game}` },
    weekend: { title: "Le joueur du week-end", line: (pct: number) => `${pct} % de ton temps le samedi et le dimanche` },
    zapper: { title: "Le touche-à-tout", line: (n: number) => `${pl(n, "jeu différent", "jeux différents")}, aucun ne prend le dessus` },
    casual: { title: "Le joueur tranquille", line: (n: number) => `${pl(n, "session", "sessions")}, sans se presser` },
  },

  stats: {
    title: "Statistiques",
    unavailable: "Statistiques indisponibles",
    emptyTitle: "Pas encore de statistiques",
    emptyBody:
      "Aucun temps de jeu connu : Steam et GOG Galaxy donnent le leur, et Ludex mesure les parties lancées depuis Ludex. Lance un jeu ici et cette page se remplira",
    trackedByLudex: "Mesuré par Ludex",
    notTracked: "Non mesuré",
    noGameTracked: "Aucun jeu mesuré",
    launcherTimes: (games: string) => `${games}, temps des launchers`,
    ludexTimes: (games: string) => `${games}, mesurés par Ludex`,
    bothTimes: (launcher: number, ludex: number) => `${frNum(launcher)} par le launcher, ${frNum(ludex)} par Ludex`,
    /** Nom de l'histogramme pour les lecteurs d'écran. */
    chartPeak: (label: string, month: string, value: string) => `${label}. Mois le plus haut : ${month}, ${value}`,
    chartEmpty: (label: string) => `${label} : rien sur les 12 derniers mois`,
    ludexSource: (since: string | null) => `Parties lancées depuis Ludex${since ? `, mesurées depuis le ${since}` : ""}, au fuseau de ce PC.`,
    librarySource: "Compteur de chaque jeu : celui du launcher (Steam, GOG Galaxy) s'il en a un, sinon le temps mesuré par Ludex. Jamais additionnés.",
    knownTime: "Temps de jeu connu",
    none: "Aucun",
    withLudex: "Avec Ludex, 12 derniers mois",
    sessionsDays: (sessions: string, days: string) => `${sessions}, ${days}`,
    bestStreak: "Plus longue série",
    streakUntil: (date: string) => `Jusqu'au ${date}`,
    noLudexGame: "Aucune partie mesurée par Ludex",
    currentStreak: "Série en cours",
    playedInTotal: (days: string) => `${days} en tout`,
    hours: "Heures par mois",
    hoursChart: "Temps de jeu mesuré par Ludex par mois",
    hoursEmpty: "Aucune partie lancée depuis Ludex pour l'instant : seuls les temps des launchers sont connus.",
    unlocks: "Succès Steam par mois",
    unlocksSource: (achievements: string) => `Dates de déblocage gardées par Steam : ${achievements} sur 12 mois.`,
    unlocksChart: "Succès Steam débloqués par mois",
    top: "Les plus joués",
    topEmpty: "Aucun jeu avec un temps de jeu connu.",
    launchers: "Par launcher",
    launchersSource: "Même compteur que « Les plus joués » : chaque jeu compte dans son launcher.",
    noTime: "Aucun temps de jeu connu.",
    genres: "Par genre",
    genresSource: (without: string | null) =>
      `Genres des fiches, corrections comprises. Un jeu à plusieurs genres compte pour chacun.${without ? ` ${without} sans genre.` : ""}`,
    genresEmpty: "Aucun jeu n'a de genre pour l'instant.",
    /** Nom du bouton d'un genre ; `time` nul : aucun temps connu. */
    genreButton: (label: string, games: string, time: string | null) => `${label}, ${games}, ${time ?? "aucun temps connu"} : voir ces jeux`,
  },

  journal: {
    title: "Journal",
    titleOf: (game: string) => `Journal de ${game}`,
    all: "Tout le journal",
    loading: "Lecture du journal…",
    unavailable: "Journal indisponible",
    emptyTitle: "Rien dans le journal pour l'instant",
    emptyBody: "Les parties lancées depuis Ludex, les succès Steam débloqués et les captures d'écran apparaîtront ici, jour par jour",
    severalSessions: "Plusieurs sessions",
    sinceYesterday: "Depuis la veille",
    ofPlayers: (percent: string) => `${percent} des joueurs`,
    unlockedAt: (time: string) => `Débloqué à ${time}`,
    /** Résumé d'un jeu dans un jour (nombres écrits tels quels). */
    sessions: (n: number) => `${n} session${n > 1 ? "s" : ""}`,
    achievements: (n: number) => `${n} succès`,
    shots: (n: number) => `${n} capture${n > 1 ? "s" : ""}`,
    dayTotal: "Temps joué ce jour-là, mesuré par Ludex",
    openGame: (game: string) => `Ouvrir la fiche de ${game}`,
    unlocked: (n: number) => `Succès débloqués : ${n}`,
    shot: (index: number, game: string) => `Capture ${index} de ${game}`,
    loadingMore: "Lecture des jours précédents…",
    retry: "Réessayer",
    more: "Jours précédents",
    start: "Début du journal",
  },
};
