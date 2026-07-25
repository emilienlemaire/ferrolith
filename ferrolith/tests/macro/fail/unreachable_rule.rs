use ferrolith::grammar;

enum Tok {
    Num(i64),
}

grammar! {
    type Token = Tok;

    #[start]
    fn top() -> i64 {
        (Num(n)) => n,
    }

    fn unused() -> i64 {
        (Num(n)) => n,
    }
}

fn main() {}
