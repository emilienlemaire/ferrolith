use std::collections::HashMap;

use ferrolith_core::grammar::{Assoc, PrecLevel, Production, Symbol, TerminalId};
use syn::{Token, parse::Parse};

use crate::internalize::app_ident;

#[derive(Clone)]
pub(super) enum GrammarSymbol {
    Bound {
        name: syn::Ident,
        sym: Box<GrammarSymbol>,
    },
    Rule(syn::Ident),
    Token(syn::Ident),
    Capture {
        variant: syn::Ident,
        fields: Vec<syn::Ident>,
    },
    App {
        name: syn::Ident,
        args: Vec<GrammarSymbol>,
    },
}

#[derive(Clone)]
pub(super) struct Arm {
    pub(super) attrs: Vec<syn::Attribute>,
    pub(super) terms: Vec<GrammarSymbol>,
    pub(super) action: syn::Expr,
}

#[derive(Clone)]
pub(super) struct Rule {
    pub(super) attrs: Vec<syn::Attribute>,
    pub(super) name: syn::Ident,
    pub(super) parameters: Vec<syn::Ident>,
    pub(super) ret_type: Option<syn::Type>,
    pub(super) arms: Vec<Arm>,
}

pub(super) struct PrecedenceDecl {
    pub(super) associativity: Assoc,
    pub(super) tokens: Vec<syn::Ident>,
}

#[derive(Default)]
pub(super) struct ParserCfg {
    pub(super) expose_automaton: bool,
    pub(super) clonable_state: bool,
}

impl ParserCfg {
    fn apply(&mut self, attr: &syn::Attribute) -> syn::Result<()> {
        let opts = attr.parse_args_with(
            syn::punctuated::Punctuated::<syn::Ident, syn::Token![,]>::parse_terminated,
        )?;
        for opt in opts {
            match opt.to_string().as_str() {
                "expose_automaton" => self.expose_automaton = true,
                "clonable_state" => self.clonable_state = true,
                other => return Err(syn::Error::new(
                    opt.span(),
                    format!("unknown parser_cfg option `{other}`"),
                )),
            }
        }
        Ok(())
    }
}

pub(super) struct Grammar {
    pub(super) tokens_type: syn::ItemType,
    pub(super) precedences: Vec<PrecedenceDecl>,
    pub(super) rules: Vec<Rule>,
    pub(super) cfg: ParserCfg,
}

fn partition_attrs(
    attrs: Vec<syn::Attribute>,
    precs: &mut Vec<PrecedenceDecl>,
) -> syn::Result<Vec<syn::Attribute>> {
    let mut rule_attrs = Vec::new();
    for attr in attrs {
        match PrecedenceDecl::try_from(&attr) {
            Ok(prec) => precs.push(prec),
            Err(PrecParseError::NotPrec) => rule_attrs.push(attr),
            Err(PrecParseError::InvalidPrec(e)) => return Err(e),
        }
    }
    Ok(rule_attrs)
}

impl syn::parse::Parse for Grammar {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut token_type = None;
        let mut precs = vec![];
        let mut rules = vec![];
        let mut cfg = ParserCfg::default();

        for attr in input.call(syn::Attribute::parse_inner)? {
            if attr.path().is_ident("parser_cfg") {
                cfg.apply(&attr)?;
            } else {
                return Err(syn::Error::new_spanned(&attr, "unknown inner attribute in grammar!"));
            }
        }

        loop {
            if input.is_empty() {
                break Ok::<(), syn::Error>(());
            }

            let attrs = input.call(syn::Attribute::parse_outer)?;

            if input.peek(syn::Token![type]) {
                token_type = Some(input.parse::<syn::ItemType>()?);
            } else if input.peek(syn::Token![fn]) {
                let rule_attrs = partition_attrs(attrs, &mut precs)?;
                rules.push(Rule::parse_with_attrs(rule_attrs, input)?);
            } else if !attrs.is_empty() {
                let leftover = partition_attrs(attrs, &mut precs)?;
                if !leftover.is_empty() {
                    return Err(syn::Error::new_spanned(
                        &leftover[0],
                        "unexpected attribute outside of a rule",
                    ));
                }
            } else {
                return Err(input.error("expected `type`, `fn`, or `#[...]`"));
            }
        }?;

        let tokens_type = token_type.ok_or_else(|| input.error("missing `type Token = ...`"))?;

        Ok(Grammar {
            tokens_type,
            precedences: precs,
            rules,
            cfg,
        })
    }
}

