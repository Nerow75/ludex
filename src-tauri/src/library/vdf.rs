//! Lecteur minimal du format texte KeyValues (VDF) de Valve.

#[derive(Debug, Clone)]
pub enum Vdf {
    Str(String),
    Obj(Vec<(String, Vdf)>),
}

impl Vdf {
    pub fn get(&self, key: &str) -> Option<&Vdf> {
        match self {
            Vdf::Obj(entries) => entries
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v),
            Vdf::Str(_) => None,
        }
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Vdf::Str(s) => Some(s),
            Vdf::Obj(_) => None,
        }
    }

    pub fn entries(&self) -> &[(String, Vdf)] {
        match self {
            Vdf::Obj(entries) => entries,
            Vdf::Str(_) => &[],
        }
    }
}

#[derive(Debug, PartialEq)]
enum Token {
    Str(String),
    Open,
    Close,
}

fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            c if c.is_whitespace() => {
                chars.next();
            }
            '/' => {
                // Commentaire "//" jusqu'à la fin de ligne.
                chars.next();
                if chars.peek() == Some(&'/') {
                    for c in chars.by_ref() {
                        if c == '\n' {
                            break;
                        }
                    }
                }
            }
            '{' => {
                chars.next();
                tokens.push(Token::Open);
            }
            '}' => {
                chars.next();
                tokens.push(Token::Close);
            }
            '"' => {
                chars.next();
                let mut s = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => match chars.next() {
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some(other) => s.push(other),
                            None => break,
                        },
                        '"' => break,
                        other => s.push(other),
                    }
                }
                tokens.push(Token::Str(s));
            }
            _ => {
                let mut s = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() || c == '{' || c == '}' || c == '"' {
                        break;
                    }
                    s.push(c);
                    chars.next();
                }
                tokens.push(Token::Str(s));
            }
        }
    }
    tokens
}

fn parse_obj(tokens: &mut std::iter::Peekable<std::vec::IntoIter<Token>>) -> Vec<(String, Vdf)> {
    let mut entries = Vec::new();
    while let Some(tok) = tokens.next() {
        match tok {
            Token::Close => break,
            Token::Open => continue,
            Token::Str(key) => match tokens.next() {
                Some(Token::Str(value)) => entries.push((key, Vdf::Str(value))),
                Some(Token::Open) => entries.push((key, Vdf::Obj(parse_obj(tokens)))),
                Some(Token::Close) | None => break,
            },
        }
    }
    entries
}

/// Retourne la racine du document sous forme d'objet.
pub fn parse(input: &str) -> Vdf {
    let mut tokens = tokenize(input).into_iter().peekable();
    Vdf::Obj(parse_obj(&mut tokens))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_library_folders() {
        let doc = parse(
            r#""libraryfolders"
{
    "0"
    {
        "path"      "C:\\Program Files (x86)\\Steam"
        "apps" { "730" "123" }
    }
}"#,
        );
        let lib = doc.get("libraryfolders").unwrap();
        let first = &lib.entries()[0].1;
        assert_eq!(first.str("path"), Some(r"C:\Program Files (x86)\Steam"));
        assert_eq!(first.get("apps").unwrap().str("730"), Some("123"));
    }
}
