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
    fn accepts_single_token() {
        let cp = start();
        let cp = match cp {
            Checkpoint::InputNeeded(s) => feed(s, Some(Tok::Num(42))),
            _ => panic!("expected InputNeeded"),
        };
        let cp = match cp {
            Checkpoint::InputNeeded(s) => feed(s, None),
            _ => panic!("expected InputNeeded after shift"),
        };
        assert!(matches!(cp, Checkpoint::Accepted(42)));
    }

    #[test]
    fn rejects_empty_input() {
        let cp = start();
        let cp = match cp {
            Checkpoint::InputNeeded(s) => feed(s, None),
            _ => panic!("expected InputNeeded"),
        };
        assert!(matches!(cp, Checkpoint::Rejected(_, _)));
    }

    #[test]
    fn rejects_unexpected_token() {
        let cp = start();
        let s = match cp {
            Checkpoint::InputNeeded(s) => s,
            _ => panic!("expected InputNeeded"),
        };
        // Shift Num(1), then feed another Num — not valid in this grammar
        let cp = feed(s, Some(Tok::Num(1)));
        let s = match cp {
            Checkpoint::InputNeeded(s) => s,
            _ => panic!("expected InputNeeded after first token"),
        };
        let cp = feed(s, Some(Tok::Num(2)));
        assert!(matches!(cp, Checkpoint::Rejected(_, _)));
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

    fn feed_all(tokens: impl IntoIterator<Item = Tok>) -> Checkpoint {
        let mut cp = start();
        for tok in tokens {
            cp = match cp {
                Checkpoint::InputNeeded(s) => feed(s, Some(tok)),
                other => return other,
            };
        }
        match cp {
            Checkpoint::InputNeeded(s) => feed(s, None),
            other => other,
        }
    }

    #[test]
    fn accepts_single_number() {
        assert!(matches!(feed_all([Tok::Num(5)]), Checkpoint::Accepted(5)));
    }

    #[test]
    fn accepts_addition() {
        assert!(matches!(
            feed_all([Tok::Num(3), Tok::Plus, Tok::Num(4)]),
            Checkpoint::Accepted(7)
        ));
    }

    #[test]
    fn rejects_trailing_plus() {
        assert!(matches!(
            feed_all([Tok::Num(1), Tok::Plus]),
            Checkpoint::Rejected(_, _)
        ));
    }

    #[test]
    fn state_exposes_current_state_id() {
        let cp = start();
        let s = match cp {
            Checkpoint::InputNeeded(s) => s,
            _ => panic!(),
        };
        let initial = s.current_state();
        let cp = feed(s, Some(Tok::Num(1)));
        let s = match cp {
            Checkpoint::InputNeeded(s) => s,
            _ => panic!(),
        };
        // After shifting a token the state must have advanced
        assert_ne!(s.current_state(), initial);
    }

    #[test]
    fn expected_terminals_at_start() {
        let s = match start() {
            Checkpoint::InputNeeded(s) => s,
            _ => panic!("expected InputNeeded"),
        };
        assert_eq!(s.expected_terminals(), vec!["Num"]);
    }

    #[test]
    fn expected_terminals_after_expr() {
        // Feed one number to reduce to `expr`; at that point both EOF and Plus are valid.
        let s = match start() {
            Checkpoint::InputNeeded(s) => s,
            _ => panic!("expected InputNeeded"),
        };
        let s = match feed(s, Some(Tok::Num(1))) {
            Checkpoint::InputNeeded(s) => s,
            _ => panic!("expected InputNeeded after Num"),
        };
        assert_eq!(s.expected_terminals(), vec!["<eof>", "Plus"]);
    }
}

mod speculative {
    use ferrolith::grammar;

    #[derive(Clone)]
    enum Tok {
        Num(i64),
        Plus,
    }

    grammar! {
        #![parser_cfg(clonable_state)]

        type Token = Tok;

        #[left(Plus)]

        #[start]
        fn expr() -> i64 {
            (Num(n))                   => n,
            (l @ expr, Plus, r @ expr) => l + r,
        }
    }

    #[test]
    fn cloned_states_are_independent() {
        // Feed `1 + 2`, then branch: one path ends here (= 3),
        // the other continues with `+ 3` (= 6).
        let mut cp = start();
        for tok in [Tok::Num(1), Tok::Plus, Tok::Num(2)] {
            cp = match cp {
                Checkpoint::InputNeeded(s) => feed(s, Some(tok)),
                other => panic!(
                    "unexpected checkpoint: {:?}",
                    std::mem::discriminant(&other)
                ),
            };
        }

        let s = match cp {
            Checkpoint::InputNeeded(s) => s,
            other => panic!(
                "unexpected checkpoint: {:?}",
                std::mem::discriminant(&other)
            ),
        };

        let s_clone = s.clone();

        // Path A: terminate here → 3
        let cp_a = feed(s, None);
        assert!(matches!(cp_a, Checkpoint::Accepted(3)));

        // Path B: continue with `+ 3` → 6
        let cp_b = feed(s_clone, Some(Tok::Plus));
        let s_b = match cp_b {
            Checkpoint::InputNeeded(s) => s,
            _ => panic!("expected InputNeeded"),
        };
        let cp_b = feed(s_b, Some(Tok::Num(3)));
        let s_b = match cp_b {
            Checkpoint::InputNeeded(s) => s,
            _ => panic!("expected InputNeeded"),
        };
        let cp_b = feed(s_b, None);
        assert!(matches!(cp_b, Checkpoint::Accepted(6)));
    }
}

