use std::collections::{BTreeMap, HashMap};

use crate::{
    first::FirstSet,
    grammar::{InternalGrammar, NonTerminalId, ProductionId, StateId, Symbol, TerminalId},
};

/// A set of terminals, stored as a bit set.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct TerminalSet {
    words: Vec<u64>,
}

impl TerminalSet {
    fn new(n_terminals: usize) -> Self {
        TerminalSet {
            words: vec![0; n_terminals.div_ceil(64)],
        }
    }

    fn insert(&mut self, terminal: TerminalId) {
        self.words[terminal / 64] |= 1 << (terminal % 64);
    }

    /// Adds every terminal of `other`; returns whether the set grew.
    fn union_with(&mut self, other: &TerminalSet) -> bool {
        let mut changed = false;
        for (word, other) in self.words.iter_mut().zip(&other.words) {
            let merged = *word | *other;
            changed |= merged != *word;
            *word = merged;
        }
        changed
    }

    /// The terminals of the set, in increasing order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = TerminalId> + '_ {
        self.words.iter().enumerate().flat_map(|(i, word)| {
            (0..64)
                .filter(move |bit| word & (1 << bit) != 0)
                .map(move |bit| i * 64 + bit)
        })
    }
}

/// The LR(1) items `[prod → α • β, a]` that share a production and a dot
/// position, for every lookahead `a` in `lookaheads`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Item {
    pub(crate) prod: ProductionId,
    pub(crate) dot: usize,
    pub(crate) lookaheads: TerminalSet,
}

/// A closed item set, sorted by `(prod, dot)` with one entry per pair.
pub(crate) type State = Vec<Item>;

pub struct Automaton {
    pub(crate) states: Vec<State>,
    pub(crate) transitions: HashMap<(StateId, Symbol), StateId>,
}

/// Grammar tables shared by every closure computation.
struct Builder<'a> {
    grammar: &'a InternalGrammar,
    n_terminals: usize,
    /// The productions of each nonterminal.
    prods_of: Vec<Vec<ProductionId>>,
    /// `suffix_first[prod][dot]` is FIRST of `rhs[dot..]` and whether that
    /// suffix can derive the empty string.
    suffix_first: Vec<Vec<(TerminalSet, bool)>>,
}

impl<'a> Builder<'a> {
    fn new(grammar: &'a InternalGrammar, first: &FirstSet) -> Self {
        let n_terminals = grammar.terminals.len();

        let mut prods_of: Vec<Vec<ProductionId>> = vec![vec![]; grammar.nonterminals.len()];
        for (id, prod) in grammar.productions.iter().enumerate() {
            let lhs: NonTerminalId = prod.lhs;
            prods_of[lhs].push(id);
        }

        let suffix_first = grammar
            .productions
            .iter()
            .map(|prod| {
                (0..=prod.rhs.len())
                    .map(|dot| {
                        let (terminals, nullable) = first.first_and_nullable(&prod.rhs[dot..]);
                        let mut set = TerminalSet::new(n_terminals);
                        for terminal in terminals {
                            set.insert(terminal);
                        }
                        (set, nullable)
                    })
                    .collect()
            })
            .collect();

        Builder {
            grammar,
            n_terminals,
            prods_of,
            suffix_first,
        }
    }