impl Rule {
    fn parse_with_attrs(
        attrs: Vec<syn::Attribute>,
        input: syn::parse::ParseStream,
    ) -> syn::Result<Self> {
        input.parse::<syn::Token![fn]>()?;

        let name = input.call(syn::Ident::parse)?;

        let params_content;
        syn::parenthesized!(params_content in input);
        let parameters: Vec<syn::Ident> =
            syn::punctuated::Punctuated::<syn::Ident, syn::Token![,]>::parse_terminated(
                &params_content,
            )?
            .into_iter()
            .collect();

        let ret_type = match input.parse::<syn::ReturnType>()? {
            syn::ReturnType::Default => None,
            syn::ReturnType::Type(_, ty) => Some(*ty),
        };

        let inner;
        syn::braced!(inner in input);

        let mut arms = vec![];
        while !inner.is_empty() {
            arms.push(inner.parse::<Arm>()?);
        }

        Ok(Rule {
            attrs,
            name,
            parameters,
            ret_type,
            arms,
        })
    }
}

impl syn::parse::Parse for Rule {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let attrs = input.call(syn::Attribute::parse_outer)?;
        Rule::parse_with_attrs(attrs, input)
    }
}

impl syn::parse::Parse for Arm {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let attrs = input.call(syn::Attribute::parse_outer)?;

        let terms;
        syn::parenthesized!(terms in input);

        let terms: Vec<_> =
            syn::punctuated::Punctuated::<GrammarSymbol, syn::Token![,]>::parse_terminated(&terms)?
                .into_iter()
                .collect();

        input.parse::<syn::Token![=>]>()?;

        let action = input.parse::<syn::Expr>()?;

        if input.peek(syn::Token![,]) {
            input.parse::<syn::Token![,]>()?;
        }

        Ok(Arm {
            attrs,
            terms,
            action,
        })
    }
}

impl syn::parse::Parse for GrammarSymbol {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let ident = input.parse::<syn::Ident>()?;

        if input.peek(syn::Token![@]) {
            input.parse::<syn::Token![@]>()?;
            let sym = input.parse::<GrammarSymbol>()?;
            return Ok(GrammarSymbol::Bound {
                name: ident,
                sym: Box::new(sym),
            });
        }

        if input.peek(syn::token::Paren) {
            let content;
            syn::parenthesized!(content in input);

            if ident.to_string().starts_with(|c: char| c.is_uppercase()) {
                let fields: Vec<_> =
                    syn::punctuated::Punctuated::<syn::Ident, syn::Token![,]>::parse_terminated(
                        &content,
                    )?
                    .into_iter()
                    .collect();
                return Ok(GrammarSymbol::Capture {
                    variant: ident,
                    fields,
                });
            } else {
                let args =
                    syn::punctuated::Punctuated::<GrammarSymbol, syn::Token![,]>::parse_terminated(
                        &content,
                    )?
                    .into_iter()
                    .collect();
                return Ok(GrammarSymbol::App { name: ident, args });
            }
        }

        if ident.to_string().starts_with(|c: char| c.is_uppercase()) {
            Ok(GrammarSymbol::Token(ident))
        } else {
            Ok(GrammarSymbol::Rule(ident))
        }
    }
}

pub(super) enum PrecParseError {
    NotPrec,
    InvalidPrec(syn::Error),
}

impl From<syn::Error> for PrecParseError {
    fn from(value: syn::Error) -> Self {
        Self::InvalidPrec(value)
    }
}

impl TryFrom<&syn::Attribute> for PrecedenceDecl {
    type Error = PrecParseError;

    fn try_from(value: &syn::Attribute) -> Result<Self, Self::Error> {
        match &value.meta {
            syn::Meta::List(syn::MetaList { path, .. }) => {
                if let Ok(ident) = path.require_ident() {
                    let assoc = match ident.to_string().as_str() {
                        "left" => Assoc::Left,
                        "right" => Assoc::Right,
                        "nonassoc" => Assoc::NonAssoc,
                        _ => return Err(PrecParseError::NotPrec),
                    };
                    let tokens: Vec<_> = value
                        .parse_args_with(
                            syn::punctuated::Punctuated::<syn::Ident, Token![,]>::parse_terminated,
                        )?
                        .into_iter()
                        .collect();
                    Ok(PrecedenceDecl {
                        associativity: assoc,
                        tokens,
                    })
                } else {
                    Err(PrecParseError::NotPrec)
                }
            }
            syn::Meta::Path(path) => {
                if let Ok(ident) = path.require_ident() {
                    match ident.to_string().as_str() {
                        "left" | "right" | "nonassoc" => {
                            Err(PrecParseError::InvalidPrec(syn::Error::new(
                                ident.span(),
                                "The precedence declaration requires tokens.",
                            )))
                        }
                        _ => Err(PrecParseError::NotPrec),
                    }
                } else {
                    Err(PrecParseError::NotPrec)
                }
            }
            syn::Meta::NameValue(syn::MetaNameValue { path, .. }) => {
                if let Ok(ident) = path.require_ident() {
                    match ident.to_string().as_str() {
                        "left" | "right" | "nonassoc" => {
                            Err(PrecParseError::InvalidPrec(syn::Error::new(
                                ident.span(),
                                "The precedence declaration requires tokens.",
                            )))
                        }
                        _ => Err(PrecParseError::NotPrec),
                    }
                } else {
                    Err(PrecParseError::NotPrec)
                }
            }
        }
    }
}

