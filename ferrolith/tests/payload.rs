// Each module uses `u32` as the payload type (a token position / byte offset).
// The return type of the start rule includes the payload so tests can assert on it.

mod merge_first {
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
        Plus,
    }

    grammar! {
        #![parser_cfg(payload_merge_strategy = first)]

        type Token = Tok;
        type Payload = u32;

        #[left(Plus)]

        #[start]
        fn expr() -> (i64, u32) {
            (Num(n)) => (n, _1_payload),
            (l @ expr, Plus, r @ expr) => (l.0 + r.0, _payload),
        }
    }

    #[test]
    fn single_token() {
        let (v, p) = parse([(Tok::Num(5), 7u32)].into_iter()).unwrap();
        assert_eq!(v, 5);
        assert_eq!(p, 7);
    }

    #[test]
    fn addition_payload_is_leftmost() {
        // Tokens at positions 10, 20, 30.
        // Reduce Num(1)->expr(payload=10), Num(2)->expr(payload=30).
        // Reduce (expr, Plus, expr): First => _payload = _1_payload = 10.
        let tokens = [(Tok::Num(1), 10u32), (Tok::Plus, 20), (Tok::Num(2), 30)];
        let (v, p) = parse(tokens.into_iter()).unwrap();
        assert_eq!(v, 3);
        assert_eq!(p, 10);
    }
}

mod merge_last {
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
        Plus,
    }

    grammar! {
        #![parser_cfg(payload_merge_strategy = last)]

        type Token = Tok;
        type Payload = u32;

        #[left(Plus)]

        #[start]
        fn expr() -> (i64, u32) {
            (Num(n)) => (n, _1_payload),
            (l @ expr, Plus, r @ expr) => (l.0 + r.0, _payload),
        }
    }

    #[test]
    fn single_token() {
        let (v, p) = parse([(Tok::Num(5), 7u32)].into_iter()).unwrap();
        assert_eq!(v, 5);
        assert_eq!(p, 7);
    }

    #[test]
    fn addition_payload_is_rightmost() {
        // Last => _payload = _3_payload = payload of r = 30.
        let tokens = [(Tok::Num(1), 10u32), (Tok::Plus, 20), (Tok::Num(2), 30)];
        let (v, p) = parse(tokens.into_iter()).unwrap();
        assert_eq!(v, 3);
        assert_eq!(p, 30);
    }
}

mod merge_first_last {
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
        Plus,
    }

    fn combine(first: u32, last: u32) -> u32 {
        (first << 8) | last
    }

    grammar! {
        #![parser_cfg(payload_merge = combine, payload_merge_strategy = first_last)]

        type Token = Tok;
        type Payload = u32;

        #[left(Plus)]

        #[start]
        fn expr() -> (i64, u32) {
            (Num(n)) => (n, _1_payload),
            (l @ expr, Plus, r @ expr) => (l.0 + r.0, _payload),
        }
    }

    #[test]
    fn single_token() {
        let (v, p) = parse([(Tok::Num(5), 7u32)].into_iter()).unwrap();
        assert_eq!(v, 5);
        assert_eq!(p, 7);
    }

    #[test]
    fn addition_calls_merge_with_first_and_last() {
        // FirstLast => _payload = combine(_1_payload, _3_payload) = combine(10, 30).
        let tokens = [(Tok::Num(1), 10u32), (Tok::Plus, 20), (Tok::Num(2), 30)];
        let (v, p) = parse(tokens.into_iter()).unwrap();
        assert_eq!(v, 3);
        assert_eq!(p, (10u32 << 8) | 30);
    }
}

mod merge_fold {
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
        Plus,
    }

    fn fold_fn(a: u32, b: u32) -> u32 {
        a * 10 + b
    }

    grammar! {
        #![parser_cfg(payload_merge = fold_fn, payload_merge_strategy = fold)]

        type Token = Tok;
        type Payload = u32;

        #[start]
        fn sum() -> (i64, u32) {
            (Num(a), Plus, Num(b)) => (a + b, _payload),
        }
    }

    #[test]
    fn three_symbols_right_fold() {
        // Positions: Num=0, Plus=1, Num=2.
        // Right fold: fold_fn(_1, fold_fn(_2, _3)) = fold_fn(0, fold_fn(1, 2)) = fold_fn(0, 12) = 12.
        let tokens = [(Tok::Num(1), 0u32), (Tok::Plus, 1), (Tok::Num(2), 2)];
        let (v, p) = parse(tokens.into_iter()).unwrap();
        assert_eq!(v, 3);
        assert_eq!(p, 12);
    }
}

mod explicit_payload_override {
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
        Plus,
    }

    grammar! {
        #![parser_cfg(payload_merge_strategy = first)]

        type Token = Tok;
        type Payload = u32;

        #[start]
        fn pair() -> (i64, u32) {
            (Num(a), Plus, Num(b)) => {
                _payload = _1_payload + _3_payload;
                (a + b, _payload)
            }
        }
    }

    #[test]
    fn user_override_replaces_merge_result() {
        // First strategy would give _1_payload = 10, but the action overrides to 10+30=40.
        let tokens = [(Tok::Num(3), 10u32), (Tok::Plus, 20), (Tok::Num(4), 30)];
        let (v, p) = parse(tokens.into_iter()).unwrap();
        assert_eq!(v, 7);
        assert_eq!(p, 40);
    }
}

