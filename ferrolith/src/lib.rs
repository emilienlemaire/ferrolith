//! # Ferrolith
//!
//! Ferrolith is an LR(1) parser generator for Rust. You describe your grammar
//! inside a [`grammar!`] macro and Ferrolith compiles it at build time into a
//! fast, fully typed Rust parser — no generated files, no build scripts.
//!
//! ---
//!
//! ## Quick start
//!
//! Add the crate to your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! ferrolith = "0.1"
//! ```
//!
//! Then write your grammar. The following example parses integer arithmetic
//! expressions like `1 + 2 * 3`:
//!
//! ```rust
//! use ferrolith::grammar;
//!
//! // Your token type — any enum will do.
//! #[derive(Debug)]
//! enum Token {
//!     Num(i64),
//!     Plus,
//!     Star,
//! }
//!
//! grammar! {
//!     type Token = Token;
//!
//!     #[left(Plus)]
//!     #[left(Star)]
//!
//!     #[start]
//!     fn expr() -> i64 {
//!         (Num(n))                   => n,
//!         (l @ expr, Plus, r @ expr) => l + r,
//!         (l @ expr, Star, r @ expr) => l * r,
//!     }
//! }
//!
//! # fn main() {
//!     let result = parse(
//!         [Token::Num(2), Token::Plus, Token::Num(3), Token::Star, Token::Num(4)].into_iter()
//!     );
//!     // Star binds tighter than Plus, so this is 2 + (3 * 4) = 14.
//!     assert_eq!(result.unwrap(), 14);
//! # }
//! ```
//!
//! The `grammar!` block generates a `parse` function in the current scope. All
//! internal generated symbols are prefixed with `__` to avoid name collisions.
//!
//! ---
//!
//! ## Token type
//!
//! Declare the token type with:
//!
//! ```rust,ignore
//! type Token = YourEnum;
//! ```
//!
//! This must appear exactly once in the `grammar!` block. Ferrolith never
//! requires your token type to implement any trait — it only needs to appear
//! as `impl Iterator<Item = YourEnum>` when you call `parse`.
//!
//! ### Referring to tokens in rules
//!
//! Inside rule alternatives, token variants are written in **PascalCase** (matching
//! your enum). There are two forms:
//!
//! - **Unit variant**: `Plus` matches `Token::Plus` and discards the token (no binding).
//! - **Capture**: `Num(n)` matches `Token::Num(n)` and binds the inner value to `n`.
//!   Multiple fields: `Pair(a, b)` matches `Token::Pair(a, b)`.
//!
//! ---
//!
//! ## Grammar rules
//!
//! Each rule is a Rust-like function with one or more alternatives:
//!
//! ```rust,ignore
//! fn rule_name() -> ReturnType {
//!     (symbol, symbol, ...) => action_expression,
//!     (symbol, symbol, ...) => action_expression,
//! }
//! ```
//!
//! - **`rule_name`**: a lowercase identifier; this is the nonterminal's name.
//! - **`-> ReturnType`**: optional on non-start rules, required on the `#[start]` rule.
//! - **Symbols**: a comma-separated sequence inside parentheses. An empty sequence
//!   `()` matches the empty string (ε).
//! - **Action**: any valid Rust expression or block, evaluated when the alternative
//!   is reduced. Its value becomes the rule's return value.
//!
//! Exactly one rule must be annotated with `#[start]`. That rule's return type is
//! what `parse` returns.
//!
//! ### Referring to rules (nonterminals)
//!
//! A **lowercase** identifier that matches a rule name is treated as a nonterminal
//! reference. Write it bare to match the rule without capturing its return value,
//! or use a binding to access the value in the action (see [Bindings](#bindings)).
//!
//! ---
//!
//! ## Bindings
//!
//! ### Named bindings
//!
//! Use `name @ symbol` to bind the value produced by `symbol` to `name`:
//!
//! ```rust,ignore
//! fn expr() -> i64 {
//!     (Num(n))                   => n,
//!     (l @ expr, Plus, r @ expr) => l + r,
//! }
//! ```
//!
//! `l @ expr` runs the `expr` rule and binds its `i64` result to `l`.
//! `Num(n)` destructures the token and binds the inner `i64` to `n`.
//!
//! You can also write `mut name @ symbol` to get a mutable binding:
//!
//! ```rust,ignore
//! (mut xs @ star(item), v @ item) => { xs.push(v); xs }
//! ```
//!
//! ### Positional bindings
//!
//! Every symbol at position *k* (1-indexed from left) is also accessible as `_k`,
//! whether or not it has a named binding:
//!
//! ```rust,ignore
//! fn expr() -> i64 {
//!     (expr, Plus, expr) => _1 + _3, // _2 would be the Plus token itself
//! }
//! ```
//!
//! Named and positional bindings can be mixed freely:
//!
//! ```rust,ignore
//! (l @ expr, Plus, expr) => l + _3,
//! ```
//!
//! Positional bindings of non-Copy types follow move semantics. If you use both
//! `l @ expr` and `_1` for the same symbol, `l` consumes `_1` (which remains
//! available only if the type is `Copy`).
//!
//! ---
//!
//! ## Built-in combinators
//!
//! Five combinators are available in every grammar without any definition:
//!
//! | Combinator | Matches | Return type |
//! |---|---|---|
//! | `star(x)` | zero or more repetitions of `x` | `Vec<ReturnType of x>` |
//! | `plus(x)` | one or more repetitions of `x` | `Vec<ReturnType of x>` |
//! | `opt(x)` | zero or one occurrence of `x` | `Option<ReturnType of x>` |
//! | `sep(sep, x)` | one or more `x` separated by token `sep` | `Vec<ReturnType of x>` |
//! | `sep_opt(sep, x)` | zero or more `x` separated by token `sep` | `Vec<ReturnType of x>` |
//!
//! `x` must be a rule name (nonterminal). `sep` must be a token variant (PascalCase).
//!
//! ```rust
//! use ferrolith::grammar;
//!
//! enum Token { Num(i64), Comma }
//!
//! grammar! {
//!     type Token = Token;
//!
//!     #[start]
//!     fn program() -> Vec<i64> {
//!         (xs @ sep(Comma, number)) => xs,
//!     }
//!
//!     fn number() -> i64 {
//!         (Num(n)) => n,
//!     }
//! }
//!
//! # fn main() {
//!     assert_eq!(
//!         parse([Token::Num(1), Token::Comma, Token::Num(2), Token::Comma, Token::Num(3)].into_iter()).unwrap(),
//!         vec![1, 2, 3],
//!     );
//! # }
//! ```
//!
//! You can override a built-in by defining a rule with the same name in your grammar.
//!
//! ---
//!
//! ## Precedence and associativity
//!
//! Declare operator precedence with `#[left]`, `#[right]`, or `#[nonassoc]`
//! attributes **before any rule**, listing tokens from **lowest to highest** precedence:
//!
//! ```rust,ignore
//! #[left(Plus, Minus)]   // lowest precedence
//! #[left(Star, Slash)]   // higher
//! #[right(Caret)]        // highest, right-associative
//! ```
//!
//! When an LR conflict arises between shifting a token and reducing a production,
//! Ferrolith resolves it using these declarations:
//!
//! - Higher token precedence: shift (the operator binds tighter).
//! - Higher production precedence: reduce.
//! - Equal precedence, left: reduce (left-associative).
//! - Equal precedence, right: shift (right-associative).
//! - Equal precedence, nonassoc: parse error at runtime.
//!
//! A production's precedence is that of its **rightmost terminal** by default.
//! Override it per-alternative with `#[prec(Token)]`:
//!
//! ```rust,ignore
//! fn expr() -> Expr {
//!     (Num(n))                   => Expr::Num(n),
//!     (l @ expr, Plus, r @ expr) => Expr::Add(Box::new(l), Box::new(r)),
//!     // Unary minus has no rightmost terminal, so we declare its precedence explicitly.
//!     #[prec(UMinus)]
//!     (Minus, e @ expr)          => Expr::Neg(Box::new(e)),
//! }
//! ```
//!
//! Unresolved shift/reduce conflicts (no precedence info on either side) produce a
//! **compile-time warning** and default to shift. Reduce/reduce conflicts are always
//! a **compile-time error**.
//!
//! ---
//!
//! ## Parametric rules
//!
//! Rules can take parameters, enabling reusable higher-order combinators beyond the
//! built-ins. A parameter can stand in for either a nonterminal or a terminal:
//!
//! ```rust
//! use ferrolith::grammar;
//!
//! enum Token { Int(i64), Bool(bool), Comma, Semi }
//!
//! grammar! {
//!     type Token = Token;
//!
//!     // One or more `x` separated by token `sep`.
//!     fn sep_list(sep, x) -> Vec<x> {
//!         (v @ x)                                  => vec![v],
//!         (mut xs @ sep_list(sep, x), sep, v @ x) => { xs.push(v); xs },
//!     }
//!
//!     #[start]
//!     fn program() -> (Vec<i64>, Vec<bool>) {
//!         (ints @ sep_list(Comma, int), Semi, bools @ sep_list(Semi, flag))
//!             => (ints, bools),
//!     }
//!
//!     fn int()  -> i64  { (Int(n))  => n, }
//!     fn flag() -> bool { (Bool(b)) => b, }
//! }
//! # fn main() {}
//! ```
//!
//! Each distinct instantiation (e.g. `sep_list(Comma, int)` and `sep_list(Semi, flag)`)
//! becomes its own nonterminal in the compiled grammar. Parametric rules themselves
//! are never compiled directly — only their concrete instantiations are.
//!
//! The return type `Vec<x>` uses the parameter name as a type placeholder; Ferrolith
//! substitutes it with the actual return type of the argument rule.
//!
//! ---
//!
//! ## Grammar validation
//!
//! At compile time, Ferrolith checks your grammar for three classes of error:
//!
//! - **Undefined rule reference**: a rule body refers to a name that is not defined
//!   anywhere in the grammar (neither a user rule nor a built-in combinator).
//! - **Nonproductive rule**: a rule can never produce a finite terminal string
//!   (e.g. a rule whose only alternative refers back to itself with no base case).
//! - **Unreachable rule**: a concrete rule is never reachable from the start rule.
//!
//! All errors for a single macro expansion are collected and reported together so you
//! can fix them all in one pass.
//!
//! ---
//!
//! ## Error recovery
//!
//! Ferrolith supports panic-mode error recovery using the `error` pseudo-token,
//! following the yacc/bison model. Add an `error` alternative to any rule to
//! declare that rule a recovery point:
//!
//! ```rust,ignore
//! fn stmt() -> Option<Stmt> {
//!     (s @ expr, Semi) => Some(s),
//!     // If parsing `expr` fails, discard tokens until `Semi`, then continue.
//!     (error, Semi)    => None,
//! }
//! ```
//!
//! `error` is a reserved keyword in the grammar DSL; it cannot be used as a
//! binding name or rule name. It takes no binding and carries no value.
//!
//! ### Recovery algorithm
//!
//! When the parser encounters an unexpected token it cannot shift or reduce on,
//! it runs the following steps:
//!
//! 1. **Default reductions**: if the current state can only reduce (no shifts or
//!    accepts are possible), the parser forces that reduction and retries, repeating
//!    until the state machine reaches a point where no forced reduction applies.
//! 2. **Find a recovery state**: walk the state stack from top to bottom looking
//!    for a state that can shift the `error` token. If none is found, the parse
//!    fails immediately with a [`ParseError`].
//! 3. **Truncate**: pop the state and value stacks back to the recovery state and
//!    shift `error` into it.
//! 4. **Discard tokens**: consume tokens from the input until one is found that
//!    the parser can act on in the post-`error` state (the sync token, e.g. `Semi`
//!    above). If EOF is reached with no valid sync token, the parse fails.
//! 5. **Continue**: resume normal parsing from the post-`error` state.
//!
//! The `error` pseudo-token is filtered out of the `expected` field of
//! [`ParseError`]; only real user-declared tokens appear there.
//!
//! ---
//!
//! ## Error handling
//!
//! `parse` returns `Result<T, ParseError<Token>>`. [`ParseError`] has two variants:
//!
//! - [`ParseError::UnexpectedToken`]: the parser received a token it did not expect.
//! - [`ParseError::UnexpectedEof`]: the token stream ended before the grammar was satisfied.
//!
//! Both variants carry an `expected` field listing the names of the terminals that
//! *would* have been valid at that point, which is useful for error messages:
//!
//! ```rust,ignore
//! match parse(tokens) {
//!     Ok(value) => println!("parsed: {:?}", value),
//!     Err(ferrolith::ParseError::UnexpectedToken { token, expected }) => {
//!         eprintln!("unexpected {:?}, expected one of: {}", token, expected.join(", "));
//!     }
//!     Err(ferrolith::ParseError::UnexpectedEof { expected }) => {
//!         eprintln!("unexpected end of input, expected one of: {}", expected.join(", "));
//!     }
//! }
//! ```
//!
//! `ParseError` implements `Display` (when `T: Display`) and `Error`.
//!
//! ---
//!
//! ## Incremental API
//!
//! The generated `parse` function is a thin wrapper around a lower-level incremental
//! API that lets you feed tokens one at a time and inspect the parser state between
//! steps. This is useful for interactive tools, speculative parsing, or error
//! recovery scaffolding.
//!
//! ```rust,ignore
//! // Start a fresh parse.
//! let mut cp = start();
//!
//! for token in token_stream {
//!     cp = match cp {
//!         Checkpoint::InputNeeded(state) => feed(state, Some(token)),
//!         other => break,
//!     };
//! }
//!
//! // Signal end-of-input.
//! let result = match cp {
//!     Checkpoint::InputNeeded(state) => feed(state, None),
//!     other => other,
//! };
//!
//! match result {
//!     Checkpoint::Accepted(value)  => println!("ok: {:?}", value),
//!     Checkpoint::Rejected(_, err) => eprintln!("error: {}", err),
//!     Checkpoint::InputNeeded(_)   => eprintln!("unexpected end of input"),
//! }
//! ```
//!
//! ### `ParserState`
//!
//! The state carried by `Checkpoint::InputNeeded` exposes two methods:
//!
//! - `state.current_state() -> usize`: the numeric LR state the parser is in.
//! - `state.expected_terminals() -> Vec<&'static str>`: terminal names that would
//!   advance the parse without error, equivalent to the `expected` field on [`ParseError`].
//!
//! ### Speculative parsing (opt-in Clone)
//!
//! Add `#![parser_cfg(clonable_state)]` to make `ParserState` implement `Clone`.
//! You can then fork the parser before feeding a token to explore multiple paths:
//!
//! ```rust,ignore
//! grammar! {
//!     #![parser_cfg(clonable_state)]
//!     // ...
//! }
//!
//! let fork = state.clone();
//! let path_a = feed(state, Some(token_a));
//! let path_b = feed(fork,  Some(token_b));
//! ```
//!
//! Requires that all types returned by grammar rules implement `Clone`.
//!
//! ---
//!
//! ## Automaton introspection (opt-in)
//!
//! Add `#![parser_cfg(expose_automaton)]` to expose the compiled LR automaton:
//!
//! ```rust,ignore
//! grammar! {
//!     #![parser_cfg(expose_automaton)]
//!     // ...
//! }
//!
//! // A static view of the whole automaton:
//! let view: &ferrolith::AutomatonView = &AUTOMATON;
//! println!("{} states", view.n_states);
//!
//! // Direct table lookups, useful for tooling or visualisation:
//! let act = action(state_id, terminal_id);
//! let dst = goto(state_id, nonterminal_id);
//! ```
//!
//! See [`AutomatonView`] for the full API.

