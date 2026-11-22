// Textes français (langue de référence) de l'espace « extras » : mode canapé, écran de lancement,
// recherche rapide, menu contextuel, captures, Vitrine, Progression, boutons de la fenêtre.
// Voir `src/lib/i18n.ts`.
import { frNum } from "../fmt";

export default {
  /** Noms des touches affichés dans les indices (« Ctrl », « Alt », lettres et flèches restent tels quels). */
  keys: { enter: "Entrée", esc: "Échap", space: "Espace", pageUp: "Pg↑", pageDown: "Pg↓", shift: "Maj", del: "Suppr" },

  /** Écran de lancement (`LaunchScreen.tsx`, `launch.ts`). */
  launch: {
    launching: "Lancement…",
    running: "Bonne partie",
    failed: "Impossible de lancer le jeu",
    noAnswer: "Le launcher n'a pas répondu.",
    relay: (name: string) => `${name} prend le relais…`,
    starting: "Le jeu démarre…",
    dialog: (title: string) => `Lancement de ${title}`,
    close: "Fermer",
    hint: "N'importe quelle touche, B ou un clic pour revenir à Ludex",
  },

  /** Mode canapé (`CouchMode.tsx`, `couch.ts`). */
  couch: {
    rows: { resume: "Reprendre", favorites: "Favoris", recent: "Récents", all: "Tous les jeux" },
    dialog: "Mode canapé",
    quit: "Quitter le mode canapé",
    quitTitle: "Quitter le mode canapé (Échap)",
    empty: "Aucun jeu à afficher. Quitte le mode canapé pour ajouter un jeu ou relancer un scan.",
    inGame: "En jeu",
    favorite: "Favori",
    hints: {
      confirm: "Valider",
      play: "Jouer",
      closeFiche: "Fermer la fiche",
      fiche: "Fiche",
      favorite: "Favori",
      row: "Rangée",
      back: "Retour",
      quit: "Quitter",
    },
    today: "Aujourd'hui",
    todayTitle: "Parties lancées depuis Ludex",
    played: (playtime: string) => `${playtime} de jeu`,
    lastPlayed: (when: string) => `Joué ${when}`,
    neverLaunched: "Jamais lancé",
    achievements: (count: string) => `${count} succès`,
    ficheLabel: (title: string) => `Fiche de ${title}`,
    play: "Jouer",
    inFavorites: "Dans les favoris",
    addFavorite: "Ajouter aux favoris",
    stats: {
      playtime: "Temps de jeu",
      tracked: "Mesuré par Ludex",
      lastSession: "Dernière session",
      achievements: "Succès",
      lifespan: "Durée de vie",
      story: (time: string) => `${time} pour l'histoire`,
    },
    percent: (n: number) => `${frNum(n)} %`,
  },

  /** Recherche rapide (`QuickLauncher.tsx`). */
  quick: {
    libraryError: "Bibliothèque illisible",
    unknownError: "Erreur inconnue",
    cantLaunch: (title: string) => `Impossible de lancer ${title}`,
    notScanned: "Bibliothèque pas encore lue : ouvre Ludex le temps d'un scan.",
    noMatch: (query: string) => `Aucun jeu pour « ${query} ».`,
    placeholder: "Lancer un jeu…",
    inputLabel: "Rechercher un jeu à lancer",
    results: "Résultats",
    recent: "Derniers jeux joués",
    launching: "Lancement…",
    choose: "choisir",
    play: "jouer",
    close: "fermer",
  },

  /** Menu contextuel d'un jeu (`contextMenu.ts`). */
  menu: {
    play: "Jouer",
    open: "Ouvrir la fiche",
    addFavorite: "Ajouter aux favoris",
    removeFavorite: "Retirer des favoris",
    status: "Statut",
    noStatus: "Aucun",
    addToCollection: "Ajouter à une collection",
    newCollection: "Nouvelle collection…",
    openFolder: "Ouvrir le dossier",
    shortcut: "Créer un raccourci sur le Bureau",
    rename: "Renommer",
    hide: "Masquer",
    unhide: "Afficher",
  },

  /** Visionneuse et vignettes (`ScreenshotViewer.tsx`). */
  viewer: {
    shot: "Capture",
    andMore: (label: string, n: number) => `${label}, et ${frNum(n)} autres`,
    steamAppid: (appid: number) => `Steam, appid ${appid}`,
    dialog: (title: string) => `Captures de ${title}`,
    openFolder: "Ouvrir le dossier",
    openFolderTitle: "Ouvrir le dossier de la capture",
    close: "Fermer",
    closeTitle: "Fermer (Échap)",
    prev: "Capture précédente",
    prevTitle: "Précédente (←)",
    next: "Capture suivante",
    nextTitle: "Suivante (→)",
    shotOf: (title: string, date: string) => `Capture de ${title}, ${date}`,
  },

  /** Page « Captures » (`ScreenshotsPage.tsx`, `screenshots.ts`). */
  shots: {
    sorts: { recent: "Récentes", title: "A → Z", count: "Nombre" },
    kinds: { all: "Tout", images: "Images", videos: "Vidéos" },
    steamGame: (appid: number) => `Jeu Steam ${appid}`,
    uninstalled: "Désinstallé",
    others: "Autres captures",
    /** « 1 capture », « 12 captures », « 3 vidéos » (`kind` : `all`, `images`, `videos`). */
    count: (n: number, kind: string) => {
      const word = kind === "videos" ? "vidéo" : kind === "images" ? "image" : "capture";
      return `${frNum(n)} ${word}${n > 1 ? "s" : ""}`;
    },
    title: "Captures",
    kindGroup: "Genre de capture",
    sortGroup: "Trier les jeux",
    reload: "Relire les captures",
    none: "Aucune capture",
    noneText: "Ludex lit les captures de Steam (F12 en jeu) et celles de la Xbox Game Bar (Win+Alt+Impr. écran).",
    games: "Jeux",
    all: "Toutes les captures",
    latest: (date: string) => `la plus récente le ${date}`,
    viewDetails: "Voir la fiche",
  },

  /** Section « Vitrine » (`ShowcaseView.tsx`). */
  showcase: {
    loading: "Lecture des succès…",
    errorTitle: "Vitrine indisponible",
    errorText: "Les succès n'ont pas pu être lus. Réessaie après un rescan (F5)",
    emptyTitle: "Aucun succès pour l'instant",
    emptyText: "Les succès des jeux Steam déjà lancés, et ceux des jeux GOG avec le compte GOG connecté, apparaîtront ici",
    title: "Vitrine",
    unlocked: "Succès débloqués",
    progress: "Progression",
    perfect: "Jeux à 100 %",
    withAchievements: "Jeux avec succès",
    rarest: "Les plus rares",
    noneUnlocked: "Aucun succès débloqué pour l'instant",
    noPercent: "Pourcentages mondiaux indisponibles (Steam injoignable). Ils reviendront à la prochaine ouverture",
    recent: "Derniers déblocages",
    unlockedOn: (date: string) => `Débloqué le ${date}`,
    completedOn: (date: string) => `Terminé le ${date}`,
    count: (n: number) => `${frNum(n)} succès`,
    percent: (n: number) => `${frNum(n)} %`,
  },

  /** Section « Progression » (`ProgressBoard.tsx`). */
  progress: {
    emptyTitle: "Aucun jeu suivi pour l'instant",
    emptyText: "Ouvre la fiche d'un jeu et choisis un statut : À faire, En cours, Terminé ou Abandonné. Il apparaîtra ici, rangé dans sa colonne",
    title: "Progression",
    /** Indice « Maj ← → ou LT RT change le statut, Suppr le retire » (morceaux entre les touches). */
    hintOr: " ou ",
    hintChange: " change le statut, ",
    hintRemove: " le retire",
    noGame: "Aucun jeu",
    itemLabel: (title: string, status: string) => `${title}, ${status}. Maj+flèches pour changer de statut`,
    inGame: "En jeu",
  },

  /** Boutons de la fenêtre (`WindowControls.tsx`). */
  window: { group: "Fenêtre", minimize: "Réduire", maximize: "Agrandir", restore: "Restaurer", close: "Fermer" },
};
