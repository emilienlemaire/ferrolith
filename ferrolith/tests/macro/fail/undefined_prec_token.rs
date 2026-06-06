use ferrolith::grammar;

enum Tok {
    Num(i64),
    Plus,
}

grammar! {
    type Token = Tok;

    #[left(Undefined)]

    #[start]
    fn expr() -> i64 {
        (Num(n))                   => n,
        (l @ expr, Plus, r @ expr) => l + r,
    }
}

fn main() {}
