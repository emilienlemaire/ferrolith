mod basic {
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
            (Num(n))           => n,
            (expr, Plus, expr) => _1 + _3,
        }
    }

    #[test]
    fn positional_addition() {
        assert_eq!(
            parse([Tok::Num(3), Tok::Plus, Tok::Num(4)].into_iter()).unwrap(),
            7
        );
    }

    #[test]
    fn positional_left_associative() {
        // (1 + 2) + 3 = 6
        assert_eq!(
            parse([Tok::Num(1), Tok::Plus, Tok::Num(2), Tok::Plus, Tok::Num(3)].into_iter())
                .unwrap(),
            6
        );
    }
}

mod mixed_named_positional {
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
            (Num(n))               => n,
            (l @ expr, Plus, expr) => l + _3,
        }
    }

    #[test]
    fn named_first_positional_third() {
        assert_eq!(
            parse([Tok::Num(10), Tok::Plus, Tok::Num(5)].into_iter()).unwrap(),
            15
        );
    }
}

mod unbound_app_positional {
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
    }

    grammar! {
        type Token = Tok;

        #[start]
        fn nums() -> Vec<i64> {
            // star(num) is an App symbol without a binding — exercises the
            // App branch in gen_reduce_function via the positional _1.
            (star(num)) => _1,
        }

        fn num() -> i64 {
            (Num(n)) => n,
        }
    }

    #[test]
    fn empty_star_is_empty_vec() {
        assert_eq!(parse(std::iter::empty()).unwrap(), vec![]);
    }

    #[test]
    fn star_collects_items_in_order() {
        assert_eq!(
            parse([Tok::Num(1), Tok::Num(2), Tok::Num(3)].into_iter()).unwrap(),
            vec![1, 2, 3],
        );
    }
}

mod bound_copy_positional {
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
            (l @ expr, Plus, r @ expr) => _1 + r,
        }
    }

    #[test]
    fn positional_for_bound_copy_type() {
        // _1 refers to `l @ expr` at position 1; i64 is Copy so _1 is still
        // available after `let l = _1` copies rather than moves it.
        assert_eq!(
            parse([Tok::Num(4), Tok::Plus, Tok::Num(6)].into_iter()).unwrap(),
            10
        );
    }
}
