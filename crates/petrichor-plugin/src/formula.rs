#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormulaError {
    pub message: String,
}

impl FormulaError {
    pub fn new(message: impl Into<String>) -> Self {
        FormulaError {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for FormulaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for FormulaError {}

pub type FormulaResult<T> = std::result::Result<T, FormulaError>;

pub trait EvalContext {
    fn resolve(&self, name: &str) -> Option<f64>;

    fn random(&self) -> f64 {
        0.0
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NullContext;

impl EvalContext for NullContext {
    fn resolve(&self, _name: &str) -> Option<f64> {
        None
    }
}

pub fn evaluate(input: &str) -> FormulaResult<f64> {
    evaluate_with(input, &NullContext)
}

pub fn evaluate_with(input: &str, context: &dyn EvalContext) -> FormulaResult<f64> {
    let inner = input.trim();
    let inner = if inner.starts_with('(') && inner.ends_with(')') {
        &inner[1..inner.len() - 1]
    } else {
        inner
    };
    if inner.trim().is_empty() {
        return Err(FormulaError::new("formule vide"));
    }
    let mut parser = Parser::new(inner);
    let value = parser.parse_expr(context)?;
    parser.skip_ws();
    if !parser.at_end() {
        return Err(FormulaError::new(format!(
            "caractère inattendu à la position {}",
            parser.position()
        )));
    }
    Ok(value)
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn new(input: &str) -> Self {
        Parser {
            chars: input.chars().collect(),
            pos: 0,
        }
    }

    fn position(&self) -> usize {
        self.pos
    }

    fn at_end(&self) -> bool {
        self.pos >= self.chars.len()
    }

    fn skip_ws(&mut self) {
        while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.skip_ws();
        self.chars.get(self.pos).copied()
    }

    fn eat(&mut self, ch: char) -> bool {
        self.skip_ws();
        if self.chars.get(self.pos) == Some(&ch) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn consume(&mut self, token: &str) -> bool {
        self.skip_ws();
        let token_chars: Vec<char> = token.chars().collect();
        if self.chars[self.pos..].starts_with(&token_chars) {
            self.pos += token_chars.len();
            true
        } else {
            false
        }
    }

    fn parse_expr(&mut self, ctx: &dyn EvalContext) -> FormulaResult<f64> {
        self.parse_ternary(ctx)
    }

    fn parse_ternary(&mut self, ctx: &dyn EvalContext) -> FormulaResult<f64> {
        let condition = self.parse_logical_or(ctx)?;
        if self.eat('?') {
            let then_value = self.parse_expr(ctx)?;
            if !self.eat(':') {
                return Err(FormulaError::new("':' attendu dans l'opérateur ternaire"));
            }
            let else_value = self.parse_expr(ctx)?;
            return Ok(if condition != 0.0 {
                then_value
            } else {
                else_value
            });
        }
        Ok(condition)
    }

    fn parse_logical_or(&mut self, ctx: &dyn EvalContext) -> FormulaResult<f64> {
        let mut left = self.parse_logical_and(ctx)?;
        while self.consume("||") {
            let right = self.parse_logical_and(ctx)?;
            left = bool_val(left != 0.0 || right != 0.0);
        }
        Ok(left)
    }

    fn parse_logical_and(&mut self, ctx: &dyn EvalContext) -> FormulaResult<f64> {
        let mut left = self.parse_equality(ctx)?;
        while self.consume("&&") {
            let right = self.parse_equality(ctx)?;
            left = bool_val(left != 0.0 && right != 0.0);
        }
        Ok(left)
    }

    fn parse_equality(&mut self, ctx: &dyn EvalContext) -> FormulaResult<f64> {
        let mut left = self.parse_comparison(ctx)?;
        loop {
            if self.consume("<>") {
                let right = self.parse_comparison(ctx)?;
                left = bool_val(left != right);
            } else if self.consume("==") || self.single_eq() {
                let right = self.parse_comparison(ctx)?;
                left = bool_val(left == right);
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn single_eq(&mut self) -> bool {
        self.skip_ws();
        if self.chars.get(self.pos) == Some(&'=') {
            if self.chars.get(self.pos + 1) == Some(&'=') {
                return false;
            }
            self.pos += 1;
            return true;
        }
        false
    }

    fn parse_comparison(&mut self, ctx: &dyn EvalContext) -> FormulaResult<f64> {
        let mut left = self.parse_bit_or(ctx)?;
        loop {
            self.skip_ws();
            let first = self.chars.get(self.pos).copied();
            let second = self.chars.get(self.pos + 1).copied();
            match (first, second) {
                (Some('<'), Some('=')) => {
                    self.pos += 2;
                    let right = self.parse_bit_or(ctx)?;
                    left = bool_val(left <= right);
                }
                (Some('>'), Some('=')) => {
                    self.pos += 2;
                    let right = self.parse_bit_or(ctx)?;
                    left = bool_val(left >= right);
                }
                (Some('<'), Some('>')) => break,
                (Some('<'), _) => {
                    self.pos += 1;
                    let right = self.parse_bit_or(ctx)?;
                    left = bool_val(left < right);
                }
                (Some('>'), _) => {
                    self.pos += 1;
                    let right = self.parse_bit_or(ctx)?;
                    left = bool_val(left > right);
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_bit_or(&mut self, ctx: &dyn EvalContext) -> FormulaResult<f64> {
        let mut left = self.parse_bit_xor(ctx)?;
        while self.consume("|") {
            if self.chars.get(self.pos) == Some(&'|') {
                self.pos -= 1;
                break;
            }
            let right = self.parse_bit_xor(ctx)?;
            left = (to_int(left) | to_int(right)) as f64;
        }
        Ok(left)
    }

    fn parse_bit_xor(&mut self, ctx: &dyn EvalContext) -> FormulaResult<f64> {
        let mut left = self.parse_bit_and(ctx)?;
        while self.consume("^") {
            let right = self.parse_bit_and(ctx)?;
            left = (to_int(left) ^ to_int(right)) as f64;
        }
        Ok(left)
    }

    fn parse_bit_and(&mut self, ctx: &dyn EvalContext) -> FormulaResult<f64> {
        let mut left = self.parse_shift(ctx)?;
        while self.consume("&") {
            if self.chars.get(self.pos) == Some(&'&') {
                self.pos -= 1;
                break;
            }
            let right = self.parse_shift(ctx)?;
            left = (to_int(left) & to_int(right)) as f64;
        }
        Ok(left)
    }

    fn parse_shift(&mut self, ctx: &dyn EvalContext) -> FormulaResult<f64> {
        let mut left = self.parse_additive(ctx)?;
        loop {
            if self.consume("<<") {
                let right = self.parse_additive(ctx)?;
                left = (to_int(left) << (to_int(right) as u32 & 63)) as f64;
            } else if self.consume(">>") {
                let right = self.parse_additive(ctx)?;
                left = (to_int(left) >> (to_int(right) as u32 & 63)) as f64;
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_additive(&mut self, ctx: &dyn EvalContext) -> FormulaResult<f64> {
        let mut left = self.parse_multiplicative(ctx)?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some('+') => {
                    self.pos += 1;
                    let right = self.parse_multiplicative(ctx)?;
                    left += right;
                }
                Some('-') => {
                    self.pos += 1;
                    let right = self.parse_multiplicative(ctx)?;
                    left -= right;
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self, ctx: &dyn EvalContext) -> FormulaResult<f64> {
        let mut left = self.parse_unary(ctx)?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some('*') if self.chars.get(self.pos + 1) != Some(&'*') => {
                    self.pos += 1;
                    left *= self.parse_unary(ctx)?;
                }
                Some('/') => {
                    self.pos += 1;
                    let right = self.parse_unary(ctx)?;
                    if right == 0.0 {
                        return Err(FormulaError::new("division par zéro"));
                    }
                    left /= right;
                }
                Some('%') => {
                    self.pos += 1;
                    let right = self.parse_unary(ctx)?;
                    if right == 0.0 {
                        return Err(FormulaError::new("modulo par zéro"));
                    }
                    left %= right;
                }
                Some('$') => {
                    self.pos += 1;
                    let right = self.parse_unary(ctx)?;
                    if right == 0.0 {
                        return Err(FormulaError::new("division par zéro"));
                    }
                    left = (left / right).ceil();
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_unary(&mut self, ctx: &dyn EvalContext) -> FormulaResult<f64> {
        self.skip_ws();
        match self.peek() {
            Some('-') => {
                self.pos += 1;
                Ok(-self.parse_unary(ctx)?)
            }
            Some('+') => {
                self.pos += 1;
                self.parse_unary(ctx)
            }
            Some('~') => {
                self.pos += 1;
                Ok((!to_int(self.parse_unary(ctx)?)) as f64)
            }
            _ => self.parse_power(ctx),
        }
    }

    fn parse_power(&mut self, ctx: &dyn EvalContext) -> FormulaResult<f64> {
        let base = self.parse_primary(ctx)?;
        if self.consume("**") {
            let exponent = self.parse_unary(ctx)?;
            Ok(base.powf(exponent))
        } else {
            Ok(base)
        }
    }

    fn parse_primary(&mut self, ctx: &dyn EvalContext) -> FormulaResult<f64> {
        self.skip_ws();
        match self.peek() {
            Some('(') => {
                self.pos += 1;
                let value = self.parse_expr(ctx)?;
                if !self.eat(')') {
                    return Err(FormulaError::new("')' attendue"));
                }
                Ok(value)
            }
            Some(ch) if ch.is_ascii_digit() || ch == '.' => self.parse_number(),
            Some(ch) if ch.is_alphabetic() || ch == '_' => self.parse_ident(ctx),
            Some(other) => Err(FormulaError::new(format!("symbole inattendu: « {other} »"))),
            None => Err(FormulaError::new("expression incomplète")),
        }
    }

    fn parse_number(&mut self) -> FormulaResult<f64> {
        let start = self.pos;
        while let Some(ch) = self.chars.get(self.pos) {
            if ch.is_ascii_digit() || *ch == '.' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let text: String = self.chars[start..self.pos].iter().collect();
        text.parse::<f64>()
            .map_err(|_| FormulaError::new(format!("nombre invalide: « {text} »")))
    }

    fn parse_ident(&mut self, ctx: &dyn EvalContext) -> FormulaResult<f64> {
        let start = self.pos;
        while let Some(ch) = self.chars.get(self.pos) {
            if ch.is_alphanumeric() || *ch == '_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let name: String = self.chars[start..self.pos].iter().collect();
        let lower = name.to_ascii_lowercase();

        self.skip_ws();
        if self.chars.get(self.pos) == Some(&'(') {
            self.pos += 1;
            let mut args = Vec::new();
            if !self.eat(')') {
                loop {
                    args.push(self.parse_expr(ctx)?);
                    if self.eat(',') {
                        continue;
                    }
                    if self.eat(')') {
                        break;
                    }
                    return Err(FormulaError::new(format!(
                        "argument manquant dans l'appel à {name}"
                    )));
                }
            }
            return call_function(&lower, &args);
        }

        if let Some(value) = constant(&lower) {
            return Ok(value);
        }
        if lower == "random" {
            return Ok(ctx.random());
        }
        ctx.resolve(&name)
            .ok_or_else(|| FormulaError::new(format!("identifiant inconnu: « {name} »")))
    }
}

fn constant(name: &str) -> Option<f64> {
    match name {
        "pi" => Some(std::f64::consts::PI),
        "e" => Some(std::f64::consts::E),
        _ => None,
    }
}

fn call_function(name: &str, args: &[f64]) -> FormulaResult<f64> {
    let unary = |f: fn(f64) -> f64| -> FormulaResult<f64> {
        if args.len() != 1 {
            return Err(FormulaError::new(format!(
                "{name} attend 1 argument, reçu {}",
                args.len()
            )));
        }
        Ok(f(args[0]))
    };

    match name {
        "abs" => unary(f64::abs),
        "exp" => unary(f64::exp),
        "ln" => unary(f64::ln),
        "log" => unary(f64::log10),
        "sqrt" => unary(f64::sqrt),
        "sin" => unary(f64::sin),
        "cos" => unary(f64::cos),
        "tan" => unary(f64::tan),
        "asin" => unary(f64::asin),
        "acos" => unary(f64::acos),
        "atan" => unary(f64::atan),
        "floor" => unary(f64::floor),
        "ceil" => unary(f64::ceil),
        "round" => unary(f64::round),
        "trunc" => unary(f64::trunc),
        "frac" => unary(|x| x - x.trunc()),
        "rad" => unary(f64::to_radians),
        "deg" => unary(f64::to_degrees),
        "sgn" => unary(|x| {
            if x > 0.0 {
                1.0
            } else if x < 0.0 {
                -1.0
            } else {
                0.0
            }
        }),
        "neg" => unary(|x| -x),
        "atan2" => {
            if args.len() != 2 {
                return Err(FormulaError::new("atan2 attend 2 arguments"));
            }
            Ok(args[0].atan2(args[1]))
        }
        "min" => {
            if args.len() < 2 {
                return Err(FormulaError::new("min attend au moins 2 arguments"));
            }
            Ok(args.iter().copied().fold(f64::INFINITY, f64::min))
        }
        "max" => {
            if args.len() < 2 {
                return Err(FormulaError::new("max attend au moins 2 arguments"));
            }
            Ok(args.iter().copied().fold(f64::NEG_INFINITY, f64::max))
        }
        "clamp" => {
            if args.len() != 3 {
                return Err(FormulaError::new("clamp attend 3 arguments"));
            }
            Ok(args[0].clamp(args[1].min(args[2]), args[1].max(args[2])))
        }
        _ => Err(FormulaError::new(format!("fonction inconnue: « {name} »"))),
    }
}

fn bool_val(value: bool) -> f64 {
    if value {
        1.0
    } else {
        0.0
    }
}

fn to_int(value: f64) -> i64 {
    value.trunc() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(input: &str) -> f64 {
        evaluate(input).unwrap()
    }

    #[test]
    fn evaluates_arithmetic_with_precedence() {
        assert_eq!(eval("(2+3*4)"), 14.0);
        assert_eq!(eval("(2*3+1)"), 7.0);
        assert_eq!(eval("((2+3)*4)"), 20.0);
        assert_eq!(eval("(10-2-3)"), 5.0);
    }

    #[test]
    fn evaluates_power_right_associative() {
        assert_eq!(eval("(2**3)"), 8.0);
        assert_eq!(eval("(2**3**2)"), 512.0);
    }

    #[test]
    fn evaluates_unary_minus_and_not() {
        assert_eq!(eval("(-5)"), -5.0);
        assert_eq!(eval("(~0)"), -1.0);
        assert_eq!(eval("(2**-1)"), 0.5);
    }

    #[test]
    fn evaluates_modulo_and_ceiling_division() {
        assert_eq!(eval("(7%3)"), 1.0);
        assert_eq!(eval("(7$2)"), 4.0);
    }

    #[test]
    fn evaluates_bitwise_operators() {
        assert_eq!(eval("(6&3)"), 2.0);
        assert_eq!(eval("(6|1)"), 7.0);
        assert_eq!(eval("(5^1)"), 4.0);
        assert_eq!(eval("(1<<4)"), 16.0);
        assert_eq!(eval("(16>>2)"), 4.0);
    }

    #[test]
    fn evaluates_comparisons() {
        assert_eq!(eval("(3>2)"), 1.0);
        assert_eq!(eval("(3<2)"), 0.0);
        assert_eq!(eval("(3>=3)"), 1.0);
        assert_eq!(eval("(3=3)"), 1.0);
        assert_eq!(eval("(3<>4)"), 1.0);
    }

    #[test]
    fn evaluates_logical_and_ternary() {
        assert_eq!(eval("(1&&0)"), 0.0);
        assert_eq!(eval("(1||0)"), 1.0);
        assert_eq!(eval("(1?10:20)"), 10.0);
        assert_eq!(eval("(0?10:20)"), 20.0);
    }

    #[test]
    fn evaluates_functions() {
        assert_eq!(eval("(abs(-4))"), 4.0);
        assert_eq!(eval("(round(2.5))"), 3.0);
        assert_eq!(eval("(floor(2.9))"), 2.0);
        assert_eq!(eval("(ceil(2.1))"), 3.0);
        assert_eq!(eval("(min(3,7))"), 3.0);
        assert_eq!(eval("(max(3,7))"), 7.0);
        assert_eq!(eval("(clamp(15,0,10))"), 10.0);
        assert_eq!(eval("(sgn(-9))"), -1.0);
        assert_eq!(eval("(frac(2.25))"), 0.25);
    }

    #[test]
    fn evaluates_constants() {
        assert!((eval("(pi)") - std::f64::consts::PI).abs() < 1e-12);
        assert!((eval("(deg(pi))") - 180.0).abs() < 1e-9);
        assert!((eval("(rad(180))") - std::f64::consts::PI).abs() < 1e-9);
    }

    #[test]
    fn reports_errors() {
        assert!(evaluate("(1/0)").is_err());
        assert!(evaluate("(2**)").is_err());
        assert!(evaluate("(foo(1))").is_err());
        assert!(evaluate("(1+2").is_err());
        assert!(evaluate("()").is_err());
        assert!(evaluate("(3?1)").is_err());
    }

    #[test]
    fn uses_context_for_identifiers() {
        struct Ctx;
        impl EvalContext for Ctx {
            fn resolve(&self, name: &str) -> Option<f64> {
                match name {
                    "cycles" => Some(10.0),
                    _ => None,
                }
            }
        }
        assert_eq!(evaluate_with("(cycles*2)", &Ctx).unwrap(), 20.0);
        assert!(evaluate_with("(unknown+1)", &Ctx).is_err());
    }
}
