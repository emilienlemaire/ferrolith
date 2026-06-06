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
