use ferrolith::grammar;

enum Tok {
    Num(i64),
    Comma,
}

grammar! {
    #![parser_cfg(payload_merge_strategy = first, no_payload_default)]

    type Token = Tok;
    type Payload = u32;

    #[start]
    fn top() -> (Vec<i64>, Option<i64>, Vec<i64>) {
        (a @ star(num), b @ opt(num), c @ sep_opt(Comma, num)) => (a, b, c),
    }

    fn num() -> i64 {
        (Num(n)) => n,
    }
}

fn main() {}
