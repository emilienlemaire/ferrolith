use ferrolith::grammar;

enum Tok {
    Num(i64),
    Semi,
}

// `sep` and `list(Semi)` have no return type: there is no value to bind.
grammar! {
    type Token = Tok;

    #[start]
    fn top() -> i64 {
        (Num(n), s @ sep, l @ list(Semi)) => n,
    }

    fn sep() {
        (Semi) => (),
    }

    fn list(x) {
        (x)             => (),
        (list(x), x)    => (),
    }
}

fn main() {}
