use ferrolith::grammar;

// Two rules both derive `A`, so after seeing `A` with lookahead `B`
// there is a reduce/reduce conflict between a_item and b_item.
enum Tok {
    A,
    B,
}

grammar! {
    type Token = Tok;

    #[start]
    fn top() -> i64 {
        (v @ a_item, B) => v,
        (v @ b_item, B) => v,
    }

    fn a_item() -> i64 {
        (A) => 0,
    }

    fn b_item() -> i64 {
        (A) => 1,
    }
}

fn main() {}
