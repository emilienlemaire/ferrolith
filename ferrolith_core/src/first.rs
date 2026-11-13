use std::collections::{HashMap, HashSet};

use crate::grammar::{NonTerminalId, Production, Symbol, TerminalId};

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct FirstSet {
    pub(crate) first: HashMap<NonTerminalId, HashSet<TerminalId>>,
    pub(crate) nullable: HashMap<NonTerminalId, bool>,
}

impl FirstSet {
    fn new() -> Self {
        Self {
            first: HashMap::new(),
            nullable: HashMap::new(),
        }
    }

    pub fn compute(prods: &[Production]) -> FirstSet {
        let mut firsts = FirstSet::new();

        loop {
            let old_firsts = firsts.clone();

            for prod in prods {
                let mut all_nullable = true;
                if prod.rhs.is_empty() {
                    firsts
                        .nullable
                        .entry(prod.lhs)
                        .and_modify(|b| {
                            *b = true;
                        })
                        .or_insert(true);
                } else {
                    for symbol in &prod.rhs {
                        match symbol {
                            Symbol::Terminal(id) => {
                                firsts
                                    .first
                                    .entry(prod.lhs)
                                    .and_modify(|set| {
                                        set.insert(*id);
                                    })
                                    .or_insert({
                                        let mut set = HashSet::new();
                                        set.insert(*id);
                                        set
                                    });
                                all_nullable = false;
                                break;
                            }
                            Symbol::NonTerminal(id) => {
                                let first_set = match firsts.first.get(id) {
                                    Some(fs) => fs.clone(),
                                    None => HashSet::new(),
                                };
                                firsts
                                    .first
                                    .entry(prod.lhs)
                                    .and_modify(|set| {
                                        set.extend(&first_set);
                                    })
                                    .or_insert(first_set);
                                let nullable = firsts.nullable.get(id).copied().unwrap_or(false);
                                if !nullable {
                                    all_nullable = false;
                                    break;
                                }
                            }
                        }
                    }
                }
                if all_nullable {
                    firsts
                        .nullable
                        .entry(prod.lhs)
                        .and_modify(|nullable| *nullable = true)
                        .or_insert(true);
                }
            }

            if firsts == old_firsts {
                break firsts;
            }
        }
    }

    pub(crate) fn first_of_sequence(
        &self,
        sequence: &[Symbol],
        lookahead: TerminalId,
    ) -> HashSet<TerminalId> {
        let mut set = HashSet::new();
        let mut all_nullable = true;
        for symbol in sequence {
            match symbol {
                Symbol::Terminal(id) => {
                    all_nullable = false;
                    set.insert(*id);
                    break;
                }
                Symbol::NonTerminal(id) => {
                    if let Some(fs) = self.first.get(id) {
                        set.extend(fs);
                    }
                    match self.nullable.get(id) {
                        Some(true) => continue,
                        _ => {
                            all_nullable = false;
                            break;
                        }
                    }
                }
            };
        }

        if all_nullable {
            set.insert(lookahead);
        }
        set
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar::Production;

    // Grammar used across all tests:
    //   0: S' → E          (lhs=0)
    //   1: E  → E Plus T   (lhs=1)
    //   2: E  → T          (lhs=1)
    //   3: T  → n          (lhs=2)
    // Terminals:    0=EOF, 1=Plus, 2=n
    // Nonterminals: 0=S',  1=E,    2=T
    fn prods() -> Vec<Production> {
        vec![
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
        ]
    }

    #[test]
    fn compute_first_nonterminals() {
        let firsts = FirstSet::compute(&prods());
        // FIRST(E) = FIRST(T) = {n=2}
        assert_eq!(firsts.first[&1], HashSet::from([2]));
        assert_eq!(firsts.first[&2], HashSet::from([2]));
    }

    #[test]
    fn compute_no_nullable_nonterminals() {
        let firsts = FirstSet::compute(&prods());
        assert!(!firsts.nullable.get(&1).copied().unwrap_or(false));
        assert!(!firsts.nullable.get(&2).copied().unwrap_or(false));
    }

    #[test]
    fn compute_nullable_epsilon_production() {
        let prods = vec![Production {
            lhs: 0,
            rhs: vec![],
            action: Some(0),
            prec: None,
        }];
        let firsts = FirstSet::compute(&prods);
        assert!(firsts.nullable[&0]);
    }

    #[test]
    fn first_of_sequence_empty_returns_lookahead() {
        let firsts = FirstSet::compute(&prods());
        assert_eq!(firsts.first_of_sequence(&[], 0), HashSet::from([0]));
    }

    #[test]
    fn first_of_sequence_leading_terminal() {
        let firsts = FirstSet::compute(&prods());
        // FIRST([n, …]) = {n=2}, lookahead irrelevant
        let set = firsts.first_of_sequence(&[Symbol::Terminal(2)], 0);
        assert_eq!(set, HashSet::from([2]));
    }

    #[test]
    fn first_of_sequence_nonnullable_nonterminal_stops_chain() {
        let firsts = FirstSet::compute(&prods());
        // FIRST([E, Plus], lookahead=0): E is not nullable, so only FIRST(E)={n=2}
        let set = firsts.first_of_sequence(&[Symbol::NonTerminal(1), Symbol::Terminal(1)], 0);
        assert_eq!(set, HashSet::from([2]));
    }

    #[test]
    fn first_of_sequence_nullable_nonterminal_continues_chain() {
        // A → ε, so FIRST([A, x=T(1)], lookahead=0) = FIRST(A) ∪ FIRST(x) = {} ∪ {1} = {1}
        let prods = vec![Production {
            lhs: 0,
            rhs: vec![],
            action: Some(0),
            prec: None,
        }];
        let firsts = FirstSet::compute(&prods);
        let set = firsts.first_of_sequence(&[Symbol::NonTerminal(0), Symbol::Terminal(1)], 0);
        assert!(set.contains(&1));
        assert!(!set.contains(&0)); // lookahead not added: chain terminates at the terminal
    }
}
