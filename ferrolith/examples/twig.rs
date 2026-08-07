use ferrelex::{
    lexbuf::{refiller::Utf8Refiller, utf8::LexBuf},
    lexer::lex,
};
use ferrolith::grammar;
use std::collections::HashMap;

// ── Span ──────────────────────────────────────────────────────────────────────

/// Source location of a token or expression: line and column of the first
/// character (inclusive) and the character just past the last one (exclusive).
/// Lines are 1-indexed; columns are 0-indexed.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Span {
    start_line: usize,
    start_col: usize,
    end_line: usize,
    end_col: usize,
}

/// Payload merge function: keep the start of `first` and the end of `last`.
fn merge_span(first: Span, last: Span) -> Span {
    Span {
        start_line: first.start_line,
        start_col:  first.start_col,
        end_line:   last.end_line,
        end_col:    last.end_col,
    }
}

/// Extract a `Span` from the current lexeme position.
/// `buf.location()` returns `ferrelex_core::location::Location`; we access its
/// public fields without naming the type directly.
fn span_of(buf: &LexBuf) -> Span {
    let loc = buf.location();
    Span {
        start_line: loc.start.line,
        start_col:  loc.start.col,
        end_line:   loc.end.line,
        end_col:    loc.end.col,
    }
}

// ── AST ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Num(i64, Span),
    Bool(bool, Span),
    Var(String, Span),
    Neg(Box<Expr>, Span),
    BinOp(BinOp, Box<Expr>, Box<Expr>, Span),
}

