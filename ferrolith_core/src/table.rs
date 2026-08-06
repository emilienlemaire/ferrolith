use std::collections::HashMap;

use proc_macro2::Span;

use crate::{
    automaton::Automaton,
    grammar::{Assoc, InternalGrammar, NonTerminalId, ProductionId, StateId, Symbol, TerminalId},
};

/// The action the parser should take for a given (state, terminal) pair.
///
/// Returned by the generated `__action` function and exposed through the
/// `action` function when `#![parser_cfg(expose_automaton)]` is set.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Action {
    /// Shift the current token and push the given state onto the state stack.
    Shift(StateId),
    /// Reduce by the given production: pop `rhs.len()` entries from both stacks,
    /// evaluate the action expression, and push the result.
    Reduce(ProductionId),
    /// The input is fully matched. The parse is complete.
    Accept,
    /// The token is not valid in the current state. Triggers error recovery if
    /// the grammar contains an `error` alternative; otherwise the parse fails.
    Error,
}

pub type Actions = HashMap<(StateId, TerminalId), Action>;
pub type Gotos = HashMap<(StateId, NonTerminalId), StateId>;

pub struct ShiftReduceConflict {
    pub reduce_span: Span,
    pub reduce_name: String,
    pub terminal_span: Span,
    pub terminal_name: String,
}

pub fn build_tables(
    automaton: Automaton,
    grammar: &InternalGrammar,
) -> syn::Result<(
    Actions,
    Gotos,
    usize,
    Vec<Option<Symbol>>,
    Vec<ShiftReduceConflict>,
)> {
    let Automaton {
        states,
        transitions,
    } = automaton;
    let n_states = states.len();
    let mut conflicts = vec![];

    // Invert transitions to get the incoming symbol for each state.
    // State 0 has no incoming symbol (it's the initial state).
    let mut incoming: Vec<Option<Symbol>> = vec![None; n_states];
    for ((_, sym), &dest) in &transitions {
        incoming[dest] = Some(sym.clone());
    }
    let mut actions: Actions = HashMap::new();
    let mut gotos: Gotos = HashMap::new();

    for (id, state) in states.iter().enumerate() {
        for item in state {
            let prod = &grammar.productions[item.prod];
            let symbol = prod.rhs.get(item.dot);

            if let Some(symbol @ Symbol::Terminal(symbol_id)) = symbol {
                if let Some(state_id) = transitions.get(&(id, *symbol)) {
                    try_insert(
                        &mut actions,
                        &mut conflicts,
                        id,
                        *symbol_id,
                        Action::Shift(*state_id),
                        grammar,
                    )?;
                } else {
                    return Err(syn::Error::new(
                        Span::call_site(),
                        "Failed to retrieve transition",
                    ));
                }
            }

            if symbol.is_none() {
                if prod.lhs == 0 {
                    try_insert(&mut actions, &mut conflicts, id, 0, Action::Accept, grammar)?;
                } else {
                    try_insert(
                        &mut actions,
                        &mut conflicts,
                        id,
                        item.lookahead,
                        Action::Reduce(item.prod),
                        grammar,
                    )?;
                }
            }
        }
    }

    for ((state_id, symbol), dest) in transitions {
        if let Symbol::NonTerminal(id) = symbol {
            gotos.insert((state_id, id), dest);
        }
    }

    Ok((actions, gotos, n_states, incoming, conflicts))
}

