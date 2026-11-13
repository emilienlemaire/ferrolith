use ferrolith::grammar;

enum Tok {
    Num(i64),
    Plus,
}

// A token belongs to a single precedence level.
grammar! {
    type Token = Tok;

    #[left(Plus)]
    #[right(Plus)]

    #[start]
    fn expr() -> i64 {
        (Num(n))                   => n,
        (l @ expr, Plus, r @ expr) => l + r,
    }
}

fn main() {}
