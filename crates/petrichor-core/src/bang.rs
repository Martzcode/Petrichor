#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bang {
    pub name: String,
    pub args: Vec<String>,
    pub external: bool,
}

impl Bang {
    pub fn is_bang(&self) -> bool {
        !self.external
    }

    pub fn arg(&self, index: usize) -> Option<&str> {
        self.args.get(index).map(String::as_str)
    }

    pub fn number(&self, index: usize) -> Option<f64> {
        self.arg(index).and_then(|value| value.trim().parse().ok())
    }

    pub fn bool_arg(&self, index: usize) -> bool {
        self.arg(index)
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    }
}

pub fn parse(input: &str) -> Vec<Bang> {
    let mut bangs = Vec::new();
    let mut current = String::new();
    let mut inside = false;
    let mut quote: Option<char> = None;

    for ch in input.chars() {
        if let Some(delimiter) = quote {
            current.push(ch);
            if ch == delimiter {
                quote = None;
            }
            continue;
        }

        match ch {
            '"' | '\'' => {
                quote = Some(ch);
                current.push(ch);
            }
            '[' if !inside => {
                if let Some(bang) = parse_one(&current) {
                    bangs.push(bang);
                }
                current.clear();
                inside = true;
            }
            ']' if inside => {
                if let Some(bang) = parse_one(&current) {
                    bangs.push(bang);
                }
                current.clear();
                inside = false;
            }
            _ => current.push(ch),
        }
    }

    if let Some(bang) = parse_one(&current) {
        bangs.push(bang);
    }

    bangs
}

pub fn parse_one(input: &str) -> Option<Bang> {
    let normalized = normalize_rainmeter(input.trim());
    if normalized.is_empty() {
        return None;
    }

    let external = !normalized.starts_with('!');
    let tokens = tokenize(&normalized);
    let first = tokens.first()?;
    let name = first.trim_start_matches('!').trim().to_ascii_uppercase();
    if name.is_empty() {
        return None;
    }

    Some(Bang {
        name,
        args: tokens[1..].to_vec(),
        external,
    })
}

fn normalize_rainmeter(input: &str) -> String {
    let prefix = "!rainmeter";
    let Some(head) = input.get(..prefix.len()) else {
        return input.to_string();
    };
    if !head.eq_ignore_ascii_case(prefix) {
        return input.to_string();
    }

    let rest = input[prefix.len()..].trim();
    if rest.is_empty() {
        return String::new();
    }
    if rest.starts_with('!') {
        rest.to_string()
    } else {
        format!("!{rest}")
    }
}

fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;

    for ch in input.chars() {
        if let Some(delimiter) = quote {
            if ch == delimiter {
                quote = None;
            } else {
                current.push(ch);
            }
            continue;
        }

        match ch {
            '"' | '\'' => {
                quote = Some(ch);
                started = true;
            }
            ch if ch.is_whitespace() => {
                if started {
                    tokens.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            ch => {
                current.push(ch);
                started = true;
            }
        }
    }

    if started {
        tokens.push(current);
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_bang() {
        let bangs = parse("!Refresh");
        assert_eq!(bangs.len(), 1);
        assert_eq!(bangs[0].name, "REFRESH");
        assert!(bangs[0].is_bang());
        assert!(bangs[0].args.is_empty());
    }

    #[test]
    fn tolerates_rainmeter_prefixes() {
        assert_eq!(parse_one("!RainmeterRefresh").unwrap().name, "REFRESH");
        assert_eq!(parse_one("!Rainmeter !Redraw").unwrap().name, "REDRAW");
        let bang = parse_one("!Rainmeter SetVariable Foo bar").unwrap();
        assert_eq!(bang.name, "SETVARIABLE");
        assert_eq!(bang.args, vec!["Foo", "bar"]);
    }

    #[test]
    fn parses_leading_bang_followed_by_a_chain() {
        let bangs = parse("!Log \"hello\"[!Redraw]");
        assert_eq!(bangs.len(), 2);
        assert_eq!(bangs[0].name, "LOG");
        assert_eq!(bangs[0].args, vec!["hello"]);
        assert_eq!(bangs[1].name, "REDRAW");
    }

    #[test]
    fn splits_chained_bangs() {
        let bangs = parse("[!SetVariable Foo \"bar baz\"][!Redraw]");
        assert_eq!(bangs.len(), 2);
        assert_eq!(bangs[0].name, "SETVARIABLE");
        assert_eq!(bangs[0].args, vec!["Foo", "bar baz"]);
        assert_eq!(bangs[1].name, "REDRAW");
    }

    #[test]
    fn keeps_quoted_arguments() {
        let bang = parse_one("!Move \"10\" \"20\"").unwrap();
        assert_eq!(bang.args, vec!["10", "20"]);
        assert_eq!(bang.number(0), Some(10.0));
        assert_eq!(bang.number(1), Some(20.0));
    }

    #[test]
    fn parses_empty_quoted_argument() {
        let bang = parse_one("!SetVariable Foo \"\"").unwrap();
        assert_eq!(bang.args, vec!["Foo", ""]);
    }

    #[test]
    fn marks_external_commands() {
        let bang = parse_one("xdg-open https://example.org").unwrap();
        assert!(!bang.is_bang());
        assert_eq!(bang.name, "XDG-OPEN");
        assert_eq!(bang.args, vec!["https://example.org"]);
    }

    #[test]
    fn handles_multibyte_external_commands() {
        let bang = parse_one("ééééé").unwrap();
        assert!(!bang.is_bang());
        assert_eq!(bang.name, "ééééé");
    }

    #[test]
    fn ignores_empty_input() {
        assert!(parse("").is_empty());
        assert!(parse("   ").is_empty());
        assert!(parse("[]").is_empty());
    }
}