impl Expr {
    fn span(&self) -> Span {
        match self {
            Expr::Num(_, s)       => *s,
            Expr::Bool(_, s)      => *s,
            Expr::Var(_, s)       => *s,
            Expr::Neg(_, s)       => *s,
            Expr::BinOp(_, _, _, s) => *s,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Stmt {
    Let(String, Expr, Span),
    Print(Expr, Span),
}

// ── Token ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Token {
    Ident(String),
    Int(i64),
    True,
    False,
    Let,
    Print,
    Plus,
    Minus,
    Star,
    Slash,
    EqEq,
    BangEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    Eq,
    LParen,
    RParen,
    Semi,
}

// ── Lexer ─────────────────────────────────────────────────────────────────────

lex! {
    const INT:   Regex = Plus(digit_ascii);
    const IDENT: Regex = (alpha_ascii | '_', Star(word_ascii));

    fn next_token(buf: &mut LexBuf) -> Option<(Token, Span)> {
        #[lexer]
        match buf {
            #[skip] whitespace_ascii => {}
            INT => {
                let n = buf.lexeme_str().parse().unwrap();
                Some((Token::Int(n), span_of(buf)))
            }
            IDENT => {
                let tok = match buf.lexeme_str() {
                    "let"   => Token::Let,
                    "print" => Token::Print,
                    "true"  => Token::True,
                    "false" => Token::False,
                    s       => Token::Ident(s.to_owned()),
                };
                Some((tok, span_of(buf)))
            }
            "==" => Some((Token::EqEq,   span_of(buf))),
            "!=" => Some((Token::BangEq, span_of(buf))),
            "<=" => Some((Token::LtEq,   span_of(buf))),
            ">=" => Some((Token::GtEq,   span_of(buf))),
            '+'  => Some((Token::Plus,   span_of(buf))),
            '-'  => Some((Token::Minus,  span_of(buf))),
            '*'  => Some((Token::Star,   span_of(buf))),
            '/'  => Some((Token::Slash,  span_of(buf))),
            '<'  => Some((Token::Lt,     span_of(buf))),
            '>'  => Some((Token::Gt,     span_of(buf))),
            '='  => Some((Token::Eq,     span_of(buf))),
            '('  => Some((Token::LParen, span_of(buf))),
            ')'  => Some((Token::RParen, span_of(buf))),
            ';'  => Some((Token::Semi,   span_of(buf))),
            eof  => None,
            _    => None,
        }
    }
}

// ── Parser ────────────────────────────────────────────────────────────────────

grammar! {
    #![parser_cfg(payload_merge = merge_span, payload_merge_strategy = first_last)]

    type Token   = Token;
    type Payload = Span;

    #[start]
    fn program() -> Vec<Stmt> {
        ()                             => vec![],
        (mut prog @ program, s @ stmt) => { prog.push(s); prog },
    }

    fn stmt() -> Stmt {
        (Let, Ident(name), Eq, e @ expr, Semi) => Stmt::Let(name, e, _payload),
        (Print, e @ expr, Semi)                 => Stmt::Print(e, _payload),
    }

    // Comparisons (lowest precedence, left-associative).
    fn expr() -> Expr {
        (l @ expr, EqEq,   r @ add_expr) => Expr::BinOp(BinOp::Eq, Box::new(l), Box::new(r), _payload),
        (l @ expr, BangEq, r @ add_expr) => Expr::BinOp(BinOp::Ne, Box::new(l), Box::new(r), _payload),
        (l @ expr, Lt,     r @ add_expr) => Expr::BinOp(BinOp::Lt, Box::new(l), Box::new(r), _payload),
        (l @ expr, Gt,     r @ add_expr) => Expr::BinOp(BinOp::Gt, Box::new(l), Box::new(r), _payload),
        (l @ expr, LtEq,   r @ add_expr) => Expr::BinOp(BinOp::Le, Box::new(l), Box::new(r), _payload),
        (l @ expr, GtEq,   r @ add_expr) => Expr::BinOp(BinOp::Ge, Box::new(l), Box::new(r), _payload),
        (e @ add_expr)                    => e,
    }

    // Addition and subtraction.
    fn add_expr() -> Expr {
        (l @ add_expr, Plus,  r @ mul_expr) => Expr::BinOp(BinOp::Add, Box::new(l), Box::new(r), _payload),
        (l @ add_expr, Minus, r @ mul_expr) => Expr::BinOp(BinOp::Sub, Box::new(l), Box::new(r), _payload),
        (e @ mul_expr)                       => e,
    }

    // Multiplication and division.
    fn mul_expr() -> Expr {
        (l @ mul_expr, Star,  r @ unary) => Expr::BinOp(BinOp::Mul, Box::new(l), Box::new(r), _payload),
        (l @ mul_expr, Slash, r @ unary) => Expr::BinOp(BinOp::Div, Box::new(l), Box::new(r), _payload),
        (e @ unary)                       => e,
    }

    // Unary negation.
    fn unary() -> Expr {
        (Minus, e @ unary) => Expr::Neg(Box::new(e), _payload),
        (e @ atom)          => e,
    }

    // Atomic expressions: literals, variables, parenthesised sub-expressions.
    fn atom() -> Expr {
        (Int(n))                   => Expr::Num(n, _1_payload),
        (True)                     => Expr::Bool(true, _1_payload),
        (False)                    => Expr::Bool(false, _1_payload),
        (Ident(name))              => Expr::Var(name, _1_payload),
        (LParen, e @ expr, RParen) => e,
    }
}

// ── Interpreter ───────────────────────────────────────────────────────────────

fn eval(expr: &Expr, env: &HashMap<String, i64>) -> i64 {
    match expr {
        Expr::Num(n, _)    => *n,
        Expr::Bool(b, _)   => *b as i64,
        Expr::Var(name, _) => *env.get(name).unwrap_or_else(|| panic!("undefined: {name}")),
        Expr::Neg(e, _)    => -eval(e, env),
        Expr::BinOp(op, l, r, _) => {
            let lv = eval(l, env);
            let rv = eval(r, env);
            match op {
                BinOp::Add => lv + rv,
                BinOp::Sub => lv - rv,
                BinOp::Mul => lv * rv,
                BinOp::Div => lv / rv,
                BinOp::Eq  => (lv == rv) as i64,
                BinOp::Ne  => (lv != rv) as i64,
                BinOp::Lt  => (lv <  rv) as i64,
                BinOp::Gt  => (lv >  rv) as i64,
                BinOp::Le  => (lv <= rv) as i64,
                BinOp::Ge  => (lv >= rv) as i64,
            }
        }
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

const SOURCE: &str = include_str!("sample.twig");

fn main() {
    // Lex and parse.
    let mut buf = LexBuf::new(Utf8Refiller::new(SOURCE.to_owned()));
    let tokens = std::iter::from_fn(move || next_token(&mut buf));
    let program = parse(tokens).expect("parse error");

    println!("Parsed {} statements:\n", program.len());
    for (i, stmt) in program.iter().enumerate() {
        println!("  [{i}] {stmt:?}");
    }
    println!();

    // ── AST structure and span checks ────────────────────────────────────────

    assert_eq!(program.len(), 8, "expected 8 statements");

    // Statement 0: `let a = 6;`  (line 1)
    //
    //   col: 0123456789
    //        let a = 6;
    //
    // `let` starts at col 0 (end 3); `;` ends at col 10.
    // `6`   starts at col 8 (end 9).
    match &program[0] {
        Stmt::Let(name, expr, stmt_span) => {
            assert_eq!(name, "a");
            assert_eq!(
                *stmt_span,
                Span { start_line: 1, start_col: 0, end_line: 1, end_col: 10 }
            );
            match expr {
                Expr::Num(n, expr_span) => {
                    assert_eq!(*n, 6);
                    assert_eq!(
                        *expr_span,
                        Span { start_line: 1, start_col: 8, end_line: 1, end_col: 9 }
                    );
                }
                other => panic!("expected Num, got {other:?}"),
            }
        }
        other => panic!("expected Let, got {other:?}"),
    }

    // Statement 2: `let product = a * b;`  (line 3)
    //
    //   col: 0         1
    //        0123456789012345678901
    //        let product = a * b;
    //
    // `let` starts at col 0 (end 3); `;` ends at col 20.
    // `a`   starts at col 14 (end 15); `b` ends at col 19.
    // BinOp payload spans from `a` start to `b` end: col 14..19.
    match &program[2] {
        Stmt::Let(name, expr, stmt_span) => {
            assert_eq!(name, "product");
            assert_eq!(
                *stmt_span,
                Span { start_line: 3, start_col: 0, end_line: 3, end_col: 20 }
            );
            match expr {
                Expr::BinOp(BinOp::Mul, l, r, expr_span) => {
                    assert!(matches!(l.as_ref(), Expr::Var(v, _) if v == "a"));
                    assert!(matches!(r.as_ref(), Expr::Var(v, _) if v == "b"));
                    assert_eq!(
                        *expr_span,
                        Span { start_line: 3, start_col: 14, end_line: 3, end_col: 19 }
                    );
                }
                other => panic!("expected BinOp(Mul), got {other:?}"),
            }
        }
        other => panic!("expected Let, got {other:?}"),
    }

    // Statement 4: `print product;`  (line 5)
    // Confirm the printed expression carries the variable's span.
    match &program[4] {
        Stmt::Print(expr, _) => {
            assert_eq!(expr.span(), Span { start_line: 5, start_col: 6, end_line: 5, end_col: 13 });
        }
        other => panic!("expected Print, got {other:?}"),
    }

    // ── Interpreter checks ───────────────────────────────────────────────────

    let mut env: HashMap<String, i64> = HashMap::new();
    let mut prints: Vec<i64> = Vec::new();

    for stmt in &program {
        match stmt {
            Stmt::Let(name, expr, _) => {
                let value = eval(expr, &env);
                env.insert(name.clone(), value);
            }
            Stmt::Print(expr, _) => {
                let value = eval(expr, &env);
                prints.push(value);
                println!("print => {value}");
            }
        }
    }

    assert_eq!(env["a"], 6);
    assert_eq!(env["b"], 7);
    assert_eq!(env["product"], 42); // 6 * 7
    assert_eq!(env["sum"], 13);     // 6 + 7

    assert_eq!(prints[0], 42); // print product;
    assert_eq!(prints[1], 13); // print sum;
    assert_eq!(prints[2], 1);  // print product > sum;  (42 > 13 → true → 1)
    assert_eq!(prints[3], 1);  // print product == 42;  (42 == 42 → true → 1)

    println!("\nAll assertions passed.");
}