fn try_insert(
    action: &mut HashMap<(StateId, TerminalId), Action>,
    conflicts: &mut Vec<ShiftReduceConflict>,
    state: StateId,
    terminal: TerminalId,
    new_action: Action,
    grammar: &InternalGrammar,
) -> syn::Result<()> {
    let key = (state, terminal);

    let pending_conflict;

    let (prod_id, new_is_reduce) = {
        let existing = match action.get(&key) {
            None => {
                action.insert(key, new_action);
                return Ok(());
            }
            Some(e) => e,
        };

        match (existing, &new_action) {
            (Action::Shift(_), Action::Shift(_)) => return Ok(()),
            (Action::Reduce(p1), Action::Reduce(p2)) if p1 == p2 => return Ok(()),
            (Action::Reduce(p1), Action::Reduce(p2)) => {
                let info_of = |prod_id: usize| {
                    grammar.productions[prod_id]
                        .action
                        .map(|a| {
                            let (ident, _) = &grammar.actions_order[a];
                            (ident.span(), ident.to_string())
                        })
                        .unwrap_or_else(|| (Span::call_site(), format!("production {prod_id}")))
                };
                let (span1, name1) = info_of(*p1);
                let (span2, name2) = info_of(*p2);
                // Sort by name so the error order is deterministic regardless
                // of which production was inserted into the table first.
                let ((sa, na), (sb, nb)) = if name1 <= name2 {
                    ((span1, name1), (span2, name2))
                } else {
                    ((span2, name2), (span1, name1))
                };
                let mut err = syn::Error::new(sa, format!("reduce/reduce conflict with `{nb}`"));
                err.combine(syn::Error::new(
                    sb,
                    format!("reduce/reduce conflict with `{na}`"),
                ));
                return Err(err);
            }
            (Action::Shift(_), Action::Reduce(p)) => {
                let action_id = grammar.productions[*p].action;
                let (reduce_name, reduce_span) = if let Some(action_id) = action_id {
                    let ident = &grammar.actions_order[action_id].0;
                    (ident.to_string(), ident.span())
                } else {
                    (format!("production {p}"), Span::call_site())
                };
                let (terminal_name, terminal_span) = {
                    let ident = &grammar.terminals[terminal];
                    (ident.to_string(), ident.span())
                };
                pending_conflict = Some(ShiftReduceConflict {
                    reduce_span,
                    reduce_name,
                    terminal_span,
                    terminal_name,
                });
                (*p, true)
            }
            (Action::Reduce(p), Action::Shift(_)) => {
                let action_id = grammar.productions[*p].action;
                let (reduce_name, reduce_span) = if let Some(action_id) = action_id {
                    let ident = &grammar.actions_order[action_id].0;
                    (ident.to_string(), ident.span())
                } else {
                    (format!("production {p}"), Span::call_site())
                };
                let (terminal_name, terminal_span) = {
                    let ident = &grammar.terminals[terminal];
                    (ident.to_string(), ident.span())
                };
                pending_conflict = Some(ShiftReduceConflict {
                    reduce_span,
                    reduce_name,
                    terminal_span,
                    terminal_name,
                });
                (*p, false)
            }
            _ => return Ok(()),
        }
    };

    let term_prec = grammar.prec_table.get(&terminal);
    let prod_prec = grammar.productions[prod_id].prec;

    match (term_prec, prod_prec) {
        (Some((term_level, assoc)), Some(prod_level)) => {
            if *term_level > prod_level {
                if !new_is_reduce {
                    action.insert(key, new_action);
                }
            } else if prod_level > *term_level {
                if new_is_reduce {
                    action.insert(key, new_action);
                }
            } else {
                match assoc {
                    Assoc::Left => {
                        if new_is_reduce {
                            action.insert(key, new_action);
                        }
                    }
                    Assoc::Right => {
                        if !new_is_reduce {
                            action.insert(key, new_action);
                        }
                    }
                    Assoc::NonAssoc => {
                        action.insert(key, Action::Error);
                    }
                }
            }
        }
        _ => {
            // default: shift wins
            if let Some(conflict) = pending_conflict {
                conflicts.push(conflict);
            }
            if !new_is_reduce {
                action.insert(key, new_action);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        automaton::build_automaton,
        first::FirstSet,
        grammar::{InternalGrammar, Production},
    };
    use proc_macro2::Span;

    fn ident(s: &str) -> syn::Ident {
        syn::Ident::new(s, Span::call_site())
    }

    // Grammar:
    //   0: S' → E          (lhs=0)
    //   1: E  → E Plus T   (lhs=1)
    //   2: E  → T          (lhs=1)
    //   3: T  → n          (lhs=2)
    // Terminals:    0=EOF, 1=Plus, 2=n
    // Nonterminals: 0=S',  1=E,    2=T
    fn test_grammar() -> InternalGrammar {
        InternalGrammar {
            terminals: vec![ident("__eof__"), ident("Plus"), ident("n")],
            nonterminals: vec![ident("__start__"), ident("E"), ident("T")],
            productions: vec![
                Production {
                    lhs: 0,
                    rhs: vec![Symbol::NonTerminal(1)],
                    action: None,
                    prec: None,
                },
                Production {
                    lhs: 1,
                    rhs: vec![
                        Symbol::NonTerminal(1),
                        Symbol::Terminal(1),
                        Symbol::NonTerminal(2),
                    ],
                    action: Some(0),
                    prec: None,
                },
                Production {
                    lhs: 1,
                    rhs: vec![Symbol::NonTerminal(2)],
                    action: Some(1),
                    prec: None,
                },
                Production {
                    lhs: 2,
                    rhs: vec![Symbol::Terminal(2)],
                    action: Some(2),
                    prec: None,
                },
            ],
            start: 0,
            prec_table: HashMap::new(),
            ret_types: vec![None, None, None],
            actions_order: vec![],
        }
    }

    // Build tables and return ((actions, gotos), [s0..s5]).
    // State IDs are resolved via transitions so the test is robust against
    // non-deterministic HashSet iteration order inside build_automaton.
    fn build() -> ((Actions, Gotos), [StateId; 6]) {
        let grammar = test_grammar();
        let first = FirstSet::compute(&grammar.productions);
        let automaton = build_automaton(&grammar, &first).unwrap();

        let s0 = 0;
        let s1 = automaton.transitions[&(s0, Symbol::NonTerminal(1))]; // after E
        let s2 = automaton.transitions[&(s0, Symbol::NonTerminal(2))]; // after T
        let s3 = automaton.transitions[&(s0, Symbol::Terminal(2))]; // after n
        let s4 = automaton.transitions[&(s1, Symbol::Terminal(1))]; // after E+
        let s5 = automaton.transitions[&(s4, Symbol::NonTerminal(2))]; // after E+T

        let (actions, gotos, _, _, _) = build_tables(automaton, &grammar).unwrap();

        ((actions, gotos), [s0, s1, s2, s3, s4, s5])
    }

    #[test]
    fn action_shift_n_from_initial() {
        let ((actions, _), [s0, _, _, s3, ..]) = build();
        assert!(matches!(actions[&(s0, 2)], Action::Shift(s) if s == s3));
    }

    #[test]
    fn action_accept_on_eof_after_e() {
        let ((actions, _), [_, s1, ..]) = build();
        assert!(matches!(actions[&(s1, 0)], Action::Accept));
    }

    #[test]
    fn action_shift_plus_after_e() {
        let ((actions, _), [_, s1, _, _, s4, _]) = build();
        assert!(matches!(actions[&(s1, 1)], Action::Shift(s) if s == s4));
    }

    #[test]
    fn action_reduce_e_to_t_after_t() {
        let ((actions, _), [_, _, s2, ..]) = build();
        assert!(matches!(actions[&(s2, 0)], Action::Reduce(2)));
        assert!(matches!(actions[&(s2, 1)], Action::Reduce(2)));
    }

    #[test]
    fn action_reduce_t_to_n_after_n() {
        let ((actions, _), [_, _, _, s3, ..]) = build();
        assert!(matches!(actions[&(s3, 0)], Action::Reduce(3)));
        assert!(matches!(actions[&(s3, 1)], Action::Reduce(3)));
    }

    #[test]
    fn action_shift_n_after_e_plus() {
        let ((actions, _), [_, _, _, s3, s4, _]) = build();
        assert!(matches!(actions[&(s4, 2)], Action::Shift(s) if s == s3));
    }

    #[test]
    fn action_reduce_e_to_e_plus_t_after_e_plus_t() {
        let ((actions, _), [_, _, _, _, _, s5]) = build();
        assert!(matches!(actions[&(s5, 0)], Action::Reduce(1)));
        assert!(matches!(actions[&(s5, 1)], Action::Reduce(1)));
    }

    #[test]
    fn goto_table() {
        let ((_, gotos), [s0, s1, s2, _, s4, s5]) = build();
        assert_eq!(gotos[&(s0, 1)], s1); // goto(I0, E) = I1
        assert_eq!(gotos[&(s0, 2)], s2); // goto(I0, T) = I2
        assert_eq!(gotos[&(s4, 2)], s5); // goto(I4, T) = I5
    }
}
