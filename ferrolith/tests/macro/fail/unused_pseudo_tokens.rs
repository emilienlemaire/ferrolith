use ferrolith::grammar;

enum Tok {
    Num(i64),
    Plus,
}

grammar! {
    type Token = Tok;

    #[left(Plus)]
    #[left(Foo, Bar)]

    #[start]
    fn expr() -> i64 {
        (Num(n))                   => n,
        (l @ expr, Plus, r @ expr) => l + r,
    }
}

fn main() {}
