//! Template language for file and folder names (FileBot-like).
//!
//! Syntax: literal text with `{expression}` placeholders.
//! Expressions: `field`, `'string'`, `123`, `a + b`, `a == b` (`!=`, `<`, `>`, `<=`, `>=`),
//! `cond ? a : b`, and method calls `.pad(n)`, `.upper()`, `.lower()`, `.replace(a, b)`,
//! `.or(x)`, `.trim()`.

use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Empty,
    Str(String),
    Num(i64),
}

impl Value {
    pub fn as_string(&self) -> String {
        match self {
            Value::Empty => String::new(),
            Value::Str(s) => s.clone(),
            Value::Num(n) => n.to_string(),
        }
    }
    fn truthy(&self) -> bool {
        match self {
            Value::Empty => false,
            Value::Str(s) => !s.trim().is_empty(),
            Value::Num(n) => *n != 0,
        }
    }
    fn as_num(&self) -> Option<i64> {
        match self {
            Value::Num(n) => Some(*n),
            Value::Str(s) => s.trim().parse().ok(),
            Value::Empty => None,
        }
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        if s.is_empty() { Value::Empty } else { Value::Str(s.to_string()) }
    }
}
impl From<String> for Value {
    fn from(s: String) -> Self {
        if s.is_empty() { Value::Empty } else { Value::Str(s) }
    }
}
impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Value::Num(n)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TemplateError {
    Syntax { pos: usize, msg: String },
    UnknownField(String),
    UnknownFunction(String),
    BadArguments(String),
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TemplateError::Syntax { pos, msg } => write!(f, "syntax error at {pos}: {msg}"),
            TemplateError::UnknownField(n) => write!(f, "unknown field: {n}"),
            TemplateError::UnknownFunction(n) => write!(f, "unknown function: {n}"),
            TemplateError::BadArguments(n) => write!(f, "bad arguments for {n}"),
        }
    }
}
impl std::error::Error for TemplateError {}

#[derive(Debug, Clone, PartialEq)]
enum CmpOp {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Field(String),
    Str(String),
    Num(i64),
    Call(Box<Expr>, String, Vec<Expr>),
    Concat(Box<Expr>, Box<Expr>),
    Cmp(Box<Expr>, CmpOp, Box<Expr>),
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Lit(String),
    Expr(Expr),
}

/// A parsed template.
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    tokens: Vec<Token>,
    source: String,
}

/// Values available to a template.
#[derive(Debug, Clone, Default)]
pub struct Context {
    values: BTreeMap<String, Value>,
}

impl Context {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set(&mut self, key: &str, value: impl Into<Value>) -> &mut Self {
        self.values.insert(key.to_string(), value.into());
        self
    }
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.values.keys()
    }
}

