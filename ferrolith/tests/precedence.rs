// Tests for precedence overrides with `#[prec(...)]`, including pseudo-tokens:
// names declared in a precedence level that are not variants of the token type.
//
// Actions build parenthesised strings so the tests can assert on the grouping:
// with plain integers, `-2 * 3` is -6 whichever way it is parsed.

mod no_prec_attr {
    // Control: without `#[prec]`, unary minus takes the precedence of its rightmost
    // terminal, `Minus`, which is lower than `Star`, so `Star` is shifted.
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
        Minus,
        Star,
    }

    grammar! {
        type Token = Tok;

        #[left(Minus)]
        #[left(Star)]

        #[start]
        fn expr() -> String {
            (Num(n))                    => n.to_string(),
            (l @ expr, Minus, r @ expr) => format!("({l}-{r})"),
            (l @ expr, Star, r @ expr)  => format!("({l}*{r})"),
            (Minus, e @ expr)           => format!("(-{e})"),
        }
    }

    #[test]
    fn unary_minus_binds_like_binary_minus() {
        let tokens = [Tok::Minus, Tok::Num(2), Tok::Star, Tok::Num(3)];
        assert_eq!(parse(tokens.into_iter()).unwrap(), "(-(2*3))");
    }
}

mod prec_real_token {
    // `#[prec(Star)]` gives unary minus the level of `Star`; on the tie with a
    // `Star` lookahead, `Star`'s left associativity reduces.
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
        Minus,
        Star,
    }

    grammar! {
        type Token = Tok;

        #[left(Minus)]
        #[left(Star)]

        #[start]
        fn expr() -> String {
            (Num(n))                    => n.to_string(),
            (l @ expr, Minus, r @ expr) => format!("({l}-{r})"),
            (l @ expr, Star, r @ expr)  => format!("({l}*{r})"),
            #[prec(Star)]
            (Minus, e @ expr)           => format!("(-{e})"),
        }
    }

    #[test]
    fn unary_minus_binds_like_star() {
        let tokens = [Tok::Minus, Tok::Num(2), Tok::Star, Tok::Num(3)];
        assert_eq!(parse(tokens.into_iter()).unwrap(), "((-2)*3)");
    }
}

mod prec_pseudo_token {
    // `UMinus` is not a variant of `Tok`: it only names a precedence level.
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
        Minus,
        Star,
    }

    grammar! {
        type Token = Tok;

        #[left(Minus)]
        #[left(Star)]
        #[right(UMinus)]

        #[start]
        fn expr() -> String {
            (Num(n))                    => n.to_string(),
            (l @ expr, Minus, r @ expr) => format!("({l}-{r})"),
            (l @ expr, Star, r @ expr)  => format!("({l}*{r})"),
            #[prec(UMinus)]
            (Minus, e @ expr)           => format!("(-{e})"),
        }
    }

    #[test]
    fn binds_tighter_than_star() {
        let tokens = [Tok::Minus, Tok::Num(2), Tok::Star, Tok::Num(3)];
        assert_eq!(parse(tokens.into_iter()).unwrap(), "((-2)*3)");
    }

    #[test]
    fn binds_tighter_than_binary_minus() {
        let tokens = [Tok::Minus, Tok::Num(2), Tok::Minus, Tok::Num(3)];
        assert_eq!(parse(tokens.into_iter()).unwrap(), "((-2)-3)");
    }

    #[test]
    fn nested() {
        let tokens = [Tok::Minus, Tok::Minus, Tok::Num(2), Tok::Star, Tok::Num(3)];
        assert_eq!(parse(tokens.into_iter()).unwrap(), "((-(-2))*3)");
    }
}

