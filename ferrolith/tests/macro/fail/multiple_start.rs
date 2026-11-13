use ferrolith::grammar;

enum Tok {
    A,
    B,
}

grammar! {
    type Token = Tok;

    #[start]
    fn rule1() -> () {
        (A) => (),
    }

    #[start]
    fn rule2() -> () {
        (B) => (),
    }
}

fn main() {}
