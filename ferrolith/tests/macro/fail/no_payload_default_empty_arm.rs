use ferrolith::grammar;

enum Tok {
    Num(i64),
}

grammar! {
    #![parser_cfg(payload_merge_strategy = first, no_payload_default)]

    type Token = Tok;
    type Payload = u32;

    #[start]
    fn items() -> Vec<i64> {
        ()                          => vec![],
        (mut xs @ items, Num(n))    => { xs.push(n); xs },
    }
}

fn main() {}
