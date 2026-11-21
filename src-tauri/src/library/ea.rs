//! EA app (et Origin) : chaque jeu garde un `__Installer\installerdata.xml` dans son dossier, avec
//! ses identifiants de contenu, son titre et ses exécutables (`<runtime><launcher>`). Les dossiers
//! viennent de la recherche des dossiers de jeux (`local.rs`). Le jeu est lancé par son propre
//! exécutable : un jeu EA ouvre lui-même l'EA app s'il en a besoin.

use super::local::{self, Kind};
use super::{program_data, Game, Launch, Platform};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// Blocs `<tag>…</tag>` successifs d'un XML simple.
fn blocks<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let (open, close) = (format!("<{tag}"), format!("</{tag}>"));
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = xml[from..].find(&open) {
        let start = from + i + open.len();
        if !xml[start..].starts_with(['>', ' ', '\t', '\n', '\r']) {
            from = start;
            continue;
        }
        let Some(body) = xml[start..].find('>').map(|j| start + j + 1) else { break };
        let Some(end) = xml[body..].find(&close).map(|j| body + j) else { break };
        out.push(&xml[body..end]);
        from = end + close.len();
    }
    out
}

/// Texte du premier `<tag>`, éventuellement filtré sur sa langue.
fn text(xml: &str, tag: &str, locale: Option<&str>) -> Option<String> {
    let open = format!("<{tag}");
    let mut from = 0;
    while let Some(i) = xml[from..].find(&open) {
        let start = from + i;
        let head_end = start + xml[start..].find('>')?;
        let head = &xml[start..head_end];
        from = head_end;
        if locale.is_some_and(|l| !head.contains(&format!("locale=\"{l}\""))) {
            continue;
        }
        let end = xml[head_end..].find(&format!("</{tag}>"))? + head_end;
        let value = xml[head_end + 1..end].trim();
        return (!value.is_empty()).then(|| value.replace("&amp;", "&"));
    }
    None
}

/// « Need for Speed(TM) Rivals » → « Need for Speed Rivals ».
fn clean_title(title: &str) -> String {
    let t = title.replace("(TM)", "").replace("(R)", "").replace("™", "").replace("®", "").replace("(x64)", "").replace("(x86)", "");
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Jeu EA depuis son manifeste. `game_dir` : dossier qui contient `__Installer`.
fn build(xml: &str, game_dir: &Path) -> Option<Game> {
    let content_id = blocks(xml, "contentIDs").first().and_then(|b| text(b, "contentID", None))?;

    // Premier exécutable qui n'est pas la version d'essai.
    let launcher = blocks(xml, "launcher").into_iter().find(|b| {
        let file = text(b, "filePath", None).unwrap_or_default();
        text(b, "trial", None).as_deref() != Some("1") && local::is_game_exe_name(file.rsplit(['\\', ']', '/']).next().unwrap_or(""))
    });
    let exe = launcher
        .and_then(|b| text(b, "filePath", None))
        .map(|p| p.rsplit(']').next().unwrap_or(&p).trim_start_matches(['/', '\\']).replace('/', "\\"))
        .map(|rel| game_dir.join(rel))
        .filter(|p| p.is_file())
        .or_else(|| local::main_exe(game_dir))?;
    let args: Vec<String> = launcher
        .and_then(|b| text(b, "parameters", None))
        .map(|p| p.split_whitespace().map(String::from).collect())
        .unwrap_or_default();

    let folder = game_dir.file_name().and_then(|n| n.to_str()).unwrap_or("Jeu EA");
    let title = ["fr_FR", "en_US"]
        .into_iter()
        .find_map(|l| text(xml, "gameTitle", Some(l)))
        .or_else(|| launcher.and_then(|b| text(b, "name", Some("en_US"))))
        .unwrap_or_else(|| local::pretty_folder(folder));

    let mut game = Game::new(Platform::Ea, &content_id, &clean_title(&title), Launch::Exec { path: exe, args });
    game.install_dir = Some(game_dir.to_string_lossy().into_owned());
    Some(game)
}

pub fn scan() -> Result<Option<Vec<Game>>, String> {
    let app = program_data().join("EA Desktop").is_dir() || program_data().join("Origin").is_dir();
    let mut seen = HashSet::new();
    let games: Vec<Game> = local::candidates()
        .into_iter()
        .filter_map(|dir| match local::kind(&dir) {
            Kind::Ea(manifest) => {
                let game_dir = manifest.parent()?.parent()?.to_path_buf();
                let game = fs::read_to_string(&manifest).ok().and_then(|text| build(&text, &game_dir));
                if game.is_none() {
                    let folder = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    super::skip(&folder, tr_str!("Manifeste EA illisible ou sans exécutable", "EA manifest unreadable or without an executable"));
                }
                game
            }
            _ => None,
        })
        .filter(|g| {
            // Même jeu copié dans deux dossiers.
            let first = seen.insert(g.platform_id.clone());
            if !first {
                super::skip(&g.title, tr_str!("Même jeu déjà vu dans un autre dossier", "Same game already found in another folder"));
            }
            first
        })
        .collect();
    Ok((app || !games.is_empty()).then_some(games))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FC24: &str = r#"<DiPManifest>
  <contentIDs>
    <contentID>198235</contentID>
  </contentIDs>
  <gameTitles>
    <gameTitle locale="en_US">EA SPORTS FC™ 24</gameTitle>
    <gameTitle locale="fr_FR">EA SPORTS FC 24</gameTitle>
  </gameTitles>
  <runtime>
    <launcher uid="1-2">
      <name locale="en_US">EA SPORTS FC 24 Trial</name>
      <filePath>[HKEY_LOCAL_MACHINE\SOFTWARE\EA Sports\EA SPORTS FC 24\Install Dir]FC24_Trial.exe</filePath>
      <parameters>-trial</parameters>
      <trial>1</trial>
    </launcher>
    <launcher uid="2-2">
      <filePath>[HKEY_LOCAL_MACHINE\SOFTWARE\EA Sports\EA SPORTS FC 24\Install Dir]FC24.exe</filePath>
      <parameters>-dx12 -novid</parameters>
      <trial>0</trial>
    </launcher>
  </runtime>
</DiPManifest>"#;

    #[test]
    fn builds_from_installer_data() {
        let dir = std::env::temp_dir().join(format!("ludex-ea-{}", std::process::id())).join("EA SPORTS FC 24");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("FC24.exe"), b"").unwrap();
        fs::write(dir.join("FC24_Trial.exe"), b"").unwrap();

        let game = build(FC24, &dir).unwrap();
        assert_eq!(game.id, "ea:198235");
        assert_eq!(game.title, "EA SPORTS FC 24");
        assert_eq!(game.launch_label, "FC24.exe -dx12 -novid");

        // Sans titre : nom de l'exécutable choisi dans le dossier, titre tiré du dossier.
        let nfs = r#"<contentIDs><contentID>1004776</contentID><contentID>1012933</contentID></contentIDs>
            <runtime><launcher><filePath>[HKEY_LOCAL_MACHINE\SOFTWARE\EA Games\NFS\Install Dir]FC24.exe</filePath></launcher></runtime>"#;
        let other = build(nfs, &dir).unwrap();
        assert_eq!(other.id, "ea:1004776");
        assert_eq!(clean_title("Need for Speed(TM) Rivals (x64)"), "Need for Speed Rivals");
        assert!(build("<runtime/>", &dir).is_none());
        let _ = fs::remove_dir_all(dir.parent().unwrap());
    }
}
