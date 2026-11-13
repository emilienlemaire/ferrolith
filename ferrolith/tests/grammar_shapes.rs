// Grammars with shapes that the other test files do not exercise: lookahead
// that only canonical LR(1) can tell apart, nullable start rules, long and
// deeply nested inputs, the dangling `else`, and token variants with several
// fields.

mod lr1_not_lalr {
    // The textbook grammar that is LR(1) but not LALR(1): after `A E` or `B E`,
    // whether `E` reduces to `e` or to `f` depends on the next token, and the
    // answer is opposite in the two contexts. Merging the two states (as LALR
    // does) gives a reduce/reduce conflict.
    use ferrolith::{ParseError, grammar};

    #[derive(Debug, Clone, Copy)]
    enum Tok {
        A,
        B,
        E,
    }

    grammar! {
        type Token = Tok;

        #[start]
        fn s() -> &'static str {
            (A, e, A) => "aEa",
            (B, e, B) => "bEb",
            (A, f, B) => "aFb",
            (B, f, A) => "bFa",
        }

        fn e() {
            (E) => (),
        }

        fn f() {
            (E) => (),
        }
    }

    #[test]
    fn every_sentence_takes_the_right_reduction() {
        use Tok::*;
        assert_eq!(parse([A, E, A].into_iter()).unwrap(), "aEa");
        assert_eq!(parse([B, E, B].into_iter()).unwrap(), "bEb");
        assert_eq!(parse([A, E, B].into_iter()).unwrap(), "aFb");
        assert_eq!(parse([B, E, A].into_iter()).unwrap(), "bFa");
    }

    #[test]
    fn errors_are_reported_at_the_first_bad_token() {
        use Tok::*;
        match parse([A, E, E].into_iter()) {
            Err(ParseError::UnexpectedToken {
                token: E,
                mut expected,
            }) => {
                expected.sort();
                assert_eq!(expected, ["A", "B"]);
            }
            other => panic!("unexpected result: {other:?}"),
        }
        match parse([B, E].into_iter()) {
            Err(ParseError::UnexpectedEof { mut expected }) => {
                expected.sort();
                assert_eq!(expected, ["A", "B"]);
            }
            other => panic!("unexpected result: {other:?}"),
        }
        assert!(parse([A, E, A, A].into_iter()).is_err());
        assert!(parse([E].into_iter()).is_err());
    }
}

mod nullable_start {
    // The start rule derives the empty string through a user-written empty
    // alternative.
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
    }

    grammar! {
        type Token = Tok;

        #[start]
        fn list() -> Vec<i64> {
            ()                         => vec![],
            (mut xs @ list, Num(n))    => { xs.push(n); xs },
        }
    }

    #[test]
    fn empty_input_is_accepted() {
        assert_eq!(parse(std::iter::empty()).unwrap(), Vec::<i64>::new());
    }

    #[test]
    fn empty_input_is_accepted_by_feed() {
        let Checkpoint::InputNeeded(state) = start() else {
            unreachable!()
        };
        let mut expected = state.expected_terminals();
        expected.sort();
        assert_eq!(expected, ["<eof>", "Num"]);
        match feed(state, None) {
            Checkpoint::Accepted(xs) => assert!(xs.is_empty()),
            _ => panic!("empty input not accepted"),
        }
    }

    #[test]
    fn elements_are_kept_in_order() {
        let tokens = [Tok::Num(1), Tok::Num(2), Tok::Num(3)];
        assert_eq!(parse(tokens.into_iter()).unwrap(), [1, 2, 3]);
    }
}

mod long_inputs {
    // The parser keeps its stacks on the heap: neither a long left-recursive
    // list (constant stack depth) nor a long right-recursive one (stack depth
    // equal to the input length) may overflow.
    use ferrolith::grammar;

    const LEN: usize = 200_000;

    mod left_recursive {
        use super::*;

        pub enum Tok {
            Num(i64),
        }

        grammar! {
            type Token = Tok;

            #[start]
            pub fn sum() -> i64 {
                (Num(n))            => n,
                (s @ sum, Num(n))   => s + n,
            }
        }
    }

    mod right_recursive {
        use super::*;

        pub enum Tok {
            Num(i64),
            LParen,
            RParen,
        }

        grammar! {
            type Token = Tok;

            #[start]
            pub fn sum() -> i64 {
                (Num(n))                    => n,
                (Num(n), s @ sum)           => n + s,
                (LParen, s @ sum, RParen)   => s,
            }
        }
    }

    #[test]
    fn left_recursion() {
        let tokens = (0..LEN).map(|_| left_recursive::Tok::Num(1));
        assert_eq!(left_recursive::parse(tokens).unwrap(), LEN as i64);
    }

    #[test]
    fn right_recursion() {
        let tokens = (0..LEN).map(|_| right_recursive::Tok::Num(1));
        assert_eq!(right_recursive::parse(tokens).unwrap(), LEN as i64);
    }

    #[test]
    fn deep_nesting() {
        use right_recursive::Tok;
        let tokens = (0..LEN)
            .map(|_| Tok::LParen)
            .chain([Tok::Num(7)])
            .chain((0..LEN).map(|_| Tok::RParen));
        assert_eq!(right_recursive::parse(tokens).unwrap(), 7);
    }

