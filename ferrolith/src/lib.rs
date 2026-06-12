pub use ferrolith_macro::grammar;

pub mod action {
    pub use ferrolith_core::table::Action;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Symbol {
    Terminal(usize),
    Nonterminal(usize),
}

impl Symbol {
    pub fn name(self, view: &AutomatonView) -> &'static str {
        match self {
            Symbol::Terminal(i) => view.terminals[i],
            Symbol::Nonterminal(i) => view.nonterminals[i],
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Symbol::Terminal(_))
    }

    pub fn is_nonterminal(self) -> bool {
        matches!(self, Symbol::Nonterminal(_))
    }
}

pub struct ProductionInfo {
    pub lhs: usize,
    pub rhs: &'static [Symbol],
}

pub struct StateInfo {
    pub incoming: Option<Symbol>,
}

pub struct AutomatonView {
    pub n_states: usize,
    pub terminals: &'static [&'static str],
    pub nonterminals: &'static [&'static str],
    pub productions: &'static [ProductionInfo],
    pub states: &'static [StateInfo],
}

pub enum ParseError<T> {
    UnexpectedToken(T),
    UnexpectedEof,
}

impl<T> std::fmt::Debug for ParseError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnexpectedToken(_) => write!(f, "ParseError::UnexpectedToken(..)"),
            ParseError::UnexpectedEof => write!(f, "ParseError::UnexpectedEof"),
        }
    }
}