struct Parser<'a> {
    chars: Vec<char>,
    pos: usize,
    base: usize,
    _src: &'a str,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str, base: usize) -> Self {
        Self { chars: src.chars().collect(), pos: 0, base, _src: src }
    }
    fn err(&self, msg: &str) -> TemplateError {
        TemplateError::Syntax { pos: self.base + self.pos, msg: msg.to_string() }
    }
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }
    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.pos += 1;
        }
    }
    fn eat(&mut self, s: &str) -> bool {
        self.skip_ws();
        let sc: Vec<char> = s.chars().collect();
        if self.chars[self.pos..].starts_with(&sc) {
            self.pos += sc.len();
            true
        } else {
            false
        }
    }
    fn expect(&mut self, s: &str) -> Result<(), TemplateError> {
        if self.eat(s) { Ok(()) } else { Err(self.err(&format!("expected '{s}'"))) }
    }

    fn parse_cond(&mut self) -> Result<Expr, TemplateError> {
        let c = self.parse_concat()?;
        if self.eat("?") {
            let a = self.parse_cond()?;
            self.expect(":")?;
            let b = self.parse_cond()?;
            return Ok(Expr::Cond(Box::new(c), Box::new(a), Box::new(b)));
        }
        Ok(c)
    }
    fn parse_concat(&mut self) -> Result<Expr, TemplateError> {
        let mut left = self.parse_cmp()?;
        while self.eat("+") {
            let right = self.parse_cmp()?;
            left = Expr::Concat(Box::new(left), Box::new(right));
        }
        Ok(left)
    }
    fn parse_cmp(&mut self) -> Result<Expr, TemplateError> {
        let left = self.parse_postfix()?;
        self.skip_ws();
        let op = if self.eat("==") {
            CmpOp::Eq
        } else if self.eat("!=") {
            CmpOp::Ne
        } else if self.eat("<=") {
            CmpOp::Le
        } else if self.eat(">=") {
            CmpOp::Ge
        } else if self.eat("<") {
            CmpOp::Lt
        } else if self.eat(">") {
            CmpOp::Gt
        } else {
            return Ok(left);
        };
        let right = self.parse_postfix()?;
        Ok(Expr::Cmp(Box::new(left), op, Box::new(right)))
    }
    fn parse_postfix(&mut self) -> Result<Expr, TemplateError> {
        let mut e = self.parse_primary()?;
        loop {
            self.skip_ws();
            if self.peek() == Some('.') {
                self.pos += 1;
                let name = self.parse_ident()?;
                self.expect("(")?;
                let mut args = Vec::new();
                self.skip_ws();
                if self.peek() != Some(')') {
                    loop {
                        args.push(self.parse_cond()?);
                        if self.eat(",") {
                            continue;
                        }
                        break;
                    }
                }
                self.expect(")")?;
                e = Expr::Call(Box::new(e), name, args);
            } else {
                break;
            }
        }
        Ok(e)
    }
    fn parse_ident(&mut self) -> Result<String, TemplateError> {
        self.skip_ws();
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_alphanumeric() || c == '_') {
            self.pos += 1;
        }
        if start == self.pos {
            return Err(self.err("expected a name"));
        }
        Ok(self.chars[start..self.pos].iter().collect())
    }
    fn parse_primary(&mut self) -> Result<Expr, TemplateError> {
        self.skip_ws();
        match self.peek() {
            Some('\'') | Some('"') => {
                let q = self.peek().unwrap();
                self.pos += 1;
                let mut s = String::new();
                loop {
                    match self.peek() {
                        None => return Err(self.err("unterminated string")),
                        Some(c) if c == q => {
                            self.pos += 1;
                            break;
                        }
                        Some('\\') => {
                            self.pos += 1;
                            if let Some(c) = self.peek() {
                                s.push(c);
                                self.pos += 1;
                            }
                        }
                        Some(c) => {
                            s.push(c);
                            self.pos += 1;
                        }
                    }
                }
                Ok(Expr::Str(s))
            }
            Some('(') => {
                self.pos += 1;
                let e = self.parse_cond()?;
                self.expect(")")?;
                Ok(e)
            }
            Some(c) if c.is_ascii_digit() => {
                let start = self.pos;
                while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                    self.pos += 1;
                }
                let s: String = self.chars[start..self.pos].iter().collect();
                Ok(Expr::Num(s.parse().map_err(|_| self.err("bad number"))?))
            }
            Some(c) if c.is_alphabetic() || c == '_' => Ok(Expr::Field(self.parse_ident()?)),
            Some(c) => Err(self.err(&format!("unexpected '{c}'"))),
            None => Err(self.err("unexpected end of expression")),
        }
    }
    fn finish(&mut self) -> Result<(), TemplateError> {
        self.skip_ws();
        if self.pos < self.chars.len() {
            return Err(self.err("unexpected text after expression"));
        }
        Ok(())
    }
}