mod payload_default {
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
    }

    grammar! {
        #![parser_cfg(payload_merge_strategy = last, payload_default = 0u32)]

        type Token = Tok;
        type Payload = u32;

        #[start]
        fn list() -> (Vec<i64>, u32) {
            () => (vec![], _payload),
            (rest @ list, Num(n)) => {
                let (mut v, _) = rest;
                v.push(n);
                (v, _payload)
            }
        }
    }

    #[test]
    fn epsilon_uses_payload_default() {
        let (v, p) = parse(std::iter::empty::<(Tok, u32)>()).unwrap();
        assert_eq!(v, Vec::<i64>::new());
        assert_eq!(p, 0);
    }

    #[test]
    fn last_token_payload_propagates() {
        // Last strategy: payload of (list, Num) = _2_payload = Num's position.
        let tokens = [(Tok::Num(1), 10u32), (Tok::Num(2), 20)];
        let (v, p) = parse(tokens.into_iter()).unwrap();
        assert_eq!(v, vec![1, 2]);
        assert_eq!(p, 20);
    }
}

mod payload_in_parse_error {
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
    }

    grammar! {
        #![parser_cfg(payload_merge_strategy = first)]

        type Token = Tok;
        type Payload = u32;

        #[start]
        fn value() -> i64 {
            (Num(n)) => n,
        }
    }

    #[test]
    fn unexpected_token_carries_payload() {
        let tokens = [(Tok::Num(1), 10u32), (Tok::Num(2), 20u32)];
        let err = parse(tokens.into_iter()).unwrap_err();
        match err {
            ferrolith::ParseError::UnexpectedToken {
                token: (tok, payload),
                ..
            } => {
                assert!(matches!(tok, Tok::Num(2)));
                assert_eq!(payload, 20);
            }
            _ => panic!("expected UnexpectedToken"),
        }
    }

    #[test]
    fn unexpected_eof_has_no_token() {
        let err = parse(std::iter::empty::<(Tok, u32)>()).unwrap_err();
        assert!(matches!(err, ferrolith::ParseError::UnexpectedEof { .. }));
    }
}

mod incremental_payload {
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
    }

    grammar! {
        #![parser_cfg(payload_merge_strategy = first)]

        type Token = Tok;
        type Payload = u32;

        #[start]
        fn value() -> (i64, u32) {
            (Num(n)) => (n, _1_payload),
        }
    }

    #[test]
    fn feed_accepts_token_payload_pairs() {
        let s = match start() {
            Checkpoint::InputNeeded(s) => s,
            _ => panic!("expected InputNeeded"),
        };
        let s = match feed(s, Some((Tok::Num(42), 7u32))) {
            Checkpoint::InputNeeded(s) => s,
            _ => panic!("expected InputNeeded after token"),
        };
        assert!(matches!(feed(s, None), Checkpoint::Accepted((42, 7))));
    }

    #[test]
    fn rejected_carries_token_and_payload() {
        let s = match start() {
            Checkpoint::InputNeeded(s) => s,
            _ => panic!("expected InputNeeded"),
        };
        let s = match feed(s, Some((Tok::Num(1), 10u32))) {
            Checkpoint::InputNeeded(s) => s,
            _ => panic!("expected InputNeeded after first token"),
        };
        match feed(s, Some((Tok::Num(2), 20u32))) {
            Checkpoint::Rejected(
                _,
                ferrolith::ParseError::UnexpectedToken {
                    token: (tok, payload),
                    ..
                },
            ) => {
                assert!(matches!(tok, Tok::Num(2)));
                assert_eq!(payload, 20);
            }
            _ => panic!("expected Rejected with UnexpectedToken carrying payload"),
        }
    }
}

// Regression: payload types that are not plain paths must expand to valid code
// (the default fallback is generated as `<T>::default()`, not `T::default()`).
mod tuple_payload {
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
    }

    grammar! {
        #![parser_cfg(payload_merge_strategy = first)]

        type Token = Tok;
        type Payload = (u32, u32);

        #[start]
        fn values() -> (Vec<i64>, (u32, u32)) {
            (xs @ star(value)) => (xs, _payload),
        }

        fn value() -> i64 {
            (Num(n)) => n,
        }
    }

    #[test]
    fn epsilon_uses_default_tuple() {
        let (xs, p) = parse(std::iter::empty()).unwrap();
        assert!(xs.is_empty());
        assert_eq!(p, (0, 0));
    }

    #[test]
    fn tuple_payload_propagates() {
        let (xs, _) = parse([(Tok::Num(1), (0, 1)), (Tok::Num(2), (2, 3))].into_iter()).unwrap();
        assert_eq!(xs, vec![1, 2]);
    }
}

mod generic_payload {
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
        Plus,
    }

    fn first_some(a: Option<u32>, b: Option<u32>) -> Option<u32> {
        a.or(b)
    }

    grammar! {
        #![parser_cfg(payload_merge = first_some, payload_merge_strategy = first_last)]

        type Token = Tok;
        type Payload = Option<u32>;

        #[left(Plus)]

        #[start]
        fn expr() -> (i64, Option<u32>) {
            (Num(n)) => (n, _1_payload),
            (l @ expr, Plus, r @ expr) => (l.0 + r.0, _payload),
        }
    }

    #[test]
    fn generic_payload_is_merged() {
        let tokens = [(Tok::Num(1), None), (Tok::Plus, Some(2)), (Tok::Num(2), Some(3))];
        let (v, p) = parse(tokens.into_iter()).unwrap();
        assert_eq!(v, 3);
        assert_eq!(p, Some(3));
    }
}
