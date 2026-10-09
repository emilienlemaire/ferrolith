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

    // Terminals assigned in encounter order: [<eof>=0, Num=1]
    // Initial state: only Num is valid.
    #[test]
    fn unexpected_eof_expects_num() {
        let err = parse(std::iter::empty()).unwrap_err();
        match err {
            ferrolith::ParseError::UnexpectedEof { expected } => {
                assert_eq!(expected, vec!["Num"]);
            }
            _ => panic!("expected UnexpectedEof"),
        }
    }

    // After successfully reducing value, only EOF is valid.
    #[test]
    fn unexpected_token_expects_eof() {
        let err = parse([Tok::Num(1), Tok::Num(2)].into_iter()).unwrap_err();
        match err {
            ferrolith::ParseError::UnexpectedToken { expected, .. } => {
                assert_eq!(expected, vec!["<eof>"]);
            }
            _ => panic!("expected UnexpectedToken"),
        }
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

    // Terminals assigned in encounter order: [<eof>=0, Num=1, Plus=2]
    // Initial state: only Num is valid.
    #[test]
    fn unexpected_eof_at_start_expects_num() {
        let err = parse(std::iter::empty()).unwrap_err();
        match err {
            ferrolith::ParseError::UnexpectedEof { expected } => {
                assert_eq!(expected, vec!["Num"]);
            }
            _ => panic!("expected UnexpectedEof"),
        }
    }

    // After `expr Plus`, only Num can continue the right-hand side.
    #[test]
    fn unexpected_eof_after_plus_expects_num() {
        let err = parse([Tok::Num(1), Tok::Plus].into_iter()).unwrap_err();
        match err {
            ferrolith::ParseError::UnexpectedEof { expected } => {
                assert_eq!(expected, vec!["Num"]);
            }
            _ => panic!("expected UnexpectedEof"),
        }
    }

    // After reducing to `expr`, both EOF and Plus are valid.
    #[test]
    fn unexpected_token_after_expr_expects_eof_and_plus() {
        let err = parse([Tok::Num(1), Tok::Num(2)].into_iter()).unwrap_err();
        match err {
            ferrolith::ParseError::UnexpectedToken { expected, .. } => {
                assert_eq!(expected, vec!["<eof>", "Plus"]);
            }
            _ => panic!("expected UnexpectedToken"),
        }
    }
}

mod incremental {
    use ferrolith::{ParseError, grammar};

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

    // Terminals: [<eof>=0, Num=1]
    #[test]
    fn feed_valid_sequence_accepts() {
        let s = match start() {
            Checkpoint::InputNeeded(s) => s,
            _ => panic!("expected InputNeeded"),
        };
        let s = match feed(s, Some(Tok::Num(42))) {
            Checkpoint::InputNeeded(s) => s,
            _ => panic!("expected InputNeeded after token"),
        };
        assert!(matches!(feed(s, None), Checkpoint::Accepted(42)));
    }

    #[test]
    fn feed_eof_at_start_expects_num() {
        let s = match start() {
            Checkpoint::InputNeeded(s) => s,
            _ => panic!("expected InputNeeded"),
        };
        match feed(s, None) {
            Checkpoint::Rejected(_, ParseError::UnexpectedEof { expected }) => {
                assert_eq!(expected, vec!["Num"]);
            }
            _ => panic!("expected Rejected with UnexpectedEof"),
        }
    }

    #[test]
    fn feed_unexpected_second_token_expects_eof() {
        let s = match start() {
            Checkpoint::InputNeeded(s) => s,
            _ => panic!("expected InputNeeded"),
        };
        let s = match feed(s, Some(Tok::Num(1))) {
            Checkpoint::InputNeeded(s) => s,
            _ => panic!("expected InputNeeded after first token"),
        };
        match feed(s, Some(Tok::Num(2))) {
            Checkpoint::Rejected(_, ParseError::UnexpectedToken { expected, .. }) => {
                assert_eq!(expected, vec!["<eof>"]);
            }
            _ => panic!("expected Rejected with UnexpectedToken"),
        }
    }
}

mod unknown_variant {
    // `Minus` is a variant of the token type that the grammar never mentions.
    // It must be reported as an unexpected token, not panic.
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
        Plus,
        Minus,
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
    fn unknown_variant_is_unexpected_token() {
        let err = parse([Tok::Num(1), Tok::Minus].into_iter()).unwrap_err();
        match err {
            ferrolith::ParseError::UnexpectedToken { token, expected } => {
                assert!(matches!(token, Tok::Minus));
                assert_eq!(expected, vec!["<eof>", "Plus"]);
            }
            _ => panic!("expected UnexpectedToken"),
        }
    }

    #[test]
    fn unknown_variant_first() {
        let err = parse([Tok::Minus].into_iter()).unwrap_err();
        match err {
            ferrolith::ParseError::UnexpectedToken { token, expected } => {
                assert!(matches!(token, Tok::Minus));
                assert_eq!(expected, vec!["Num"]);
            }
            _ => panic!("expected UnexpectedToken"),
        }
    }

    #[test]
    fn unknown_variant_after_plus() {
        let err = parse([Tok::Num(1), Tok::Plus, Tok::Minus].into_iter()).unwrap_err();
        match err {
            ferrolith::ParseError::UnexpectedToken { token, expected } => {
                assert!(matches!(token, Tok::Minus));
                assert_eq!(expected, vec!["Num"]);
            }
            _ => panic!("expected UnexpectedToken"),
        }
    }
}
