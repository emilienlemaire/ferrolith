use ferrolith::grammar;

enum Tok {
    Num(i64),
    Plus,
}

grammar! {
    type Token = Tok;

    #[left(Plus)]

    #[start]
    pub fn expr() -> i64 {
        (t @ term)                 => t,
        (l @ expr, Plus, r @ term) => l + r,
    }

    pub(crate) fn term() -> i64 {
        (Num(n)) => n,
    }
}

fn main() {}
