#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bang {
    pub name: String,
    pub raw_name: String,
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
    let chars: Vec<char> = input.chars().collect();
    let mut bangs = Vec::new();
    let mut current = String::new();
    let mut inside = false;
    let mut quote = Quote::None;
    let mut index = 0;

    while index < chars.len() {
        let ch = chars[index];

        match quote {
            Quote::Single => {
                current.push(ch);
                if ch == '\'' {
                    quote = Quote::None;
                }
                index += 1;
            }
            Quote::Double => {
                current.push(ch);
                if ch == '"' && !starts_with_triple(&chars, index) {
                    quote = Quote::None;
                }
                index += 1;
            }
            Quote::Triple => {
                if starts_with_triple(&chars, index) {
                    current.push_str("\"\"\"");
                    quote = Quote::None;
                    index += 3;
                } else {
                    current.push(ch);
                    index += 1;
                }
            }
            Quote::None => {
                if starts_with_triple(&chars, index) {
                    current.push_str("\"\"\"");
                    quote = Quote::Triple;
                    index += 3;
                } else {
                    match ch {
                        '"' => {
                            quote = Quote::Double;
                            current.push(ch);
                        }
                        '\'' => {
                            quote = Quote::Single;
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
                    index += 1;
                }
            }
        }
    }

    if let Some(bang) = parse_one(&current) {
        bangs.push(bang);
    }

    bangs
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Quote {
    None,
    Single,
    Double,
    Triple,
}

fn starts_with_triple(chars: &[char], index: usize) -> bool {
    chars.get(index) == Some(&'"')
        && chars.get(index + 1) == Some(&'"')
        && chars.get(index + 2) == Some(&'"')
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
        raw_name: first.trim_start_matches('!').trim().to_string(),
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
    let chars: Vec<char> = input.chars().collect();
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quote = Quote::None;
    let mut index = 0;

    while index < chars.len() {
        let ch = chars[index];

        match quote {
            Quote::Single => {
                if ch == '\'' {
                    quote = Quote::None;
                } else {
                    current.push(ch);
                }
                index += 1;
            }
            Quote::Double => {
                if ch == '"' {
                    quote = Quote::None;
                } else {
                    current.push(ch);
                }
                index += 1;
            }
            Quote::Triple => {
                if starts_with_triple(&chars, index) {
                    quote = Quote::None;
                    index += 3;
                } else {
                    current.push(ch);
                    index += 1;
                }
            }
            Quote::None => {
                if starts_with_triple(&chars, index) {
                    quote = Quote::Triple;
                    started = true;
                    index += 3;
                } else {
                    match ch {
                        '"' => {
                            quote = Quote::Double;
                            started = true;
                        }
                        '\'' => {
                            quote = Quote::Single;
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
                    index += 1;
                }
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
        assert_eq!(bang.raw_name, "xdg-open");
        assert_eq!(bang.args, vec!["https://example.org"]);
    }

    #[test]
    fn handles_multibyte_external_commands() {
        let bang = parse_one("ééééé").unwrap();
        assert!(!bang.is_bang());
        assert_eq!(bang.name, "ééééé");
    }

    #[test]
    fn keeps_triple_quoted_literals_as_one_argument() {
        let bang = parse_one(r#"!SetVariable Text """a "b" [c] d""""#).unwrap();
        assert_eq!(bang.args, vec!["Text", r#"a "b" [c] d"#]);
    }

    #[test]
    fn ignores_brackets_inside_quoted_arguments() {
        let bangs = parse("!SetVariable Text \"[not a bang]\" [!Redraw]");
        assert_eq!(bangs.len(), 2);
        assert_eq!(bangs[0].args, vec!["Text", "[not a bang]"]);
        assert_eq!(bangs[1].name, "REDRAW");
    }

    #[test]
    fn parses_delay_and_play_commands() {
        let bangs = parse("[!Log start][!Delay 250][PLAY \"sound.wav\"]");
        assert_eq!(bangs.len(), 3);
        assert_eq!(bangs[1].name, "DELAY");
        assert_eq!(bangs[1].number(0), Some(250.0));
        assert_eq!(bangs[2].name, "PLAY");
        assert!(!bangs[2].is_bang());
    }

    #[test]
    fn ignores_empty_input() {
        assert!(parse("").is_empty());
        assert!(parse("   ").is_empty());
        assert!(parse("[]").is_empty());
    }
}
