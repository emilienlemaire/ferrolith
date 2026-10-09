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
        // Not used by the grammar.
        Minus,
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

    #[test]
    fn unknown_variant_carries_payload() {
        let tokens = [(Tok::Num(1), 0u32), (Tok::Minus, 7u32)];
        let err = parse(tokens.into_iter()).unwrap_err();
        match err {
            ferrolith::ParseError::UnexpectedToken {
                token: (tok, payload),
                ..
            } => {
                assert!(matches!(tok, Tok::Minus));
                assert_eq!(payload, 7);
            }
            _ => panic!("expected UnexpectedToken"),
        }
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

// Regression: generic token type combined with a payload, which goes through the
// `(Token, Payload)` code paths of `parse` and `feed`.
mod generic_token {
    use ferrolith::grammar;

    enum Tok<T> {
        Num(T),
        Plus,
    }

    grammar! {
        #![parser_cfg(payload_merge_strategy = first)]

        type Token = Tok<i64>;
        type Payload = u32;

        #[left(Plus)]

        #[start]
        fn expr() -> (i64, u32) {
            (Num(n)) => (n, _1_payload),
            (l @ expr, Plus, r @ expr) => (l.0 + r.0, _payload),
        }
    }

    #[test]
    fn parses_with_generic_token() {
        let tokens = [(Tok::Num(1), 10u32), (Tok::Plus, 20), (Tok::Num(2), 30)];
        assert_eq!(parse(tokens.into_iter()).unwrap(), (3, 10));
    }

    #[test]
    fn feed_with_generic_token() {
        let Checkpoint::InputNeeded(s) = start() else { panic!("expected InputNeeded") };
        let Checkpoint::InputNeeded(s) = feed(s, Some((Tok::Num(5), 7))) else {
            panic!("expected InputNeeded after token")
        };
        assert!(matches!(feed(s, None), Checkpoint::Accepted((5, 7))));
    }
}

// Non-`Copy` payloads: the merge consumes `_k_payload`, so the macro must clone the
// ones the action reads and move the others.
mod vec_payload_read_in_action {
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
        Plus,
        Neg,
    }

    fn concat(mut a: Vec<u32>, b: Vec<u32>) -> Vec<u32> {
        a.extend(b);
        a
    }

    grammar! {
        #![parser_cfg(payload_merge = concat, payload_merge_strategy = first_last)]

        type Token = Tok;
        type Payload = Vec<u32>;

        #[left(Plus)]
        #[left(Neg)]

        #[start]
        fn expr() -> (i64, Vec<u32>) {
            // Single symbol: `_payload = _1_payload` and the action reads `_1_payload`.
            (Num(n)) => (n, _1_payload),
            // `_1_payload` feeds the merge and is read inside a macro call.
            (Neg, e @ expr) => {
                assert_eq!(format!("{:?}", _1_payload), "[3]");
                (-e.0, _payload.clone())
            },
            (l @ expr, Plus, r @ expr) => (l.0 + r.0, _payload.clone()),
        }
    }

    #[test]
    fn reads_after_merge() {
        let tokens = [
            (Tok::Num(1), vec![1]),
            (Tok::Plus, vec![2]),
            (Tok::Neg, vec![3]),
            (Tok::Num(2), vec![4]),
        ];
        let (v, p) = parse(tokens.into_iter()).unwrap();
        assert_eq!(v, -1);
        // Neg: concat([3], [4]); Plus: concat([1], [3, 4]).
        assert_eq!(p, vec![1, 3, 4]);
    }
}

mod vec_payload_fold_without_reads {
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
        LParen,
        RParen,
    }

    fn concat(mut a: Vec<u32>, b: Vec<u32>) -> Vec<u32> {
        a.extend(b);
        a
    }

    grammar! {
        #![parser_cfg(payload_merge = concat, payload_merge_strategy = fold)]

        type Token = Tok;
        type Payload = Vec<u32>;

        #[start]
        fn top() -> (i64, Vec<u32>) {
            (e @ expr) => (e, _payload.clone()),
        }

        fn expr() -> i64 {
            (Num(n))                   => n,
            (LParen, e @ expr, RParen) => e,
        }
    }

    #[test]
    fn folds_every_position() {
        let tokens = [
            (Tok::LParen, vec![1]),
            (Tok::LParen, vec![2]),
            (Tok::Num(5), vec![3]),
            (Tok::RParen, vec![4]),
            (Tok::RParen, vec![5]),
        ];
        assert_eq!(parse(tokens.into_iter()).unwrap(), (5, vec![1, 2, 3, 4, 5]));
    }
}