pub use ferrolith_macro::grammar;

pub mod action {
    //! The [`Action`] type used in the generated action table.
    pub use ferrolith_core::table::Action;
}

/// A symbol in the compiled grammar: either a terminal (token variant) or a
/// nonterminal (rule). Indices correspond to positions in
/// [`AutomatonView::terminals`] and [`AutomatonView::nonterminals`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Symbol {
    /// A terminal (token variant). The index is the terminal's ID in the
    /// compiled grammar; `0` is always the implicit EOF terminal.
    Terminal(usize),
    /// A nonterminal (grammar rule). The index is the nonterminal's ID;
    /// `0` is the augmented start symbol added by the LR construction.
    Nonterminal(usize),
}

impl Symbol {
    /// Returns the human-readable name of this symbol using the string tables
    /// in `view`.
    pub fn name(self, view: &AutomatonView) -> &'static str {
        match self {
            Symbol::Terminal(i) => view.terminals[i],
            Symbol::Nonterminal(i) => view.nonterminals[i],
        }
    }

    /// Returns `true` if this symbol is a terminal.
    pub fn is_terminal(self) -> bool {
        matches!(self, Symbol::Terminal(_))
    }

    /// Returns `true` if this symbol is a nonterminal.
    pub fn is_nonterminal(self) -> bool {
        matches!(self, Symbol::Nonterminal(_))
    }
}

