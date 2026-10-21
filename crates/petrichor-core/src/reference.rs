#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceError {
    pub message: String,
}

impl ReferenceError {
    pub fn new(message: impl Into<String>) -> Self {
        ReferenceError {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ReferenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ReferenceError {}

pub type ReferenceResult<T> = std::result::Result<T, ReferenceError>;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MeasureValue {
    pub number: f64,
    pub string: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

impl MeasureValue {
    pub fn number(value: f64) -> Self {
        MeasureValue {
            number: value,
            string: None,
            min: None,
            max: None,
        }
    }

    pub fn text(value: impl Into<String>) -> Self {
        let value = value.into();
        MeasureValue {
            number: value.parse().unwrap_or(0.0),
            string: Some(value),
            min: None,
            max: None,
        }
    }

    pub fn with_range(mut self, min: f64, max: f64) -> Self {
        self.min = Some(min);
        self.max = Some(max);
        self
    }

    pub fn relative(&self) -> f64 {
        match (self.min, self.max) {
            (Some(min), Some(max)) if max > min => {
                ((self.number - min) / (max - min)).clamp(0.0, 1.0)
            }
            _ => 0.0,
        }
    }
}

pub trait MeasureSource {
    fn measure(&self, name: &str) -> Option<MeasureValue>;

    fn call(&self, _measure: &str, _function: &str) -> Option<String> {
        None
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Modifier {
    Decimals(u32),
    Percent,
    Scale(f64),
    MaxValue,
    MinValue,
    TimeStamp,
    EscapeRegExp,
    EncodeUrl,
    Function(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Reference {
    pub name: String,
    pub modifiers: Vec<Modifier>,
}

pub fn is_reference_token(content: &str) -> bool {
    let trimmed = content.trim();
    !trimmed.is_empty()
        && !trimmed.starts_with('#')
        && !trimmed.starts_with('\\')
        && !trimmed.starts_with('$')
}

pub fn parse_reference(content: &str) -> Reference {
    let mut parts = content.split(':');
    let name = parts.next().unwrap_or("").trim().to_string();
    let mut modifiers = Vec::new();

    for part in parts {
        for token in part.split(',') {
            let token = token.trim();
            if token.is_empty() {
                continue;
            }
            modifiers.push(parse_modifier(token));
        }
    }

    Reference { name, modifiers }
}

fn parse_modifier(token: &str) -> Modifier {
    let lower = token.to_ascii_lowercase();
    match lower.as_str() {
        "%" => Modifier::Percent,
        "maxvalue" => Modifier::MaxValue,
        "minvalue" => Modifier::MinValue,
        "timestamp" => Modifier::TimeStamp,
        "escaperegexp" => Modifier::EscapeRegExp,
        "encodeurl" => Modifier::EncodeUrl,
        _ => {
            if let Some(scale) = token.strip_prefix('/') {
                if let Ok(value) = scale.trim().parse::<f64>() {
                    return Modifier::Scale(value);
                }
                return Modifier::Scale(f64::NAN);
            }
            if let Ok(decimals) = token.parse::<u32>() {
                return Modifier::Decimals(decimals.min(32));
            }
            if token.ends_with(')') {
                return Modifier::Function(token.to_string());
            }
            Modifier::Decimals(0)
        }
    }
}

pub fn resolve_reference(content: &str, source: &dyn MeasureSource) -> ReferenceResult<String> {
    let reference = parse_reference(content);

    if reference
        .modifiers
        .iter()
        .find_map(|m| match m {
            Modifier::Function(f) => Some(f.clone()),
            _ => None,
        })
        .is_some()
    {
        let function = reference
            .modifiers
            .iter()
            .find_map(|m| match m {
                Modifier::Function(f) => Some(f.clone()),
                _ => None,
            })
            .unwrap();
        let (func_name, args) = split_function(&function);
        let call = format!("{func_name}({args})");
        return source
            .call(&reference.name, &call)
            .ok_or_else(|| ReferenceError::new(format!("fonction « {call} » non disponible")));
    }

    let value = source
        .measure(&reference.name)
        .ok_or_else(|| ReferenceError::new(format!("mesure inconnue: « {} »", reference.name)))?;

    let mut number = value.number;
    let mut decimals: Option<u32> = None;
    let mut do_escape = false;
    let mut do_encode = false;

    for modifier in &reference.modifiers {
        match modifier {
            Modifier::Decimals(n) => decimals = Some(*n),
            Modifier::Percent => number = value.relative() * 100.0,
            Modifier::Scale(scale) => {
                if scale.is_finite() && *scale != 0.0 {
                    number /= scale;
                }
            }
            Modifier::MaxValue => {
                if let Some(max) = value.max {
                    number = max;
                }
            }
            Modifier::MinValue => {
                if let Some(min) = value.min {
                    number = min;
                }
            }
            Modifier::TimeStamp => {
                decimals = Some(0);
            }
            Modifier::EscapeRegExp => do_escape = true,
            Modifier::EncodeUrl => do_encode = true,
            Modifier::Function(_) => {}
        }
    }

    let mut output = if decimals.is_none() {
        match &value.string {
            Some(text) if !text.is_empty() => text.clone(),
            _ => format_number(number, None),
        }
    } else {
        format_number(number, decimals)
    };

    if do_escape {
        output = escape_regex(&output);
    }
    if do_encode {
        output = encode_url(&output);
    }

    Ok(output)
}

pub fn resolve_in_text(text: &str, source: &dyn MeasureSource) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut output = String::with_capacity(text.len());
    let mut i = 0usize;

    while i < chars.len() {
        if chars[i] == '[' {
            if let Some(offset) = chars[i + 1..].iter().position(|c| *c == ']') {
                let content: String = chars[i + 1..i + 1 + offset].iter().collect();
                if is_reference_token(&content) {
                    if let Ok(resolved) = resolve_reference(&content, source) {
                        output.push_str(&resolved);
                        i += offset + 2;
                        continue;
                    }
                }
                output.push('[');
                i += 1;
                continue;
            }
        }
        output.push(chars[i]);
        i += 1;
    }

    output
}

fn format_number(value: f64, decimals: Option<u32>) -> String {
    match decimals {
        Some(n) => format!("{:.*}", n as usize, value),
        None => {
            if !value.is_finite() {
                return "0".to_string();
            }
            let mut text = format!("{value:.10}");
            if text.contains('.') {
                while text.ends_with('0') {
                    text.pop();
                }
                if text.ends_with('.') {
                    text.pop();
                }
            }
            if text == "-0" {
                text = "0".to_string();
            }
            text
        }
    }
}

fn escape_regex(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for ch in input.chars() {
        if "\\.+*?()[]{}|^$".contains(ch) {
            output.push('\\');
        }
        output.push(ch);
    }
    output
}

fn encode_url(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                output.push(byte as char)
            }
            b' ' => output.push('+'),
            _ => output.push_str(&format!("%{byte:02X}")),
        }
    }
    output
}

fn split_function(call: &str) -> (String, String) {
    match call.find('(') {
        Some(index) => (
            call[..index].to_string(),
            call[index + 1..call.len().saturating_sub(1)].to_string(),
        ),
        None => (call.to_string(), String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct MockSource {
        values: BTreeMap<String, MeasureValue>,
    }

    impl MockSource {
        fn new() -> Self {
            MockSource {
                values: BTreeMap::new(),
            }
        }

        fn set(&mut self, name: &str, value: MeasureValue) {
            self.values.insert(name.to_ascii_lowercase(), value);
        }
    }

    impl MeasureSource for MockSource {
        fn measure(&self, name: &str) -> Option<MeasureValue> {
            self.values.get(&name.to_ascii_lowercase()).cloned()
        }

        fn call(&self, measure: &str, function: &str) -> Option<String> {
            if measure.eq_ignore_ascii_case("Script") && function.starts_with("greet(") {
                return Some("bonjour".to_string());
            }
            None
        }
    }

    #[test]
    fn resolves_number_with_default_formatting() {
        let mut source = MockSource::new();
        source.set("CPU", MeasureValue::number(42.5));
        assert_eq!(resolve_reference("CPU", &source).unwrap(), "42.5");
    }

    #[test]
    fn strips_trailing_zeros() {
        let mut source = MockSource::new();
        source.set("CPU", MeasureValue::number(50.0));
        assert_eq!(resolve_reference("CPU", &source).unwrap(), "50");
    }

    #[test]
    fn respects_decimals() {
        let mut source = MockSource::new();
        source.set("CPU", MeasureValue::number(1.0 / 3.0));
        assert_eq!(resolve_reference("CPU:2", &source).unwrap(), "0.33");
        assert_eq!(resolve_reference("CPU:0", &source).unwrap(), "0");
    }

    #[test]
    fn resolves_percent() {
        let mut source = MockSource::new();
        source.set("CPU", MeasureValue::number(25.0).with_range(0.0, 50.0));
        assert_eq!(resolve_reference("CPU:%", &source).unwrap(), "50");
    }

    #[test]
    fn resolves_scale_and_max() {
        let mut source = MockSource::new();
        source.set("Net", MeasureValue::number(2048.0).with_range(0.0, 10000.0));
        assert_eq!(resolve_reference("Net:/1024", &source).unwrap(), "2");
        assert_eq!(resolve_reference("Net:MaxValue", &source).unwrap(), "10000");
        assert_eq!(resolve_reference("Net:MinValue", &source).unwrap(), "0");
        assert_eq!(
            resolve_reference("Net:MaxValue:/1000,1", &source).unwrap(),
            "10.0"
        );
    }

    #[test]
    fn uses_string_value_when_present() {
        let mut source = MockSource::new();
        source.set("Title", MeasureValue::text("Titre du morceau"));
        assert_eq!(
            resolve_reference("Title", &source).unwrap(),
            "Titre du morceau"
        );
    }

    #[test]
    fn escapes_and_encodes() {
        let mut source = MockSource::new();
        source.set("Title", MeasureValue::text("a.b(c)"));
        assert_eq!(
            resolve_reference("Title:EscapeRegExp", &source).unwrap(),
            "a\\.b\\(c\\)"
        );
        assert_eq!(
            resolve_reference("Title:EncodeUrl", &source).unwrap(),
            "a.b%28c%29"
        );
    }

    #[test]
    fn calls_script_functions() {
        let source = MockSource::new();
        assert_eq!(
            resolve_reference("Script:greet(1)", &source).unwrap(),
            "bonjour"
        );
    }

    #[test]
    fn reports_unknown_measure() {
        let source = MockSource::new();
        assert!(resolve_reference("Absent", &source).is_err());
    }

    #[test]
    fn resolves_references_in_text() {
        let mut source = MockSource::new();
        source.set("CPU", MeasureValue::number(12.0));
        source.set("RAM", MeasureValue::number(75.0));
        assert_eq!(
            resolve_in_text("CPU: [CPU]%  RAM: [RAM:1]%", &source),
            "CPU: 12%  RAM: 75.0%"
        );
    }

    #[test]
    fn leaves_unknown_and_special_tokens() {
        let source = MockSource::new();
        assert_eq!(
            resolve_in_text("[#Var] [\\euro] [Inconnu]", &source),
            "[#Var] [\\euro] [Inconnu]"
        );
    }

    #[test]
    fn detects_reference_tokens() {
        assert!(is_reference_token("MeasureCPU"));
        assert!(!is_reference_token("#Var#"));
        assert!(!is_reference_token("\\euro"));
        assert!(!is_reference_token("$MouseX"));
    }
}