impl Template {
    /// Parse a template string. Fails on syntax errors only; unknown fields fail at render time.
    pub fn parse(src: &str) -> Result<Template, TemplateError> {
        let chars: Vec<char> = src.chars().collect();
        let mut tokens = Vec::new();
        let mut lit = String::new();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '{' {
                if i + 1 < chars.len() && chars[i + 1] == '{' {
                    lit.push('{');
                    i += 2;
                    continue;
                }
                // find matching '}' honoring quotes
                let start = i + 1;
                let mut j = start;
                let mut quote: Option<char> = None;
                while j < chars.len() {
                    let d = chars[j];
                    match quote {
                        Some(q) => {
                            if d == '\\' {
                                j += 1;
                            } else if d == q {
                                quote = None;
                            }
                        }
                        None => {
                            if d == '\'' || d == '"' {
                                quote = Some(d);
                            } else if d == '}' {
                                break;
                            }
                        }
                    }
                    j += 1;
                }
                if j >= chars.len() {
                    return Err(TemplateError::Syntax { pos: i, msg: "missing '}'".into() });
                }
                if !lit.is_empty() {
                    tokens.push(Token::Lit(std::mem::take(&mut lit)));
                }
                let inner: String = chars[start..j].iter().collect();
                let mut p = Parser::new(&inner, start);
                let e = p.parse_cond()?;
                p.finish()?;
                tokens.push(Token::Expr(e));
                i = j + 1;
            } else if c == '}' && i + 1 < chars.len() && chars[i + 1] == '}' {
                lit.push('}');
                i += 2;
            } else {
                lit.push(c);
                i += 1;
            }
        }
        if !lit.is_empty() {
            tokens.push(Token::Lit(lit));
        }
        Ok(Template { tokens, source: src.to_string() })
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    /// Field names referenced by the template.
    pub fn fields(&self) -> Vec<String> {
        fn walk(e: &Expr, out: &mut Vec<String>) {
            match e {
                Expr::Field(n) => {
                    if !out.contains(n) {
                        out.push(n.clone())
                    }
                }
                Expr::Str(_) | Expr::Num(_) => {}
                Expr::Call(a, _, args) => {
                    walk(a, out);
                    args.iter().for_each(|x| walk(x, out));
                }
                Expr::Concat(a, b) | Expr::Cmp(a, _, b) => {
                    walk(a, out);
                    walk(b, out);
                }
                Expr::Cond(a, b, c) => {
                    walk(a, out);
                    walk(b, out);
                    walk(c, out);
                }
            }
        }
        let mut out = Vec::new();
        for t in &self.tokens {
            if let Token::Expr(e) = t {
                walk(e, &mut out);
            }
        }
        out
    }

    /// Render without sanitizing (raw string).
    pub fn render(&self, ctx: &Context) -> Result<String, TemplateError> {
        let mut out = String::new();
        for t in &self.tokens {
            match t {
                Token::Lit(s) => out.push_str(s),
                Token::Expr(e) => out.push_str(&eval(e, ctx)?.as_string()),
            }
        }
        Ok(out)
    }
}

fn eval(e: &Expr, ctx: &Context) -> Result<Value, TemplateError> {
    Ok(match e {
        Expr::Field(n) => ctx.get(n).cloned().ok_or_else(|| TemplateError::UnknownField(n.clone()))?,
        Expr::Str(s) => Value::from(s.as_str()),
        Expr::Num(n) => Value::Num(*n),
        Expr::Concat(a, b) => {
            let mut s = eval(a, ctx)?.as_string();
            s.push_str(&eval(b, ctx)?.as_string());
            Value::from(s)
        }
        Expr::Cmp(a, op, b) => {
            let va = eval(a, ctx)?;
            let vb = eval(b, ctx)?;
            let ord = match (va.as_num(), vb.as_num()) {
                (Some(x), Some(y)) => x.cmp(&y),
                _ => va.as_string().cmp(&vb.as_string()),
            };
            let r = match op {
                CmpOp::Eq => ord.is_eq(),
                CmpOp::Ne => ord.is_ne(),
                CmpOp::Lt => ord.is_lt(),
                CmpOp::Gt => ord.is_gt(),
                CmpOp::Le => ord.is_le(),
                CmpOp::Ge => ord.is_ge(),
            };
            Value::Num(r as i64)
        }
        Expr::Cond(c, a, b) => {
            if eval(c, ctx)?.truthy() { eval(a, ctx)? } else { eval(b, ctx)? }
        }
        Expr::Call(recv, name, args) => {
            let v = eval(recv, ctx)?;
            let argv: Vec<Value> = args.iter().map(|a| eval(a, ctx)).collect::<Result<_, _>>()?;
            call(&v, name, &argv)?
        }
    })
}

