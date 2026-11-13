use ferrolith::grammar;

enum Tok {
    Num(i64),
    Plus,
}

// A rule name can only be defined once.
grammar! {
    type Token = Tok;

    #[start]
    fn top() -> i64 {
        (x @ item) => x,
    }

    fn item() -> i64 {
        (Num(n)) => n,
    }

    fn item() -> i64 {
        (Plus) => 0,
    }
}

fn main() {}
