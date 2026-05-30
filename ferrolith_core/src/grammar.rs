use std::collections::HashMap;

pub type TerminalId = usize;
pub type NonTerminalId = usize;
pub type ProductionId = usize;
pub type StateId = usize;
pub type PrecLevel = usize;
pub type ActionId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Symbol {
    Terminal(TerminalId),
    NonTerminal(NonTerminalId),
}

#[derive(Debug, Clone, Copy)]
pub enum Assoc {
    Left,
    Right,
    NonAssoc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Production {
    pub lhs: NonTerminalId,
    pub rhs: Vec<Symbol>,
    pub action: Option<ActionId>,
    pub prec: Option<PrecLevel>,
}

pub struct InternalGrammar {
    pub terminals: Vec<syn::Ident>,
    pub nonterminals: Vec<syn::Ident>,
    pub productions: Vec<Production>,
    pub start: NonTerminalId,
    pub prec_table: HashMap<TerminalId, (PrecLevel, Assoc)>,
    pub ret_types: Vec<Option<syn::Type>>,
    pub actions_order: Vec<(syn::Ident, ActionId)>,
}
