use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadError {
    pub message: String,
}

impl ReadError {
    pub fn new(message: impl Into<String>) -> Self {
        ReadError {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ReadError {}

pub type ReadResult<T> = std::result::Result<T, ReadError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const WHITE: Color = Color {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    pub const BLACK: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    pub const TRANSPARENT: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    pub fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color { r, g, b, a }
    }

    pub fn to_hex(&self) -> String {
        format!("{:02X}{:02X}{:02X}{:02X}", self.a, self.r, self.g, self.b)
    }

    pub fn to_css(&self) -> String {
        format!(
            "rgba({}, {}, {}, {})",
            self.r,
            self.g,
            self.b,
            (self.a as f64 / 255.0)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Rect {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

impl Rect {
    pub fn new(left: f64, top: f64, right: f64, bottom: f64) -> Self {
        Rect {
            left,
            top,
            right,
            bottom,
        }
    }

    pub fn width(&self) -> f64 {
        self.right - self.left
    }

    pub fn height(&self) -> f64 {
        self.bottom - self.top
    }
}

pub fn is_formula(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with('(') && trimmed.ends_with(')') && trimmed.len() >= 2
}

pub fn try_read_f64(value: &str) -> ReadResult<f64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ReadError::new("valeur numérique vide"));
    }
    trimmed
        .parse::<f64>()
        .map_err(|_| ReadError::new(format!("nombre invalide: « {trimmed} »")))
}

pub fn read_f64(value: &str, default: f64) -> f64 {
    try_read_f64(value).unwrap_or(default)
}

pub fn read_f32(value: &str, default: f32) -> f32 {
    try_read_f64(value).map(|v| v as f32).unwrap_or(default)
}

pub fn try_read_i32(value: &str) -> ReadResult<i32> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ReadError::new("valeur entière vide"));
    }
    if let Ok(parsed) = trimmed.parse::<i32>() {
        return Ok(parsed);
    }
    if let Ok(parsed) = trimmed.parse::<i64>() {
        return Ok(parsed.clamp(i32::MIN as i64, i32::MAX as i64) as i32);
    }
    let float = trimmed
        .parse::<f64>()
        .map_err(|_| ReadError::new(format!("entier invalide: « {trimmed} »")))?;
    Ok(float.trunc() as i32)
}

pub fn read_i32(value: &str, default: i32) -> i32 {
    try_read_i32(value).unwrap_or(default)
}

pub fn try_read_u32(value: &str) -> ReadResult<u32> {
    let parsed = try_read_i32(value)?;
    if parsed < 0 {
        return Err(ReadError::new("valeur négative pour un entier non signé"));
    }
    Ok(parsed as u32)
}

pub fn read_u32(value: &str, default: u32) -> u32 {
    try_read_u32(value).unwrap_or(default)
}

pub fn try_read_usize(value: &str) -> ReadResult<usize> {
    let parsed = try_read_i32(value)?;
    if parsed < 0 {
        return Err(ReadError::new("valeur négative inattendue"));
    }
    Ok(parsed as usize)
}

pub fn try_read_bool(value: &str) -> ReadResult<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" | "" => Ok(false),
        other => Err(ReadError::new(format!("booléen invalide: « {other} »"))),
    }
}

pub fn read_bool(value: &str, default: bool) -> bool {
    try_read_bool(value).unwrap_or(default)
}

pub fn try_read_color(value: &str) -> ReadResult<Color> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ReadError::new("couleur vide"));
    }

    if trimmed.contains(',') {
        let parts: Vec<&str> = trimmed.split(',').map(|p| p.trim()).collect();
        if parts.len() < 3 || parts.len() > 4 {
            return Err(ReadError::new(format!(
                "couleur composantes invalides: « {trimmed} »"
            )));
        }
        let r = component(parts[0])?;
        let g = component(parts[1])?;
        let b = component(parts[2])?;
        let a = match parts.get(3) {
            Some(value) => component(value)?,
            None => 255,
        };
        return Ok(Color::new(r, g, b, a));
    }

    let hex = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .or_else(|| trimmed.strip_prefix('#'))
        .unwrap_or(trimmed);

    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ReadError::new(format!(
            "couleur hexadécimale invalide: « {trimmed} »"
        )));
    }

    let parse_pair = |s: &str| -> ReadResult<u8> {
        u8::from_str_radix(s, 16)
            .map_err(|_| ReadError::new(format!("octet hex invalide: « {s} »")))
    };

    match hex.len() {
        6 => Ok(Color::new(
            parse_pair(&hex[0..2])?,
            parse_pair(&hex[2..4])?,
            parse_pair(&hex[4..6])?,
            255,
        )),
        8 => Ok(Color::new(
            parse_pair(&hex[2..4])?,
            parse_pair(&hex[4..6])?,
            parse_pair(&hex[6..8])?,
            parse_pair(&hex[0..2])?,
        )),
        _ => Err(ReadError::new(format!(
            "longueur de couleur hexadécimale invalide: « {trimmed} »"
        ))),
    }
}

