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
};