mod pseudo_token_level_respected {
    // `UMinus` sits between `Minus` and `Star`: tighter than binary minus,
    // looser than `Star`. Its own associativity (`nonassoc`) is ignored.
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
        Minus,
        Star,
    }

    grammar! {
        type Token = Tok;

        #[left(Minus)]
        #[nonassoc(UMinus)]
        #[left(Star)]

        #[start]
        fn expr() -> String {
            (Num(n))                    => n.to_string(),
            (l @ expr, Minus, r @ expr) => format!("({l}-{r})"),
            (l @ expr, Star, r @ expr)  => format!("({l}*{r})"),
            #[prec(UMinus)]
            (Minus, e @ expr)           => format!("(-{e})"),
        }
    }

    #[test]
    fn looser_than_star() {
        let tokens = [Tok::Minus, Tok::Num(2), Tok::Star, Tok::Num(3)];
        assert_eq!(parse(tokens.into_iter()).unwrap(), "(-(2*3))");
    }

    #[test]
    fn tighter_than_binary_minus() {
        let tokens = [Tok::Minus, Tok::Num(2), Tok::Minus, Tok::Num(3)];
        assert_eq!(parse(tokens.into_iter()).unwrap(), "((-2)-3)");
    }
}

mod pseudo_token_in_parametric_rule {
    // The only `#[prec(UMinus)]` is inside a parametric rule: the unused
    // pseudo-token check must see it, and the instantiated rule must use it.
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
        Minus,
        Star,
    }

    grammar! {
        type Token = Tok;

        #[left(Minus)]
        #[left(Star)]
        #[right(UMinus)]

        fn neg(op, x) -> String {
            #[prec(UMinus)]
            (op, e @ x) => format!("(-{e})"),
        }

        #[start]
        fn expr() -> String {
            (Num(n))                    => n.to_string(),
            (l @ expr, Minus, r @ expr) => format!("({l}-{r})"),
            (l @ expr, Star, r @ expr)  => format!("({l}*{r})"),
            (e @ neg(Minus, expr))      => e,
        }
    }

    #[test]
    fn binds_tighter_than_star() {
        let tokens = [Tok::Minus, Tok::Num(2), Tok::Star, Tok::Num(3)];
        assert_eq!(parse(tokens.into_iter()).unwrap(), "((-2)*3)");
    }
}

mod nonassoc_operator {
    // `1 < 2 < 3` must be rejected: on a tie with a `nonassoc` level, the action
    // is an error, even though every other lookahead in that state reduces.
    use ferrolith::{ParseError, grammar};

    #[derive(Debug)]
    enum Tok {
        Num(i64),
        Lt,
    }

    grammar! {
        type Token = Tok;

        #[nonassoc(Lt)]

        #[start]
        fn expr() -> String {
            (Num(n))                 => n.to_string(),
            (l @ expr, Lt, r @ expr) => format!("({l}<{r})"),
        }
    }

    #[test]
    fn single_comparison_parses() {
        let tokens = [Tok::Num(1), Tok::Lt, Tok::Num(2)];
        assert_eq!(parse(tokens.into_iter()).unwrap(), "(1<2)");
    }

    #[test]
    fn chained_comparison_is_rejected() {
        let tokens = [Tok::Num(1), Tok::Lt, Tok::Num(2), Tok::Lt, Tok::Num(3)];
        match parse(tokens.into_iter()) {
            Err(ParseError::UnexpectedToken {
                token: Tok::Lt,
                expected,
                ..
            }) => {
                assert_eq!(expected, vec!["<eof>"]);
            }
            other => panic!("expected an error on the second `Lt`, got {other:?}"),
        }
    }

    #[test]
    fn single_comparison_parses_with_feed() {
        let mut cp = start();
        for t in [Some(Tok::Num(1)), Some(Tok::Lt), Some(Tok::Num(2)), None] {
            cp = match cp {
                Checkpoint::InputNeeded(s) => feed(s, t),
                _ => panic!("finished too early"),
            };
        }
        match cp {
            Checkpoint::Accepted(s) => assert_eq!(s, "(1<2)"),
            _ => panic!("expected the input to be accepted"),
        }
    }

    #[test]
    fn chained_comparison_is_rejected_by_feed() {
        let mut cp = start();
        for t in [Tok::Num(1), Tok::Lt, Tok::Num(2), Tok::Lt] {
            cp = match cp {
                Checkpoint::InputNeeded(s) => feed(s, Some(t)),
                _ => panic!("rejected too early"),
            };
        }
        assert!(matches!(
            cp,
            Checkpoint::Rejected(_, ParseError::UnexpectedToken { token: Tok::Lt, .. })
        ));
    }
}
