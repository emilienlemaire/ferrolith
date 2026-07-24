use ferrolith::grammar;

enum Tok {
    A,
}

grammar! {
    #![parser_cfg(turbo_mode)]

    type Token = Tok;

    #[start]
    fn rule() {
        (A) => (),
    }
}

fn main() {}
