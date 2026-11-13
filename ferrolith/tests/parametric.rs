// Tests for parametric (higher-order) rules desugared via desargue_applications.

mod star {
    // Uses the built-in star combinator — no local definition needed.
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
    }

    grammar! {
        type Token = Tok;

        #[start]
        fn nums() -> Vec<i64> {
            (items @ star(expr)) => items,
        }

        fn expr() -> i64 {
            (Num(n)) => n,
        }
    }

    #[test]
    fn empty_input() {
        assert_eq!(parse(std::iter::empty()).unwrap(), vec![]);
    }

    #[test]
    fn single_element() {
        assert_eq!(parse([Tok::Num(1)].into_iter()).unwrap(), vec![1]);
    }

    #[test]
    fn multiple_elements() {
        assert_eq!(
            parse([Tok::Num(1), Tok::Num(2), Tok::Num(3)].into_iter()).unwrap(),
            vec![1, 2, 3]
        );
    }
}

mod separated {
    // sep_list(sep, x) — one-or-more x separated by terminal sep
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
        Comma,
    }

    grammar! {
        type Token = Tok;

        fn sep_list(sep, x) -> Vec<x> {
            (v @ x)                              => vec![v],
            (mut xs @ sep_list(sep, x), sep, v @ x) => { xs.push(v); xs },
        }

        #[start]
        fn nums() -> Vec<i64> {
            (items @ sep_list(Comma, expr)) => items,
        }

        fn expr() -> i64 {
            (Num(n)) => n,
        }
    }

    #[test]
    fn single_element() {
        assert_eq!(parse([Tok::Num(1)].into_iter()).unwrap(), vec![1]);
    }

    #[test]
    fn multiple_elements() {
        assert_eq!(
            parse(
                [
                    Tok::Num(1),
                    Tok::Comma,
                    Tok::Num(2),
                    Tok::Comma,
                    Tok::Num(3)
                ]
                .into_iter()
            )
            .unwrap(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn rejects_trailing_separator() {
        assert!(parse([Tok::Num(1), Tok::Comma].into_iter()).is_err());
    }

    #[test]
    fn rejects_empty_input() {
        assert!(parse(std::iter::empty()).is_err());
    }
}

mod multiple_instantiations {
    // Two instantiations of the same parametric rule in one grammar —
    // verifies that each gets its own nonterminal and __Value variant.
    use ferrolith::grammar;

    enum Tok {
        Int(i64),
        Bool(bool),
        Comma,
        Semi,
    }

    grammar! {
        type Token = Tok;

        fn sep_list(sep, x) -> Vec<x> {
            (v @ x)                              => vec![v],
            (mut xs @ sep_list(sep, x), sep, v @ x) => { xs.push(v); xs },
        }

        #[start]
        fn program() -> (Vec<i64>, Vec<bool>) {
            (ints @ sep_list(Comma, int_expr), Semi, bools @ sep_list(Semi, bool_expr))
                => (ints, bools),
        }

        fn int_expr() -> i64 {
            (Int(n)) => n,
        }

        fn bool_expr() -> bool {
            (Bool(b)) => b,
        }
    }

    #[test]
    fn parses_both_lists() {
        let tokens = [
            Tok::Int(1),
            Tok::Comma,
            Tok::Int(2),
            Tok::Semi,
            Tok::Bool(true),
            Tok::Semi,
            Tok::Bool(false),
        ];
        assert_eq!(
            parse(tokens.into_iter()).unwrap(),
            (vec![1, 2], vec![true, false])
        );
    }

    #[test]
    fn single_elements() {
        let tokens = [Tok::Int(42), Tok::Semi, Tok::Bool(true)];
        assert_eq!(parse(tokens.into_iter()).unwrap(), (vec![42], vec![true]));
    }
}

mod token_arguments {
    // A token argument stands for the token type in the instantiated return
    // type: `opt(Semi)` returns `Option<Tok>`, not `Option<x>`.
    use ferrolith::grammar;

    #[derive(Debug, PartialEq)]
    enum Tok {
        Dot,
        Comma,
        Semi,
    }

    grammar! {
        type Token = Tok;

        fn wrap(x) -> (x,) {
            (v @ x) => (v,),
        }

        #[start]
        fn program() -> (Option<Tok>, Vec<Tok>, (Tok,)) {
            (head @ opt(Semi), tail @ star(Comma), w @ wrap(Dot)) => (head, tail, w),
        }
    }

    #[test]
    fn token_types_are_substituted() {
        let tokens = [Tok::Semi, Tok::Comma, Tok::Comma, Tok::Dot];
        assert_eq!(
            parse(tokens.into_iter()).unwrap(),
            (Some(Tok::Semi), vec![Tok::Comma, Tok::Comma], (Tok::Dot,))
        );
    }

    #[test]
    fn missing_optional_token() {
        assert_eq!(
            parse([Tok::Dot].into_iter()).unwrap(),
            (None, vec![], (Tok::Dot,))
        );
    }
}

mod nested_applications {
    // An application argument stands for the instantiated rule's return type:
    // `opt(plus(num))` returns `Option<Vec<i64>>`.
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
        Semi,
    }

    grammar! {
        type Token = Tok;

        #[start]
        fn program() -> Option<Vec<i64>> {
            (v @ opt(plus(num)), Semi) => v,
        }

        fn num() -> i64 {
            (Num(n)) => n,
        }
    }

    #[test]
    fn present() {
        let tokens = [Tok::Num(1), Tok::Num(2), Tok::Semi];
        assert_eq!(parse(tokens.into_iter()).unwrap(), Some(vec![1, 2]));
    }

    #[test]
    fn absent() {
        assert_eq!(parse([Tok::Semi].into_iter()).unwrap(), None);
    }
}

mod generic_calling_generic {
    // A parametric rule whose body applies another parametric rule to its
    // own parameter.
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
    }

    grammar! {
        type Token = Tok;

        fn many(x) -> Vec<x> {
            (xs @ star(x)) => xs,
        }

        #[start]
        fn program() -> Vec<i64> {
            (xs @ many(num)) => xs,
        }

        fn num() -> i64 {
            (Num(n)) => n,
        }
    }

    #[test]
    fn parses() {
        let tokens = [Tok::Num(1), Tok::Num(2)];
        assert_eq!(parse(tokens.into_iter()).unwrap(), vec![1, 2]);
    }
}
