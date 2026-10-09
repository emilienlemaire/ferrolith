mod parser {
    use ferrolith::grammar;

    pub enum Tok {
        Num(i64),
    }

    grammar! {
        type Token = Tok;

        #[start]
        fn value() -> i64 {
            (Num(n)) => n,
        }
    }
}

fn main() {
    let _ = parser::parse([parser::Tok::Num(1)].into_iter());
}
