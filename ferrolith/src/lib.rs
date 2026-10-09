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
//! The right-hand side can be any enum type, including a module path
//! (`lexer::Tok`) or a generic instantiation (`Tok<i64>`):
//!
//! ```rust
//! # use ferrolith::grammar;
//! enum Tok<T> { Num(T), Plus }
//!
//! grammar! {
//!     type Token = Tok<i64>;
//! #   #[left(Plus)]
//! #   #[start]
//! #   fn expr() -> i64 {
//! #       (Num(n)) => n,
//! #       (l @ expr, Plus, r @ expr) => l + r,
//! #   }
//!     // ...
//! }
//! # fn main() {
//! # assert_eq!(parse([Tok::Num(1), Tok::Plus, Tok::Num(2)].into_iter()).unwrap(), 3);
//! # }
//! ```
//!
//! Lifetime parameters other than `'static` are not supported: `type Token = Tok<'a>;`
//! fails with "use of undeclared lifetime name". Use owned data in your tokens (e.g.
//! `String` instead of `&'a str`), or `Tok<'static>` if your input is `'static`.
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
//! The grammar does not have to use every variant, so one lexer can feed several
//! grammars. A variant that no rule mentions is never valid: the parser reports it as
//! [`ParseError::UnexpectedToken`], or discards it during [error recovery](#error-recovery).
//!
//! ---
//!
//! ## Token payload
//!
//! A *payload* is user-defined data attached to each token and propagated through
//! the parse. The canonical use case is source locations: attach a byte span to
//! every token so action expressions can build an AST that carries span information.
//!
//! ### Declaring a payload type
//!
//! Add a `type Payload = T` declaration alongside `type Token`:
//!
//! ```rust,ignore
//! grammar! {
//!     type Token  = MyToken;
//!     type Payload = Span;   // any type you choose
//!     // ...
//! }
//! ```
//!
//! The payload type must implement [`Clone`]. It must also implement [`Default`]
//! unless you set `payload_default` or `no_payload_default` in `#![parser_cfg(...)]`
//! (see [Default payload for epsilon rules](#default-payload-for-epsilon-rules)): the
//! generated parser uses it for epsilon alternatives, which have no symbols to take
//! a payload from, and for an `error` token shifted at end of input (see
//! [Error recovery](#error-recovery)).
//!
//! Payloads are moved, not cloned, wherever possible. A `_k_payload` is cloned
//! only when your action reads it *and* the merge strategy also consumes it, so
//! actions that don't look at payloads never clone. For a [`Copy`] type such as a
//! byte span, a clone is just a copy; for an expensive payload, consider wrapping
//! it in [`Rc`](std::rc::Rc).
//!
//! **Note:** reading `_payload` itself in an action moves it, and the parser still
//! needs it afterwards to pass it up to the parent rule. With a non-`Copy`
//! payload, write `_payload.clone()` when storing it in your result.
//!
//! When a payload type is declared, `parse` and the incremental `feed` function
//! expect `(Token, Payload)` pairs instead of bare tokens:
//!
//! ```rust
//! # use ferrolith::grammar;
//! # #[derive(Debug, Clone, Copy, Default)]
//! # struct Span { start: usize, end: usize }
//! # fn merge_span(a: Span, b: Span) -> Span { Span { start: a.start, end: b.end } }
//! # enum MyToken { Num(i64) }
//! # struct Lexed { kind: MyToken, span: Span }
//! # mod plain {
//! #   use super::*;
//! #   grammar! {
//! #       type Token = MyToken;
//! #       #[start]
//! #       fn expr() -> i64 { (Num(n)) => n, }
//! #   }
//! #   pub fn run(tokens: Vec<Lexed>) -> Result<i64, ferrolith::ParseError<MyToken>> {
//! #   let tokens = tokens.into_iter();
//! // Without payload:
//! parse(tokens.map(|t| t.kind))
//! #   }
//! # }
//!
//! # mod with_payload {
//! #   use super::*;
//! #   grammar! {
//! #       #![parser_cfg(payload_merge = merge_span)]
//! #       type Token = MyToken;
//! #       type Payload = Span;
//! #       #[start]
//! #       fn expr() -> i64 { (Num(n)) => n, }
//! #   }
//! #   pub fn run(tokens: Vec<Lexed>) -> Result<i64, ferrolith::ParseError<(MyToken, Span)>> {
//! #   let tokens = tokens.into_iter();
//! // With payload:
//! parse(tokens.map(|t| (t.kind, t.span)))
//! #   }
//! # }
//! # fn main() {}
//! ```
//!
//! ### Accessing payloads in actions
//!
//! Inside an action expression, every symbol at position *k* (1-indexed from the
//! left) exposes a `_k_payload` binding containing that symbol's payload:
//!
//! ```rust
//! # use ferrolith::grammar;
//! # #[derive(Debug, Clone, Copy, Default)]
//! # struct Span { start: usize, end: usize }
//! # impl Span {
//! #   fn merge(a: Span, b: Span) -> Span { Span { start: a.start, end: b.end } }
//! # }
//! # enum MyToken { Num(i64), Plus }
//! # enum Expr { Num(i64, Span), Add(Box<Expr>, Box<Expr>, Span) }
//! # grammar! {
//! #   #![parser_cfg(payload_merge = Span::merge)]
//! #   type Token = MyToken;
//! #   type Payload = Span;
//! #   #[left(Plus)]
//! #   #[start]
//! fn expr() -> Expr {
//!     (Num(n)) => Expr::Num(n, _1_payload),
//!     (l @ expr, Plus, r @ expr) => {
//!         Expr::Add(Box::new(l), Box::new(r), Span::merge(_1_payload, _3_payload))
//!     }
//! }
//! # }
//! # fn main() {}
//! ```
//!
//! Nonterminal symbols (like `l @ expr`) carry the payload that was propagated
//! when that sub-expression was reduced.
//!
//! ### Merge strategies
//!
//! When a rule reduces, Ferrolith computes the payload to attach to the resulting
//! nonterminal by *merging* the payloads of its RHS symbols. Choose a strategy
//! with `payload_merge_strategy` in `#![parser_cfg(...)]`:
//!
//! | Strategy | What it produces |
//! |---|---|
//! | `first_last` (default) | Calls `payload_merge(first, last)` — requires `payload_merge` |
//! | `fold` | Right-folds all positions via `payload_merge` — requires `payload_merge` |
//! | `first` | The payload of the first RHS symbol |
//! | `last` | The payload of the last RHS symbol |
//!
//! Provide the merge function via `payload_merge = path`:
//!
//! ```rust
//! # use ferrolith::grammar;
//! # #[derive(Debug, Clone, Copy, Default)]
//! # struct Span { start: usize, end: usize }
//! # enum MyToken { Num(i64), Plus }
//! fn merge_span(a: Span, b: Span) -> Span {
//!     Span { start: a.start, end: b.end }
//! }
//!
//! grammar! {
//!     #![parser_cfg(payload_merge = merge_span, payload_merge_strategy = first_last)]
//!
//!     type Token   = MyToken;
//!     type Payload = Span;
//!     // ...
//! #   #[left(Plus)]
//! #   #[start]
//! #   fn expr() -> i64 {
//! #       (Num(n)) => n,
//! #       (l @ expr, Plus, r @ expr) => l + r,
//! #   }
//! }
//! # fn main() {}
//! ```
//!
//! Rules with a single RHS symbol always propagate that symbol's payload directly,
//! regardless of strategy.
//!
//! ### Overriding the payload in an action
//!
//! Assign to the reserved variable `_payload` inside an action to override whatever
//! the merge strategy would have produced. The parent rule will then see this value
//! in its `_k_payload` binding for this nonterminal:
//!
//! ```rust
//! # use ferrolith::grammar;
//! # #[derive(Debug, Clone, Copy, Default)]
//! # struct Span { start: usize, end: usize }
//! # impl Span {
//! #   fn merge(a: Span, b: Span) -> Span { Span { start: a.start, end: b.end } }
//! # }
//! # enum MyToken { Num(i64), Plus }
//! # enum Expr { Num(i64), Add(Box<Expr>, Box<Expr>) }
//! # grammar! {
//! #   #![parser_cfg(payload_merge = Span::merge)]
//! #   type Token = MyToken;
//! #   type Payload = Span;
//! #   #[left(Plus)]
//! #   #[start]
//! fn expr() -> Expr {
//! #   (Num(n)) => Expr::Num(n),
//!     (l @ expr, Plus, r @ expr) => {
//!         _payload = Span::merge(_1_payload, _3_payload);
//!         Expr::Add(Box::new(l), Box::new(r))
//!     }
//! }
//! # }
//! # fn main() {}
//! ```
//!
//! ### Default payload for epsilon rules
//!
//! Epsilon alternatives (empty symbol list) cannot derive a payload from RHS symbols.
//! They use `payload_default` from `parser_cfg` if set, otherwise `Payload::default()`.
//! The same fallback gives the `error` pseudo-token its payload when recovery starts
//! at end of input.
//!
//! ```rust
//! # use ferrolith::grammar;
//! # #[derive(Debug, Clone, Copy, Default)]
//! # struct Span { start: usize, end: usize }
//! # impl Span { const DUMMY: Span = Span { start: 0, end: 0 }; }
//! # fn merge_span(a: Span, b: Span) -> Span { Span { start: a.start, end: b.end } }
//! # enum MyToken { Num(i64) }
//! # enum Expr { Num(i64) }
//! grammar! {
//!     #![parser_cfg(payload_merge = merge_span, payload_merge_strategy = first_last,
//!                   payload_default = Span::DUMMY)]
//!
//!     type Token   = MyToken;
//!     type Payload = Span;
//!
//!     #[start]
//!     fn items() -> Vec<Expr> {
//!         ()                              => vec![],   // _payload = Span::DUMMY
//!         (mut xs @ items, e @ expr) => { xs.push(e); xs }
//!     }
//! #
//! #   fn expr() -> Expr { (Num(n)) => Expr::Num(n), }
//! }
//! # fn main() {}
//! ```
//!
//! #### Disabling the default payload
//!
//! If your payload has no meaningful default (a span must point somewhere), set
//! `no_payload_default` instead. The payload type then does not need [`Default`],
//! and the grammar must not need a fallback:
//!
//! - an empty alternative, or a use of a built-in that can match nothing (`star`,
//!   `opt`, `sep_opt`), is a compile-time error;
//! - [error recovery](#error-recovery) that starts at end of input gives up and
//!   returns [`ParseError::UnexpectedEof`].
//!
//! `no_payload_default` cannot be combined with `payload_default = ...`.
//!
//! ```rust
//! # use ferrolith::grammar;
//! #[derive(Debug, Clone, Copy)]
//! pub struct Span { start: usize, end: usize }   // no `Default`
//! # fn merge_span(a: Span, b: Span) -> Span { Span { start: a.start, end: b.end } }
//! # enum MyToken { Num(i64), Comma }
//!
//! grammar! {
//!     #![parser_cfg(payload_merge = merge_span, no_payload_default)]
//!
//!     type Token   = MyToken;
//!     type Payload = Span;
//!
//!     #[start]
//!     fn items() -> Vec<i64> {
//!         (xs @ sep(Comma, num)) => xs,   // `sep_opt` would be rejected
//!     }
//!
//!     fn num() -> i64 { (Num(n)) => n, }
//! }
//! # fn main() {}
//! ```
//!
//! ---
//!
//! ## Grammar rules
//!
//! Each rule is a Rust-like function with one or more alternatives:
//!
//! ```text
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
//! ### Visibility of the generated API
//!
//! The visibility written on the `#[start]` rule is given to everything the macro
//! generates for you: `parse`, `start`, `feed`, `ParserState`, `Checkpoint`, and,
//! with [`expose_automaton`](#automaton-introspection-opt-in), `AUTOMATON`, `action`
//! and `goto`. Without a visibility they are private to the enclosing module, like
//! any `fn`. Use `pub` or `pub(crate)` to call the parser from another module:
//!
//! ```rust
//! mod parser {
//!     use ferrolith::grammar;
//!
//!     pub enum Token { Num(i64), Plus }
//!
//!     grammar! {
//!         type Token = Token;
//!
//!         #[left(Plus)]
//!
//!         #[start]
//!         pub fn expr() -> i64 {
//!             (Num(n))                   => n,
//!             (l @ expr, Plus, r @ expr) => l + r,
//!         }
//!     }
//! }
//!
//! # fn main() {
//! use parser::Token;
//! let sum = parser::parse([Token::Num(1), Token::Plus, Token::Num(2)].into_iter());
//! assert_eq!(sum.unwrap(), 3);
//! # }
//! ```
//!
//! Keep it consistent with the start rule's return type: a `pub` parser returning a
//! private type triggers rustc's `private_interfaces` warning. Writing a visibility
//! on any other rule is a compile-time error.
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
//! ```rust
//! # use ferrolith::grammar;
//! # enum Token { Num(i64), Plus }
//! # grammar! {
//! #   type Token = Token;
//! #   #[left(Plus)]
//! #   #[start]
//! fn expr() -> i64 {
//!     (Num(n))                   => n,
//!     (l @ expr, Plus, r @ expr) => l + r,
//! }
//! # }
//! # fn main() {}
//! ```
//!
//! `l @ expr` runs the `expr` rule and binds its `i64` result to `l`.
//! `Num(n)` destructures the token and binds the inner `i64` to `n`.
//!
//! You can also write `mut name @ symbol` to get a mutable binding:
//!
//! ```rust
//! # use ferrolith::grammar;
//! # enum Token { Num(i64) }
//! # grammar! {
//! #   type Token = Token;
//! #   #[start]
//! #   fn items() -> Vec<i64> {
//! (mut xs @ star(item), v @ item) => { xs.push(v); xs }
//! #   }
//! #   fn item() -> i64 { (Num(n)) => n, }
//! # }
//! # fn main() {}
//! ```
//!
//! ### Positional bindings
//!
//! Every symbol at position *k* (1-indexed from left) is also accessible as `_k`,
//! whether or not it has a named binding:
//!
//! ```rust
//! # use ferrolith::grammar;
//! # enum Token { Num(i64), Plus }
//! # grammar! {
//! #   type Token = Token;
//! #   #[left(Plus)]
//! #   #[start]
//! fn expr() -> i64 {
//! #   (Num(n)) => n,
//!     (expr, Plus, expr) => _1 + _3, // _2 would be the Plus token itself
//! }
//! # }
//! # fn main() {}
//! ```
//!
//! Named and positional bindings can be mixed freely:
//!
//! ```rust
//! # use ferrolith::grammar;
//! # enum Token { Num(i64), Plus }
//! # grammar! {
//! #   type Token = Token;
//! #   #[left(Plus)]
//! #   #[start]
//! #   fn expr() -> i64 {
//! #       (Num(n)) => n,
//! (l @ expr, Plus, expr) => l + _3,
//! #   }
//! # }
//! # fn main() {}
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
//! ```rust
//! # use ferrolith::grammar;
//! # enum Token { Num(i64), Plus, Minus, Star, Slash, Caret }
//! # grammar! {
//! #   type Token = Token;
//! #[left(Plus, Minus)]   // lowest precedence
//! #[left(Star, Slash)]   // higher
//! #[right(Caret)]        // highest, right-associative
//! #   #[start]
//! #   fn expr() -> i64 {
//! #       (Num(n)) => n,
//! #       (l @ expr, Plus, r @ expr) => l + r,
//! #       (l @ expr, Minus, r @ expr) => l - r,
//! #       (l @ expr, Star, r @ expr) => l * r,
//! #       (l @ expr, Slash, r @ expr) => l / r,
//! #       (l @ expr, Caret, r @ expr) => l.pow(r as u32),
//! #   }
//! # }
//! # fn main() {}
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
//! Override it per-alternative with `#[prec(Name)]`, where `Name` is any name listed
//! in a precedence declaration:
//!
//! ```rust
//! # use ferrolith::grammar;
//! # enum Token { Num(i64), Minus, Star }
//! grammar! {
//!     type Token = Token;
//!
//!     #[left(Minus)]
//!     #[left(Star)]
//!     #[right(UMinus)]   // pseudo-token: not a variant of `Token`
//!
//!     #[start]
//!     fn expr() -> i64 {
//!         (Num(n))                    => n,
//!         (l @ expr, Minus, r @ expr) => l - r,
//!         (l @ expr, Star, r @ expr)  => l * r,
//!         // `Minus` is the rightmost terminal, which would make `-a * b` parse as
//!         // `-(a * b)`. Unary minus must bind tighter than `Star`, so override it.
//!         #[prec(UMinus)]
//!         (Minus, e @ expr)           => -e,
//!     }
//! }
//! # fn main() {
//! # let tokens = [Token::Minus, Token::Num(2), Token::Star, Token::Num(3)];
//! # assert_eq!(parse(tokens.into_iter()).unwrap(), -6);
//! # }
//! ```
//!
//! A name in a precedence declaration does not have to be a token. Such a
//! *pseudo-token* (`UMinus` above, like bison's `%prec`) only names a precedence
//! level for `#[prec(...)]`. Its associativity is ignored: when a production and the
//! lookahead token have the same level, the lookahead token's associativity
//! decides. Declaring a name that is neither a token used in a rule nor referenced
//! by a `#[prec(...)]` is a compile-time error, which catches misspelled tokens.
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
//! ```rust
//! # use ferrolith::grammar;
//! # enum Token { Num(i64), Semi }
//! # type Stmt = i64;
//! # grammar! {
//! #   type Token = Token;
//! #   #[start]
//! #   fn program() -> Vec<Option<Stmt>> { (xs @ star(stmt)) => xs, }
//! #   fn expr() -> Stmt { (Num(n)) => n, }
//! fn stmt() -> Option<Stmt> {
//!     (s @ expr, Semi) => Some(s),
//!     // If parsing `expr` fails, discard tokens until `Semi`, then continue.
//!     (error, Semi)    => None,
//! }
//! # }
//! # fn main() {}
//! ```
//!
//! `error` is a reserved keyword in the grammar DSL; it cannot be used as a
//! binding name or rule name. It takes no binding and carries no value.
//!
//! `error` can also end an alternative, as in `(error) => None`. The tokens that
//! may follow the rule then act as sync tokens: in a list such as
//! `(mut ss @ stmts, s @ stmt, Semi)`, recovery inside `stmt` discards input up to
//! the next `Semi`.
//!
//! With a [payload type](#token-payload), `error` does carry a payload: the payload
//! of the token that triggered recovery, or the [default payload](#default-payload-for-epsilon-rules)
//! when recovery starts at end of input. Read it as `_k_payload` like any other
//! symbol, for instance to report where the error happened.
//!
//! Error recovery is only performed by `parse`. The [incremental API](#incremental-api)
//! returns `Checkpoint::Rejected` at the first error.
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
//! 3. **Truncate**: pop the state, value, and payload stacks back to the recovery
//!    state and shift `error` into it.
//! 4. **Discard tokens**: consume tokens from the input until one is found that
//!    the parser can act on in the post-`error` state (the sync token, e.g. `Semi`
//!    above), taking into account any reductions the parser would perform first
//!    (such as reducing `stmt → error` before shifting `Semi`). If EOF is reached
//!    with no valid sync token, the parse fails.
//! 5. **Continue**: resume normal parsing from the post-`error` state.
//!
//! The `error` pseudo-token is filtered out of the `expected` field of
//! [`ParseError`]; only real user-declared tokens appear there.
//!
//! ---
//!
//! ## Error handling
//!
//! `parse` returns `Result<T, ParseError<Token>>`. When a [payload type](#token-payload)
//! is declared the error type becomes `ParseError<(Token, Payload)>`, so the
//! offending token and its payload are both available. [`ParseError`] has two
//! variants:
//!
//! - [`ParseError::UnexpectedToken`]: the parser received a token it did not expect.
//! - [`ParseError::UnexpectedEof`]: the token stream ended before the grammar was satisfied.
//!
//! Both variants carry an `expected` field listing the names of the terminals that
//! *would* have been valid at that point, which is useful for error messages:
//!
//! ```rust
//! # use ferrolith::grammar;
//! # #[derive(Debug)]
//! # enum Token { Num(i64), Plus }
//! # grammar! {
//! #   type Token = Token;
//! #   #[left(Plus)]
//! #   #[start]
//! #   fn expr() -> i64 {
//! #       (Num(n)) => n,
//! #       (l @ expr, Plus, r @ expr) => l + r,
//! #   }
//! # }
//! # fn main() {
//! # let tokens = vec![Token::Num(1), Token::Plus].into_iter();
//! // Without payload:
//! match parse(tokens) {
//!     Ok(value) => println!("parsed: {:?}", value),
//!     Err(ferrolith::ParseError::UnexpectedToken { token, expected }) => {
//!         eprintln!("unexpected {:?}, expected one of: {}", token, expected.join(", "));
//!     }
//!     Err(ferrolith::ParseError::UnexpectedEof { expected }) => {
//!         eprintln!("unexpected end of input, expected one of: {}", expected.join(", "));
//!     }
//! }
//! # }
//! ```
//!
//! ```rust
//! # use ferrolith::grammar;
//! # #[derive(Debug, Clone, Copy, Default)]
//! # struct Span { start: usize, end: usize }
//! # fn merge_span(a: Span, b: Span) -> Span { Span { start: a.start, end: b.end } }
//! # #[derive(Debug)]
//! # enum Token { Num(i64), Plus }
//! # grammar! {
//! #   #![parser_cfg(payload_merge = merge_span)]
//! #   type Token = Token;
//! #   type Payload = Span;
//! #   #[left(Plus)]
//! #   #[start]
//! #   fn expr() -> i64 {
//! #       (Num(n)) => n,
//! #       (l @ expr, Plus, r @ expr) => l + r,
//! #   }
//! # }
//! # fn main() {
//! # let tokens = vec![(Token::Plus, Span { start: 0, end: 1 })].into_iter();
//! // With payload:
//! match parse(tokens) {
//!     Err(ferrolith::ParseError::UnexpectedToken { token: (tok, span), expected }) => {
//!         eprintln!("at {:?}: unexpected {:?}, expected one of: {}", span, tok, expected.join(", "));
//!     }
//!     // ...
//! #   _ => {}
//! }
//! # }
//! ```
//!
//! `ParseError` implements `Display` (when `T: Display`) and `Error`.
//!
//! ---
//!
//! ## Incremental API
//!
//! Alongside `parse`, the macro generates a lower-level incremental API that lets
//! you feed tokens one at a time and inspect the parser state between steps. This
//! is useful for interactive tools, speculative parsing, or building your own error
//! handling. Unlike `parse`, `feed` does not perform [error recovery](#error-recovery):
//! it returns `Checkpoint::Rejected` at the first unexpected token.
//!
//! ```rust
//! # use ferrolith::grammar;
//! # #[derive(Debug)]
//! # enum Token { Num(i64), Plus, Star }
//! # grammar! {
//! #   type Token = Token;
//! #   #[left(Plus)]
//! #   #[left(Star)]
//! #   #[start]
//! #   fn expr() -> i64 {
//! #       (Num(n)) => n,
//! #       (l @ expr, Plus, r @ expr) => l + r,
//! #       (l @ expr, Star, r @ expr) => l * r,
//! #   }
//! # }
//! # fn main() {
//! # let token_stream = vec![Token::Num(1), Token::Plus, Token::Num(2)];
//! let mut tokens = token_stream.into_iter();
//! let mut cp = start();
//!
//! // Feed tokens until the parser finishes. `token.next()` returns `None`
//! // once the stream is exhausted, which signals the end of input.
//! let result = loop {
//!     cp = match cp {
//!         Checkpoint::InputNeeded(state) => feed(state, tokens.next()),
//!         done => break done,
//!     };
//! };
//!
//! match result {
//!     Checkpoint::Accepted(value) => println!("ok: {value:?}"),
//!     Checkpoint::Rejected(_, err) => eprintln!("error: {err:?}"),
//!     Checkpoint::InputNeeded(_) => unreachable!(),
//! }
//! # }
//! ```
//!
//! When a [payload type](#token-payload) is declared, `feed` accepts
//! `Option<(Token, Payload)>` and `Checkpoint::Rejected` carries
//! `ParseError<(Token, Payload)>`:
//!
//! ```rust
//! # use ferrolith::grammar;
//! # #[derive(Debug, Clone, Copy, Default)]
//! # struct Span { start: usize, end: usize }
//! # fn merge_span(a: Span, b: Span) -> Span { Span { start: a.start, end: b.end } }
//! # enum Token { Num(i64) }
//! # grammar! {
//! #   #![parser_cfg(payload_merge = merge_span)]
//! #   type Token = Token;
//! #   type Payload = Span;
//! #   #[start]
//! #   fn expr() -> i64 { (Num(n)) => n, }
//! # }
//! # fn main() {
//! # let Checkpoint::InputNeeded(state) = start() else { unreachable!() };
//! # let (my_token, my_span) = (Token::Num(1), Span { start: 0, end: 1 });
//! let cp = feed(state, Some((my_token, my_span)));
//! # }
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
//! ```rust
//! # use ferrolith::grammar;
//! # #[derive(Clone)]
//! # enum Token { Num(i64), Plus }
//! grammar! {
//!     #![parser_cfg(clonable_state)]
//!     // ...
//! #   type Token = Token;
//! #   #[left(Plus)]
//! #   #[start]
//! #   fn expr() -> i64 {
//! #       (Num(n)) => n,
//! #       (l @ expr, Plus, r @ expr) => l + r,
//! #   }
//! }
//!
//! # fn main() {
//! # let Checkpoint::InputNeeded(state) = start() else { unreachable!() };
//! # let (token_a, token_b) = (Token::Num(1), Token::Plus);
//! let fork = state.clone();
//! let path_a = feed(state, Some(token_a));
//! let path_b = feed(fork,  Some(token_b));
//! # }
//! ```
//!
//! Requires that all types returned by grammar rules implement `Clone`.
//!
//! ---
//!
//! ## Automaton introspection (opt-in)
//!
//! Add `#![parser_cfg(expose_automaton)]` to expose the compiled LR automaton. Like
//! the rest of the generated API, these items take the
//! [visibility of the `#[start]` rule](#visibility-of-the-generated-api):
//!
//! ```rust
//! # use ferrolith::grammar;
//! # enum Token { Num(i64), Plus }
//! grammar! {
//!     #![parser_cfg(expose_automaton)]
//!     // ...
//! #   type Token = Token;
//! #   #[left(Plus)]
//! #   #[start]
//! #   fn expr() -> i64 {
//! #       (Num(n)) => n,
//! #       (l @ expr, Plus, r @ expr) => l + r,
//! #   }
//! }
//!
//! # fn main() {
//! # let (state_id, terminal_id, nonterminal_id) = (0, 1, 1);
//! // A static view of the whole automaton:
//! let view: &ferrolith::AutomatonView = &AUTOMATON;
//! println!("{} states", view.n_states);
//!
//! // Direct table lookups, useful for tooling or visualisation:
//! let act = action(state_id, terminal_id);
//! let dst = goto(state_id, nonterminal_id);
//! # }
//! ```
//!
//! See [`AutomatonView`] for the full API.

// Compiles the README's Rust examples as doctests. Only exists under `cargo test --doc`.
#[cfg(doctest)]
#[doc = include_str!("../../README.md")]
struct ReadmeDoctests;

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
