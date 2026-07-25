use ferrolith::grammar;

enum Tok {
    Num(i64),
}

grammar! {
    type Token = Tok;

    #[start]
    fn top() -> i64 {
        (Num(n)) => n,
        (v @ nonexistent) => v,
    }
}

fn main() {}
