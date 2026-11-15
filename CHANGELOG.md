# Changelog

All notable changes to this project are documented in this file. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1] - 2026-11-15

### Fixed

- The return type of a built-in or parametric rule applied to a token
  (`opt(Semi)`) or to another application (`opt(plus(expr))`) kept the
  parameter name (`Option<x>`) instead of the token type or the inner rule's
  return type.
- A parametric rule whose body applies another parametric rule to one of its
  parameters (`fn many(x) -> Vec<x> { (xs @ star(x)) => xs }`) was never
  instantiated, and was reported as unable to produce a value. A rule that
  applies itself to an ever-growing argument is now a compile-time error
  instead of an endless expansion.

## [0.1.0] - 2026-11-13

First release.

### Added

- `grammar!` procedural macro: canonical LR(1) parser generation at compile
  time, with no build script and no generated source files.
- Grammar rules written as Rust-like functions, with named bindings
  (`l @ expr`), token captures (`Num(n)`), `mut` bindings and positional
  bindings (`_1`, `_2`, ...).
- Operator precedence and associativity with `#[left]`, `#[right]` and
  `#[nonassoc]`, and per-alternative overrides with `#[prec(...)]`, including
  pseudo-tokens that exist only in a precedence declaration.
- Built-in combinators `star`, `plus`, `opt`, `sep` and `sep_opt`, and
  user-defined parametric rules.
- Token payloads (`type Payload = ...`), propagated through reductions with a
  configurable merge strategy and available in actions as `_k_payload` and
  `_payload`.
- Panic-mode error recovery with the `error` pseudo-token. Binding it
  (`e @ error`) gives the action the `ParseError` that started the recovery.
- `ParseError<T, P>` with the unexpected token, its payload, and the exact list
  of terminals that were expected.
- Incremental API (`start`, `feed`, `Checkpoint`, `ParserState`), with the same
  error recovery as `parse`, `expected_terminals()`, and optional `Clone` for
  speculative parsing (`clonable_state`).
- Optional automaton introspection (`expose_automaton`): `AUTOMATON`, `action`,
  `goto` and `default_reduction`.
- Compile-time grammar checks: undefined, unreachable, non-productive and
  duplicate rules, reduce/reduce conflicts, unresolved shift/reduce conflicts
  (warning), duplicate or unused precedence declarations.
- Generated code compiles in crates using the 2021 edition or later.

[Unreleased]: https://github.com/emilienlemaire/ferrolith/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/emilienlemaire/ferrolith/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/emilienlemaire/ferrolith/releases/tag/v0.1.0
