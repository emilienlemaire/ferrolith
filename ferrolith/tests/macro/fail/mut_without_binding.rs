use ferrolith::grammar;

enum Tok {
    Num(i64),
}

grammar! {
    type Token = Tok;

    #[start]
    fn value() -> i64 {
        (mut Num(n)) => n,
    }
}

fn main() {}
