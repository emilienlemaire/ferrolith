use ferrolith::grammar;

enum Tok {
    Num(i64),
}

grammar! {
    type Token = Tok;

    #[start]
    fn top() -> i64 {
        (Num(n)) => n,
        (v @ cycle) => v,
    }

    fn cycle() -> i64 {
        (v @ cycle) => v,
    }
}

fn main() {}