fn call(v: &Value, name: &str, args: &[Value]) -> Result<Value, TemplateError> {
    let bad = || TemplateError::BadArguments(name.to_string());
    Ok(match name {
        "upper" => Value::from(v.as_string().to_uppercase()),
        "lower" => Value::from(v.as_string().to_lowercase()),
        "trim" => Value::from(v.as_string().trim().to_string()),
        "or" => {
            let alt = args.first().ok_or_else(bad)?;
            if v.truthy() { v.clone() } else { alt.clone() }
        }
        "pad" => {
            let width = args.first().and_then(|a| a.as_num()).ok_or_else(bad)?.max(0) as usize;
            match v.as_num() {
                Some(n) if n >= 0 => Value::from(format!("{n:0width$}")),
                _ => v.clone(),
            }
        }
        "replace" => {
            if args.len() != 2 {
                return Err(bad());
            }
            Value::from(v.as_string().replace(&args[0].as_string(), &args[1].as_string()))
        }
        _ => return Err(TemplateError::UnknownFunction(name.to_string())),
    })
}

/// Characters not allowed in file names on Windows (and `/` everywhere).
pub const FORBIDDEN: &[char] = &['\\', '/', ':', '*', '?', '"', '<', '>', '|'];

/// Make one path segment safe on Linux, Windows and macOS.
pub fn sanitize_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '/' | '\\' | '|' => out.push('-'),
            ':' => out.push_str(" -"),
            '*' | '?' | '"' => {}
            '<' => out.push('('),
            '>' => out.push(')'),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    // collapse runs of spaces created by removals, trim, and strip trailing dots/spaces (Windows)
    let mut collapsed = String::with_capacity(out.len());
    let mut prev_space = false;
    for c in out.chars() {
        if c == ' ' {
            if !prev_space {
                collapsed.push(c);
            }
            prev_space = true;
        } else {
            collapsed.push(c);
            prev_space = false;
        }
    }
    let t = collapsed.trim().trim_end_matches(['.', ' ']).to_string();
    if t.is_empty() { "_".to_string() } else { t }
}

/// Limit a segment (without extension) so that segment + extension stays within `max_bytes`.
pub fn truncate_segment(seg: &str, ext_len: usize, max_bytes: usize) -> String {
    let limit = max_bytes.saturating_sub(ext_len);
    if seg.len() <= limit {
        return seg.to_string();
    }
    let mut end = limit;
    while end > 0 && !seg.is_char_boundary(end) {
        end -= 1;
    }
    seg[..end].trim_end().to_string()
}

/// Render a file name (without extension) from a template, sanitized and length-limited.
pub fn render_file_stem(t: &Template, ctx: &Context, ext: &str, max_bytes: usize) -> Result<String, TemplateError> {
    let raw = t.render(ctx)?;
    let seg = sanitize_segment(&raw);
    let ext_len = if ext.is_empty() { 0 } else { ext.len() + 1 };
    Ok(truncate_segment(&seg, ext_len, max_bytes))
}