impl Arm {
    pub(super) fn into_production(
        &self,
        nonterminal_id: usize,
        action_id: usize,
        nonterminals: &HashMap<syn::Ident, usize>,
        terminals: &HashMap<syn::Ident, usize>,
        prec_table: &HashMap<TerminalId, (PrecLevel, Assoc)>,
    ) -> syn::Result<Production> {
        let rhs = self.flatten_symbols(nonterminals, terminals)?;
        let prec = self.resolve_prec(&rhs, prec_table, terminals)?;
        Ok(Production {
            lhs: nonterminal_id,
            rhs,
            prec,
            action: Some(action_id),
        })
    }

    fn resolve_prec(
        &self,
        rhs: &[Symbol],
        prec_table: &HashMap<TerminalId, (PrecLevel, Assoc)>,
        terminals: &HashMap<syn::Ident, usize>,
    ) -> syn::Result<Option<PrecLevel>> {
        match self.attrs.iter().find_map(|attr| {
            if attr.path().is_ident("prec".into()) {
                match attr.parse_args_with(syn::Ident::parse) {
                    Ok(ident) => match terminals.get(&ident) {
                        Some(id) => match prec_table.get(id) {
                            Some((level, _)) => Some(Ok(level)),
                            None => Some(Err(syn::Error::new(
                                ident.span(),
                                format!(
                                    "The `prec` attribute was used on a symbol without any \
                                    precedence declared: `{ident}`."
                                ),
                            ))),
                        },
                        None => Some(Err(syn::Error::new(
                            ident.span(),
                            format!(
                                "The `prec` attribute was used on a symbol without any \
                                        precedence declared: `{ident}`."
                            ),
                        ))),
                    },
                    Err(e) => Some(Err(e)),
                }
            } else {
                None
            }
        }) {
            Some(Ok(prec_level)) => Ok(Some(*prec_level)),
            Some(Err(e)) => Err(e),
            None => {
                match rhs.iter().rev().find_map(|symbol| match symbol {
                    Symbol::Terminal(id) => match prec_table.get(id) {
                        Some((prec_level, _)) => Some(prec_level),
                        None => None,
                    },
                    _ => None,
                }) {
                    Some(v) => Ok(Some(*v)),
                    _ => Ok(None),
                }
            }
        }
    }

    fn flatten_symbols(
        &self,
        nonterminals: &HashMap<syn::Ident, usize>,
        terminals: &HashMap<syn::Ident, usize>,
    ) -> syn::Result<Vec<Symbol>> {
        fn flatten_symbol(
            symbol: &GrammarSymbol,
            nonterminals: &HashMap<syn::Ident, usize>,
            terminals: &HashMap<syn::Ident, usize>,
        ) -> syn::Result<Symbol> {
            match symbol {
                GrammarSymbol::Bound { sym, .. } => flatten_symbol(sym, nonterminals, terminals),
                GrammarSymbol::Rule(ident) => {
                    let id = nonterminals.get(&ident).ok_or_else(|| {
                        syn::Error::new(
                            ident.span(),
                            format!("Failed to find non-terminal id for `{ident}`"),
                        )
                    })?;
                    Ok(Symbol::NonTerminal(*id))
                }
                GrammarSymbol::Token(ident) => {
                    let id = terminals.get(&ident).ok_or_else(|| {
                        syn::Error::new(
                            ident.span(),
                            format!("Failed to find terminal id for `{ident}`"),
                        )
                    })?;
                    Ok(Symbol::Terminal(*id))
                }
                GrammarSymbol::Capture { variant, .. } => {
                    let id = terminals.get(&variant).ok_or_else(|| {
                        syn::Error::new(
                            variant.span(),
                            format!("Failed to find terminal id for `{variant}`"),
                        )
                    })?;
                    Ok(Symbol::Terminal(*id))
                }
                GrammarSymbol::App { name, args } => {
                    let ident = app_ident(name, args);
                    let id = nonterminals.get(&ident).ok_or_else(|| {
                        syn::Error::new(name.span(), format!("Unexpanded application `{name}`"))
                    })?;
                    Ok(Symbol::NonTerminal(*id))
                }
            }
        }

        let mut symbols = vec![];
        for term in &self.terms {
            let symbol = flatten_symbol(&term, nonterminals, terminals)?;
            symbols.push(symbol);
        }

        Ok(symbols)
    }
}
