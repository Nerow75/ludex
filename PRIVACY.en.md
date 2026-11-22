# Privacy

[Français](PRIVACY.md) · **English**

Ludex is a local game launcher. It has no server and no telemetry and asks for no account (the GOG and Epic Games accounts and the Steam Web API key are optional and off by default): no usage statistics, no crash report and no identifier of your PC is ever sent. This document lists everything Ludex contacts on the Internet and what it sends there. It is kept up to date from the code (version 0.2.0).

## What Ludex reads on your PC

To find your games, Ludex reads, read only, the files and registry keys of the installed launchers (Steam libraries, Epic manifests, the local databases of GOG Galaxy, Amazon Games and itch.io, Ubisoft, Rockstar and Xbox registry entries…), the "Jeux" or "Games" folders on your drives and known save folders. None of this leaves your PC.

What Ludex writes stays in `%APPDATA%\com.ludex.app` (preferences, play history, caches, error log) and, for save backups, in the folder you pick (`Documents\Ludex\Sauvegardes` by default).

## Services contacted

Every request goes straight from your PC to the service, over HTTPS, without cookies or identifiers. Each service sees your IP address and Ludex's `User-Agent` header (`Ludex (lanceur de jeux)`, with the version number for HowLongToBeat, plain `Ludex` for the save database), as with any connection. Redirects are refused for images, HowLongToBeat, news, system requirements, achievements, mods and the wishlist; they are followed for updates (GitHub release files), the save database, the cover search and Steam game details.

### Ludex updates

| Host | When | Sent |
| --- | --- | --- |
| `github.com` (then the GitHub file server it redirects to) | 8 s after opening, then every 12 h until a version is found; the installer is only downloaded after you click "Install and restart" | Nothing: fixed address `releases/latest/download/latest.json`; the Tauri updater's `User-Agent` |

### Save database

