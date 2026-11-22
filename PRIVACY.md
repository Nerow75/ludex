# Confidentialité

**Français** · [English](PRIVACY.en.md)

Ludex est un lanceur de jeux local. Il n'a ni serveur ni télémétrie et ne demande aucun compte (les comptes GOG et Epic Games et la clé Web API Steam sont facultatifs et coupés par défaut) : aucune statistique d'usage, aucun rapport de plantage, aucun identifiant de ton PC n'est envoyé. Ce document liste tout ce que Ludex contacte sur Internet et ce qu'il y envoie. Il est tenu à jour à partir du code (version 0.2.0).

## Ce que Ludex lit sur ton PC

Pour trouver tes jeux, Ludex lit en lecture seule les fichiers et les clés du registre des launchers installés (bibliothèques Steam, manifestes Epic, bases locales de GOG Galaxy, Amazon Games et itch.io, registre d'Ubisoft, Rockstar, Xbox…), les dossiers « Jeux » ou « Games » de tes disques et les dossiers de sauvegardes connus. Rien de tout cela ne quitte ton PC.

Ce que Ludex écrit reste dans `%APPDATA%\com.ludex.app` (préférences, historique des parties, caches, journal d'erreurs) et, pour les copies de sauvegardes, dans le dossier que tu choisis (`Documents\Ludex\Sauvegardes` par défaut).

## Services contactés

Toutes les requêtes partent directement de ton PC vers le service, en HTTPS, sans cookie ni identifiant. Chaque service voit ton adresse IP et l'en-tête `User-Agent` de Ludex (`Ludex (lanceur de jeux)`, avec le numéro de version pour HowLongToBeat, `Ludex` seul pour la base des sauvegardes), comme pour toute connexion. Les redirections sont refusées pour les images, HowLongToBeat, les actualités, la configuration requise, les succès, les mods et la liste de souhaits ; elles sont suivies pour les mises à jour (fichiers des releases GitHub), la base des sauvegardes, la recherche de jaquettes et les fiches Steam.

### Mises à jour de Ludex

| Hôte | Quand | Envoyé |
| --- | --- | --- |
| `github.com` (puis le serveur de fichiers de GitHub vers lequel il redirige) | 8 s après l'ouverture, puis toutes les 12 h tant qu'aucune version n'est trouvée ; téléchargement de l'installeur seulement après ton clic sur « Installer et relancer » | Rien : adresse fixe `releases/latest/download/latest.json` ; `User-Agent` de l'outil de mise à jour de Tauri |

### Base des sauvegardes

| Hôte | Quand | Envoyé |
| --- | --- | --- |
| `raw.githubusercontent.com` | Au plus une fois par semaine, après un scan | Rien : téléchargement du manifeste de [Ludusavi](https://github.com/mtkennerly/ludusavi-manifest) ; un manifeste inchangé n'est pas retéléchargé (`If-None-Match` avec l'étiquette donnée par GitHub) |

### Steam (sans compte)

| Hôte et adresse | Quand | Envoyé |
| --- | --- | --- |
| `store.steampowered.com/api/storesearch` | Après un scan, pour chaque jeu hors Steam sans jaquette | Titre du jeu, langue, pays `FR` |
| `store.steampowered.com/api/storesearch` | Quand tu cherches un jeu dans la liste de souhaits | Le texte tapé (une ligne, 100 caractères au plus), langue, pays `FR` |
| `store.steampowered.com/api/appdetails` | À l'ouverture de l'onglet Aperçu d'un jeu (configuration requise, gardée 30 jours) | Numéro Steam du jeu (appid), langue |
| `store.steampowered.com/api/appdetails` | Une fois par jour au plus (prix de la liste de souhaits) | Numéros Steam des jeux de la liste, pays `fr` |
| `api.steampowered.com/IStoreBrowseService/GetItems` | Après un scan (fiches des jeux, chemins des images), gardé 30 jours | Numéros Steam des jeux, langue, pays `FR` |
| `api.steampowered.com/IStoreService/GetTagList` | Tous les 30 jours au plus | Langue |
| `api.steampowered.com/ISteamNews/GetNewsForApp` | À l'ouverture d'une fiche (si les annonces ont plus de 6 h), et une fois par jour pour les jeux « En cours » | Numéro Steam du jeu |
| `api.steampowered.com/ISteamUserStats/GetGlobalAchievementPercentagesForApp` | Vitrine et liste des succès, gardé 7 jours | Numéro Steam du jeu |
| `api.steampowered.com/ISteamRemoteStorage/GetPublishedFileDetails` | Onglet Mods d'un jeu Steam, gardé 30 jours | Numéros des mods Workshop installés |

### Microsoft Store (sans compte)

| Hôte | Quand | Envoyé |
| --- | --- | --- |
| `displaycatalog.mp.microsoft.com` | Après un scan, pour un jeu sans jaquette introuvable sur Steam | Titre du jeu, puis numéro du produit trouvé ; marché `FR`, langue `fr-fr` |

### Jeux gratuits d'Epic (sans compte)

| Hôte | Quand | Envoyé |
| --- | --- | --- |
| `store-site-backend-static.ak.epicgames.com/freeGamesPromotions` | À l'ouverture de la section « Souhaits », et en fond (toutes les 6 h au plus, peu après le début ou la fin d'une offre) si la notification « Jeux gratuits sur Epic » est activée | Langue de Ludex (`fr` ou `en-US`) et pays `FR` ; rien d'autre |

### HowLongToBeat

| Hôte | Quand | Envoyé |
| --- | --- | --- |
| `howlongtobeat.com` | Après un scan, une requête toutes les 1,2 s, chaque titre gardé 30 jours | Une demande de jeton de recherche (`init`), puis les mots du titre du jeu ; si l'adresse de recherche a changé, lecture de la page d'accueil et de ses scripts pour la retrouver |

### Comptes facultatifs : GOG et Epic Games (désactivés par défaut)

Seulement si tu connectes ces comptes dans Paramètres > Comptes, pour la section « Pas installés » (jeux possédés qui ne sont pas sur ce PC). La connexion se fait sur la page de la boutique, dans une fenêtre à part en navigation privée : Ludex ne voit jamais ton identifiant ni ton mot de passe. Il garde seulement les jetons de la boutique, chiffrés pour ton compte Windows (DPAPI) dans `comptes.bin`, jamais montrés à l'interface, dans le journal, le Diagnostic ni l'export. « Se déconnecter » efface les jetons et la liste.

| Hôte | Quand | Envoyé |
| --- | --- | --- |
| `auth.gog.com`, `embed.gog.com`, `login.gog.com` | Connexion (page de GOG), puis échange et renouvellement des jetons | Le code de connexion donné par GOG, puis le jeton de renouvellement ; identifiant public du client GOG Galaxy |
| `embed.gog.com/user/data/games` | Liste de tes jeux, relue après 6 h ou par « Relire » | Le jeton d'accès de GOG |
| `api.gog.com/products` | Fiches publiques des jeux possédés (gardées 30 jours), puis l'installeur d'un jeu quand tu cliques sur « Installer » | Numéros GOG des jeux ; le jeton d'accès seulement pour demander le lien de téléchargement |
| `*.gog.com` (serveurs de téléchargement de GOG) | Téléchargement de l'installeur, après ton clic sur « Installer » | Rien d'autre que l'adresse donnée par GOG |
| `www.epicgames.com` | Connexion (page d'Epic, dans sa fenêtre) | Ce que tu saisis sur la page d'Epic, que Ludex ne lit pas |
| `account-public-service-prod03.ol.epicgames.com` | Échange, renouvellement et fin des jetons (à la déconnexion) | Le code de connexion donné par Epic, puis le jeton de renouvellement ; identifiant public du client du launcher Epic |
| `library-service.live.use1a.on.epicgames.com` | Liste de tes jeux, relue après 6 h ou par « Relire » | Le jeton d'accès d'Epic |
| `catalog-public-service-prod06.ol.epicgames.com` | Fiches des jeux possédés (titre, type, plateforme, image), gardées 30 jours | Numéros de catalogue des jeux ; le jeton d'accès d'Epic |

Ludex ne télécharge rien pour Epic : « Installer dans Epic » ouvre le launcher Epic de ton PC sur le jeu.

### Clé Web API Steam facultative (absente par défaut)

Sans clé, la section « Pas installés » montre les jeux Steam déjà lancés sur ton compte, lus sur ton PC (`localconfig.vdf` et `appinfo.vdf` de Steam) : rien n'est envoyé. Seulement si tu colles ta clé Web API Steam personnelle dans Paramètres > Comptes, Ludex demande la liste complète de tes jeux à Steam. La clé est vérifiée par cette même requête, puis chiffrée pour ton compte Windows (DPAPI) dans `comptes.bin`, jamais montrée à l'interface, dans le journal, le Diagnostic ni l'export. « Retirer la clé » l'efface avec la liste.

| Hôte | Quand | Envoyé |
| --- | --- | --- |
| `api.steampowered.com/IPlayerService/GetOwnedGames` | À l'enregistrement de la clé, puis liste relue après 6 h ou par « Relire » | Ta clé Web API et l'identifiant Steam (SteamID64) du compte de ce PC |

« Obtenir une clé » ouvre `steamcommunity.com/dev/apikey` dans ton navigateur. « Installer dans Steam » ouvre Steam sur le jeu (`steam://install/<numéro>`) : Ludex ne télécharge rien pour Steam.

### Images

Ces adresses sont construites par Ludex à partir d'un numéro Steam ou d'un nom d'image ; Ludex télécharge certaines images dans son cache, l'interface en affiche d'autres directement.

| Hôte | Contenu | Envoyé |
| --- | --- | --- |
| `cdn.cloudflare.steamstatic.com`, `shared.akamai.steamstatic.com` | Jaquettes, images de fond, logos, icônes des succès, images de la liste de souhaits | Adresse de l'image (numéro Steam du jeu dedans) |
| `store-images.s-microsoft.com` | Jaquettes et images de fond trouvées dans le Microsoft Store | Adresse de l'image |
| `clan.akamai.steamstatic.com`, `clan.fastly.steamstatic.com` | Images des actualités Steam | Adresse de l'image |
| `images.steamusercontent.com` | Vignettes des mods Workshop | Adresse de la vignette |
| `cdn1.epicgames.com`, `cdn2.unrealengine.com` | Images des jeux gratuits d'Epic, vignettes des jeux Epic pas installés (compte Epic connecté) | Adresse de l'image (réduite par le CDN) |
| `shared.akamai.steamstatic.com` | En-têtes des jeux Steam pas installés | Adresse de l'image (numéro Steam du jeu dedans) |
| `images.gog-statics.com` | Vignettes des jeux GOG pas installés (compte GOG connecté) | Adresse de la vignette |

## Ce qui n'est jamais envoyé

- Ton nom d'utilisateur Windows, tes chemins de dossiers, le nom de ton PC.
- Ton identifiant de compte Steam (lu sur le disque pour trouver ton temps de jeu), sauf à `api.steampowered.com` si tu enregistres une clé Web API Steam.
- Ton temps de jeu, ton historique, tes favoris, statuts, notes, collections, options de lancement.
- Le contenu de tes fichiers, de tes sauvegardes ou de tes captures.

## Ce qui part seulement si tu le fais

- **Titres** : le titre envoyé à Steam, au Microsoft Store et à HowLongToBeat est celui que Ludex affiche. Pour un jeu « Autres », il vient du nom de l'exécutable ou du dossier ; si tu renommes un jeu, c'est le nouveau titre qui est utilisé. Les alias de recherche, eux, restent sur ton PC.
- **Liens** : « Voir le dépôt », « Lire sur Steam », « Voir dans Steam », « Voir sur Epic » ouvrent ton navigateur, Steam ou le launcher Epic sur une adresse fixée par Ludex.
- **Rapport de diagnostic** : copié dans le presse-papiers seulement quand tu cliques sur « Copier le rapport ». Il est anonymisé (dossier et nom d'utilisateur remplacés par `%USERPROFILE%` et `%USERNAME%`, compte Steam masqué) ; tu choisis à qui l'envoyer.
- **Export des données** : une archive `.zip` écrite là où tu le choisis, jamais envoyée.
- **Statut Discord** (désactivé par défaut) : s'il est activé, Ludex parle au client Discord de ton PC par un canal local ; c'est Discord qui affiche alors à tes amis le titre du jeu, son launcher, l'heure de début et l'image Steam du jeu (ou seulement « En jeu » si tu masques le nom). Ludex ne contacte pas les serveurs de Discord.

## Installeur

L'installeur NSIS télécharge WebView2 depuis Microsoft seulement s'il manque sur ton PC (Windows 11 l'a déjà). Il n'envoie rien d'autre.

## Contact

Questions ou signalements : [issues du dépôt](https://github.com/Nerow75/ludex/issues).
