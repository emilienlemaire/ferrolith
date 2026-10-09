use ferrolith::grammar;

enum Tok {
    Num(i64),
    Minus,
}

grammar! {
    type Token = Tok;

    #[left(Minus)]

    #[start]
    fn expr() -> i64 {
        (Num(n))                    => n,
        (l @ expr, Minus, r @ expr) => l - r,
        #[prec(Nope)]
        (Minus, e @ expr)           => -e,
    }
}

fn main() {}
