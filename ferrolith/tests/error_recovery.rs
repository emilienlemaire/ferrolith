mod stmt_recovery {
    // A list of semicolon-terminated arithmetic statements.
    // Broken statements recover at the next semicolon and produce None.
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
        Plus,
        Semi,
        // Not used by the grammar: always an unexpected token.
        Bang,
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
                [
                    Tok::Num(1),
                    Tok::Semi,
                    Tok::Plus,
                    Tok::Semi,
                    Tok::Num(3),
                    Tok::Semi
                ]
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
            parse([Tok::Num(1), Tok::Plus, Tok::Semi, Tok::Num(5), Tok::Semi].into_iter()).unwrap(),
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

    // A variant the grammar never mentions is discarded like any other junk.
    #[test]
    fn unknown_variant_discarded_by_recovery() {
        assert_eq!(
            parse([Tok::Num(1), Tok::Bang, Tok::Semi, Tok::Num(2), Tok::Semi].into_iter()).unwrap(),
            vec![None, Some(2)]
        );
    }

    // The error fires on the second `Plus`; recovery then reaches end of input
    // while waiting for `Semi`, which is what `expected` reports.
    #[test]
    fn eof_while_recovering_expects_sync_token() {
        let err = parse([Tok::Num(1), Tok::Plus, Tok::Plus].into_iter()).unwrap_err();
        match err {
            ferrolith::ParseError::UnexpectedEof { expected } => {
                assert_eq!(expected, vec!["Semi"]);
            }
            _ => panic!("expected UnexpectedEof"),
        }
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

mod payload_after_recovery {
    // The `error` pseudo-token takes the payload of the token that triggered
    // recovery, keeping the payload stack aligned with the value stack.
    // `payload_default = 99` makes default fallbacks distinguishable from positions.
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
        Plus,
        Semi,
    }

    grammar! {
        #![parser_cfg(payload_merge_strategy = first, payload_default = 99)]

        type Token = Tok;
        type Payload = u32;

        #[left(Plus)]

        #[start]
        fn stmts() -> Vec<(Option<i64>, u32)> {
            () => vec![],
            (mut ss @ stmts, s @ stmt) => { ss.push(s); ss },
        }

        fn stmt() -> (Option<i64>, u32) {
            (v @ expr, Semi) => (Some(v), _payload),
            (error, Semi)    => (None, _payload),
        }

        fn expr() -> i64 {
            (Num(n))                   => n,
            (l @ expr, Plus, r @ expr) => l + r,
        }
    }

    #[test]
    fn error_takes_offending_token_payload() {
        let tokens = [
            (Tok::Num(1), 10u32),
            (Tok::Semi, 11),
            (Tok::Plus, 20),
            (Tok::Semi, 21),
            (Tok::Num(3), 30),
            (Tok::Semi, 31),
        ];
        assert_eq!(
            parse(tokens.into_iter()).unwrap(),
            vec![(Some(1), 10), (None, 20), (Some(3), 30)]
        );
    }

    // pop_count > 0: `expr Plus` is popped before `error` is shifted.
    #[test]
    fn recovery_after_partial_parse() {
        let tokens = [
            (Tok::Num(1), 10u32),
            (Tok::Plus, 11),
            (Tok::Plus, 12),
            (Tok::Semi, 13),
        ];
        assert_eq!(parse(tokens.into_iter()).unwrap(), vec![(None, 12)]);
    }

    // The offending token is the sync token itself: it is not discarded,
    // so its payload is cloned for `error` and shifted again with `Semi`.
    #[test]
    fn offending_token_is_sync_token() {
        let tokens = [(Tok::Num(1), 10u32), (Tok::Plus, 11), (Tok::Semi, 12)];
        assert_eq!(parse(tokens.into_iter()).unwrap(), vec![(None, 12)]);
    }

    #[test]
    fn recovery_through_feed() {
        let tokens = [
            (Tok::Num(1), 10u32),
            (Tok::Plus, 11),
            (Tok::Plus, 12),
            (Tok::Semi, 13),
        ];
        let mut state = match start() {
            Checkpoint::InputNeeded(s) => s,
            _ => panic!("expected InputNeeded"),
        };
        for tok in tokens {
            state = match feed(state, Some(tok)) {
                Checkpoint::InputNeeded(s) => s,
                _ => panic!("expected InputNeeded"),
            };
        }
        match feed(state, None) {
            Checkpoint::Accepted(v) => assert_eq!(v, vec![(None, 12)]),
            _ => panic!("expected Accepted"),
        }
    }
}

mod payload_recovery_at_eof {
    // Recovery triggered at end of input has no token to take a payload from,
    // so `error` falls back to `payload_default`.
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
        Plus,
    }

    grammar! {
        #![parser_cfg(payload_merge_strategy = first, payload_default = 99)]

        type Token = Tok;
        type Payload = u32;

        #[start]
        fn top() -> u32 {
            (Num(_n), Plus) => _payload,
            (error)         => _payload,
        }
    }

    #[test]
    fn clean_input() {
        assert_eq!(
            parse([(Tok::Num(1), 3u32), (Tok::Plus, 4)].into_iter()).unwrap(),
            3
        );
    }

    #[test]
    fn offending_token_payload() {
        assert_eq!(parse([(Tok::Plus, 5u32)].into_iter()).unwrap(), 5);
    }

    #[test]
    fn eof_uses_payload_default() {
        assert_eq!(parse(std::iter::empty()).unwrap(), 99);
    }
}

