pub use ferrolith_macro::grammar;

pub mod action {
    pub use ferrolith_core::table::Action;
}

pub enum ParseError<T> {
    UnexpectedToken(T),
    UnexpectedEof,
}