    #[test]
    fn unclosed_nesting_is_an_error() {
        use right_recursive::Tok;
        let tokens = (0..LEN).map(|_| Tok::LParen).chain([Tok::Num(7)]);
        assert!(right_recursive::parse(tokens).is_err());
    }
}

// The unresolved conflict is reported through a `deprecated` warning.
#[allow(deprecated)]
mod dangling_else_default {
    // Without precedence, a shift/reduce conflict is resolved by shifting: the
    // `else` attaches to the nearest `if`.
    use ferrolith::grammar;

    enum Tok {
        If,
        Else,
        Atom,
    }

    grammar! {
        type Token = Tok;

        #[start]
        fn stmt() -> String {
            (Atom)                           => "a".to_string(),
            (If, s @ stmt)                   => format!("if({s})"),
            (If, t @ stmt, Else, e @ stmt)   => format!("if({t})else({e})"),
        }
    }

    #[test]
    fn else_attaches_to_the_nearest_if() {
        use Tok::*;
        assert_eq!(
            parse([If, If, Atom, Else, Atom].into_iter()).unwrap(),
            "if(if(a)else(a))"
        );
        assert_eq!(
            parse([If, If, Atom, Else, Atom, Else, Atom].into_iter()).unwrap(),
            "if(if(a)else(a))else(a)"
        );
    }
}

mod dangling_else_with_precedence {
    // The classic resolution: the `if` without `else` gets a pseudo-token of
    // lower precedence than `Else`, so `Else` is shifted. No warning is emitted.
    #![deny(warnings)]
    use ferrolith::grammar;

    enum Tok {
        If,
        Else,
        Atom,
    }

    grammar! {
        type Token = Tok;

        #[nonassoc(Then)]
        #[nonassoc(Else)]

        #[start]
        fn stmt() -> String {
            (Atom)                           => "a".to_string(),
            #[prec(Then)]
            (If, s @ stmt)                   => format!("if({s})"),
            (If, t @ stmt, Else, e @ stmt)   => format!("if({t})else({e})"),
        }
    }

    #[test]
    fn else_attaches_to_the_nearest_if() {
        use Tok::*;
        assert_eq!(
            parse([If, If, Atom, Else, Atom].into_iter()).unwrap(),
            "if(if(a)else(a))"
        );
        assert_eq!(parse([If, Atom].into_iter()).unwrap(), "if(a)");
    }
}

mod token_fields {
    // Variants with several fields and with a non-`Copy` field.
    use ferrolith::grammar;

    enum Tok {
        Pair(i64, i64),
        Name(String),
        Triple(u8, String, bool),
        Comma,
    }

    #[derive(Debug, PartialEq)]
    enum Item {
        Pair(i64, i64),
        Name(String),
        Triple(u8, String, bool),
    }

    grammar! {
        type Token = Tok;

        #[start]
        fn items() -> Vec<Item> {
            (i @ item)                              => vec![i],
            (mut is @ items, Comma, i @ item)       => { is.push(i); is },
        }

        fn item() -> Item {
            (Pair(a, b))        => Item::Pair(a, b),
            (Name(s))           => Item::Name(s),
            (Triple(a, b, c))   => Item::Triple(a, b, c),
        }
    }

    #[test]
    fn fields_are_bound_in_order() {
        let tokens = [
            Tok::Pair(1, 2),
            Tok::Comma,
            Tok::Name("x".to_string()),
            Tok::Comma,
            Tok::Triple(3, "y".to_string(), true),
        ];
        assert_eq!(
            parse(tokens.into_iter()).unwrap(),
            [
                Item::Pair(1, 2),
                Item::Name("x".to_string()),
                Item::Triple(3, "y".to_string(), true),
            ]
        );
    }
}

mod error_display {
    // `ParseError` is a `std::error::Error` when the token type is `Display`.
    use ferrolith::grammar;
    use std::fmt;

    enum Tok {
        Num(i64),
        Plus,
    }

    impl fmt::Display for Tok {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Tok::Num(n) => write!(f, "`{n}`"),
                Tok::Plus => write!(f, "`+`"),
            }
        }
    }

    grammar! {
        type Token = Tok;

        #[left(Plus)]

        #[start]
        fn expr() -> i64 {
            (Num(n))                   => n,
            (l @ expr, Plus, r @ expr) => l + r,
        }
    }

    fn run(tokens: Vec<Tok>) -> Result<i64, Box<dyn std::error::Error>> {
        Ok(parse(tokens.into_iter())?)
    }

    #[test]
    fn unexpected_token_message() {
        let err = run(vec![Tok::Num(1), Tok::Num(2)]).unwrap_err();
        assert_eq!(
            err.to_string(),
            "unexpected token `2`, expected one of: <eof>, Plus"
        );
    }

    #[test]
    fn unexpected_eof_message() {
        let err = run(vec![Tok::Num(1), Tok::Plus]).unwrap_err();
        assert_eq!(
            err.to_string(),
            "unexpected end of input, expected one of: Num"
        );
    }

    #[test]
    fn valid_input_still_parses() {
        assert_eq!(run(vec![Tok::Num(1), Tok::Plus, Tok::Num(2)]).unwrap(), 3);
    }
}
