use std::collections::BTreeMap;

pub const MAX_EXPANSION_PASSES: usize = 100;

#[derive(Debug, Clone)]
pub struct Variables {
    map: BTreeMap<String, String>,
    context: BTreeMap<String, String>,
    mouse_x: i32,
    mouse_y: i32,
}

impl Default for Variables {
    fn default() -> Self {
        Variables::new()
    }
}

impl Variables {
    pub fn new() -> Self {
        Variables {
            map: BTreeMap::new(),
            context: BTreeMap::new(),
            mouse_x: 0,
            mouse_y: 0,
        }
    }

    pub fn set(&mut self, name: &str, value: &str) {
        self.map
            .insert(name.to_ascii_lowercase(), value.to_string());
    }

    pub fn set_context(&mut self, name: &str, value: &str) {
        self.context
            .insert(name.to_ascii_lowercase(), value.to_string());
    }

    pub fn set_mouse(&mut self, x: i32, y: i32) {
        self.mouse_x = x;
        self.mouse_y = y;
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        let key = name.to_ascii_lowercase();
        self.map
            .get(&key)
            .or_else(|| self.context.get(&key))
            .map(|s| s.as_str())
    }

    pub fn remove(&mut self, name: &str) {
        self.map.remove(&name.to_ascii_lowercase());
    }

    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.map.keys().map(|s| s.as_str())
    }

    pub fn to_raw_map(&self) -> BTreeMap<String, String> {
        self.map.clone()
    }

    pub fn expand(&self, input: &str) -> String {
        self.expand_inner(input, 0)
    }

    fn expand_inner(&self, input: &str, depth: usize) -> String {
        if depth >= MAX_EXPANSION_PASSES {
            return input.to_string();
        }

        let chars: Vec<char> = input.chars().collect();
        let mut output = String::with_capacity(input.len());
        let mut i = 0usize;

        while i < chars.len() {
            let ch = chars[i];

            if ch == '$' && i + 1 < chars.len() && chars[i + 1] == '*' {
                if let Some(offset) = find_pair(&chars, i + 2, '*', '$') {
                    for c in &chars[i + 2..i + 2 + offset] {
                        output.push(*c);
                    }
                    i += offset + 4;
                    continue;
                }
            }

            if ch == '$' {
                if let Some((value, consumed)) = self.read_mouse_dollar(&chars, i) {
                    output.push_str(&value);
                    i += consumed;
                    continue;
                }
            }

            if ch == '#' {
                if let Some(offset) = find_char(&chars, i + 1, '#') {
                    let name: String = chars[i + 1..i + 1 + offset].iter().collect();
                    if !name.is_empty() {
                        if let Some(value) = self.get(&name) {
                            output.push_str(&self.expand_inner(value, depth + 1));
                            i += offset + 2;
                            continue;
                        }
                    }
                }
            }

            if ch == '[' {
                if i + 1 < chars.len() && chars[i + 1] == '#' {
                    if let Some(offset) = find_char(&chars, i + 2, ']') {
                        let name: String = chars[i + 2..i + 2 + offset].iter().collect();
                        if let Some(value) = self.get(&name) {
                            output.push_str(&self.expand_inner(value, depth + 1));
                            i += offset + 3;
                            continue;
                        }
                    }
                } else if i + 1 < chars.len() && chars[i + 1] == '\\' {
                    if let Some(offset) = find_char(&chars, i + 2, ']') {
                        let name: String = chars[i + 2..i + 2 + offset].iter().collect();
                        if let Some(entity) = decode_entity(&name) {
                            output.push(entity);
                            i += offset + 3;
                            continue;
                        }
                    }
                } else if i + 1 < chars.len() && chars[i + 1] == '$' {
                    if let Some(offset) = find_char(&chars, i + 2, ']') {
                        let name: String = chars[i + 2..i + 2 + offset].iter().collect();
                        if let Some(value) = self.mouse_var(&name) {
                            output.push_str(&value);
                            i += offset + 3;
                            continue;
                        }
                    }
                }
            }

            if ch == '%' {
                if let Some(offset) = find_char(&chars, i + 1, '%') {
                    let name: String = chars[i + 1..i + 1 + offset].iter().collect();
                    if is_env_name(&name) {
                        if let Ok(value) = std::env::var(&name).or_else(|_| {
                            crate::compat::windows_env(&name).ok_or(std::env::VarError::NotPresent)
                        }) {
                            output.push_str(&self.expand_inner(&value, depth + 1));
                            i += offset + 2;
                            continue;
                        }
                    }
                }
            }

            output.push(ch);
            i += 1;
        }

        output
    }

    fn read_mouse_dollar(&self, chars: &[char], start: usize) -> Option<(String, usize)> {
        for (name, value) in [("mousex", self.mouse_x), ("mousey", self.mouse_y)] {
            let token: Vec<char> = format!("${name}$").chars().collect();
            if chars[start..].len() >= token.len()
                && chars[start..start + token.len()]
                    .iter()
                    .zip(&token)
                    .all(|(a, b)| a.eq_ignore_ascii_case(b))
            {
                return Some((value.to_string(), token.len()));
            }
        }
        None
    }

    fn mouse_var(&self, name: &str) -> Option<String> {
        match name.to_ascii_lowercase().as_str() {
            "mousex" => Some(self.mouse_x.to_string()),
            "mousey" => Some(self.mouse_y.to_string()),
            _ => None,
        }
    }
}

fn find_char(chars: &[char], from: usize, target: char) -> Option<usize> {
    chars[from..].iter().position(|c| *c == target)
}

