mod single_value {
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
    }

    grammar! {
        type Token = Tok;

        #[start]
        fn value() -> i64 {
            (Num(n)) => n,
        }
    }

    #[test]
    fn parses_a_number() {
        assert_eq!(parse([Tok::Num(42)].into_iter()).unwrap(), 42);
    }

    #[test]
    fn rejects_empty_input() {
        assert!(parse(std::iter::empty()).is_err());
    }
}

mod unit_rule_without_binding {
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
        Semi,
    }

    grammar! {
        type Token = Tok;

        #[start]
        fn stmt() -> i64 {
            // sep has no return type and no binding — exercises the None branch
            // in the Rule arm of gen_reduce_function (let _ = #vN).
            (Num(n), sep) => n,
        }

        fn sep() {
            (Semi) => (),
        }
    }

    #[test]
    fn parses_number_followed_by_semi() {
        assert_eq!(parse([Tok::Num(42), Tok::Semi].into_iter()).unwrap(), 42);
    }

    #[test]
    fn rejects_missing_semi() {
        assert!(parse([Tok::Num(1)].into_iter()).is_err());
    }
}

mod addition {
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
        Plus,
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

    #[test]
    fn parses_single_number() {
        assert_eq!(parse([Tok::Num(5)].into_iter()).unwrap(), 5);
    }

    #[test]
    fn parses_addition() {
        assert_eq!(
            parse([Tok::Num(3), Tok::Plus, Tok::Num(4)].into_iter()).unwrap(),
            7
        );
    }

    #[test]
    fn left_associative() {
        assert_eq!(
            parse([Tok::Num(1), Tok::Plus, Tok::Num(2), Tok::Plus, Tok::Num(3)].into_iter())
                .unwrap(),
            6
        );
    }

    #[test]
    fn rejects_trailing_plus() {
        assert!(parse([Tok::Num(1), Tok::Plus].into_iter()).is_err());
    }
}

// Regression: a generic token type must not be spliced into patterns as
// `Tok<i64>::Num(..)` (invalid syntax). Covers every pattern site: a capture,
// a capture inside a binding, and a unit variant.
mod generic_token {
    use ferrolith::grammar;

    enum Tok<T> {
        Num(T),
        Neg(T),
        Plus,
    }

    grammar! {
        type Token = Tok<i64>;

        #[left(Plus)]

        #[start]
        fn expr() -> i64 {
            (Num(n))                   => n,
            (x @ Neg(n))               => -n,
            (l @ expr, Plus, r @ expr) => l + r,
        }
    }

    #[test]
    fn parses_with_generic_token() {
        let tokens = [Tok::Num(1), Tok::Plus, Tok::Neg(4), Tok::Plus, Tok::Num(10)];
        assert_eq!(parse(tokens.into_iter()).unwrap(), 7);
    }

    #[test]
    fn rejects_with_generic_token() {
        assert!(parse([Tok::Plus].into_iter()).is_err());
    }
}

// The token type can also be a multi-segment path.
mod qualified_token_path {
    use ferrolith::grammar;

    mod lexer {
        pub enum Tok {
            Num(i64),
        }
    }

    grammar! {
        type Token = lexer::Tok;

        #[start]
        fn value() -> i64 {
            (Num(n)) => n,
        }
    }

    #[test]
    fn parses_with_qualified_token_path() {
        assert_eq!(parse([lexer::Tok::Num(3)].into_iter()).unwrap(), 3);
    }
}
