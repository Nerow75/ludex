//! Xbox et Game Pass : paquets Windows (MSIX) listés dans le registre des paquets de
//! l'utilisateur. Un paquet est un jeu s'il contient un `MicrosoftGame.config` à sa racine.
//! Lancement par `shell:AppsFolder\<famille>!<application>`.

use super::{art_url, xml_attr, Game, Launch, LocalArt, Platform};
use std::fs;
use std::path::{Path, PathBuf};

/// `Nom_Version_Archi_Ressource_Éditeur` → `Nom_Éditeur`.
fn family_name(full_name: &str) -> Option<String> {
    let parts: Vec<&str> = full_name.split('_').collect();
    (parts.len() == 5).then(|| format!("{}_{}", parts[0], parts[4]))
}

/// Fichier `MicrosoftGame.config` (la casse varie d'un jeu à l'autre).
fn game_config(root: &Path) -> Option<PathBuf> {
    fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.eq_ignore_ascii_case("MicrosoftGame.config")))
}

/// Nom affichable : jamais une référence de ressource `ms-resource:`.
fn readable(name: Option<String>) -> Option<String> {
    name.filter(|n| !n.is_empty() && !n.starts_with("ms-resource:"))
}

fn build(full_name: &str, root: &Path, registry_name: Option<String>) -> Option<Game> {
    let config = fs::read_to_string(game_config(root)?).ok()?;
    let manifest = fs::read_to_string(root.join("AppxManifest.xml")).ok()?;
    let family = family_name(full_name)?;
    let app_id = xml_attr(&manifest, "Application", "Id")?;
    let title = readable(xml_attr(&config, "ShellVisuals", "DefaultDisplayName"))
        .or_else(|| readable(registry_name))
        .unwrap_or_else(|| family.split('_').next().unwrap_or(&family).to_string());

    let windir = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let launch = Launch::Exec {
        path: windir.join("explorer.exe"),
        args: vec![format!(r"shell:AppsFolder\{family}!{app_id}")],
    };
    let mut game = Game::new(Platform::Xbox, &family, &title, launch);
    game.install_dir = Some(root.to_string_lossy().into_owned());

    // Écran de démarrage du jeu (souvent 1920×1080) comme visuel large.
    let splash = xml_attr(&config, "ShellVisuals", "SplashScreenImage").map(|f| root.join(f)).filter(|p| p.is_file());
    if splash.is_some() {
        game.hero_url = Some(art_url(&game, "hero"));
    }
    game.local_art = LocalArt { hero: splash, ..LocalArt::default() };
    Some(game)
}

pub fn scan() -> Result<Option<Vec<Game>>, String> {
    #[cfg(windows)]
    {
        use super::reg;
        let packages = r"Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages";
        let Some(repo) = reg::hkcu(packages) else { return Ok(None) };
        let games = reg::children(&repo)
            .into_iter()
            .filter_map(|(full_name, key)| {
                let root = reg::string(&key, "PackageRootFolder")?;
                build(&full_name, Path::new(&root), reg::string(&key, "DisplayName"))
            })
            .collect();
        Ok(Some(games))
    }
    #[cfg(not(windows))]
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_game_from_package() {
        assert_eq!(
            family_name("Microsoft.4297127D64EC6_2.6.2.0_x64__8wekyb3d8bbwe").as_deref(),
            Some("Microsoft.4297127D64EC6_8wekyb3d8bbwe")
        );
        let root = std::env::temp_dir().join(format!("ludex-xbox-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("MicrosoftGame.Config"), r#"<Game><ShellVisuals DefaultDisplayName="Minecraft Launcher" SplashScreenImage="Splash.png"/></Game>"#).unwrap();
        fs::write(root.join("AppxManifest.xml"), r#"<Applications><Application Id="Game" Executable="Minecraft.exe"/></Applications>"#).unwrap();
        fs::write(root.join("Splash.png"), b"x").unwrap();

        let game = build("Microsoft.4297127D64EC6_2.6.2.0_x64__8wekyb3d8bbwe", &root, None).unwrap();
        assert_eq!(game.title, "Minecraft Launcher");
        assert_eq!(game.id, "xbox:Microsoft.4297127D64EC6_8wekyb3d8bbwe");
        assert!(game.launch_label.ends_with(r"shell:AppsFolder\Microsoft.4297127D64EC6_8wekyb3d8bbwe!Game"));
        assert!(game.hero_url.unwrap().ends_with("/xbox/Microsoft.4297127D64EC6_8wekyb3d8bbwe/hero"));

        // Un paquet sans MicrosoftGame.config n'est pas un jeu.
        fs::remove_file(root.join("MicrosoftGame.Config")).unwrap();
        assert!(build("A_1_x64__b", &root, None).is_none());
        let _ = fs::remove_dir_all(&root);
    }
}