fn find_pair(chars: &[char], from: usize, first: char, second: char) -> Option<usize> {
    let mut i = from;
    while i + 1 < chars.len() {
        if chars[i] == first && chars[i + 1] == second {
            return Some(i - from);
        }
        i += 1;
    }
    None
}

fn is_env_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub fn decode_entity(name: &str) -> Option<char> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.chars().all(|c| c.is_ascii_digit()) {
        return trimmed.parse::<u32>().ok().and_then(char::from_u32);
    }
    if let Some(hex) = trimmed
        .strip_prefix('x')
        .or_else(|| trimmed.strip_prefix('X'))
    {
        if !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return u32::from_str_radix(hex, 16).ok().and_then(char::from_u32);
        }
    }
    named_entity(&trimmed.to_ascii_lowercase())
}

fn named_entity(name: &str) -> Option<char> {
    Some(match name {
        "quot" => '"',
        "amp" => '&',
        "apos" => '\'',
        "lt" => '<',
        "gt" => '>',
        "nbsp" => '\u{00A0}',
        "copy" => '©',
        "reg" => '®',
        "trade" => '™',
        "deg" => '°',
        "plusmn" => '±',
        "times" => '×',
        "divide" => '÷',
        "middot" => '·',
        "hellip" => '…',
        "laquo" => '«',
        "raquo" => '»',
        "lsquo" => '‘',
        "rsquo" => '’',
        "ldquo" => '“',
        "rdquo" => '”',
        "ndash" => '–',
        "mdash" => '—',
        "euro" => '€',
        "pound" => '£',
        "cent" => '¢',
        "yen" => '¥',
        "sect" => '§',
        "para" => '¶',
        "bull" => '•',
        "dagger" => '†',
        "permil" => '‰',
        "prime" => '′',
        "minus" => '−',
        "infin" => '∞',
        "ne" => '≠',
        "le" => '≤',
        "ge" => '≥',
        "sup2" => '²',
        "sup3" => '³',
        "frac12" => '½',
        "frac14" => '¼',
        "frac34" => '¾',
        "alpha" => 'α',
        "beta" => 'β',
        "gamma" => 'γ',
        "delta" => 'δ',
        "pi" => 'π',
        "lambda" => 'λ',
        "mu" => 'μ',
        "sigma" => 'σ',
        "omega" => 'ω',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_historic_variables() {
        let mut vars = Variables::new();
        vars.set("Accent", "FF8800");
        assert_eq!(vars.expand("Couleur #Accent# fin"), "Couleur FF8800 fin");
    }

    #[test]
    fn expands_bracket_variables() {
        let mut vars = Variables::new();
        vars.set("Nom", "Monde");
        assert_eq!(vars.expand("Bonjour [#Nom]"), "Bonjour Monde");
    }

    #[test]
    fn expands_recursively() {
        let mut vars = Variables::new();
        vars.set("A", "10");
        vars.set("B", "#A#+5");
        assert_eq!(vars.expand("#B#"), "10+5");
    }

    #[test]
    fn expands_windows_environment_variables() {
        let vars = Variables::new();
        let expanded = vars.expand("%USERPROFILE%/Skins");
        assert!(!expanded.contains('%'));
        assert!(expanded.ends_with("/Skins"));
    }

    #[test]
    fn leaves_unknown_variables_intact() {
        let vars = Variables::new();
        assert_eq!(vars.expand("#Inconnu#"), "#Inconnu#");
    }

    #[test]
    fn prevents_infinite_recursion() {
        let mut vars = Variables::new();
        vars.set("A", "#B#");
        vars.set("B", "#A#");
        let out = vars.expand("#A#");
        assert!(out.contains('#'));
    }

    #[test]
    fn expands_context_variables() {
        let mut vars = Variables::new();
        vars.set_context("CURRENTSECTION", "MeterText");
        assert_eq!(vars.expand("#CURRENTSECTION#"), "MeterText");
    }

    #[test]
    fn expands_mouse_variables() {
        let mut vars = Variables::new();
        vars.set_mouse(320, 240);
        assert_eq!(vars.expand("$MouseX$,$MouseY$"), "320,240");
        assert_eq!(vars.expand("[$MouseX];[$MouseY]"), "320;240");
    }

    #[test]
    fn honors_escape_sequences() {
        let mut vars = Variables::new();
        vars.set("X", "valeur");
        assert_eq!(vars.expand("$*#X#*$"), "#X#");
    }

    #[test]
    fn expands_environment_variables() {
        std::env::set_var("PETRICHOR_TEST_VAR", "/chemin/test");
        let vars = Variables::new();
        assert_eq!(
            vars.expand("%PETRICHOR_TEST_VAR%/file"),
            "/chemin/test/file"
        );
        std::env::remove_var("PETRICHOR_TEST_VAR");
    }

    #[test]
    fn decodes_entities() {
        let vars = Variables::new();
        assert_eq!(vars.expand("Prix [\\8364]10"), "Prix €10");
        assert_eq!(vars.expand("[\\x20AC]"), "€");
        assert_eq!(vars.expand("[\\euro]"), "€");
        assert_eq!(vars.expand("[\\copy]"), "©");
    }

    #[test]
    fn leaves_measure_references_untouched() {
        let vars = Variables::new();
        assert_eq!(vars.expand("[MeasureCPU]"), "[MeasureCPU]");
        assert_eq!(vars.expand("[MeasureCPU:2]"), "[MeasureCPU:2]");
    }
}