mod trailing_error {
    // `error` ends its rule, so the state after shifting it only reduces
    // (`stmt → error •`). That reduction is a default reduction, stripped from
    // the packed action table: recovery must simulate it to see that `Semi`
    // can follow, instead of discarding every remaining token.
    use ferrolith::grammar;

    #[derive(Debug)]
    enum Tok {
        Num(i64),
        Semi,
    }

    grammar! {
        type Token = Tok;

        #[start]
        fn stmts() -> Vec<Option<i64>> {
            () => vec![],
            (mut ss @ stmts, s @ stmt, Semi) => { ss.push(s); ss },
        }

        fn stmt() -> Option<i64> {
            (Num(n)) => Some(n),
            (error)  => None,
        }
    }

    #[test]
    fn junk_between_separators() {
        // `Num(3)` fails where `Semi` is expected; `stmt` is popped, then the
        // junk is discarded up to the next `Semi`.
        let tokens = [
            Tok::Num(1),
            Tok::Semi,
            Tok::Num(2),
            Tok::Num(3),
            Tok::Num(4),
            Tok::Semi,
            Tok::Num(5),
            Tok::Semi,
        ];
        assert_eq!(
            parse(tokens.into_iter()).unwrap(),
            vec![Some(1), None, Some(5)]
        );
    }

    #[test]
    fn junk_at_end() {
        let tokens = [Tok::Num(1), Tok::Semi, Tok::Num(2), Tok::Num(3), Tok::Semi];
        assert_eq!(parse(tokens.into_iter()).unwrap(), vec![Some(1), None]);
    }

    #[test]
    fn unrecoverable() {
        let tokens = [Tok::Num(1), Tok::Semi, Tok::Num(2), Tok::Num(3)];
        assert!(matches!(
            parse(tokens.into_iter()),
            Err(ferrolith::ParseError::UnexpectedEof { .. })
        ));
    }
}

mod recovery_without_payload_default {
    // With `no_payload_default`, `error` still takes the offending token's
    // payload, but recovery that starts at end of input gives up.
    use ferrolith::grammar;

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Pos(u32);

    enum Tok {
        Num(i64),
        Plus,
    }

    grammar! {
        #![parser_cfg(payload_merge_strategy = first, no_payload_default)]

        type Token = Tok;
        type Payload = Pos;

        #[start]
        fn top() -> Pos {
            (Num(_n), Plus) => _payload,
            (error)         => _payload,
        }
    }

    #[test]
    fn clean_input() {
        let tokens = [(Tok::Num(1), Pos(3)), (Tok::Plus, Pos(4))];
        assert_eq!(parse(tokens.into_iter()).unwrap(), Pos(3));
    }

    #[test]
    fn error_takes_offending_token_payload() {
        assert_eq!(parse([(Tok::Plus, Pos(5))].into_iter()).unwrap(), Pos(5));
    }

    #[test]
    fn recovery_at_eof_fails() {
        assert!(matches!(
            parse(std::iter::empty()),
            Err(ferrolith::ParseError::UnexpectedEof { .. })
        ));
    }
}
