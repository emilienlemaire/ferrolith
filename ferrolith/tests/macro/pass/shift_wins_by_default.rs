use ferrolith::grammar;

// Without a precedence declaration, shift/reduce conflicts are resolved by
// defaulting to shift (right-associative behavior). This grammar is ambiguous
// but compiles; `1 + 2 + 3` is parsed right-associatively as `1 + (2 + 3)`.
enum Tok {
    Num(i64),
    Plus,
}

grammar! {
    type Token = Tok;

    #[start]
    fn expr() -> i64 {
        (Num(n))                   => n,
        (l @ expr, Plus, r @ expr) => l + r,
    }
}

fn main() {}
