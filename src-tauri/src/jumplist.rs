//! Jump List de la barre des tâches : clic droit sur l'icône de Ludex = les 5 derniers jeux joués
//! (mêmes jeux que le menu de la zone de notification). Chaque entrée relance l'exécutable de Ludex
//! avec le lien `ludex://launch/<id>` pour seul argument : l'instance unique le relaie à Ludex
//! déjà ouvert, ou Ludex démarre et lance le jeu après son premier scan (`deeplink.rs`).
//!
//! L'identifiant d'application (AppUserModelID) est celui que l'installeur NSIS de Tauri pose sur
//! ses raccourcis (`identifier` de `tauri.conf.json`) : la barre des tâches range ainsi la fenêtre,
//! l'icône épinglée et la Jump List ensemble, et la désinstallation efface la liste.

use crate::deeplink;
use crate::library::Game;
use crate::shortcut;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{channel, Sender};

/// AppUserModelID de Ludex : `identifier` de `tauri.conf.json`, repris par l'installeur.
pub const APP_ID: &str = "com.ludex.app";
/// Titre de la catégorie de la Jump List, dans la langue actuelle.
pub fn category() -> String {
    tr!("Jeux récents", "Recent games")
}
/// Au-delà, le titre est coupé : la Jump List a une largeur fixe.
const MAX_TITLE: usize = 60;

/// Entrée de la Jump List.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JumpItem {
    pub title: String,
    /// Argument passé à Ludex : le lien du jeu.
    pub args: String,
    pub icon: PathBuf,
}

fn short_title(title: &str) -> String {
    let title = title.trim();
    if title.chars().count() > MAX_TITLE {
        format!("{}…", title.chars().take(MAX_TITLE - 1).collect::<String>().trim_end())
    } else {
        title.to_string()
    }
}

/// Entrées pour les jeux récents (déjà choisis par `tray::recent`) : lien du jeu, icône de son
/// exécutable s'il est le sien, sinon celle de Ludex.
pub fn items(recent: &[&Game], ludex_exe: &Path) -> Vec<JumpItem> {
    recent
        .iter()
        .map(|g| JumpItem {
            title: short_title(&g.title),
            args: deeplink::launch_url(&g.id),
            icon: shortcut::game_exe(g).unwrap_or(ludex_exe).to_path_buf(),
        })
        .collect()
}

/// Identifiant d'application du processus : à poser avant la création de toute fenêtre.
pub fn set_process_app_id() {
    #[cfg(windows)]
    {
        let id = windows::core::HSTRING::from(APP_ID);
        // SAFETY : appel sans pointeur conservé, chaîne valide pendant l'appel.
        if let Err(e) = unsafe { windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(&id) } {
            eprintln!("Identifiant d'application non posé : {e}");
        }
    }
}

/// Liste demandée : titre de la catégorie (langue du moment) et entrées. Comparée entière, pour
/// que la liste soit reposée après un changement de langue même si les jeux n'ont pas changé.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Request {
    category: String,
    items: Vec<JumpItem>,
}

/// Fil qui pose la Jump List (COM) : la dernière liste demandée seulement, jamais deux fois la même.
pub struct JumpList {
    exe: Option<PathBuf>,
    tx: Mutex<Option<Sender<Request>>>,
}

impl JumpList {
    /// `exe` : exécutable de Ludex ; absent, la Jump List n'est jamais posée.
    pub fn spawn(exe: Option<PathBuf>) -> Self {
        let tx = exe.as_ref().and_then(|_| {
            let (tx, rx) = channel::<Request>();
            let spawned = std::thread::Builder::new().name("jump-list".into()).spawn(move || {
                #[cfg(windows)]
                let _com = com::Apartment::enter();
                let mut last: Option<Request> = None;
                while let Ok(mut request) = rx.recv() {
                    // Plusieurs demandes en attente : seule la dernière compte.
                    while let Ok(newer) = rx.try_recv() {
                        request = newer;
                    }
                    if last.as_ref() == Some(&request) {
                        continue;
                    }
                    #[cfg(windows)]
                    match com::apply(&request.category, &request.items) {
                        Ok(()) => last = Some(request),
                        Err(e) => eprintln!("Jump List non mise à jour : {e}"),
                    }
                    #[cfg(not(windows))]
                    {
                        last = Some(request);
                    }
                }
            });
            spawned.map_err(|e| eprintln!("Jump List indisponible : {e}")).ok().map(|_| tx)
        });
        JumpList { exe, tx: Mutex::new(tx) }
    }

