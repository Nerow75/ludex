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
- `cd src-tauri && cargo test scan_this_pc -- --ignored --nocapture` : scan réel du PC (avec temps de jeu et les 5 jeux les plus joués)
- `cd src-tauri && cargo test processes_this_pc -- --ignored --nocapture` : relevé réel des processus, jeux de la bibliothèque déjà ouverts
- `cd src-tauri && cargo clippy --all-targets` : doit rester sans avertissement

## Architecture

- `src-tauri/src/library/` : une source par launcher (`steam.rs`, `epic.rs`, `gog.rs`, `ubisoft.rs`, `ea.rs`, `battlenet.rs`, `rockstar.rs`, `xbox.rs`, `riot.rs`, puis `local.rs` pour « Autres »), lecture de fichiers locaux et du registre uniquement, aucune connexion à un compte. `vdf.rs` parse le format Valve. `mod.rs` définit `Game`, `Launch`, `ScanResult`, les utilitaires `reg` (registre en lecture) et `xml_attr`, et agrège les sources dans l'ordre de priorité : `Ok(None)` = launcher absent, `Ok(Some(vec![]))` = installé sans jeu ; un jeu dont le dossier est déjà vu par une source précédente (même dossier, parent ou sous-dossier) est écarté.
- Sources : GOG (registre `GOG.com\Games`, lancement par Galaxy ou l'exe), Ubisoft (registre `Ubisoft\Launcher\Installs`, `uplay://launch/<id>/0`), Rockstar (registre `Rockstar Games`, exe `Play*.exe`, images Steam via une table titre → appid), Xbox (registre des paquets MSIX, jeu = `MicrosoftGame.config` à la racine, `explorer.exe shell:AppsFolder\<famille>!<app>`, visuel = écran de démarrage local). `Launch::Exec` part du dossier de l'exécutable.
- Dossiers de jeux (`local.rs`) : sous-dossiers des dossiers « Jeux » / « Games » à la racine des lecteurs (un « Jeux Epic » dedans est lu aussi, `XboxGames` exclu) et emplacements de la désinstallation Windows qui contiennent un moteur (Unity, Unreal, Steamworks), liste gardée 10 s pour un même scan. Chaque dossier est rangé : `.product.db` + `.build.info` = Battle.net (code produit → `battlenet://<code>` si le client est là, sinon l'exe), `__Installer\installerdata.xml` = EA (contentID, titre, exe non « trial » lancé directement), sinon « Autres » : exe choisi par score (nom proche du dossier, profondeur ≤ 3, taille, listes d'exclusion installeurs/utilitaires/anti-triche), titre = ProductName de l'exe (crate `pelite`) sinon dossier ou nom de l'exe ; launchers installés écartés, doublons de titre écartés. Plus des emplacements fixes : FiveM, RedM, RAGE MP, Roblox, jeux Ankama (lancés par l'Ankama Launcher), Pokeisland.
- `src-tauri/src/lib.rs` : commandes Tauri `scan_library`, `launch_game` (renvoie l'heure du lancement et démarre le suivi de session), `open_install_dir`, `set_favorite`, `set_hidden`, `game_sessions` (sessions en cours). Jeux scannés, préférences et `Tracker` gardés dans `AppState`. Événement `game-session` : `{ id, phase: detecting|running|ended, playtimeMinutes, playtimeSource }`, le temps n'étant rempli que quand Ludex vient d'ajouter du temps. Protocole `ludex` (`http://ludex.localhost/<plateforme>/<id>/<cover|hero|logo>` sous Windows) : sert les images locales d'un jeu de l'état (`Game.local_art`, jamais sérialisé).
- `src-tauri/src/store.rs` : préférences (favoris, masqués, outils ré-affichés, dernier lancement Ludex, temps mesuré par Ludex en secondes dans `playtime`) dans `user-data.json` du dossier de données de l'app (`%APPDATA%/com.ludex.app`).
- **Temps de jeu** (`Game.playtime_minutes` + `playtime_source` : `launcher` ou `ludex`, jamais additionnés ; `None` = inconnu) :
  - Steam : `userdata/<steamid3>/config/localconfig.vdf`, `UserLocalConfigStore > Software > Valve > Steam > apps > <appid>` (`Playtime` en minutes, `LastPlayed`). Compte : `config/loginusers.vdf`, `MostRecent` = 1 sinon `timestamp` le plus récent (le Steam actuel n'écrit plus `MostRecent`), steamid3 = SteamID64 − 76561197960265728 ; repli sur le `localconfig.vdf` le plus récent de `userdata`. Appli absente du fichier = 0 min.
  - GOG : base de Galaxy `ProgramData\GOG.com\Galaxy\storage\galaxy-2.0.db` (SQLite via `rusqlite` `bundled`, lecture seule), tables `GameTimes` (`releaseKey` = `gog_<id>`, `minutesInGame`) et `LastPlayedDates`. Seulement si le jeu est lancé par Galaxy.
  - Epic, Ubisoft, Rockstar, Xbox, Riot : aucune donnée locale fiable (fichiers chiffrés ou temps côté serveur), donc mesure par Ludex.
  - `src-tauri/src/session.rs` : mesure par Ludex. `Tracker` = un fil qui dort sans session ; après `launch_game`, relevé toutes les 5 s (crate `sysinfo`, chemins d'exécutables mis en cache) des processus situés sous le dossier du jeu (plus `%LOCALAPPDATA%\Packages\<famille>` pour Xbox ; racines trop larges refusées). Détection jusqu'à 10 min après le lancement, absence tolérée 5 min tant que la session fait moins d'une minute (lanceur intermédiaire type `PlayGTAV.exe`), 30 s ensuite. Écart de plus de 20 s entre deux relevés (veille) non compté, sessions de moins d'une minute ignorées, plafond 24 h. Cumul enregistré chaque minute. Steam et GOG Galaxy (`launcher_tracks_playtime`) sont suivis pour « En jeu » mais jamais cumulés.
- Steam fournit `LastPlayed` et `SizeOnDisk`, Epic `InstallSize`. Images Steam : d'abord `appcache/librarycache/<appid>/` (à plat ou dans des sous-dossiers hachés, jaquette `library_600x900` ou `library_capsule`), sinon le CDN. Les outils (Wallpaper Engine, `* Editor`, `* Dedicated Server`…) sont `is_tool` : masqués par défaut, jamais exclus.
- **Sécurité** : le front n'envoie qu'un id de jeu. La commande de lancement et le dossier sont reconstruits côté Rust depuis l'état. Ne jamais exposer de commande qui prend un chemin ou une URI arbitraire venant du front. CSP stricte dans `tauri.conf.json` (`csp` et `devCsp`, appliquée seulement en build : `npx tauri build --no-bundle` pour la tester) ; toute nouvelle origine d'image va dans `img-src`. Le front n'a que `core:default` comme permission.
- `src/lib/api.ts` : types et appels `invoke`. `src/lib/color.ts` : couleur d'ambiance extraite de la jaquette (fallback : hash du titre).
- `src/lib/library.ts` : filtres, tri (Récents, Plus joués, A → Z, Taille), récents, préférences d'affichage (localStorage). `src/lib/format.ts` : dates relatives, tailles, temps de jeu (`formatPlaytime` : « 45 min », « 2,5 h », « 1 240 h », « Jamais joué », « Non mesuré »), recherche.
- `src/components/` : GameCard, LibraryGrid (colonnes mesurées, insère la fiche sous la rangée), GameStrip (fiche dépliable, stat « Temps de jeu » avec sa provenance au survol), ResumeRow, Art et Logo, Cover, States, Icons. Mention `.live` « En jeu » sur la carte, la tuile et la fiche.
- `src/App.tsx` : écran unique « Focus » : grand champ de recherche, Reprendre, filtres et tri, grille, barre de raccourcis (taper n'importe où, flèches, Entrée, Espace, Échap, Ctrl+D, Ctrl+K, F5). Manette (`src/lib/gamepad.ts`, Gamepad API, ignorée quand Ludex n'a pas le focus) : croix ou stick naviguer, A jouer, X fiche, Y favori, B fermer, LB/RB filtre, LT/RT tri ; la barre du bas montre ces boutons après une action à la manette. Écoute `game-session` (ensemble `playing`, temps mis à jour en direct) et rescanne en silence 10 s après la fin d'une partie.

## Design system

- Tokens dans `src/styles/tokens.css` (préfixe `--lx-*`), styles dans `src/styles/app.css`.
- Logo : une touche de clavier turquoise avec le triangle « Jouer » (`assets/logo/ludex-mark.svg`, version avec le nom `ludex-lockup.svg`, composant `LudexMark`). Icônes de l'app régénérées par `npx tauri icon assets/logo/ludex-mark.svg` (supprimer ensuite `icons/android` et `icons/ios`).
- Palette validée : base monochrome (#0B0B0B, #1A1A1A, #242424, #333333, blanc) + accent turquoise #14B8A6. Thème clair via `[data-theme="light"]`. D'autres thèmes viendront plus tard : toujours passer par les tokens, jamais de couleur en dur.
- `--lx-ambient` : couleur dérivée du jeu sélectionné, réservée au contour de sélection. Bouton Jouer en blanc cassé (`--lx-text` sur `--lx-bg`).
- Passe unslop appliquée : grille de 4px, icônes 18px trait 1.5, graisses 400 à 600, pas de majuscules espacées, chiffres tabulaires, rayons s/m/l = 8/10/12, une seule courbe `--lx-ease`, survol sans déplacement (contour ou voile qui s'éclaircit), léger `scale` au clic.

## État actuel

- Testé sur le PC de l'utilisateur : Steam 69 jeux, Riot 2, Rockstar 1 (GTA V), Xbox 1 (Minecraft Launcher), Epic et Ubisoft installés sans jeu, GOG absent, EA 3 (FC 24, FIFA 21, NFS Rivals), Battle.net 1 (StarCraft II, client absent : lancé par l'exe), Autres 24 (Cyberpunk, Tarkov, Genshin, FiveM, Dofus, Roblox, Sims 4…). Epic, GOG et Ubisoft avec jeux testés seulement en unitaire. Les jeux « Autres », EA et Battle.net n'ont que des jaquettes générées ; aucun n'a été lancé. Scan complet ~2,5 s en debug, dont 1,6 s pour « Autres ».
- Temps de jeu (branche `temps-de-jeu`) : Steam lu sur ce PC pour les 69 jeux ; GOG testé seulement en unitaire ; mesure par Ludex testée en unitaire et par un relevé réel des processus, pas encore sur une vraie partie. Limites : jeu lancé hors de Ludex non mesuré ; Ludex fermé pendant la partie = temps déjà mesuré gardé, suite perdue (pas de rattachement au redémarrage) ; client Riot (`LeagueClient.exe`) compté comme du jeu ; processus protégés dont le chemin est illisible (certains anti-triche) non vus ; `PlaytimeDisconnected` de Steam ignoré.
- Refonte UI « Focus » faite (branche `refonte-ui-focus`). Audit et maquettes : `maquettes/refonte-ui.html` (non versionné).
- Problèmes connus : jeux Riot sans images (jaquette générée). Amis et Succès pas commencés.
- Dépendances Rust ajoutées : `sysinfo` (sans fonctions par défaut, `system`) et `rusqlite` (`bundled`, compile SQLite : premier build plus long).
- Idées et tâches : projet Ludex du tableau « Tâches NRW » (https://claude.ai/artifact/LLukjvgWpKWsVhMMez4a8t).

## Conventions

- Interface, messages d'erreur et commentaires en français.
- Ne jamais utiliser le tiret cadratin dans les textes.