/// Render a relative folder path: the template is split on `/` and each segment is sanitized.
pub fn render_folder_path(t_src: &str, ctx: &Context, max_bytes: usize) -> Result<Vec<String>, TemplateError> {
    let mut segs = Vec::new();
    for part in t_src.split('/') {
        if part.trim().is_empty() {
            continue;
        }
        let t = Template::parse(part)?;
        let raw = t.render(ctx)?;
        segs.push(truncate_segment(&sanitize_segment(&raw), 0, max_bytes));
    }
    Ok(segs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Context {
        let mut c = Context::new();
        c.set("albumartist", "Jody Wisternoff")
            .set("artist", "Jody Wisternoff")
            .set("album", "Anjunadeep 09")
            .set("year", "2017")
            .set("track", "03")
            .set("tracktotal", "20")
            .set("disc", "1")
            .set("disctotal", "1")
            .set("title", "Wherever You Go")
            .set("series", "");
        c
    }

    #[test]
    fn renders_default_album_template() {
        let t = Template::parse("{albumartist} - {album} - {year} - {track} von {tracktotal} - {title}").unwrap();
        assert_eq!(t.render(&ctx()).unwrap(), "Jody Wisternoff - Anjunadeep 09 - 2017 - 03 von 20 - Wherever You Go");
    }

    #[test]
    fn functions_and_conditions() {
        let c = ctx();
        let t = Template::parse("{artist.upper()} - {track.pad(3)} - {series.or(album)}").unwrap();
        assert_eq!(t.render(&c).unwrap(), "JODY WISTERNOFF - 003 - Anjunadeep 09");
        let t = Template::parse("{disctotal > 1 ? 'CD' + disc + ' - ' : ''}{track} - {title}").unwrap();
        assert_eq!(t.render(&c).unwrap(), "03 - Wherever You Go");
        let mut c2 = ctx();
        c2.set("disctotal", "2").set("disc", "2");
        assert_eq!(t.render(&c2).unwrap(), "CD2 - 03 - Wherever You Go");
        let t = Template::parse("{title.replace('You', 'U')}").unwrap();
        assert_eq!(t.render(&c).unwrap(), "Wherever U Go");
        let t = Template::parse("{year == 2017 ? 'yes' : 'no'}").unwrap();
        assert_eq!(t.render(&c).unwrap(), "yes");
    }

    #[test]
    fn escapes_and_errors() {
        let t = Template::parse("{{literal}} {title}").unwrap();
        assert_eq!(t.render(&ctx()).unwrap(), "{literal} Wherever You Go");
        assert!(matches!(Template::parse("{title"), Err(TemplateError::Syntax { .. })));
        assert!(matches!(Template::parse("{title.nope()}").unwrap().render(&ctx()), Err(TemplateError::UnknownFunction(_))));
        assert!(matches!(Template::parse("{nope}").unwrap().render(&ctx()), Err(TemplateError::UnknownField(_))));
        assert_eq!(Template::parse("{a + 'x'}").unwrap().fields(), vec!["a".to_string()]);
    }

    #[test]
    fn sanitizes_segments() {
        assert_eq!(sanitize_segment("AC/DC: Back*in?Black\"<>|"), "AC-DC - BackinBlack()-");
        assert_eq!(sanitize_segment("  Ende.  "), "Ende");
        assert_eq!(sanitize_segment("***"), "_");
        assert_eq!(truncate_segment("abcdef", 4, 8), "abcd");
        let long = "ä".repeat(300);
        let t = truncate_segment(&long, 4, 255);
        assert!(t.len() + 4 <= 255 && t.chars().all(|c| c == 'ä'));
    }

    #[test]
    fn folder_path() {
        let segs = render_folder_path("{albumartist}/{album} ({year})", &ctx(), 255).unwrap();
        assert_eq!(segs, vec!["Jody Wisternoff", "Anjunadeep 09 (2017)"]);
        let stem = render_file_stem(&Template::parse("{artist}: {title}").unwrap(), &ctx(), "flac", 255).unwrap();
        assert_eq!(stem, "Jody Wisternoff - Wherever You Go");
    }
}
