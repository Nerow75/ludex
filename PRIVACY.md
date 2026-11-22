# Confidentialité

**Français** · [English](PRIVACY.en.md)

Ludex est un lanceur de jeux local. Il n'a ni serveur ni télémétrie, et aucun compte n'est demandé (seul le compte GOG, facultatif, peut être connecté) : aucune statistique d'usage, aucun rapport de plantage, aucun identifiant de ton PC n'est envoyé. Ce document liste tout ce que Ludex contacte sur Internet et ce qu'il y envoie. Il est tenu à jour à partir du code (version 0.2.0).

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

### HowLongToBeat

| Hôte | Quand | Envoyé |
| --- | --- | --- |
| `howlongtobeat.com` | Après un scan, une requête toutes les 1,2 s, chaque titre gardé 30 jours | Une demande de jeton de recherche (`init`), puis les mots du titre du jeu ; si l'adresse de recherche a changé, lecture de la page d'accueil et de ses scripts pour la retrouver |

### Compte GOG (facultatif, coupé par défaut)

Seulement si tu connectes ton compte GOG dans les Paramètres (section « Comptes »). Ta connexion se fait dans une fenêtre de GOG : Ludex ne voit jamais ton mot de passe. Les jetons de GOG restent chiffrés sur ton PC (DPAPI de Windows) et ne sont envoyés qu'à GOG ; « Se déconnecter » les efface.

| Hôte | Quand | Envoyé |
| --- | --- | --- |
| `auth.gog.com` | À la connexion, puis quand le jeton expire | Code de connexion donné par GOG, puis jeton de rafraîchissement ; identifiant public du client GOG Galaxy |
| `embed.gog.com` | Section « Pas installés », relue après 6 h ou par « Relire » | Jeton du compte |
| `api.gog.com` | Fiches des jeux possédés (gardées 30 jours) ; installeur d'un jeu quand tu cliques sur « Installer » | Numéros GOG des jeux ; jeton du compte pour le lien de téléchargement |
| CDN de GOG (`*.gog.com`) | Téléchargement d'un installeur que tu as demandé | Adresse donnée par GOG |
| `gameplay.gog.com` | Succès des jeux GOG installés : après un scan, une fois par jour au plus par jeu (plus tôt si tu y as joué depuis), et à l'ouverture de l'onglet Succès s'ils ont plus de 10 min | Jeton du compte, ton numéro de compte GOG, numéro GOG du jeu, langue |
| `images.gog-statics.com`, `images.gog.com` | Vignettes des jeux et icônes des succès | Adresse de l'image |

### Images

Ces adresses sont construites par Ludex à partir d'un numéro Steam ou d'un nom d'image ; Ludex télécharge certaines images dans son cache, l'interface en affiche d'autres directement.

| Hôte | Contenu | Envoyé |
| --- | --- | --- |
| `cdn.cloudflare.steamstatic.com`, `shared.akamai.steamstatic.com` | Jaquettes, images de fond, logos, icônes des succès, images de la liste de souhaits | Adresse de l'image (numéro Steam du jeu dedans) |
| `store-images.s-microsoft.com` | Jaquettes et images de fond trouvées dans le Microsoft Store | Adresse de l'image |
| `clan.akamai.steamstatic.com`, `clan.fastly.steamstatic.com` | Images des actualités Steam | Adresse de l'image |
| `images.steamusercontent.com` | Vignettes des mods Workshop | Adresse de la vignette |

## Ce qui n'est jamais envoyé

- Ton nom d'utilisateur Windows, tes chemins de dossiers, le nom de ton PC.
- Ton identifiant de compte Steam (lu sur le disque pour trouver ton temps de jeu, jamais transmis).
- Ton temps de jeu, ton historique, tes favoris, statuts, notes, collections, options de lancement.
- Le contenu de tes fichiers, de tes sauvegardes ou de tes captures.

## Ce qui part seulement si tu le fais

- **Titres** : le titre envoyé à Steam, au Microsoft Store et à HowLongToBeat est celui que Ludex affiche. Pour un jeu « Autres », il vient du nom de l'exécutable ou du dossier ; si tu renommes un jeu, c'est le nouveau titre qui est utilisé. Les alias de recherche, eux, restent sur ton PC.
- **Liens** : « Voir le dépôt », « Lire sur Steam », « Voir dans Steam » ouvrent ton navigateur ou Steam sur une adresse fixée par Ludex.
- **Rapport de diagnostic** : copié dans le presse-papiers seulement quand tu cliques sur « Copier le rapport ». Il est anonymisé (dossier et nom d'utilisateur remplacés par `%USERPROFILE%` et `%USERNAME%`, compte Steam masqué) ; tu choisis à qui l'envoyer.
- **Export des données** : une archive `.zip` écrite là où tu le choisis, jamais envoyée.
- **Statut Discord** (désactivé par défaut) : s'il est activé, Ludex parle au client Discord de ton PC par un canal local ; c'est Discord qui affiche alors à tes amis le titre du jeu, son launcher, l'heure de début et l'image Steam du jeu (ou seulement « En jeu » si tu masques le nom). Ludex ne contacte pas les serveurs de Discord.

## Installeur

L'installeur NSIS télécharge WebView2 depuis Microsoft seulement s'il manque sur ton PC (Windows 11 l'a déjà). Il n'envoie rien d'autre.

## Contact

Questions ou signalements : [issues du dépôt](https://github.com/Nerow75/ludex/issues).
