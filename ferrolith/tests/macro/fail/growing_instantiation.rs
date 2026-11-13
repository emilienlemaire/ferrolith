use ferrolith::grammar;

enum Tok {
    A,
}

grammar! {
    type Token = Tok;

    fn nest(x) -> () {
        (x) => (),
        (nest(opt(x))) => (),
    }

    #[start]
    fn start() -> () {
        (nest(A)) => (),
    }
}

fn main() {}
