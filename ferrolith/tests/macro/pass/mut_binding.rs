use ferrolith::grammar;

enum Tok {
    Num(i64),
}

grammar! {
    type Token = Tok;

    fn star(x) -> Vec<x> {
        ()                         => vec![],
        (mut xs @ star(x), v @ x) => { xs.push(v); xs },
    }

    #[start]
    fn nums() -> Vec<i64> {
        (items @ star(expr)) => items,
    }

    fn expr() -> i64 {
        (Num(n)) => n,
    }
}

fn main() {}
