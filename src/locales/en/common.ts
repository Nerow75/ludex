// English texts for the « common » namespace: same keys as `../fr/common.ts` (checked by TypeScript).
import type fr from "../fr/common";

export default {
  mood: { chill: "Chill", intense: "Intense", story: "Story", friends: "With friends" },
  status: { todo: "Backlog", playing: "Playing", done: "Finished", dropped: "Dropped" },
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
    other: "Other",
  },
  theme: { dark: "Dark", light: "Light", system: "Match Windows", oled: "OLED black", contrast: "High contrast" },
  accent: { turquoise: "Turquoise", blue: "Blue", violet: "Violet", pink: "Pink", green: "Green" },
  genre: {
    Action: "Action",
    Aventure: "Adventure",
    Stratégie: "Strategy",
    RPG: "RPG",
    Simulation: "Simulation",
    Sport: "Sports",
    Course: "Racing",
    Occasionnel: "Casual",
    "Massivement multijoueur": "Massively multiplayer",
  },
  language: { system: "Match Windows", fr: "Français", en: "English" },
} satisfies typeof fr;
