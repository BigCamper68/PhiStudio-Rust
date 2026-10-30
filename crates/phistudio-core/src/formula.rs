//! Bounded parameter expressions. This authors events; prpr evaluates playback.
use crate::ChartError;
#[derive(Clone, Debug)]
pub struct Formula(Node);
#[derive(Clone, Debug)]
enum Node {
    Number(f64),
    Time,
    Neg(Box<Node>),
    Binary(u8, Box<Node>, Box<Node>),
    Call(String, Vec<Node>),
}
struct Parser {
    text: Vec<u8>,
    at: usize,
    nodes: usize,
}
fn bad() -> ChartError {
    ChartError::Invalid(
        "Invalid expression: use t, pi, numbers, + - * / % ^ and supported functions".into(),
    )
}
impl Formula {
    /// Numeric property expressions have no time variable. Never silently sample t=0.
    pub fn constant(text: &str) -> Result<f64, ChartError> {
        let formula = Self::parse(text)?;
        if formula.0.has_time() {
            return Err(ChartError::Invalid(
                "Numeric expressions cannot use t".into(),
            ));
        }
        formula.sample(0.)
    }
    pub fn parse(text: &str) -> Result<Self, ChartError> {
        if text.len() > 4096 {
            return Err(bad());
        }
        let mut p = Parser {
            text: text.replace('$', "").to_ascii_lowercase().into_bytes(),
            at: 0,
            nodes: 0,
        };
        let node = p.expr(0, 0)?;
        p.space();
        if p.at != p.text.len() {
            return Err(bad());
        }
        Ok(Self(node))
    }
    pub fn sample(&self, t: f64) -> Result<f64, ChartError> {
        let value = self.0.eval(t);
        if value.is_finite() {
            Ok(value)
        } else {
            Err(ChartError::Invalid(
                "Expression returned a non-finite value".into(),
            ))
        }
    }
}
impl Parser {
    fn space(&mut self) {
        while self.text.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
    }
    fn take(&mut self, c: u8) -> bool {
        self.space();
        if self.text.get(self.at) == Some(&c) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn expr(&mut self, min: u8, depth: usize) -> Result<Node, ChartError> {
        self.nodes += 1;
        if depth > 64 || self.nodes > 512 {
            return Err(bad());
        }
        self.space();
        let mut left = if self.take(b'-') {
            Node::Neg(Box::new(self.expr(5, depth + 1)?))
        } else if self.take(b'+') {
            self.expr(5, depth + 1)?
        } else if self.take(b'(') {
            let n = self.expr(0, depth + 1)?;
            if !self.take(b')') {
                return Err(bad());
            }
            n
        } else if self
            .text
            .get(self.at)
            .is_some_and(|c| c.is_ascii_digit() || *c == b'.')
        {
            let start = self.at;
            while self
                .text
                .get(self.at)
                .is_some_and(|c| c.is_ascii_digit() || *c == b'.')
            {
                self.at += 1;
            }
            if matches!(self.text.get(self.at), Some(b'e')) {
                self.at += 1;
                if matches!(self.text.get(self.at), Some(b'+' | b'-')) {
                    self.at += 1;
                }
                while self.text.get(self.at).is_some_and(u8::is_ascii_digit) {
                    self.at += 1;
                }
            }
            let n = std::str::from_utf8(&self.text[start..self.at])
                .map_err(|_| bad())?
                .parse::<f64>()
                .map_err(|_| bad())?;
            if !n.is_finite() {
                return Err(bad());
            }
            Node::Number(n)
        } else {
            let start = self.at;
            while self
                .text
                .get(self.at)
                .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
            {
                self.at += 1;
            }
            let name = String::from_utf8(self.text[start..self.at].to_vec()).map_err(|_| bad())?;
            match name.as_str() {
                "t" => Node::Time,
                "pi" => Node::Number(std::f64::consts::PI),
                "e" => Node::Number(std::f64::consts::E),
                _ => {
                    if !self.take(b'(') {
                        return Err(bad());
                    }
                    let mut args = vec![];
                    loop {
                        args.push(self.expr(0, depth + 1)?);
                        if self.take(b')') {
                            break;
                        }
                        if args.len() >= 3 || !self.take(b',') {
                            return Err(bad());
                        }
                    }
                    let arity = match name.as_str() {
                        "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "sqrt" | "abs"
                        | "exp" | "ln" | "log" | "floor" | "ceil" | "round" | "sign" => 1,
                        "min" | "max" | "pow" | "atan2" => 2,
                        "clamp" | "lerp" => 3,
                        _ => return Err(bad()),
                    };
                    if args.len() != arity {
                        return Err(bad());
                    }
                    Node::Call(name, args)
                }
            }
        };
        loop {
            self.space();
            let Some(op) = self.text.get(self.at).copied() else {
                break;
            };
            let prec = match op {
                b'+' | b'-' => 1,
                b'*' | b'/' | b'%' => 3,
                b'^' => 5,
                _ => break,
            };
            if prec < min {
                break;
            }
            self.at += 1;
            let right = self.expr(if op == b'^' { prec } else { prec + 1 }, depth + 1)?;
            left = Node::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }
}
impl Node {
    fn has_time(&self) -> bool {
        match self {
            Self::Time => true,
            Self::Number(_) => false,
            Self::Neg(n) => n.has_time(),
            Self::Binary(_, a, b) => a.has_time() || b.has_time(),
            Self::Call(_, args) => args.iter().any(Self::has_time),
        }
    }
    fn eval(&self, t: f64) -> f64 {
        match self {
            Self::Number(n) => *n,
            Self::Time => t,
            Self::Neg(n) => -n.eval(t),
            Self::Binary(op, a, b) => {
                let (a, b) = (a.eval(t), b.eval(t));
                match op {
                    b'+' => a + b,
                    b'-' => a - b,
                    b'*' => a * b,
                    b'/' => a / b,
                    b'%' => a % b,
                    b'^' => a.powf(b),
                    _ => f64::NAN,
                }
            }
            Self::Call(name, args) => {
                let v: Vec<_> = args.iter().map(|a| a.eval(t)).collect();
                let a = v[0];
                match name.as_str() {
                    "sin" => a.sin(),
                    "cos" => a.cos(),
                    "tan" => a.tan(),
                    "asin" => a.asin(),
                    "acos" => a.acos(),
                    "atan" => a.atan(),
                    "sqrt" => a.sqrt(),
                    "abs" => a.abs(),
                    "exp" => a.exp(),
                    "ln" => a.ln(),
                    "log" => a.log10(),
                    "floor" => a.floor(),
                    "ceil" => a.ceil(),
                    "round" => a.round(),
                    "sign" => a.signum(),
                    "min" => a.min(v[1]),
                    "max" => a.max(v[1]),
                    "pow" => a.powf(v[1]),
                    "atan2" => a.atan2(v[1]),
                    "clamp" => {
                        if v[1] <= v[2] {
                            a.clamp(v[1], v[2])
                        } else {
                            f64::NAN
                        }
                    }
                    "lerp" => a + (v[1] - a) * v[2],
                    _ => f64::NAN,
                }
            }
        }
    }
}
