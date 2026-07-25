//! Filter-expression parsing: a small tokenizer + recursive-descent parser
//! over the supported Bases subset, and the classifier that pattern-matches
//! the AST into [`AtomKind`]s. Anything outside the profile parses to `None`
//! and becomes an opaque (preserved, ignored) node upstream.
//!
//! Deliberately hand-rolled rather than a parser-generator dependency: the
//! grammar is tiny and frozen (comparisons, calls, member access, date
//! arithmetic), the hard part is the *classification* — which no parsing
//! crate helps with — and this crate compiles into the iOS app, where extra
//! proc-macro dependencies cost real build time. The whole surface is two
//! functions ([`parse_expression`], [`parse_duration`]), so swapping the
//! internals for `pest`/`winnow` later would touch nothing else.

use super::model::{
    AtomKind, CompareOp, CompareValue, ContainsMode, DateBase, DateExpr, DateOffset, DurationSpec,
    FieldRef,
};
use chrono::NaiveDate;
use regex::Regex;
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// Expression parsing
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Ident(String),
    Str(String),
    Num(f64),
    Symbol(&'static str),
}

/// Lexes a filter expression. `None` on any character we don't understand
/// (including `&&`/`||`, which we deliberately don't support) — the whole atom
/// then becomes opaque.
fn tokenize(input: &str) -> Option<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '"' || c == '\'' {
            chars.next();
            let mut value = String::new();
            loop {
                let next = chars.next()?;
                if next == '\\' {
                    value.push(chars.next()?);
                } else if next == c {
                    break;
                } else {
                    value.push(next);
                }
            }
            tokens.push(Token::Str(value));
        } else if c.is_ascii_digit() {
            let mut number = String::new();
            while chars
                .peek()
                .is_some_and(|c| c.is_ascii_digit() || *c == '.')
            {
                number.push(chars.next().expect("peeked"));
            }
            tokens.push(Token::Num(number.parse().ok()?));
        } else if c.is_alphabetic() || c == '_' {
            let mut ident = String::new();
            while chars
                .peek()
                .is_some_and(|c| c.is_alphanumeric() || *c == '_')
            {
                ident.push(chars.next().expect("peeked"));
            }
            tokens.push(Token::Ident(ident));
        } else {
            chars.next();
            let symbol = match c {
                '=' if chars.peek() == Some(&'=') => {
                    chars.next();
                    "=="
                }
                '=' => return None,
                '!' => {
                    if chars.peek() == Some(&'=') {
                        chars.next();
                        "!="
                    } else {
                        "!"
                    }
                }
                '>' => {
                    if chars.peek() == Some(&'=') {
                        chars.next();
                        ">="
                    } else {
                        ">"
                    }
                }
                '<' => {
                    if chars.peek() == Some(&'=') {
                        chars.next();
                        "<="
                    } else {
                        "<"
                    }
                }
                '+' => "+",
                '-' => "-",
                '(' => "(",
                ')' => ")",
                '[' => "[",
                ']' => "]",
                '.' => ".",
                ',' => ",",
                _ => return None,
            };
            tokens.push(Token::Symbol(symbol));
        }
    }
    Some(tokens)
}

/// The small expression AST the classifier pattern-matches into [`AtomKind`]s.
#[derive(Clone, Debug, PartialEq)]
enum Expr {
    Ident(String),
    Str(String),
    Num(f64),
    Bool(bool),
    Member(Box<Expr>, String),
    Call(Box<Expr>, Vec<Expr>),
    Not(Box<Expr>),
    Cmp(Box<Expr>, CompareOp, Box<Expr>),
    Add(Box<Expr>, bool, Box<Expr>),
}

