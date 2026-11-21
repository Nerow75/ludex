// Textes français (langue de référence) de l'espace « common » : libellés partagés. Voir `src/lib/i18n.ts`.
export default {
  mood: { chill: "Chill", intense: "Intense", story: "Histoire", friends: "Entre amis" },
  status: { todo: "À faire", playing: "En cours", done: "Terminé", dropped: "Abandonné" },
  platform: {
    steam: "Steam",
    epic: "Epic Games",
    gog: "GOG",
    ubisoft: "Ubisoft",
    rockstar: "Rockstar",
    xbox: "Xbox",
    riot: "Riot",
    battlenet: "Battle.net",
    ea: "EA",
    amazon: "Amazon Games",
    itch: "itch.io",
    other: "Autres",
  },
  theme: { dark: "Sombre", light: "Clair", system: "Comme Windows", oled: "Noir OLED", contrast: "Contraste élevé" },
  accent: { turquoise: "Turquoise", blue: "Bleu", violet: "Violet", pink: "Rose", green: "Vert" },
  /** Genres de Ludex (`metadata.rs`, `GENRES`) : le nom français sert d'identifiant (corrections, filtres). */
  genre: {
    Action: "Action",
    Aventure: "Aventure",
    Stratégie: "Stratégie",
    RPG: "RPG",
    Simulation: "Simulation",
    Sport: "Sport",
    Course: "Course",
    Occasionnel: "Occasionnel",
    "Massivement multijoueur": "Massivement multijoueur",
  } as Record<string, string>,
  /** Réglage de langue (Paramètres) : chaque langue écrite dans sa propre langue. */
  language: { system: "Comme Windows", fr: "Français", en: "English" },

  /** Dates relatives d'un horodatage (`relativeTime` de `format.ts`). */
  ago: {
    never: "Jamais lancé",
    justNow: "à l'instant",
    minutes: (n: number) => `il y a ${n} min`,
    hours: (n: number) => `il y a ${n} h`,
    yesterday: "hier",
    days: (n: number) => `il y a ${n} jours`,
    months: (n: number) => `il y a ${n} mois`,
    overAYear: "il y a plus d'un an",
  },
  /** Jours relatifs d'une date `AAAA-MM-JJ` (`relativeDay`), en début de phrase. */
  day: {
    today: "Aujourd'hui",
    yesterday: "Hier",
    days: (n: number) => `Il y a ${n} jours`,
    months: (n: number) => `Il y a ${n} mois`,
    years: (n: number) => `Il y a ${n} an${n > 1 ? "s" : ""}`,
  },
  /** Temps de jeu et durées (nombres déjà écrits dans la langue). */
  time: {
    notTracked: "Non mesuré",
    neverPlayed: "Jamais joué",
    minutes: (n: string) => `${n} min`,
    hours: (n: string) => `${n} h`,
    /** Durée d'une session d'une heure au moins : « 3 h », « 1 h 05 ». */
    hoursMinutes: (h: number, m: number) => (m === 0 ? `${h} h` : `${h} h ${String(m).padStart(2, "0")}`),
    /** Date et heure : « 10 décembre 2020 à 14:23 » (date et heure déjà écrites). */
    dateAt: (date: string, clock: string) => `${date} à ${clock}`,
  },
  /** Tailles : séparateur décimal de `toFixed` et unités. */
  size: {
    unknown: "Inconnue",
    decimal: ",",
    bytes: (n: string) => `${n} o`,
    kb: (n: string) => `${n} Ko`,
    mb: (n: string) => `${n} Mo`,
    gb: (n: string) => `${n} Go`,
    tb: (n: string) => `${n} To`,
  },
  percent: {
    value: (n: string) => `${n} %`,
    under: (n: string) => `moins de ${n} %`,
  },
  /** État d'installation : mention courte (carte) et note de la fiche (`p` : % connu ou `null`). */
  install: {
    updateRequired: "Mise à jour requise",
    queued: "En file d'attente",
    updating: (p: number | null) => `Téléchargement${p === null ? "" : ` ${p} %`}`,
    paused: (p: number | null) => `En pause${p === null ? "" : ` ${p} %`}`,
    incomplete: "Installation incomplète",
    noteUpdateRequired: (launcher: string) => `Mise à jour requise : ${launcher} la fera au lancement`,
    noteQueued: (launcher: string) => `Mise à jour en file d'attente chez ${launcher}`,
    noteUpdating: (p: number | null) => (p === null ? "Téléchargement en cours" : `Téléchargement ${p} %`),
    notePaused: (launcher: string, p: number | null) => `Mise à jour en pause${p === null ? "" : ` ${p} %`} : ${launcher} la reprendra au lancement`,
    noteIncomplete: (launcher: string) => `Installation incomplète : ${launcher} la terminera au lancement`,
    /** Débit et temps restant d'un téléchargement (`downloadDetail`). */
    underAMinute: "moins d'une minute",
    remaining: (duration: string) => `${duration} restantes`,
  },
  /** Tris de la bibliothèque et texte sous les jaquettes (`library.ts`). */
  sort: {
    recent: "Récents",
    playtime: "Plus joués",
    title: "A → Z",
    size: "Taille",
    year: "Année de sortie",
    achievements: "Avancement des succès",
    lifespan: "Durée de vie",
  },
  sortMeta: {
    unknownYear: "Année inconnue",
    achievements: (count: string) => `${count} succès`,
    noAchievements: "Sans succès",
    unknownLifespan: "Durée inconnue",
    story: (time: string) => `Histoire : ${time}`,
  },
  statusFilter: { any: "Tous les statuts", none: "Sans statut" },
  /** Lecteurs d'écran (`a11y.ts`). */
  a11y: {
    games: (n: number) => `${n} jeu${n > 1 ? "x" : ""}`,
    favorite: "favori",
    inGame: "en jeu",
    achievementsPercent: (p: number) => `succès ${p} %`,
    achievements: (count: string) => `succès ${count}`,
    playtime: (time: string) => `temps de jeu ${time}`,
    played: (when: string) => `joué ${when}`,
    neverLaunched: "jamais lancé",
    noMatch: (q: string) => `Aucun jeu ne correspond à « ${q} »`,
    forQuery: (games: string, q: string) => `${games} pour « ${q} »`,
    filter: (name: string) => `filtre ${name}`,
    noGames: "aucun jeu",
    /** Section et ses filtres, puis le nombre de jeux : « Tous les jeux : 12 jeux ». */
    listed: (head: string, games: string) => `${head} : ${games}`,
    position: (what: string, n: number, count: number) => `${what} ${n} sur ${count}`,
  },
  /** Périodes de l'Historique (`history.ts`). */
  history: {
    today: "Aujourd'hui",
    yesterday: "Hier",
    week: "7 derniers jours",
    month: "30 derniers jours",
    year: "12 derniers mois",
    older: "Plus ancien",
  },
  /** Toasts des changements de la bibliothèque (`scan.ts`). */
  scanToast: {
    added: (title: string) => `Nouveau jeu : ${title}`,
    addedMany: (n: number) => `${n} nouveaux jeux`,
    removed: (title: string) => `Jeu retiré : ${title}`,
    removedMany: (n: number) => `${n} jeux retirés`,
  },
  /** Sections fixes de la barre latérale et leurs catégories (`sections.tsx`). */
  sections: {
    forYou: "Pour toi",
    all: "Tous les jeux",
    favorites: "Favoris",
    tags: "Explorer",
    progress: "Progression",
    history: "Historique",
    journal: "Journal",
    recap: "Récap",
    stats: "Statistiques",
    showcase: "Vitrine",
    screenshots: "Captures",
    saves: "Sauvegardes",
    storage: "Stockage",
    wishlist: "Souhaits",
    favoritesEmpty: "Ouvre la fiche d'un jeu et clique l'étoile, ou appuie sur Ctrl+D quand il est sélectionné",
    mineEmpty: "Ouvre la fiche d'un jeu et choisis « Collections » pour l'ajouter ici",
    steamEmpty: "Collection lue dans Steam : elle se modifie dans Steam",
  },
  /** Petits composants partagés. */
  resume: "Reprendre",
  inGame: "En jeu",
  playGame: (title: string) => `Jouer à ${title}`,
  skeleton: "Recherche de tes jeux",
  inlineInputHint: "Entrée pour valider, Échap pour annuler",
};
