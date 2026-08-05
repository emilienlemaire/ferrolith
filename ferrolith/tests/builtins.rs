// Tests for the built-in combinators: star, plus, opt, sep, sep_opt.
// These tests are written against the planned built-in API and will not
// compile until the built-ins are injected in internalize::inject_builtins.

mod star {
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
    }

    grammar! {
        type Token = Tok;

        #[start]
        fn nums() -> Vec<i64> {
            (xs @ star(num)) => xs,
        }

        fn num() -> i64 {
            (Num(n)) => n,
        }
    }

    #[test]
    fn empty_input_gives_empty_vec() {
        assert_eq!(parse(std::iter::empty()).unwrap(), vec![]);
    }

    #[test]
    fn single_element() {
        assert_eq!(parse([Tok::Num(1)].into_iter()).unwrap(), vec![1]);
    }

    #[test]
    fn multiple_elements_in_order() {
        assert_eq!(
            parse([Tok::Num(1), Tok::Num(2), Tok::Num(3)].into_iter()).unwrap(),
            vec![1, 2, 3]
        );
    }
}

mod plus {
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
    }

    grammar! {
        type Token = Tok;

        #[start]
        fn nums() -> Vec<i64> {
            (xs @ plus(num)) => xs,
        }

        fn num() -> i64 {
            (Num(n)) => n,
        }
    }

    #[test]
    fn empty_input_is_error() {
        assert!(parse(std::iter::empty()).is_err());
    }

    #[test]
    fn single_element() {
        assert_eq!(parse([Tok::Num(1)].into_iter()).unwrap(), vec![1]);
    }

    #[test]
    fn multiple_elements_in_order() {
        assert_eq!(
            parse([Tok::Num(1), Tok::Num(2), Tok::Num(3)].into_iter()).unwrap(),
            vec![1, 2, 3]
        );
    }
}

mod opt {
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
    }

    grammar! {
        type Token = Tok;

        #[start]
        fn maybe_num() -> Option<i64> {
            (v @ opt(num)) => v,
        }

        fn num() -> i64 {
            (Num(n)) => n,
        }
    }

    #[test]
    fn empty_input_gives_none() {
        assert_eq!(parse(std::iter::empty()).unwrap(), None);
    }

    #[test]
    fn single_element_gives_some() {
        assert_eq!(parse([Tok::Num(42)].into_iter()).unwrap(), Some(42));
    }

    #[test]
    fn two_elements_is_error() {
        assert!(parse([Tok::Num(1), Tok::Num(2)].into_iter()).is_err());
    }
}

// --- new std items (sep, sep_opt) ----------------------------------------

mod sep {
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
        Comma,
    }

    grammar! {
        type Token = Tok;

        #[start]
        fn nums() -> Vec<i64> {
            (xs @ sep(Comma, num)) => xs,
        }

        fn num() -> i64 {
            (Num(n)) => n,
        }
    }

    #[test]
    fn empty_input_is_error() {
        assert!(parse(std::iter::empty()).is_err());
    }

    #[test]
    fn single_element() {
        assert_eq!(parse([Tok::Num(1)].into_iter()).unwrap(), vec![1]);
    }

    #[test]
    fn multiple_elements() {
        assert_eq!(
            parse(
                [Tok::Num(1), Tok::Comma, Tok::Num(2), Tok::Comma, Tok::Num(3)].into_iter()
            )
            .unwrap(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn trailing_separator_is_error() {
        assert!(parse([Tok::Num(1), Tok::Comma].into_iter()).is_err());
    }
}

mod sep_opt {
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
        Comma,
    }

    grammar! {
        type Token = Tok;

        #[start]
        fn nums() -> Vec<i64> {
            (xs @ sep_opt(Comma, num)) => xs,
        }

        fn num() -> i64 {
            (Num(n)) => n,
        }
    }

    #[test]
    fn empty_input_gives_empty_vec() {
        assert_eq!(parse(std::iter::empty()).unwrap(), vec![]);
    }

    #[test]
    fn single_element() {
        assert_eq!(parse([Tok::Num(1)].into_iter()).unwrap(), vec![1]);
    }

    #[test]
    fn multiple_elements() {
        assert_eq!(
            parse(
                [Tok::Num(1), Tok::Comma, Tok::Num(2), Tok::Comma, Tok::Num(3)].into_iter()
            )
            .unwrap(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn trailing_separator_is_error() {
        assert!(parse([Tok::Num(1), Tok::Comma].into_iter()).is_err());
    }
}