    /// Completes `kernel` into a closed state: for each item
    /// `[A → α • B β, L]` and each production `B → γ`, the state contains
    /// `[B → • γ, FIRST(β L)]`.
    ///
    /// Lookaheads are propagated with a worklist: an item is revisited only
    /// when its lookahead set grows.
    fn closure(&self, kernel: Vec<Item>) -> State {
        let mut items = kernel;
        // Position in `items` of the item `[prod → • γ]`, for each production.
        let mut initial_item: Vec<Option<usize>> = vec![None; self.grammar.productions.len()];
        for (idx, item) in items.iter().enumerate() {
            if item.dot == 0 {
                initial_item[item.prod] = Some(idx);
            }
        }

        let mut worklist: Vec<usize> = (0..items.len()).collect();
        while let Some(idx) = worklist.pop() {
            let Item { prod, dot, .. } = items[idx];
            let Some(Symbol::NonTerminal(nonterminal)) =
                self.grammar.productions[prod].rhs.get(dot)
            else {
                continue;
            };

            let (first, nullable) = &self.suffix_first[prod][dot + 1];
            let mut lookaheads = first.clone();
            if *nullable {
                lookaheads.union_with(&items[idx].lookaheads);
            }

            for &prod in &self.prods_of[*nonterminal] {
                match initial_item[prod] {
                    Some(existing) => {
                        if items[existing].lookaheads.union_with(&lookaheads) {
                            worklist.push(existing);
                        }
                    }
                    None => {
                        initial_item[prod] = Some(items.len());
                        worklist.push(items.len());
                        items.push(Item {
                            prod,
                            dot: 0,
                            lookaheads: lookaheads.clone(),
                        });
                    }
                }
            }
        }

        items.sort_by_key(|item| (item.prod, item.dot));
        items
    }

    /// The kernels of the states reachable from `state` in one transition:
    /// for each symbol after a dot, the items with the dot moved past it.
    ///
    /// Each kernel is sorted by `(prod, dot)`, like `state`.
    fn successors(&self, state: &State) -> BTreeMap<Symbol, Vec<Item>> {
        let mut kernels: BTreeMap<Symbol, Vec<Item>> = BTreeMap::new();
        for item in state {
            if let Some(symbol) = self.grammar.productions[item.prod].rhs.get(item.dot) {
                kernels.entry(*symbol).or_default().push(Item {
                    prod: item.prod,
                    dot: item.dot + 1,
                    lookaheads: item.lookaheads.clone(),
                });
            }
        }
        kernels
    }

    fn initial_kernel(&self) -> Vec<Item> {
        let mut lookaheads = TerminalSet::new(self.n_terminals);
        lookaheads.insert(0);
        vec![Item {
            prod: 0,
            dot: 0,
            lookaheads,
        }]
    }
}