/// A production in the compiled grammar, as seen through [`AutomatonView`].
pub struct ProductionInfo {
    /// The left-hand side nonterminal ID.
    pub lhs: usize,
    /// The right-hand side sequence of symbols.
    pub rhs: &'static [Symbol],
}

/// Per-state metadata available through [`AutomatonView`].
pub struct StateInfo {
    /// The symbol whose shift (or goto) transition created this state, if any.
    /// State 0 (the initial state) has `None`.
    pub incoming: Option<Symbol>,
}

/// A read-only view of the compiled LR(1) automaton, exposed when
/// `#![parser_cfg(expose_automaton)]` is set.
///
/// Obtain the view through the `AUTOMATON` static generated in the grammar
/// scope. Use the generated `action` and `goto` functions for direct table
/// lookups.
pub struct AutomatonView {
    /// Total number of LR states.
    pub n_states: usize,
    /// Terminal names indexed by terminal ID. Index `0` is always `"<eof>"`.
    pub terminals: &'static [&'static str],
    /// Nonterminal names indexed by nonterminal ID.
    /// Index `0` is the augmented start symbol `"__start__"`.
    pub nonterminals: &'static [&'static str],
    /// All productions in the grammar, including synthetic ones generated by
    /// the built-in combinators.
    pub productions: &'static [ProductionInfo],
    /// Per-state metadata, one entry per LR state.
    pub states: &'static [StateInfo],
}

