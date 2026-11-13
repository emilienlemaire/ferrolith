use ferrolith::grammar;

enum Tok {
    Num(i64),
    Semi,
}

// The `error` pseudo-token carries no value.
grammar! {
    type Token = Tok;

    #[start]
    fn stmt() -> i64 {
        (Num(n), Semi)    => n,
        (e @ error, Semi) => 0,
    }
}

fn main() {}
