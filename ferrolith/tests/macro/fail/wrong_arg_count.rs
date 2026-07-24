use ferrolith::grammar;

enum Tok {
    Num(i64),
    Comma,
}

grammar! {
    type Token = Tok;

    fn pair(x, y) -> (x, y) {
        (a @ x, Comma, b @ y) => (a, b),
    }

    #[start]
    fn top() -> (i64, i64) {
        (p @ pair(expr)) => p,
    }

    fn expr() -> i64 {
        (Num(n)) => n,
    }
}

fn main() {}
