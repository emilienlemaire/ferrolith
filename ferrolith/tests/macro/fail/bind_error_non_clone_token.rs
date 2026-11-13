use ferrolith::grammar;

enum Tok {
    Num(i64),
    Semi,
}

// Binding `error` keeps the offending token in the error, and the same token
// may also end the recovery: the token type must be `Clone`.
grammar! {
    type Token = Tok;

    #[start]
    fn stmt() -> i64 {
        (Num(n), Semi)    => n,
        (e @ error, Semi) => { let _ = e; 0 },
    }
}

fn main() {}
