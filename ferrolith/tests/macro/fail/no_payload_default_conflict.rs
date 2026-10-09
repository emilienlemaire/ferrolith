use ferrolith::grammar;

enum Tok {
    Num(i64),
}

grammar! {
    #![parser_cfg(payload_merge_strategy = first, no_payload_default, payload_default = 0)]

    type Token = Tok;
    type Payload = u32;

    #[start]
    fn num() -> i64 {
        (Num(n)) => n,
    }
}

fn main() {}
