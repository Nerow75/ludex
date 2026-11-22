//! Requêtes HTTP de la recherche des jaquettes, des fiches et de la base des sauvegardes : jamais de
//! redirection suivie, comme partout ailleurs dans Ludex (une redirection pourrait mener vers un autre
//! hôte que celui choisi). Avec `redirects(0)`, ureq rend une redirection comme une réponse ordinaire :
//! `checked` la transforme en erreur, notée par l'appelant (diagnostic, journal), sans plantage.

use std::time::Duration;

/// User-Agent honnête des requêtes vers les services publics.
pub const USER_AGENT: &str = "Ludex (lanceur de jeux)";

/// Agent sans redirection, délai de `timeout` pour toute la requête.
pub fn agent(timeout: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(timeout).redirects(0).user_agent(USER_AGENT).build()
}

/// Réponse gardée si son statut est un succès (2xx) ou l'un de `allowed` (304 d'une requête
/// conditionnelle) ; sinon une erreur qui dit pourquoi (redirection refusée, statut inattendu).
/// Les statuts 4xx et 5xx arrivent déjà en erreur de ureq.
pub fn checked(response: ureq::Response, allowed: &[u16]) -> Result<ureq::Response, String> {
    match refusal(response.status(), response.header("Location")) {
        Some(e) if !allowed.contains(&response.status()) => Err(e),
        _ => Ok(response),
    }
}

/// Raison du refus d'un statut, `None` pour un succès.
fn refusal(status: u16, location: Option<&str>) -> Option<String> {
    if (200..300).contains(&status) {
        return None;
    }
    if (300..400).contains(&status) {
        // Seul l'hôte de la destination est noté : une adresse entière pourrait être longue ou bizarre.
        let host = location.and_then(host_of).unwrap_or_default();
        return Some(if host.is_empty() {
            tr!("redirection {status} refusée", "redirect {status} refused")
        } else {
            tr!("redirection {status} vers {host} refusée", "redirect {status} to {host} refused")
        });
    }
    Some(tr!("réponse {status} inattendue", "unexpected response {status}"))
}

/// Hôte d'une adresse absolue (`https://hôte/…`), lettres, chiffres, `.` et `-` seulement.
fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let host = rest.split(['/', '?', '#', ':']).next()?;
    let ok = !host.is_empty() && host.len() <= 253 && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    ok.then(|| host.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn statuts() {
        crate::i18n::with(crate::i18n::Lang::Fr, || {
            assert_eq!(refusal(200, None), None);
            assert_eq!(refusal(204, None), None);
            assert_eq!(refusal(302, Some("https://Evil.example/x?y")).as_deref(), Some("redirection 302 vers evil.example refusée"));
            assert_eq!(refusal(301, Some("/relatif")).as_deref(), Some("redirection 301 refusée"));
            assert_eq!(refusal(307, Some("https://a b/")).as_deref(), Some("redirection 307 refusée"));
            assert_eq!(refusal(304, None).as_deref(), Some("redirection 304 refusée"));
            assert_eq!(refusal(101, None).as_deref(), Some("réponse 101 inattendue"));
        });
        crate::i18n::with(crate::i18n::Lang::En, || {
            assert_eq!(refusal(302, Some("http://cdn.example:8080/")).as_deref(), Some("redirect 302 to cdn.example refused"));
        });
    }

    /// Faux serveur local qui répond une fois `reply` ; renvoie son adresse.
    fn serve(reply: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(reply.as_bytes());
            }
        });
        format!("http://{addr}/")
    }

    #[test]
    fn redirection_jamais_suivie() {
        // La destination n'existe pas : la suivre donnerait une erreur de connexion, pas ce refus.
        let url = serve("HTTP/1.1 302 Found\r\nLocation: http://ailleurs.invalid/piege\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        let response = agent(Duration::from_secs(5)).get(&url).call().expect("réponse 302 rendue telle quelle");
        assert_eq!(response.status(), 302);
        let err = crate::i18n::with(crate::i18n::Lang::Fr, || checked(response, &[]).unwrap_err());
        assert_eq!(err, "redirection 302 vers ailleurs.invalid refusée");
    }

    #[test]
    fn succes_et_statut_permis() {
        let url = serve("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}");
        let response = checked(agent(Duration::from_secs(5)).get(&url).call().unwrap(), &[]).unwrap();
        assert_eq!(response.into_string().unwrap(), "{}");
        let url = serve("HTTP/1.1 304 Not Modified\r\nConnection: close\r\n\r\n");
        assert_eq!(checked(agent(Duration::from_secs(5)).get(&url).call().unwrap(), &[304]).unwrap().status(), 304);
    }
}
