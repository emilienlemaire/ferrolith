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
    UnexpectedToken {
        token: T,
        expected: Vec<&'static str>,
    },
    UnexpectedEof {
        expected: Vec<&'static str>,
    },
}

impl<T> std::fmt::Debug for ParseError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnexpectedToken { expected, .. } => f
                .debug_struct("UnexpectedToken")
                .field("expected", expected)
                .finish_non_exhaustive(),
            ParseError::UnexpectedEof { expected } => f
                .debug_struct("UnexpectedEof")
                .field("expected", expected)
                .finish(),
        }
    }
}

impl<T: std::fmt::Display> std::fmt::Display for ParseError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnexpectedToken { token, expected } => {
                write!(f, "unexpected token {token}")?;
                if !expected.is_empty() {
                    write!(f, ", expected one of: {}", expected.join(", "))?;
                }
                Ok(())
            }
            ParseError::UnexpectedEof { expected } => {
                write!(f, "unexpected end of input")?;
                if !expected.is_empty() {
                    write!(f, ", expected one of: {}", expected.join(", "))?;
                }
                Ok(())
            }
        }
    }
}

impl<T: std::fmt::Display> std::error::Error for ParseError<T> {}