mod unknown_variant {
    // A variant the grammar never mentions is rejected by `feed`.
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
        Minus,
    }

    grammar! {
        type Token = Tok;

        #[start]
        fn value() -> i64 {
            (Num(n)) => n,
        }
    }

    #[test]
    fn unknown_variant_rejected() {
        let cp = match start() {
            Checkpoint::InputNeeded(s) => feed(s, Some(Tok::Minus)),
            _ => panic!("expected InputNeeded"),
        };
        assert!(matches!(
            cp,
            Checkpoint::Rejected(
                _,
                ferrolith::ParseError::UnexpectedToken { token: Tok::Minus, .. }
            )
        ));
    }

    #[test]
    fn known_variant_still_accepted() {
        let cp = match start() {
            Checkpoint::InputNeeded(s) => feed(s, Some(Tok::Num(1))),
            _ => panic!("expected InputNeeded"),
        };
        let cp = match cp {
            Checkpoint::InputNeeded(s) => feed(s, None),
            _ => panic!("expected InputNeeded after shift"),
        };
        assert!(matches!(cp, Checkpoint::Accepted(1)));
    }
}

mod incremental_recovery {
    // Error recovery through `feed`, one token at a time. Same grammar as
    // `error_recovery.rs::stmt_recovery`.
    use ferrolith::grammar;

    #[derive(Debug, Clone, Copy)]
    enum Tok {
        Num(i64),
        Plus,
        Semi,
    }

    grammar! {
        #![parser_cfg(clonable_state)]

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

    fn expect_input_needed(cp: Checkpoint) -> ParserState {
        match cp {
            Checkpoint::InputNeeded(s) => s,
            Checkpoint::Accepted(_) => panic!("expected InputNeeded, got Accepted"),
            Checkpoint::Rejected(_, e) => panic!("expected InputNeeded, got Rejected({e:?})"),
        }
    }

    /// Feeds every token then end of input, like `parse` but through `feed`.
    fn feed_all(tokens: &[Tok]) -> Result<Vec<Option<i64>>, ferrolith::ParseError<Tok>> {
        let mut state = expect_input_needed(start());
        for &tok in tokens {
            state = match feed(state, Some(tok)) {
                Checkpoint::InputNeeded(s) => s,
                Checkpoint::Accepted(_) => panic!("accepted before end of input"),
                Checkpoint::Rejected(_, e) => return Err(e),
            };
        }
        match feed(state, None) {
            Checkpoint::Accepted(v) => Ok(v),
            Checkpoint::Rejected(_, e) => Err(e),
            Checkpoint::InputNeeded(_) => panic!("InputNeeded after end of input"),
        }
    }

    // "1 + +": the second `Plus` triggers recovery and is discarded.
    fn recovering_state() -> ParserState {
        let mut state = expect_input_needed(start());
        for tok in [Tok::Num(1), Tok::Plus, Tok::Plus] {
            state = expect_input_needed(feed(state, Some(tok)));
        }
        state
    }

    #[test]
    fn recovers_like_parse() {
        use Tok::*;
        let inputs: &[&[Tok]] = &[
            &[Num(1), Semi, Num(2), Semi],
            &[Plus, Semi],
            &[Plus, Semi, Num(2), Semi],
            &[Num(1), Semi, Plus, Semi, Num(3), Semi],
            &[Num(1), Plus, Semi],
            &[Num(1), Plus, Plus, Plus, Num(9), Semi, Num(2), Semi],
            &[Plus, Semi, Plus, Semi],
            &[Plus],
            &[Num(1), Plus, Plus],
        ];
        for input in inputs {
            let incremental = feed_all(input);
            let batch = parse(input.iter().copied());
            assert_eq!(
                format!("{incremental:?}"),
                format!("{batch:?}"),
                "input: {input:?}"
            );
        }
    }

    #[test]
    fn is_recovering_while_discarding() {
        let state = recovering_state();
        assert!(state.is_recovering());
        assert_eq!(state.expected_terminals(), vec!["Semi"]);

        let state = expect_input_needed(feed(state, Some(Tok::Semi)));
        assert!(!state.is_recovering());
    }

    #[test]
    fn discarded_tokens_return_input_needed() {
        let mut state = recovering_state();
        for tok in [Tok::Num(5), Tok::Plus, Tok::Num(6)] {
            state = expect_input_needed(feed(state, Some(tok)));
            assert!(state.is_recovering());
        }
        let state = expect_input_needed(feed(state, Some(Tok::Semi)));
        assert!(matches!(feed(state, None), Checkpoint::Accepted(v) if v == vec![None]));
    }

    #[test]
    fn eof_while_recovering_rejects() {
        match feed(recovering_state(), None) {
            Checkpoint::Rejected(_, ferrolith::ParseError::UnexpectedEof { expected }) => {
                // The tokens that would have ended recovery.
                assert_eq!(expected, vec!["Semi"]);
            }
            _ => panic!("expected Rejected(UnexpectedEof)"),
        }
    }

    #[test]
    fn clone_while_recovering() {
        let state = recovering_state();
        let cloned = state.clone();
        assert!(cloned.is_recovering());

        // Path A: resynchronise, then parse one more statement.
        let mut a = state;
        for tok in [Tok::Semi, Tok::Num(2), Tok::Semi] {
            a = expect_input_needed(feed(a, Some(tok)));
        }
        assert!(matches!(feed(a, None), Checkpoint::Accepted(v) if v == vec![None, Some(2)]));

        // Path B: end of input while still recovering.
        assert!(matches!(
            feed(cloned, None),
            Checkpoint::Rejected(_, ferrolith::ParseError::UnexpectedEof { .. })
        ));
    }
}
