# Ferrolith

[![CI](https://github.com/emilienlemaire/ferrolith/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/emilienlemaire/ferrolith/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/ferrolith.svg)](https://crates.io/crates/ferrolith)
[![docs.rs](https://img.shields.io/docsrs/ferrolith)](https://docs.rs/ferrolith)
[![License: LGPL-3.0-only](https://img.shields.io/crates/l/ferrolith.svg)](https://github.com/emilienlemaire/ferrolith/blob/main/LICENSE)

Ferrolith is an LR(1) parser generator for Rust. You describe your grammar inside a `grammar!` macro and it gets compiled at build time into a fully typed Rust parser. No generated source files, no build scripts, nothing to check in.

```rust
use ferrolith::grammar;

#[derive(Debug)]
enum Token { Num(i64), Plus, Star }

grammar! {
    type Token = Token;

    #[left(Plus)]
    #[left(Star)]

    #[start]
    fn expr() -> i64 {
        (Num(n))                   => n,
        (l @ expr, Plus, r @ expr) => l + r,
        (l @ expr, Star, r @ expr) => l * r,
    }
}

fn main() {
    // Star binds tighter than Plus, so this is 2 + (3 * 4) = 14.
    let result = parse(
        [Token::Num(2), Token::Plus, Token::Num(3), Token::Star, Token::Num(4)].into_iter()
    );
    assert_eq!(result.unwrap(), 14);
}
```

The `grammar!` block generates a `parse` function in the current scope.

## Features

- **LR(1) construction** at compile time, using canonical item sets and lookahead sets
- **Named and positional bindings** (`l @ expr`, `Num(n)`, `_2`)
- **Operator precedence and associativity** via `#[left]`, `#[right]`, `#[nonassoc]`
- **Built-in combinators**: `star`, `plus`, `opt`, `sep`, `sep_opt`
- **Parametric rules** for reusable higher-order combinators
- **Token payload**: attach data (like source locations) to tokens and propagate it through the parse
- **Error recovery** via the `error` pseudo-token, modelled on yacc/bison panic-mode recovery
- **Incremental API**: feed tokens one at a time with `start()` / `feed()` / `Checkpoint`
- **Compile-time grammar checks**: undefined rules, unreachable rules, and non-productive rules all become build errors
- **Optional cloneable state** for speculative parsing
- **Optional automaton introspection** for tooling and visualisation

## Quick start

Add the crate to your `Cargo.toml`:

```toml
[dependencies]
ferrolith = "0.1"
```

Then write a `grammar!` block anywhere in your code. The block generates `parse`, `start`, `feed`, and `Checkpoint` in the surrounding scope, with the visibility of the `#[start]` rule: write `#[start] pub fn ...` to use the parser from another module. The generated names are fixed, so use one `grammar!` per module.

## Grammar at a glance

### Token type

```rust,ignore
grammar! {
    type Token = MyToken;
    // ...
}
```

Token variants are matched in PascalCase. Unit variants (`Plus`) discard the token; tuple variants (`Num(n)`) bind the inner fields.

The token type can be any enum, including a path (`lexer::Tok`) or a generic instantiation (`Tok<i64>`). Lifetime parameters other than `'static` are not supported, so tokens should own their data.

### Rules

```rust,ignore
fn expr() -> i64 {
    (Num(n))                   => n,
    (l @ expr, Plus, r @ expr) => l + r,
}
```

`l @ expr` runs the `expr` rule and binds its return value to `l`. Positional bindings (`_1`, `_2`, ...) are always available alongside named ones.

The `#[start]` rule may carry a visibility (`#[start] pub fn program() -> ...`), which the generated API inherits. Without one, `parse` and friends are private to the module; other rules cannot have a visibility.

### Precedence

```rust,ignore
#[left(Plus, Minus)]  // lowest
#[left(Star, Slash)]  // higher
#[right(Caret)]       // highest, right-associative
```

Declarations go before the first rule in the `grammar!` block. They resolve shift/reduce conflicts: higher token precedence shifts, equal precedence follows associativity. Unresolved conflicts produce a compile-time warning and default to shift; reduce/reduce conflicts are always a compile-time error.

A production takes the precedence of its rightmost terminal. Override it with `#[prec(Name)]` on the alternative, where `Name` can be a token or a pseudo-token that exists only in a precedence declaration:

```rust,ignore
#[right(UMinus)]  // pseudo-token, not a variant of the token type

fn expr() -> i64 {
    // ...
    #[prec(UMinus)]
    (Minus, e @ expr) => -e,
}
```

### Built-in combinators

| Combinator | Matches | Returns |
|---|---|---|
| `star(x)` | zero or more `x` | `Vec<_>` |
| `plus(x)` | one or more `x` | `Vec<_>` |
| `opt(x)` | zero or one `x` | `Option<_>` |
| `sep(Sep, x)` | one or more `x` separated by `Sep` | `Vec<_>` |
| `sep_opt(Sep, x)` | zero or more `x` separated by `Sep` | `Vec<_>` |

### Token payload

Attach a type to each token (source spans are the common case) and have it propagated through reductions automatically:

```rust,ignore
grammar! {
    #![parser_cfg(payload_merge = merge_span, payload_merge_strategy = first_last)]

    type Token   = MyToken;
    type Payload = Span;

    #[start]
    fn expr() -> Expr {
        (Num(n))                   => Expr::Num(n, _1_payload),
        (l @ expr, Plus, r @ expr) => Expr::Add(Box::new(l), Box::new(r), _payload),
    }
}
```

The payload type must implement `Clone`, and `Default` unless you provide a fallback with `payload_default = expr` in `parser_cfg`, or disable the fallback with `no_payload_default` (empty alternatives and `star` / `opt` / `sep_opt` then become compile errors). Payloads are only cloned when an action reads a `_k_payload` that the merge also needs; with a non-`Copy` payload, write `_payload.clone()` when storing `_payload` in your result.

`_k_payload` holds the payload of the k-th RHS symbol. `_payload` is the merged payload for the current production, which your action can also override before returning.

### Error recovery

```rust,ignore
fn stmt() -> Option<Stmt> {
    (s @ expr, Semi) => Some(s),
    (error, Semi)    => None,  // discard bad tokens up to the next semicolon
}
```

`error` is a reserved keyword in the grammar DSL. When the parser gets stuck, it walks the state stack looking for a recovery point and then discards input until it can continue.

Bind it to report what went wrong: `(e @ error, Semi) => Err(e)` gives the action the `ParseError` that started the recovery (the unexpected token and the expected terminals). This requires the token type to be `Clone`.

### Incremental API

```rust,ignore
let mut tokens = token_stream.into_iter();
let mut cp = start();

let result = loop {
    cp = match cp {
        Checkpoint::InputNeeded(state) => feed(state, tokens.next()),
        done => break done,
    }
};

match result {
    Checkpoint::Accepted(value) => println!("ok: {value:?}"),
    Checkpoint::Rejected(_, err) => eprintln!("error: {err:?}"),
    Checkpoint::InputNeeded(_) => unreachable!(),
}
```

Add `#![parser_cfg(clonable_state)]` if you need to clone the state for speculative parsing.

## Project layout

| Crate | Role |
|---|---|
| `ferrolith` | Public API: re-exports `grammar!` and the runtime types (`ParseError`, `AutomatonView`, ...) |
| `ferrolith_macro` | Proc-macro: parses the `grammar!` DSL, builds the LR(1) automaton, emits Rust code |
| `ferrolith_core` | Library: LR(1) construction (first sets, item sets, action and goto tables). Used by the macro only, not a runtime dependency |

## AI usage

This project was built with AI assistance throughout. Claude was used as a tool during the design phase to think through grammar representations, LR(1) construction trade-offs, and the macro DSL syntax. It helped generate large parts of the documentation and doctest examples, contributed to writing the test suite (basic parse tests, error recovery, payloads, parametric rules), and produced the `twig.rs` example. The code was written and reviewed by me, with AI output adjusted before being committed.

## License

LGPL-3.0-only. See `LICENSE` for the full text.