struct Parser<'a> {
    tokens: &'a [Token],
    position: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    fn next(&mut self) -> Option<&Token> {
        let token = self.tokens.get(self.position);
        self.position += 1;
        token
    }

    fn eat_symbol(&mut self, symbol: &str) -> bool {
        match self.peek() {
            Some(Token::Symbol(s)) if *s == symbol => {
                self.position += 1;
                true
            }
            _ => false,
        }
    }

    fn parse_expr(&mut self) -> Option<Expr> {
        let lhs = self.parse_add()?;
        let op = match self.peek() {
            Some(Token::Symbol("==")) => Some(CompareOp::Eq),
            Some(Token::Symbol("!=")) => Some(CompareOp::Ne),
            Some(Token::Symbol(">")) => Some(CompareOp::Gt),
            Some(Token::Symbol(">=")) => Some(CompareOp::Gte),
            Some(Token::Symbol("<")) => Some(CompareOp::Lt),
            Some(Token::Symbol("<=")) => Some(CompareOp::Lte),
            _ => None,
        };
        match op {
            Some(op) => {
                self.position += 1;
                let rhs = self.parse_add()?;
                Some(Expr::Cmp(Box::new(lhs), op, Box::new(rhs)))
            }
            None => Some(lhs),
        }
    }

    fn parse_add(&mut self) -> Option<Expr> {
        let mut lhs = self.parse_unary()?;
        loop {
            let negative = match self.peek() {
                Some(Token::Symbol("+")) => false,
                Some(Token::Symbol("-")) => true,
                _ => break,
            };
            self.position += 1;
            let rhs = self.parse_unary()?;
            lhs = Expr::Add(Box::new(lhs), negative, Box::new(rhs));
        }
        Some(lhs)
    }

    fn parse_unary(&mut self) -> Option<Expr> {
        if self.eat_symbol("!") {
            return Some(Expr::Not(Box::new(self.parse_unary()?)));
        }
        if self.eat_symbol("-") {
            if let Some(Token::Num(value)) = self.peek().cloned() {
                self.position += 1;
                return Some(Expr::Num(-value));
            }
            return None;
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Option<Expr> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.eat_symbol(".") {
                let Some(Token::Ident(name)) = self.next().cloned() else {
                    return None;
                };
                expr = Expr::Member(Box::new(expr), name);
                if self.eat_symbol("(") {
                    let args = self.parse_args()?;
                    expr = Expr::Call(Box::new(expr), args);
                }
            } else if self.eat_symbol("[") {
                let Some(Token::Str(key)) = self.next().cloned() else {
                    return None;
                };
                if !self.eat_symbol("]") {
                    return None;
                }
                // `note["画像"]` reads the same property as `note.画像`.
                expr = Expr::Member(Box::new(expr), key);
            } else {
                break;
            }
        }
        Some(expr)
    }

    fn parse_primary(&mut self) -> Option<Expr> {
        match self.next().cloned()? {
            Token::Ident(name) => {
                let expr = match name.as_str() {
                    "true" => Expr::Bool(true),
                    "false" => Expr::Bool(false),
                    _ => Expr::Ident(name),
                };
                if matches!(expr, Expr::Ident(_)) && self.eat_symbol("(") {
                    let args = self.parse_args()?;
                    return Some(Expr::Call(Box::new(expr), args));
                }
                Some(expr)
            }
            Token::Str(value) => Some(Expr::Str(value)),
            Token::Num(value) => Some(Expr::Num(value)),
            Token::Symbol("(") => {
                let expr = self.parse_expr()?;
                if self.eat_symbol(")") {
                    Some(expr)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn parse_args(&mut self) -> Option<Vec<Expr>> {
        let mut args = Vec::new();
        if self.eat_symbol(")") {
            return Some(args);
        }
        loop {
            args.push(self.parse_expr()?);
            if self.eat_symbol(")") {
                return Some(args);
            }
            if !self.eat_symbol(",") {
                return None;
            }
        }
    }
}

/// Parses one filter-expression string into a supported atom (plus a negation
/// flag), or `None` when any part of it falls outside the supported profile.
pub fn parse_expression(input: &str) -> Option<(AtomKind, bool)> {
    let tokens = tokenize(input)?;
    let mut parser = Parser {
        tokens: &tokens,
        position: 0,
    };
    let expr = parser.parse_expr()?;
    if parser.position != tokens.len() {
        return None;
    }
    classify(&expr)
}

fn classify(expr: &Expr) -> Option<(AtomKind, bool)> {
    match expr {
        Expr::Not(inner) => {
            let (kind, negated) = classify(inner)?;
            // A negated comparison flips the operator instead of carrying a
            // negation flag, so the printer emits `!=` rather than `!(a == b)`.
            if let AtomKind::Compare { field, op, value } = kind {
                Some((
                    AtomKind::Compare {
                        field,
                        op: op.flipped(),
                        value,
                    },
                    negated,
                ))
            } else {
                Some((kind, !negated))
            }
        }
        Expr::Cmp(lhs, op, rhs) => {
            let field = field_ref(lhs)?;
            let value = compare_value(rhs)?;
            Some((
                AtomKind::Compare {
                    field,
                    op: *op,
                    value,
                },
                false,
            ))
        }
        Expr::Call(callee, args) => classify_call(callee, args),
        _ => None,
    }
}

fn classify_call(callee: &Expr, args: &[Expr]) -> Option<(AtomKind, bool)> {
    let Expr::Member(receiver, method) = callee else {
        return None;
    };
    if matches!(receiver.as_ref(), Expr::Ident(name) if name == "file") {
        let kind = match method.as_str() {
            "hasTag" => AtomKind::HasTag {
                tags: string_args(args)?,
            },
            "inFolder" => AtomKind::InFolder {
                folder: single_string_arg(args)?,
            },
            "hasLink" => AtomKind::HasLink {
                field: None,
                target: single_string_arg(args)?,
            },
            _ => return None,
        };
        return Some((kind, false));
    }
    let field = field_ref(receiver)?;
    // `note.studio.contains(link("Name"))` — Bases' link-membership test on a
    // property. It reads as a scoped link rule (matched through the relation
    // graph) rather than as text membership. Only a frontmatter property can
    // hold links, so `file.name.contains(link(…))` stays opaque, as does a
    // mixture of link and string arguments.
    if let Some(target) = single_link_arg(args) {
        return match (method.as_str(), field) {
            ("contains" | "containsAny" | "containsAll", FieldRef::Note(name)) => Some((
                AtomKind::HasLink {
                    field: Some(name),
                    target,
                },
                false,
            )),
            _ => None,
        };
    }
    let kind = match method.as_str() {
        "contains" => AtomKind::Contains {
            field,
            mode: ContainsMode::Any,
            values: string_args(args)?,
        },
        "containsAny" => AtomKind::Contains {
            field,
            mode: ContainsMode::Any,
            values: string_args(args)?,
        },
        "containsAll" => AtomKind::Contains {
            field,
            mode: ContainsMode::All,
            values: string_args(args)?,
        },
        "startsWith" => AtomKind::StartsWith {
            field,
            value: single_string_arg(args)?,
        },
        "endsWith" => AtomKind::EndsWith {
            field,
            value: single_string_arg(args)?,
        },
        "isEmpty" => {
            if !args.is_empty() {
                return None;
            }
            AtomKind::IsEmpty { field }
        }
        _ => return None,
    };
    Some((kind, false))
}

/// The target of a lone `link("Name")` argument, or `None` when the arguments
/// aren't exactly that — one link literal and nothing else.
fn single_link_arg(args: &[Expr]) -> Option<String> {
    let [Expr::Call(callee, link_args)] = args else {
        return None;
    };
    if !matches!(callee.as_ref(), Expr::Ident(name) if name == "link") {
        return None;
    }
    match link_args.as_slice() {
        [Expr::Str(target)] => Some(target.clone()),
        _ => None,
    }
}

fn string_args(args: &[Expr]) -> Option<Vec<String>> {
    if args.is_empty() {
        return None;
    }
    args.iter()
        .map(|arg| match arg {
            Expr::Str(value) => Some(value.clone()),
            _ => None,
        })
        .collect()
}

fn single_string_arg(args: &[Expr]) -> Option<String> {
    match args {
        [Expr::Str(value)] => Some(value.clone()),
        _ => None,
    }
}

/// Reserved bare identifiers that are never a frontmatter-field shorthand.
const RESERVED_IDENTS: [&str; 6] = ["file", "note", "formula", "date", "now", "today"];

fn field_ref(expr: &Expr) -> Option<FieldRef> {
    match expr {
        Expr::Ident(name) if !RESERVED_IDENTS.contains(&name.as_str()) => {
            Some(FieldRef::Note(name.clone()))
        }
        Expr::Member(receiver, name) => match receiver.as_ref() {
            Expr::Ident(base) if base == "note" => Some(FieldRef::Note(name.clone())),
            Expr::Ident(base) if base == "file" && name == "name" => Some(FieldRef::FileName),
            Expr::Ident(base) if base == "file" && name == "mtime" => Some(FieldRef::FileMtime),
            _ => None,
        },
        _ => None,
    }
}

fn compare_value(expr: &Expr) -> Option<CompareValue> {
    match expr {
        Expr::Str(value) => Some(CompareValue::String(value.clone())),
        Expr::Num(value) => Some(CompareValue::Number(*value)),
        Expr::Bool(value) => Some(CompareValue::Bool(*value)),
        _ => date_expr(expr).map(CompareValue::Date),
    }
}

fn date_expr(expr: &Expr) -> Option<DateExpr> {
    match expr {
        Expr::Call(callee, args) => {
            let Expr::Ident(name) = callee.as_ref() else {
                return None;
            };
            let base = match (name.as_str(), args.as_slice()) {
                ("today", []) => DateBase::Today,
                ("now", []) => DateBase::Now,
                ("date", [Expr::Str(value)]) => {
                    let (year, month, day) = crate::dates::exact_date_parts(Some(value))?;
                    DateBase::Absolute(NaiveDate::from_ymd_opt(year, month, day)?)
                }
                _ => return None,
            };
            Some(DateExpr {
                base,
                offsets: Vec::new(),
            })
        }
        Expr::Add(lhs, negative, rhs) => {
            let mut date = date_expr(lhs)?;
            let Expr::Str(duration) = rhs.as_ref() else {
                return None;
            };
            date.offsets.push(DateOffset {
                negative: *negative,
                duration: parse_duration(duration)?,
            });
            Some(date)
        }
        _ => None,
    }
}

fn duration_piece_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(\d+)\s*([A-Za-z]+)").unwrap())
}

/// Parses a Bases duration literal (`"90d"`, `"1M 4h"`). Unit letters are
/// case-sensitive: `M` is months, `m` is minutes.
pub fn parse_duration(input: &str) -> Option<DurationSpec> {
    let mut spec = DurationSpec::default();
    let mut consumed = 0usize;
    for captures in duration_piece_regex().captures_iter(input) {
        let full = captures.get(0).expect("full match");
        // Everything between pieces must be whitespace, or the literal is junk.
        if !input[consumed..full.start()].trim().is_empty() {
            return None;
        }
        consumed = full.end();
        let amount: u32 = captures[1].parse().ok()?;
        let slot = match &captures[2] {
            "y" | "year" | "years" => &mut spec.years,
            "M" | "month" | "months" => &mut spec.months,
            "w" | "week" | "weeks" => &mut spec.weeks,
            "d" | "day" | "days" => &mut spec.days,
            "h" | "hour" | "hours" => &mut spec.hours,
            "m" | "minute" | "minutes" => &mut spec.minutes,
            "s" | "second" | "seconds" => &mut spec.seconds,
            _ => return None,
        };
        *slot = slot.saturating_add(amount);
    }
    if consumed == 0 || !input[consumed..].trim().is_empty() {
        return None;
    }
    Some(spec)
}
