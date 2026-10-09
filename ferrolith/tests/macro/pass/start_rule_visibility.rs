// The visibility of the `#[start]` rule is applied to the generated API, so it
// can be used from outside the module that contains `grammar!`.
mod parser {
    use ferrolith::grammar;

    pub(crate) enum Tok {
        Num(i64),
        Plus,
    }

    grammar! {
        #![parser_cfg(expose_automaton)]

        type Token = Tok;

        #[left(Plus)]

        #[start]
        pub(crate) fn expr() -> i64 {
            (Num(n))                   => n,
            (l @ expr, Plus, r @ expr) => l + r,
        }
    }
}

use parser::{Checkpoint, Tok};

fn main() {
    assert_eq!(parser::parse([Tok::Num(1), Tok::Plus, Tok::Num(2)].into_iter()).unwrap(), 3);

    let state = match parser::start() {
        Checkpoint::InputNeeded(state) => state,
        _ => unreachable!(),
    };
    let _: &parser::ParserState = &state;
    let _ = state.current_state();
    let _ = state.expected_terminals();
    assert!(matches!(parser::feed(state, Some(Tok::Num(1))), Checkpoint::InputNeeded(_)));

    let _ = parser::AUTOMATON.n_states;
    let _ = parser::action(0, 0);
    let _ = parser::goto;
}
