# Ludex

Launcher hub pour Windows : détecte les jeux installés de tous les launchers (Steam, Epic, Riot pour l'instant) et les lance depuis une seule interface. Objectif : plus moderne et plus beau que Playnite ou GOG Galaxy.

## Stack

- Tauri 2 (Rust, edition 2024) dans `src-tauri/`
- React 19 + TypeScript + Vite dans `src/`
- Polices embarquées via @fontsource-variable : Geist (interface), Geist Mono (chiffres, méta, touches)

## Commandes

- `npm run tauri dev` : lance l'app
- `npx tsc --noEmit` puis `npm run build` : vérif front
- `cd src-tauri && cargo test` : tests Rust
- `cd src-tauri && cargo test scan_this_pc -- --ignored --nocapture` : scan réel du PC

## Architecture

- `src-tauri/src/library/` : une source par launcher (`steam.rs`, `epic.rs`, `gog.rs`, `ubisoft.rs`, `rockstar.rs`, `xbox.rs`, `riot.rs`), lecture de fichiers locaux et du registre uniquement, aucune connexion à un compte. `vdf.rs` parse le format Valve. `mod.rs` définit `Game`, `Launch`, `ScanResult`, les utilitaires `reg` (registre en lecture) et `xml_attr`, et agrège les sources dans l'ordre de priorité : `Ok(None)` = launcher absent, `Ok(Some(vec![]))` = installé sans jeu ; un jeu dont le dossier est déjà vu par une source précédente est écarté.
- Sources : GOG (registre `GOG.com\Games`, lancement par Galaxy ou l'exe), Ubisoft (registre `Ubisoft\Launcher\Installs`, `uplay://launch/<id>/0`), Rockstar (registre `Rockstar Games`, exe `Play*.exe`, images Steam via une table titre → appid), Xbox (registre des paquets MSIX, jeu = `MicrosoftGame.config` à la racine, `explorer.exe shell:AppsFolder\<famille>!<app>`, visuel = écran de démarrage local). `Launch::Exec` part du dossier de l'exécutable.
- `src-tauri/src/lib.rs` : commandes Tauri `scan_library`, `launch_game` (renvoie l'heure du lancement), `open_install_dir`, `set_favorite`, `set_hidden`. Jeux scannés et préférences gardés dans `AppState`. Protocole `ludex` (`http://ludex.localhost/<plateforme>/<id>/<cover|hero|logo>` sous Windows) : sert les images locales d'un jeu de l'état (`Game.local_art`, jamais sérialisé).
- `src-tauri/src/store.rs` : préférences (favoris, masqués, outils ré-affichés, dernier lancement Ludex) dans `user-data.json` du dossier de données de l'app (`%APPDATA%/com.ludex.app`).
- Steam fournit `LastPlayed` et `SizeOnDisk`, Epic `InstallSize`. Images Steam : d'abord `appcache/librarycache/<appid>/` (à plat ou dans des sous-dossiers hachés, jaquette `library_600x900` ou `library_capsule`), sinon le CDN. Les outils (Wallpaper Engine, `* Editor`, `* Dedicated Server`…) sont `is_tool` : masqués par défaut, jamais exclus.
- **Sécurité** : le front n'envoie qu'un id de jeu. La commande de lancement et le dossier sont reconstruits côté Rust depuis l'état. Ne jamais exposer de commande qui prend un chemin ou une URI arbitraire venant du front. CSP stricte dans `tauri.conf.json` (`csp` et `devCsp`, appliquée seulement en build : `npx tauri build --no-bundle` pour la tester) ; toute nouvelle origine d'image va dans `img-src`. Le front n'a que `core:default` comme permission.
- `src/lib/api.ts` : types et appels `invoke`. `src/lib/color.ts` : couleur d'ambiance extraite de la jaquette (fallback : hash du titre).
- `src/lib/library.ts` : filtres, tri, récents, préférences d'affichage (localStorage). `src/lib/format.ts` : dates relatives, tailles, recherche.
- `src/components/` : GameCard, LibraryGrid (colonnes mesurées, insère la fiche sous la rangée), GameStrip (fiche dépliable), ResumeRow, Art et Logo, Cover, States, Icons.
- `src/App.tsx` : écran unique « Focus » : grand champ de recherche, Reprendre, filtres et tri, grille, barre de raccourcis (taper n'importe où, flèches, Entrée, Espace, Échap, Ctrl+D, Ctrl+K, F5).

## Design system

- Tokens dans `src/styles/tokens.css` (préfixe `--lx-*`), styles dans `src/styles/app.css`.
- Logo : une touche de clavier turquoise avec le triangle « Jouer » (`assets/logo/ludex-mark.svg`, version avec le nom `ludex-lockup.svg`, composant `LudexMark`). Icônes de l'app régénérées par `npx tauri icon assets/logo/ludex-mark.svg` (supprimer ensuite `icons/android` et `icons/ios`).
- Palette validée : base monochrome (#0B0B0B, #1A1A1A, #242424, #333333, blanc) + accent turquoise #14B8A6. Thème clair via `[data-theme="light"]`. D'autres thèmes viendront plus tard : toujours passer par les tokens, jamais de couleur en dur.
- `--lx-ambient` : couleur dérivée du jeu sélectionné, réservée au contour de sélection. Bouton Jouer en blanc cassé (`--lx-text` sur `--lx-bg`).
- Passe unslop appliquée : grille de 4px, icônes 18px trait 1.5, graisses 400 à 600, pas de majuscules espacées, chiffres tabulaires, rayons s/m/l = 8/10/12, une seule courbe `--lx-ease`, survol sans déplacement (contour ou voile qui s'éclaircit), léger `scale` au clic.

## État actuel

- Testé sur le PC de l'utilisateur : Steam 69 jeux, Riot 2, Rockstar 1 (GTA V), Xbox 1 (Minecraft Launcher), Epic et Ubisoft installés sans jeu, GOG absent. Epic, GOG et Ubisoft avec jeux testés seulement en unitaire. EA app et Battle.net pas encore pris en charge.
- Refonte UI « Focus » faite (branche `refonte-ui-focus`). Audit et maquettes : `maquettes/refonte-ui.html` (non versionné).
- Problèmes connus : jeux Riot sans images (jaquette générée). Amis et Succès pas commencés.
- Idées et tâches : projet Ludex du tableau « Tâches NRW » (https://claude.ai/artifact/LLukjvgWpKWsVhMMez4a8t).

## Conventions

- Interface, messages d'erreur et commentaires en français.
- Ne jamais utiliser le tiret cadratin dans les textes.