fn component(value: &str) -> ReadResult<u8> {
    let parsed: i32 = value
        .trim()
        .parse()
        .map_err(|_| ReadError::new(format!("composante invalide: « {value} »")))?;
    Ok(parsed.clamp(0, 255) as u8)
}

pub fn read_color(value: &str, default: Color) -> Color {
    try_read_color(value).unwrap_or(default)
}

pub fn try_read_rect(value: &str) -> ReadResult<Rect> {
    let parts = split_floats(value);
    if parts.len() != 4 {
        return Err(ReadError::new(format!(
            "rectangle attendu (x,y,w,h), reçu « {value} »"
        )));
    }
    Ok(Rect::new(
        parts[0],
        parts[1],
        parts[0] + parts[2],
        parts[1] + parts[3],
    ))
}

pub fn try_read_rect_ltrb(value: &str) -> ReadResult<Rect> {
    let parts = split_floats(value);
    if parts.len() != 4 {
        return Err(ReadError::new(format!(
            "rectangle attendu (l,t,r,b), reçu « {value} »"
        )));
    }
    Ok(Rect::new(parts[0], parts[1], parts[2], parts[3]))
}

pub fn read_floats(value: &str) -> Vec<f64> {
    split_floats(value)
}

fn split_floats(value: &str) -> Vec<f64> {
    value
        .split(';')
        .flat_map(|part| {
            if part.contains(',') {
                part.split(',').map(|s| s.to_string()).collect::<Vec<_>>()
            } else {
                vec![part.to_string()]
            }
        })
        .filter_map(|part| {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                None
            } else {
                trimmed.parse::<f64>().ok()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_integers_with_fallback() {
        assert_eq!(read_i32("42", 0), 42);
        assert_eq!(read_i32("-7", 0), -7);
        assert_eq!(read_i32("10.9", 0), 10);
        assert_eq!(read_i32("abc", 5), 5);
        assert_eq!(read_i32("", 5), 5);
    }

    #[test]
    fn detects_overflow_gracefully() {
        assert_eq!(try_read_i32("999999999999").unwrap(), i32::MAX);
    }

    #[test]
    fn reads_unsigned() {
        assert_eq!(read_u32("-4", 9), 9);
        assert_eq!(read_u32("12", 0), 12);
    }

    #[test]
    fn reads_floats() {
        assert_eq!(read_f64("1.25", 0.0), 1.25);
        assert_eq!(read_f64("nope", 1.5), 1.5);
    }

    #[test]
    fn reads_booleans() {
        assert!(read_bool("1", false));
        assert!(read_bool("true", false));
        assert!(!read_bool("0", true));
        assert!(read_bool("oups", true));
    }

    #[test]
    fn reads_rgb_colors() {
        assert_eq!(
            try_read_color("255,128,0").unwrap(),
            Color::new(255, 128, 0, 255)
        );
        assert_eq!(
            try_read_color("10,20,30,40").unwrap(),
            Color::new(10, 20, 30, 40)
        );
    }

    #[test]
    fn reads_hex_colors() {
        assert_eq!(
            try_read_color("FF8800").unwrap(),
            Color::new(255, 136, 0, 255)
        );
        assert_eq!(
            try_read_color("0x80FF8800").unwrap(),
            Color::new(255, 136, 0, 128)
        );
        assert_eq!(
            try_read_color("#FF8800").unwrap(),
            Color::new(255, 136, 0, 255)
        );
    }

    #[test]
    fn rejects_bad_colors() {
        assert!(try_read_color("").is_err());
        assert!(try_read_color("1,2").is_err());
        assert!(try_read_color("ZZZZZZ").is_err());
        assert!(try_read_color("12345").is_err());
    }

    #[test]
    fn reads_rect_xywh() {
        let rect = try_read_rect("10,20,100,50").unwrap();
        assert_eq!(rect, Rect::new(10.0, 20.0, 110.0, 70.0));
        assert_eq!(rect.width(), 100.0);
        assert_eq!(rect.height(), 50.0);
    }

    #[test]
    fn reads_rect_ltrb() {
        assert_eq!(
            try_read_rect_ltrb("10,20,110,70").unwrap(),
            Rect::new(10.0, 20.0, 110.0, 70.0)
        );
    }

    #[test]
    fn rejects_bad_rects() {
        assert!(try_read_rect("1,2,3").is_err());
        assert!(try_read_rect_ltrb("1,2,3,4,5").is_err());
    }

    #[test]
    fn reads_float_lists() {
        assert_eq!(read_floats("1;2.5;3"), vec![1.0, 2.5, 3.0]);
        assert_eq!(read_floats("0.1;0.9"), vec![0.1, 0.9]);
    }

    #[test]
    fn color_css_and_hex() {
        let color = Color::new(255, 136, 0, 128);
        assert_eq!(color.to_hex(), "80FF8800");
        assert!(color.to_css().starts_with("rgba(255, 136, 0,"));
    }

    #[test]
    fn detects_formulas() {
        assert!(is_formula("(2*3+1)"));
        assert!(!is_formula("42"));
        assert!(!is_formula("(2*3+1"));
    }
}
