use ferrolith::grammar;

enum Tok {
    Num(i64),
    Comma,
}

grammar! {
    type Token = Tok;

    fn sep_list(sep, x) -> Vec<x> {
        (v @ x)                              => vec![v],
        (mut xs @ sep_list(sep, x), sep, v @ x) => { xs.push(v); xs },
    }

    #[start]
    fn nums() -> Vec<i64> {
        (items @ sep_list(Comma, expr)) => items,
    }

    fn expr() -> i64 {
        (Num(n)) => n,
    }
}

fn main() {}