    /// Demande la Jump List des jeux récents, sans attendre.
    pub fn set(&self, recent: &[&Game]) {
        let Some(exe) = &self.exe else { return };
        let request = Request { category: category(), items: items(recent, exe) };
        if let Ok(tx) = self.tx.lock()
            && let Some(tx) = tx.as_ref()
        {
            let _ = tx.send(request);
        }
    }
}

#[cfg(windows)]
mod com {
    use super::{JumpItem, APP_ID};
    #[cfg(test)]
    use windows::Win32::UI::Shell::{ApplicationDestinations, IApplicationDestinations};
    use windows::core::{Interface, Result, HSTRING, PWSTR};
    use windows::Win32::Storage::EnhancedStorage::PKEY_Title;
    use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PROPVARIANT};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemAlloc, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::System::Variant::VT_LPWSTR;
    use windows::Win32::UI::Shell::Common::{IObjectArray, IObjectCollection};
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::UI::Shell::{DestinationList, EnumerableObjectCollection, ICustomDestinationList, IShellLinkW, ShellLink};

    /// COM initialisé pour ce fil tant que la valeur vit.
    pub struct Apartment(bool);

    impl Apartment {
        pub fn enter() -> Self {
            // SAFETY : initialisation de COM pour le fil courant, défaite à la fin du fil.
            Apartment(unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok())
        }
    }

    impl Drop for Apartment {
        fn drop(&mut self) {
            if self.0 {
                // SAFETY : appariée au `CoInitializeEx` réussi de `enter`.
                unsafe { CoUninitialize() };
            }
        }
    }

    /// Valeur `VT_LPWSTR` (texte alloué par COM, rendu par `PropVariantClear`).
    fn text_value(text: &str) -> Result<PROPVARIANT> {
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY : tampon de la bonne taille, rempli puis confié au PROPVARIANT qui le libère.
        unsafe {
            let ptr = CoTaskMemAlloc(wide.len() * 2) as *mut u16;
            if ptr.is_null() {
                return Err(windows::Win32::Foundation::E_OUTOFMEMORY.into());
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
            let mut value = PROPVARIANT::default();
            let inner = &mut value.Anonymous.Anonymous;
            inner.vt = VT_LPWSTR;
            inner.Anonymous.pwszVal = PWSTR(ptr);
            Ok(value)
        }
    }

    fn shell_link(exe: &HSTRING, item: &JumpItem) -> Result<IShellLinkW> {
        // SAFETY : objets COM du shell, chaînes valides pendant chaque appel.
        unsafe {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            link.SetPath(exe)?;
            link.SetArguments(&HSTRING::from(item.args.as_str()))?;
            link.SetIconLocation(&HSTRING::from(item.icon.as_path()), 0)?;
            link.SetDescription(&HSTRING::from(item.title.as_str()))?;
            let store: IPropertyStore = link.cast()?;
            let mut title = text_value(&item.title)?;
            let written = store.SetValue(&PKEY_Title, &title).and_then(|_| store.Commit());
            let _ = PropVariantClear(&mut title);
            written?;
            Ok(link)
        }
    }

    /// Arguments des entrées que l'utilisateur a retirées de la liste (« Supprimer de cette liste ») :
    /// Windows refuse qu'on les remette.
    fn removed_args(removed: &IObjectArray) -> Vec<String> {
        let mut out = Vec::new();
        // SAFETY : lecture d'objets du tableau renvoyé par `BeginList`.
        unsafe {
            for i in 0..removed.GetCount().unwrap_or(0) {
                let Ok(link) = removed.GetAt::<IShellLinkW>(i) else { continue };
                let mut buf = [0u16; 2048];
                if link.GetArguments(&mut buf).is_ok() {
                    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                    out.push(String::from_utf16_lossy(&buf[..len]));
                }
            }
        }
        out
    }

    /// Remplace la Jump List : catégorie « Jeux récents » (`category`, absente sans jeu récent).
    pub fn apply(category: &str, items: &[JumpItem]) -> Result<()> {
        apply_for(APP_ID, category, items)
    }

    /// Jump List de l'application `app_id` (Ludex, ou un identifiant d'essai dans les tests).
    pub fn apply_for(app_id: &str, category: &str, items: &[JumpItem]) -> Result<()> {
        let exe = match std::env::current_exe() {
            Ok(p) => HSTRING::from(p.as_path()),
            Err(_) => return Ok(()),
        };
        // SAFETY : séquence documentée de `ICustomDestinationList` (BeginList, ajouts, CommitList ou
        // AbortList), sur un fil où COM est initialisé.
        unsafe {
            let list: ICustomDestinationList = CoCreateInstance(&DestinationList, None, CLSCTX_INPROC_SERVER)?;
            list.SetAppID(&HSTRING::from(app_id))?;
            let mut slots = 0u32;
            let removed: IObjectArray = list.BeginList(&mut slots)?;
            let removed = removed_args(&removed);
            let filled = (|| -> Result<()> {
                let collection: IObjectCollection = CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)?;
                let mut count = 0;
                for item in items.iter().filter(|i| !removed.contains(&i.args)).take(slots.max(1) as usize) {
                    collection.AddObject(&shell_link(&exe, item)?)?;
                    count += 1;
                }
                if count > 0 {
                    let array: IObjectArray = collection.cast()?;
                    list.AppendCategory(&HSTRING::from(category), &array)?;
                }
                list.CommitList()
            })();
            if filled.is_err() {
                let _ = list.AbortList();
            }
            filled
        }
    }

    /// Efface la Jump List de `app_id` (tests seulement).
    #[cfg(test)]
    pub fn clear_for(app_id: &str) -> Result<()> {
        // SAFETY : objets COM du shell, fil où COM est initialisé.
        unsafe {
            let list: ICustomDestinationList = CoCreateInstance(&DestinationList, None, CLSCTX_INPROC_SERVER)?;
            list.DeleteList(&HSTRING::from(app_id))?;
            let dest: IApplicationDestinations = CoCreateInstance(&ApplicationDestinations, None, CLSCTX_INPROC_SERVER)?;
            dest.SetAppID(&HSTRING::from(app_id))?;
            let _ = dest.RemoveAllDestinations();
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{Launch, Platform};

    #[test]
    fn app_id_is_the_installer_one() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(conf["identifier"], APP_ID);
        let schemes = &conf["plugins"]["deep-link"]["desktop"]["schemes"];
        assert_eq!(schemes, &serde_json::json!([deeplink::SCHEME]));
    }

    #[test]
    fn items_launch_each_recent_game_through_ludex() {
        let ludex = Path::new(r"C:\Ludex\ludex.exe");
        let cs = Game::new(Platform::Steam, "730", "Counter-Strike 2", Launch::Uri("steam://rungameid/730".into()));
        let long = Game::new(Platform::Epic, "Fortnite & co", &"Long".repeat(30), Launch::Uri(String::new()));
        let list = items(&[&cs, &long], ludex);
        assert_eq!(
            list[0],
            JumpItem { title: "Counter-Strike 2".into(), args: "ludex://launch/steam:730".into(), icon: ludex.to_path_buf() }
        );
        assert_eq!(list[1].args, "ludex://launch/epic:Fortnite%20%26%20co");
        assert_eq!(deeplink::parse(&list[1].args), Ok("epic:Fortnite & co".into()));
        assert_eq!(list[1].title.chars().count(), MAX_TITLE);
        assert!(list[1].title.ends_with('…'));
        assert!(items(&[], ludex).is_empty());
    }

    #[test]
    fn category_follows_the_language() {
        use crate::i18n::{Lang, with};
        assert_eq!(with(Lang::Fr, category), "Jeux récents");
        assert_eq!(with(Lang::En, category), "Recent games");
        // Mêmes jeux, autre langue : la demande diffère, la liste sera reposée.
        let fr = Request { category: with(Lang::Fr, category), items: Vec::new() };
        let en = Request { category: with(Lang::En, category), items: Vec::new() };
        assert_ne!(fr, en);
    }

    /// Pose une vraie Jump List sous un identifiant d'essai (jamais celui de Ludex), puis l'efface :
    /// vérifie les appels COM (lien, arguments, icône, titre) sur ce PC.
    /// `cargo test jump_list_this_pc -- --ignored --nocapture`
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn jump_list_this_pc() {
        const TEST_ID: &str = "com.ludex.essai-jump-list";
        let _com = com::Apartment::enter();
        let exe = std::env::current_exe().unwrap();
        let games = [
            Game::new(Platform::Steam, "730", "Counter-Strike 2", Launch::Uri("steam://rungameid/730".into())),
            Game::new(Platform::Other, "manual-éé", "Jeu été & co", Launch::Uri(String::new())),
        ];
        let list = items(&games.iter().collect::<Vec<_>>(), &exe);
        let start = std::time::Instant::now();
        com::apply_for(TEST_ID, &category(), &list).expect("Jump List posée");
        println!("Jump List de {} entrées posée en {:?}", list.len(), start.elapsed());
        com::apply_for(TEST_ID, &category(), &[]).expect("Jump List vidée");
        com::clear_for(TEST_ID).expect("Jump List effacée");
        println!("Effacée.");
    }
}
