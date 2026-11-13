use ferrolith::grammar;

enum Tok {
    Num(i64),
}

// `parse` returns the value of the start rule, so it needs a type.
grammar! {
    type Token = Tok;

    #[start]
    fn top() {
        (Num(_n)) => (),
    }
}

fn main() {}
