use ferrolith::grammar;

enum Tok {
    A,
}

grammar! {
    type Token = Tok;

    fn rule() {
        (A) => (),
    }
}

fn main() {}
