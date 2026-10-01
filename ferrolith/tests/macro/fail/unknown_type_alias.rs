use ferrolith::grammar;

enum Tok {
    Num(i64),
}

grammar! {
    type Token = Tok;
    type Output = i64;

    #[start]
    fn expr() -> i64 {
        (Num(n)) => n,
    }
}

fn main() {}