// Clones happen only for payloads an action actually reads.
mod clone_only_when_read {
    use std::cell::Cell;

    thread_local! {
        static CLONES: Cell<usize> = const { Cell::new(0) };
    }

    #[derive(Default)]
    pub struct Counted(pub u32);

    impl Clone for Counted {
        fn clone(&self) -> Self {
            CLONES.with(|c| c.set(c.get() + 1));
            Counted(self.0)
        }
    }

    pub fn add(a: Counted, b: Counted) -> Counted {
        Counted(a.0 + b.0)
    }

    pub enum Tok {
        Num(i64),
        Plus,
    }

    fn tokens() -> impl Iterator<Item = (Tok, Counted)> {
        [
            (Tok::Num(1), Counted(1)),
            (Tok::Plus, Counted(0)),
            (Tok::Num(2), Counted(2)),
            (Tok::Plus, Counted(0)),
            (Tok::Num(3), Counted(3)),
        ]
        .into_iter()
    }

    mod no_reads {
        use super::*;
        use ferrolith::grammar;

        grammar! {
            #![parser_cfg(payload_merge = add, payload_merge_strategy = first_last)]

            type Token = Tok;
            type Payload = Counted;

            #[left(Plus)]

            #[start]
            fn expr() -> i64 {
                (Num(n))                   => n,
                (l @ expr, Plus, r @ expr) => l + r,
            }
        }

        #[test]
        fn never_clones() {
            CLONES.with(|c| c.set(0));
            assert_eq!(parse(tokens()).unwrap(), 6);
            assert_eq!(CLONES.with(Cell::get), 0);
        }
    }

    mod reads_first {
        use super::*;
        use ferrolith::grammar;

        grammar! {
            #![parser_cfg(payload_merge = add, payload_merge_strategy = first_last)]

            type Token = Tok;
            type Payload = Counted;

            #[left(Plus)]

            #[start]
            fn expr() -> i64 {
                (Num(n))                   => n,
                (l @ expr, Plus, r @ expr) => {
                    let _first = _1_payload.0;
                    l + r
                }
            }
        }

        #[test]
        fn clones_once_per_reading_reduction() {
            CLONES.with(|c| c.set(0));
            assert_eq!(parse(tokens()).unwrap(), 6);
            // Two `Plus` reductions, each cloning `_1_payload` once; `_3_payload` is moved.
            assert_eq!(CLONES.with(Cell::get), 2);
        }
    }
}

mod no_payload_default {
    // `Pos` does not implement `Default`: with `no_payload_default`, a grammar
    // without empty alternatives must compile and propagate payloads normally.
    // Non-nullable built-ins (`plus`, `sep`) are allowed.
    use ferrolith::grammar;

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Pos(u32, u32);

    fn merge(a: Pos, b: Pos) -> Pos {
        Pos(a.0, b.1)
    }

    enum Tok {
        Num(i64),
        Comma,
    }

    grammar! {
        #![parser_cfg(no_payload_default, payload_merge = merge)]

        type Token = Tok;
        type Payload = Pos;

        #[start]
        fn lists() -> (Vec<Vec<i64>>, Pos) {
            (ls @ plus(list)) => (ls, _payload),
        }

        fn list() -> Vec<i64> {
            (xs @ sep(Comma, num)) => xs,
        }

        fn num() -> i64 {
            (Num(n)) => n,
        }
    }

    #[test]
    fn payload_spans_input() {
        let tokens = [
            (Tok::Num(1), Pos(0, 1)),
            (Tok::Comma, Pos(1, 2)),
            (Tok::Num(2), Pos(2, 3)),
        ];
        let (ls, pos) = parse(tokens.into_iter()).unwrap();
        assert_eq!(ls, vec![vec![1, 2]]);
        assert_eq!(pos, Pos(0, 3));
    }
}