/// The error returned by `parse` and by the incremental `feed` function when
/// the input does not conform to the grammar.
///
/// Both variants carry an `expected` field listing the names of the terminals
/// that would have been valid at the point of failure. Use it to produce
/// helpful error messages pointing the user at what the parser needed next.
///
/// `ParseError<T>` implements [`std::fmt::Display`] when `T: Display`, and
/// [`std::error::Error`] under the same bound.
pub enum ParseError<T> {
    /// The parser received a token it did not expect in the current state.
    UnexpectedToken {
        /// The offending token.
        token: T,
        /// Terminal names that would have been valid here.
        expected: Vec<&'static str>,
    },
    /// The token stream ended before the grammar was fully satisfied.
    UnexpectedEof {
        /// Terminal names that would have been valid here.
        expected: Vec<&'static str>,
    },
}

impl<T> std::fmt::Debug for ParseError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnexpectedToken { expected, .. } => f
                .debug_struct("UnexpectedToken")
                .field("expected", expected)
                .finish_non_exhaustive(),
            ParseError::UnexpectedEof { expected } => f
                .debug_struct("UnexpectedEof")
                .field("expected", expected)
                .finish(),
        }
    }
}

impl<T: std::fmt::Display> std::fmt::Display for ParseError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnexpectedToken { token, expected } => {
                write!(f, "unexpected token {token}")?;
                if !expected.is_empty() {
                    write!(f, ", expected one of: {}", expected.join(", "))?;
                }
                Ok(())
            }
            ParseError::UnexpectedEof { expected } => {
                write!(f, "unexpected end of input")?;
                if !expected.is_empty() {
                    write!(f, ", expected one of: {}", expected.join(", "))?;
                }
                Ok(())
            }
        }
    }
}

impl<T: std::fmt::Display> std::error::Error for ParseError<T> {}
