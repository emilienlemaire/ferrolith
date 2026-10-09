// Without a visibility on the `#[start]` rule, the generated API is private to
// the module: a private start type must not trigger `private_interfaces`, and
// the parts of the API left unused must not trigger `dead_code`.
#![deny(warnings)]

mod parser {
    use ferrolith::grammar;

    enum Tok {
        Num(i64),
    }

    struct Value(i64);

    grammar! {
        type Token = Tok;

        #[start]
        fn value() -> Value {
            (Num(n)) => Value(n),
        }
    }

    pub fn run() -> i64 {
        parse([Tok::Num(7)].into_iter()).map(|v| v.0).unwrap_or(0)
    }
}

fn main() {
    assert_eq!(parser::run(), 7);
}