| Host | When | Sent |
| --- | --- | --- |
| `raw.githubusercontent.com` | At most once a week, after a scan | Nothing: downloads the [Ludusavi](https://github.com/mtkennerly/ludusavi-manifest) manifest; an unchanged manifest isn't downloaded again (`If-None-Match` with the tag GitHub gave) |

### Steam (no account)

| Host and address | When | Sent |
| --- | --- | --- |
| `store.steampowered.com/api/storesearch` | After a scan, for each non-Steam game without a cover | Game title, language, country `FR` |
| `store.steampowered.com/api/storesearch` | When you search for a game in the wishlist | The text you typed (one line, 100 characters at most), language, country `FR` |
| `store.steampowered.com/api/appdetails` | When a game's Overview tab opens (system requirements, kept 30 days) | The game's Steam number (appid), language |
| `store.steampowered.com/api/appdetails` | At most once a day (wishlist prices) | Steam numbers of the wishlist games, country `fr` |
| `api.steampowered.com/IStoreBrowseService/GetItems` | After a scan (game details, image paths), kept 30 days | Steam numbers of the games, language, country `FR` |
| `api.steampowered.com/IStoreService/GetTagList` | At most every 30 days | Language |
| `api.steampowered.com/ISteamNews/GetNewsForApp` | When a game page opens (if its news is over 6 h old), and once a day for "Playing" games | The game's Steam number |
| `api.steampowered.com/ISteamUserStats/GetGlobalAchievementPercentagesForApp` | Showcase and achievement list, kept 7 days | The game's Steam number |
| `api.steampowered.com/ISteamRemoteStorage/GetPublishedFileDetails` | Mods tab of a Steam game, kept 30 days | Numbers of the installed Workshop mods |

### Microsoft Store (no account)

| Host | When | Sent |
| --- | --- | --- |
| `displaycatalog.mp.microsoft.com` | After a scan, for a game without a cover that isn't on Steam | Game title, then the number of the product found; market `FR`, language `fr-fr` |

### Epic free games (no account)

| Host | When | Sent |
| --- | --- | --- |
| `store-site-backend-static.ak.epicgames.com/freeGamesPromotions` | When the "Wishlist" section opens, and in the background (every 6 h at most, shortly after an offer starts or ends) if the "Free games on Epic" notification is on | Ludex language (`fr` or `en-US`) and country `FR`; nothing else |

### HowLongToBeat

| Host | When | Sent |
| --- | --- | --- |
| `howlongtobeat.com` | After a scan, one request every 1.2 s, each title kept 30 days | A request for a search token (`init`), then the words of the game title; if the search address has changed, the home page and its scripts are read to find it again |

### Optional accounts: GOG and Epic Games (off by default)

Only if you connect these accounts in Settings > Accounts, for the "Not installed" section (games you own that aren't on this PC). You sign in on the store's own page, in a separate private window: Ludex never sees your login or password. It keeps only the store's tokens, encrypted for your Windows account (DPAPI) in `comptes.bin`, never shown to the interface, the log, the Diagnostic or the export. "Sign out" erases the tokens and the list.

| Host | When | Sent |
| --- | --- | --- |
| `auth.gog.com`, `embed.gog.com`, `login.gog.com` | Sign-in (GOG's page), then token exchange and renewal | The sign-in code given by GOG, then the refresh token; public client id of GOG Galaxy |
| `embed.gog.com/user/data/games` | List of your games, read again after 6 h or with "Reload" | GOG's access token |
| `api.gog.com/products` | Public pages of owned games (kept 30 days), then a game's installer when you click "Install" | GOG game numbers; the access token only to ask for the download link |
| `*.gog.com` (GOG download servers) | Installer download, after your click on "Install" | Nothing but the address given by GOG |
| `www.epicgames.com` | Sign-in (Epic's page, in its own window) | What you type on Epic's page, which Ludex doesn't read |
| `account-public-service-prod03.ol.epicgames.com` | Token exchange, renewal and revocation (on sign-out) | The sign-in code given by Epic, then the refresh token; public client id of the Epic launcher |
| `library-service.live.use1a.on.epicgames.com` | List of your games, read again after 6 h or with "Reload" | Epic's access token |
| `catalog-public-service-prod06.ol.epicgames.com` | Owned game details (title, type, platform, image), kept 30 days | Catalog numbers of the games; Epic's access token |

Ludex downloads nothing for Epic: "Install in Epic" opens the Epic launcher on your PC on the game.

### Optional Steam Web API key (none by default)

Without a key, the "Not installed" section shows Steam games already launched on your account, read on your PC (Steam's `localconfig.vdf` and `appinfo.vdf`): nothing is sent. Only if you paste your personal Steam Web API key in Settings > Accounts does Ludex ask Steam for the full list of your games. The key is checked by that same request, then encrypted for your Windows account (DPAPI) in `comptes.bin`, never shown to the interface, the log, the Diagnostic or the export. "Remove key" erases it along with the list.

| Host | When | Sent |
| --- | --- | --- |
| `api.steampowered.com/IPlayerService/GetOwnedGames` | When the key is saved, then list read again after 6 h or with "Refresh" | Your Web API key and the Steam ID (SteamID64) of this PC's account |

"Get a key" opens `steamcommunity.com/dev/apikey` in your browser. "Install in Steam" opens Steam on the game (`steam://install/<number>`): Ludex downloads nothing for Steam.

### Images

These addresses are built by Ludex from a Steam number or an image name; Ludex downloads some images to its cache, the interface shows others directly.

| Host | Content | Sent |
| --- | --- | --- |
| `cdn.cloudflare.steamstatic.com`, `shared.akamai.steamstatic.com` | Covers, background art, logos, achievement icons, wishlist images | The image address (with the game's Steam number in it) |
| `store-images.s-microsoft.com` | Covers and background art found in the Microsoft Store | The image address |
| `clan.akamai.steamstatic.com`, `clan.fastly.steamstatic.com` | Steam news images | The image address |
| `images.steamusercontent.com` | Workshop mod thumbnails | The thumbnail address |
| `cdn1.epicgames.com`, `cdn2.unrealengine.com` | Images of Epic free games, thumbnails of Epic games not installed (Epic account connected) | The image address (resized by the CDN) |
| `shared.akamai.steamstatic.com` | Headers of Steam games not installed | Image address (with the game's Steam number) |
| `images.gog-statics.com` | Thumbnails of GOG games not installed (GOG account connected) | The thumbnail address |

## Never sent

- Your Windows user name, your folder paths, your PC's name.
- Your Steam account identifier (read from disk to find your playtime), except to `api.steampowered.com` if you save a Steam Web API key.
- Your playtime, history, favorites, statuses, notes, collections, launch options.
- The contents of your files, saves or screenshots.

## Only sent if you do it

- **Titles**: the title sent to Steam, the Microsoft Store and HowLongToBeat is the one Ludex shows. For an "Other" game it comes from the executable or folder name; if you rename a game, the new title is used. Search aliases stay on your PC.
- **Links**: "Open the repository", "Read on Steam", "View in Steam", "View on Epic" open your browser, Steam or the Epic launcher on an address set by Ludex.
- **Diagnostics report**: copied to the clipboard only when you click "Copy report". It is anonymized (folder and user name replaced by `%USERPROFILE%` and `%USERNAME%`, Steam account hidden); you decide who to send it to.
- **Data export**: a `.zip` archive written where you choose, never sent.
- **Discord status** (off by default): when on, Ludex talks to the Discord client on your PC through a local channel; Discord then shows your friends the game title, its launcher, the start time and the game's Steam image (or just "In game" if you hide the name). Ludex doesn't contact Discord's servers.

## Installer

The NSIS installer downloads WebView2 from Microsoft only if your PC lacks it (Windows 11 already has it). It sends nothing else.

## Contact

Questions or reports: [repository issues](https://github.com/Nerow75/ludex/issues).
