use std::collections::{BTreeSet, HashMap, HashSet};

use proc_macro2::Span;

use crate::{
    first::FirstSet,
    grammar::{InternalGrammar, ProductionId, StateId, Symbol, TerminalId},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LR1Item {
    pub prod: ProductionId,
    pub dot: usize,
    pub lookahead: TerminalId,
}

pub struct Automaton {
    pub states: Vec<State>,
    pub transitions: HashMap<(StateId, Symbol), StateId>,
}

pub type State = BTreeSet<LR1Item>;

pub fn closure(items: &mut State, grammar: &InternalGrammar, first: &FirstSet) {
    loop {
        let old_items = items.clone();
        let mut new_items = BTreeSet::new();

        for item in items.iter() {
            let production = &grammar.productions[item.prod];
            if item.dot >= production.rhs.len() {
                continue;
            }
            let prod_dot = production.rhs[item.dot];
            let rest_prod = &production.rhs[item.dot + 1..];
            match prod_dot {
                Symbol::Terminal(_) => continue,
                Symbol::NonTerminal(nonterminal) => {
                    let prods_of_nonterm: Vec<_> = grammar
                        .productions
                        .iter()
                        .enumerate()
                        .filter(|(_, prod)| prod.lhs == nonterminal)
                        .collect();
                    let first_of_seq = first.first_of_sequence(rest_prod, item.lookahead);
                    for (prod, _) in prods_of_nonterm {
                        for terminal in &first_of_seq {
                            new_items.insert(LR1Item {
                                prod,
                                dot: 0,
                                lookahead: *terminal,
                            });
                        }
                    }
                }
            }
        }

        items.extend(new_items);

        if old_items == *items {
            break;
        }
    }
}

pub fn goto(state: &State, symbol: &Symbol, grammar: &InternalGrammar, first: &FirstSet) -> State {
    let mut items = state
        .into_iter()
        .filter(|item| {
            // No need for bound check here since `item.prod` is always a valid production id
            if let Some(prod_symbol) = grammar.productions[item.prod].rhs.get(item.dot) {
                *prod_symbol == *symbol
            } else {
                false
            }
        })
        .cloned()
        .map(|mut item| {
            item.dot += 1;
            item
        })
        .collect();
    closure(&mut items, grammar, first);
    items
}

pub fn build_automaton(grammar: &InternalGrammar, first: &FirstSet) -> syn::Result<Automaton> {
    let start_item = LR1Item {
        prod: 0,
        dot: 0,
        lookahead: 0,
    };

    let mut state = BTreeSet::new();
    state.insert(start_item);

    closure(&mut state, grammar, first);

    let mut sets_id = HashMap::new();
    sets_id.insert(state.clone(), 0);
    let mut worklist = vec![state];
    let mut transitions = HashMap::new();

    let mut state_id = 1;
    loop {
        let mut new_worklist = vec![];
        for state in worklist {
            let state_symbols: HashSet<_> = state
                .iter()
                .filter_map(|item| grammar.productions[item.prod].rhs.get(item.dot))
                .collect();
            for symbol in state_symbols {
                let goto_ = goto(&state, symbol, grammar, first);
                let dest_id = if let Some(&id) = sets_id.get(&goto_) {
                    id
                } else {
                    let id = state_id;
                    sets_id.insert(goto_.clone(), id);
                    new_worklist.push(goto_);
                    state_id += 1;
                    id
                };
                if let Some(start_id) = sets_id.get(&state) {
                    transitions.insert((*start_id, symbol.clone()), dest_id);
                } else {
                    return Err(syn::Error::new(
                        Span::call_site(),
                        "Failed to find state id.",
                    ));
                }
            }
        }
        if new_worklist.is_empty() {
            break;
        }
        worklist = new_worklist;
    }

    let mut states: Vec<_> = sets_id.into_iter().collect();
    states.sort_by_key(|(_, id)| *id);
    let states: Vec<_> = states.into_iter().map(|(s, _)| s).collect();
    Ok(Automaton {
        states,
        transitions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
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

    #[test]
    fn closure_of_initial_item() {
        let grammar = test_grammar();
        let first = FirstSet::compute(&grammar.productions);

        let mut state = State::new();
        state.insert(LR1Item { prod: 0, dot: 0, lookahead: 0 });
        closure(&mut state, &grammar, &first);

        // [S' → •E, $]
        assert!(state.contains(&LR1Item { prod: 0, dot: 0, lookahead: 0 }));
        // [E → •E+T, $] and [E → •E+T, +]
        assert!(state.contains(&LR1Item { prod: 1, dot: 0, lookahead: 0 }));
        assert!(state.contains(&LR1Item { prod: 1, dot: 0, lookahead: 1 }));
        // [E → •T, $] and [E → •T, +]
        assert!(state.contains(&LR1Item { prod: 2, dot: 0, lookahead: 0 }));
        assert!(state.contains(&LR1Item { prod: 2, dot: 0, lookahead: 1 }));
        // [T → •n, $] and [T → •n, +]
        assert!(state.contains(&LR1Item { prod: 3, dot: 0, lookahead: 0 }));
        assert!(state.contains(&LR1Item { prod: 3, dot: 0, lookahead: 1 }));
        assert_eq!(state.len(), 7);
    }

    #[test]
    fn goto_on_e_from_initial() {
        let grammar = test_grammar();
        let first = FirstSet::compute(&grammar.productions);

        let mut i0 = State::new();
        i0.insert(LR1Item { prod: 0, dot: 0, lookahead: 0 });
        closure(&mut i0, &grammar, &first);

        // goto(I0, E) → {[S'→E•,$], [E→E•+T,$], [E→E•+T,+]}
        let i1 = goto(&i0, &Symbol::NonTerminal(1), &grammar, &first);
        assert!(i1.contains(&LR1Item { prod: 0, dot: 1, lookahead: 0 }));
        assert!(i1.contains(&LR1Item { prod: 1, dot: 1, lookahead: 0 }));
        assert!(i1.contains(&LR1Item { prod: 1, dot: 1, lookahead: 1 }));
        assert_eq!(i1.len(), 3);
    }

    #[test]
    fn goto_on_n_from_initial() {
        let grammar = test_grammar();
        let first = FirstSet::compute(&grammar.productions);

        let mut i0 = State::new();
        i0.insert(LR1Item { prod: 0, dot: 0, lookahead: 0 });
        closure(&mut i0, &grammar, &first);

        // goto(I0, n) → {[T→n•,$], [T→n•,+]}
        let i3 = goto(&i0, &Symbol::Terminal(2), &grammar, &first);
        assert!(i3.contains(&LR1Item { prod: 3, dot: 1, lookahead: 0 }));
        assert!(i3.contains(&LR1Item { prod: 3, dot: 1, lookahead: 1 }));
        assert_eq!(i3.len(), 2);
    }

    #[test]
    fn goto_on_plus_from_after_e() {
        let grammar = test_grammar();
        let first = FirstSet::compute(&grammar.productions);

        let mut i0 = State::new();
        i0.insert(LR1Item { prod: 0, dot: 0, lookahead: 0 });
        closure(&mut i0, &grammar, &first);
        let i1 = goto(&i0, &Symbol::NonTerminal(1), &grammar, &first);

        // goto(I1, Plus) → {[E→E+•T,$], [E→E+•T,+], [T→•n,$], [T→•n,+]}
        let i4 = goto(&i1, &Symbol::Terminal(1), &grammar, &first);
        assert!(i4.contains(&LR1Item { prod: 1, dot: 2, lookahead: 0 }));
        assert!(i4.contains(&LR1Item { prod: 1, dot: 2, lookahead: 1 }));
        assert!(i4.contains(&LR1Item { prod: 3, dot: 0, lookahead: 0 }));
        assert!(i4.contains(&LR1Item { prod: 3, dot: 0, lookahead: 1 }));
        assert_eq!(i4.len(), 4);
    }

    #[test]
    fn build_automaton_state_count() {
        let grammar = test_grammar();
        let first = FirstSet::compute(&grammar.productions);
        let automaton = build_automaton(&grammar, &first).unwrap();
        // I0, I1(after E), I2(after T), I3(after n), I4(after E+), I5(after E+T)
        assert_eq!(automaton.states.len(), 6);
    }

    #[test]
    fn build_automaton_goto_n_merges_states() {
        let grammar = test_grammar();
        let first = FirstSet::compute(&grammar.productions);
        let automaton = build_automaton(&grammar, &first).unwrap();

        // goto(I0, n) and goto(I4, n) must lead to the same state
        let s0_n = automaton.transitions[&(0, Symbol::Terminal(2))];
        let s1 = automaton.transitions[&(0, Symbol::NonTerminal(1))];
        let s4 = automaton.transitions[&(s1, Symbol::Terminal(1))];
        let s4_n = automaton.transitions[&(s4, Symbol::Terminal(2))];
        assert_eq!(s0_n, s4_n);
    }
}

