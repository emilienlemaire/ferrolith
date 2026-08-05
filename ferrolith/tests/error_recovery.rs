mod stmt_recovery {
    // A list of semicolon-terminated arithmetic statements.
    // Broken statements recover at the next semicolon and produce None.
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
        Plus,
        Semi,
    }

    grammar! {
        type Token = Tok;

        #[left(Plus)]

        #[start]
        fn stmts() -> Vec<Option<i64>> {
            () => vec![],
            (mut ss @ stmts, s @ stmt) => { ss.push(s); ss },
        }

        fn stmt() -> Option<i64> {
            (v @ expr, Semi) => Some(v),
            (error, Semi)    => None,
        }

        fn expr() -> i64 {
            (Num(n))                   => n,
            (l @ expr, Plus, r @ expr) => l + r,
        }
    }

    #[test]
    fn clean_input() {
        assert_eq!(
            parse([Tok::Num(1), Tok::Semi, Tok::Num(2), Tok::Semi].into_iter()).unwrap(),
            vec![Some(1), Some(2)]
        );
    }

    // pop_count = 0: the error fires at the stmt-start state, which already
    // has `stmt → • error Semi` in its item set.
    #[test]
    fn bad_first_token_recovers() {
        assert_eq!(
            parse([Tok::Plus, Tok::Semi].into_iter()).unwrap(),
            vec![None]
        );
    }

    #[test]
    fn recovery_at_start_of_list() {
        assert_eq!(
            parse([Tok::Plus, Tok::Semi, Tok::Num(2), Tok::Semi].into_iter()).unwrap(),
            vec![None, Some(2)]
        );
    }

    #[test]
    fn recovery_in_middle_of_list() {
        assert_eq!(
            parse(
                [Tok::Num(1), Tok::Semi, Tok::Plus, Tok::Semi, Tok::Num(3), Tok::Semi]
                    .into_iter()
            )
            .unwrap(),
            vec![Some(1), None, Some(3)]
        );
    }

    #[test]
    fn recovery_at_end_of_list() {
        assert_eq!(
            parse([Tok::Num(1), Tok::Semi, Tok::Plus, Tok::Semi].into_iter()).unwrap(),
            vec![Some(1), None]
        );
    }

    #[test]
    fn all_stmts_bad() {
        assert_eq!(
            parse([Tok::Plus, Tok::Semi, Tok::Plus, Tok::Semi].into_iter()).unwrap(),
            vec![None, None]
        );
    }

    #[test]
    fn multiple_junk_tokens_discarded_before_semi() {
        // Several bad tokens are all discarded during recovery until Semi is found.
        assert_eq!(
            parse([Tok::Plus, Tok::Plus, Tok::Plus, Tok::Semi].into_iter()).unwrap(),
            vec![None]
        );
    }

    // pop_count > 0: Num(1) and Plus are shifted before the error fires.
    // Recovery must pop back past those states to reach the stmt-start state.
    #[test]
    fn error_after_partial_expr_recovers() {
        assert_eq!(
            parse([Tok::Num(1), Tok::Plus, Tok::Semi].into_iter()).unwrap(),
            vec![None]
        );
    }

    #[test]
    fn error_after_partial_expr_then_valid_stmt() {
        assert_eq!(
            parse(
                [Tok::Num(1), Tok::Plus, Tok::Semi, Tok::Num(5), Tok::Semi].into_iter()
            )
            .unwrap(),
            vec![None, Some(5)]
        );
    }

    #[test]
    fn no_sync_token_after_error_fails() {
        // Error fires but no Semi is ever available; recovery discards until EOF then fails.
        assert!(parse([Tok::Plus].into_iter()).is_err());
    }

    #[test]
    fn error_terminal_not_in_expected() {
        // The __error__ pseudo-terminal must be filtered out of the expected-tokens list.
        let err = parse([Tok::Plus].into_iter()).unwrap_err();
        let expected = match err {
            ferrolith::ParseError::UnexpectedToken { expected, .. } => expected,
            ferrolith::ParseError::UnexpectedEof { expected } => expected,
        };
        assert!(!expected.contains(&"__error__"));
    }
}

mod no_error_productions {
    // A grammar with no `error` symbol — the non-recovery error path must be unchanged.
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
        Semi,
    }

    grammar! {
        type Token = Tok;

        #[start]
        fn stmt() -> i64 {
            (Num(n), Semi) => n,
        }
    }

    #[test]
    fn clean_input_parses() {
        assert_eq!(parse([Tok::Num(42), Tok::Semi].into_iter()).unwrap(), 42);
    }

    #[test]
    fn bad_token_still_errors() {
        assert!(parse([Tok::Semi].into_iter()).is_err());
    }

    #[test]
    fn unexpected_eof_still_errors() {
        assert!(parse([Tok::Num(1)].into_iter()).is_err());
    }
}