/// Builds the canonical LR(1) automaton.
///
/// States are numbered in breadth-first order from the initial state, visiting
/// the outgoing symbols of each state in order (terminals first), so the
/// numbering is the same on every run.
pub fn build_automaton(grammar: &InternalGrammar, first: &FirstSet) -> syn::Result<Automaton> {
    let builder = Builder::new(grammar, first);

    // A closed state is determined by its kernel, so states are identified by it.
    let initial_kernel = builder.initial_kernel();
    let mut state_of_kernel: HashMap<Vec<Item>, StateId> = HashMap::new();
    state_of_kernel.insert(initial_kernel.clone(), 0);
    let mut states: Vec<State> = vec![builder.closure(initial_kernel)];
    let mut transitions = HashMap::new();

    let mut next = 0;
    while next < states.len() {
        for (symbol, kernel) in builder.successors(&states[next]) {
            let dest = match state_of_kernel.get(&kernel) {
                Some(&id) => id,
                None => {
                    let id = states.len();
                    state_of_kernel.insert(kernel.clone(), id);
                    states.push(builder.closure(kernel));
                    id
                }
            };
            transitions.insert((next, symbol), dest);
        }
        next += 1;
    }

    Ok(Automaton {
        states,
        transitions,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::{
        first::FirstSet,
        grammar::{InternalGrammar, Production},
    };
    use proc_macro2::Span;

    fn ident(s: &str) -> syn::Ident {
        syn::Ident::new(s, Span::call_site())
    }

    /// One LR(1) item with a single lookahead, as in the textbook definition.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    struct LR1Item {
        prod: ProductionId,
        dot: usize,
        lookahead: TerminalId,
    }

    fn expand(state: &State) -> BTreeSet<LR1Item> {
        state
            .iter()
            .flat_map(|item| {
                item.lookaheads.iter().map(|lookahead| LR1Item {
                    prod: item.prod,
                    dot: item.dot,
                    lookahead,
                })
            })
            .collect()
    }

    fn initial_state(builder: &Builder) -> State {
        builder.closure(builder.initial_kernel())
    }

    fn goto(builder: &Builder, state: &State, symbol: Symbol) -> State {
        builder.closure(builder.successors(state).remove(&symbol).unwrap())
    }

    /// The textbook construction, one item per lookahead and a naive fixed
    /// point: slow, but obviously the definition. `build_automaton` must
    /// produce the same automaton up to the numbering of the states.
    mod reference {
        use super::*;
        use std::collections::HashSet;

        pub type State = BTreeSet<LR1Item>;

        fn closure(items: &mut State, grammar: &InternalGrammar, first: &FirstSet) {
            loop {
                let before = items.len();
                let mut new_items = BTreeSet::new();
                for item in items.iter() {
                    let production = &grammar.productions[item.prod];
                    let Some(Symbol::NonTerminal(nonterminal)) = production.rhs.get(item.dot)
                    else {
                        continue;
                    };
                    let lookaheads =
                        first.first_of_sequence(&production.rhs[item.dot + 1..], item.lookahead);
                    for (prod, production) in grammar.productions.iter().enumerate() {
                        if production.lhs == *nonterminal {
                            for lookahead in &lookaheads {
                                new_items.insert(LR1Item {
                                    prod,
                                    dot: 0,
                                    lookahead: *lookahead,
                                });
                            }
                        }
                    }
                }
                items.extend(new_items);
                if items.len() == before {
                    break;
                }
            }
        }

        fn goto(
            state: &State,
            symbol: Symbol,
            grammar: &InternalGrammar,
            first: &FirstSet,
        ) -> State {
            let mut items: State = state
                .iter()
                .filter(|item| grammar.productions[item.prod].rhs.get(item.dot) == Some(&symbol))
                .map(|item| LR1Item {
                    dot: item.dot + 1,
                    ..*item
                })
                .collect();
            closure(&mut items, grammar, first);
            items
        }

        /// The states and the transitions between them, both by content.
        pub fn build(
            grammar: &InternalGrammar,
            first: &FirstSet,
        ) -> (HashSet<State>, HashSet<(State, Symbol, State)>) {
            let mut initial = State::new();
            initial.insert(LR1Item {
                prod: 0,
                dot: 0,
                lookahead: 0,
            });
            closure(&mut initial, grammar, first);

            let mut states = HashSet::from([initial.clone()]);
            let mut transitions = HashSet::new();
            let mut worklist = vec![initial];
            while let Some(state) = worklist.pop() {
                let symbols: HashSet<Symbol> = state
                    .iter()
                    .filter_map(|item| grammar.productions[item.prod].rhs.get(item.dot).copied())
                    .collect();
                for symbol in symbols {
                    let dest = goto(&state, symbol, grammar, first);
                    if states.insert(dest.clone()) {
                        worklist.push(dest.clone());
                    }
                    transitions.insert((state.clone(), symbol, dest));
                }
            }
            (states, transitions)
        }
    }

    fn assert_matches_reference(grammar: &InternalGrammar) {
        use std::collections::HashSet;

        let first = FirstSet::compute(&grammar.productions);
        let automaton = build_automaton(grammar, &first).unwrap();
        let (ref_states, ref_transitions) = reference::build(grammar, &first);

        let states: Vec<BTreeSet<LR1Item>> = automaton.states.iter().map(expand).collect();
        // No state is built twice.
        assert_eq!(
            states.iter().cloned().collect::<HashSet<_>>().len(),
            states.len()
        );
        assert_eq!(states.iter().cloned().collect::<HashSet<_>>(), ref_states);

        let transitions: HashSet<_> = automaton
            .transitions
            .iter()
            .map(|((from, symbol), to)| (states[*from].clone(), *symbol, states[*to].clone()))
            .collect();
        assert_eq!(transitions, ref_transitions);

        // Every state keeps its items sorted, one per (prod, dot).
        for state in &automaton.states {
            assert!(
                state
                    .windows(2)
                    .all(|w| (w[0].prod, w[0].dot) < (w[1].prod, w[1].dot))
            );
        }
    }

    /// Builds a grammar from `(lhs, rhs)` pairs. Production 0 must be the
    /// augmented start production.
    fn grammar_of(
        n_terminals: usize,
        n_nonterminals: usize,
        productions: &[(usize, &[Symbol])],
    ) -> InternalGrammar {
        InternalGrammar {
            terminals: (0..n_terminals).map(|i| ident(&format!("t{i}"))).collect(),
            nonterminals: (0..n_nonterminals)
                .map(|i| ident(&format!("n{i}")))
                .collect(),
            productions: productions
                .iter()
                .enumerate()
                .map(|(i, (lhs, rhs))| Production {
                    lhs: *lhs,
                    rhs: rhs.to_vec(),
                    action: i.checked_sub(1),
                    prec: None,
                })
                .collect(),
            start: 0,
            prec_table: HashMap::new(),
            ret_types: vec![None; n_nonterminals],
            actions_order: vec![],
        }
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

    fn item(prod: usize, dot: usize, lookahead: usize) -> LR1Item {
        LR1Item {
            prod,
            dot,
            lookahead,
        }
    }

    #[test]
    fn closure_of_initial_item() {
        let grammar = test_grammar();
        let first = FirstSet::compute(&grammar.productions);
        let builder = Builder::new(&grammar, &first);

        let state = expand(&initial_state(&builder));

        let expected = BTreeSet::from([
            // [S' → •E, $]
            item(0, 0, 0),
            // [E → •E+T, $] and [E → •E+T, +]
            item(1, 0, 0),
            item(1, 0, 1),
            // [E → •T, $] and [E → •T, +]
            item(2, 0, 0),
            item(2, 0, 1),
            // [T → •n, $] and [T → •n, +]
            item(3, 0, 0),
            item(3, 0, 1),
        ]);
        assert_eq!(state, expected);
    }

    #[test]
    fn goto_on_e_from_initial() {
        let grammar = test_grammar();
        let first = FirstSet::compute(&grammar.productions);
        let builder = Builder::new(&grammar, &first);
        let i0 = initial_state(&builder);

        // goto(I0, E) → {[S'→E•,$], [E→E•+T,$], [E→E•+T,+]}
        let i1 = expand(&goto(&builder, &i0, Symbol::NonTerminal(1)));
        assert_eq!(
            i1,
            BTreeSet::from([item(0, 1, 0), item(1, 1, 0), item(1, 1, 1)])
        );
    }

    #[test]
    fn goto_on_n_from_initial() {
        let grammar = test_grammar();
        let first = FirstSet::compute(&grammar.productions);
        let builder = Builder::new(&grammar, &first);
        let i0 = initial_state(&builder);

        // goto(I0, n) → {[T→n•,$], [T→n•,+]}
        let i3 = expand(&goto(&builder, &i0, Symbol::Terminal(2)));
        assert_eq!(i3, BTreeSet::from([item(3, 1, 0), item(3, 1, 1)]));
    }

    #[test]
    fn goto_on_plus_from_after_e() {
        let grammar = test_grammar();
        let first = FirstSet::compute(&grammar.productions);
        let builder = Builder::new(&grammar, &first);
        let i0 = initial_state(&builder);
        let i1 = goto(&builder, &i0, Symbol::NonTerminal(1));

        // goto(I1, Plus) → {[E→E+•T,$], [E→E+•T,+], [T→•n,$], [T→•n,+]}
        let i4 = expand(&goto(&builder, &i1, Symbol::Terminal(1)));
        assert_eq!(
            i4,
            BTreeSet::from([item(1, 2, 0), item(1, 2, 1), item(3, 0, 0), item(3, 0, 1)])
        );
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

    #[test]
    fn state_numbering_is_deterministic() {
        let grammar = test_grammar();
        let first = FirstSet::compute(&grammar.productions);
        let a = build_automaton(&grammar, &first).unwrap();
        let b = build_automaton(&grammar, &first).unwrap();
        assert_eq!(a.states, b.states);
        assert_eq!(a.transitions, b.transitions);
        // Breadth-first, terminals before nonterminals: `Plus` is not
        // shiftable from the initial state, so `n` leads to state 1.
        assert_eq!(a.transitions[&(0, Symbol::Terminal(2))], 1);
        assert_eq!(a.transitions[&(0, Symbol::NonTerminal(1))], 2);
        assert_eq!(a.transitions[&(0, Symbol::NonTerminal(2))], 3);
    }

    #[test]
    fn matches_reference_on_simple_grammar() {
        assert_matches_reference(&test_grammar());
    }

    #[test]
    fn matches_reference_on_ambiguous_expressions() {
        use Symbol::{NonTerminal as N, Terminal as T};
        // E → E + E | E * E | - E | ( E ) | n
        assert_matches_reference(&grammar_of(
            7,
            2,
            &[
                (0, &[N(1)]),
                (1, &[N(1), T(1), N(1)]),
                (1, &[N(1), T(2), N(1)]),
                (1, &[T(3), N(1)]),
                (1, &[T(4), N(1), T(5)]),
                (1, &[T(6)]),
            ],
        ));
    }

    #[test]
    fn matches_reference_with_nullable_rules() {
        use Symbol::{NonTerminal as N, Terminal as T};
        // S → L O ;     L → ε | L x     O → ε | y     plus a nullable chain
        // P → O O L, whose lookaheads flow through several empty suffixes.
        assert_matches_reference(&grammar_of(
            4,
            5,
            &[
                (0, &[N(1)]),
                (1, &[N(2), N(3), T(1)]),
                (1, &[N(4), T(1)]),
                (2, &[]),
                (2, &[N(2), T(2)]),
                (3, &[]),
                (3, &[T(3)]),
                (4, &[N(3), N(3), N(2)]),
            ],
        ));
    }

    #[test]
    fn matches_reference_on_lr1_not_lalr_grammar() {
        use Symbol::{NonTerminal as N, Terminal as T};
        // S → a E a | b E b | a F b | b F a     E → e     F → e
        let grammar = grammar_of(
            4,
            4,
            &[
                (0, &[N(1)]),
                (1, &[T(1), N(2), T(1)]),
                (1, &[T(2), N(2), T(2)]),
                (1, &[T(1), N(3), T(2)]),
                (1, &[T(2), N(3), T(1)]),
                (2, &[T(3)]),
                (3, &[T(3)]),
            ],
        );
        assert_matches_reference(&grammar);

        // The states after `a e` and `b e` have the same items with different
        // lookaheads: they must stay distinct.
        let first = FirstSet::compute(&grammar.productions);
        let automaton = build_automaton(&grammar, &first).unwrap();
        let after_a = automaton.transitions[&(0, T(1))];
        let after_b = automaton.transitions[&(0, T(2))];
        assert_ne!(
            automaton.transitions[&(after_a, T(3))],
            automaton.transitions[&(after_b, T(3))]
        );
    }

    #[test]
    fn matches_reference_with_right_recursion_and_many_terminals() {
        use Symbol::{NonTerminal as N, Terminal as T};
        // More than 64 terminals, to cross a word boundary of the bit sets.
        // S → A t70     A → t1 A | t65 A | t69
        assert_matches_reference(&grammar_of(
            71,
            3,
            &[
                (0, &[N(1)]),
                (1, &[N(2), T(70)]),
                (2, &[T(1), N(2)]),
                (2, &[T(65), N(2)]),
                (2, &[T(69)]),
            ],
        ));
    }
}
