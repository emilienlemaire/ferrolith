use std::collections::HashMap;

use ferrolith_core::grammar::{Assoc, PrecLevel, Production, Symbol, TerminalId};
use quote::format_ident;
use syn::{Token, parse::Parse, spanned::Spanned};

use crate::internalize::app_ident;

#[derive(Clone)]
pub(super) enum GrammarSymbol {
    Bound {
        name: syn::Ident,
        mutable: bool,
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
    ErrorToken,
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
pub(super) enum MergeStrategy {
    #[default]
    FirstLast,
    Fold,
    First,
    Last,
}

#[derive(Default)]
pub(super) struct ParserCfg {
    pub(super) expose_automaton: bool,
    pub(super) clonable_state: bool,
    pub(super) payload_merge: Option<syn::Path>,
    pub(super) payload_merge_strategy: MergeStrategy,
    pub(super) payload_default: Option<syn::Expr>,
    /// Set by the `no_payload_default` flag; the span points at the flag.
    pub(super) no_payload_default: Option<proc_macro2::Span>,
}

impl ParserCfg {
    fn apply(&mut self, attr: &syn::Attribute) -> syn::Result<()> {
        let opts = attr.parse_args_with(
            syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
        )?;
        let mut err: Option<syn::Error> = None;
        for opt in opts {
            match opt {
                syn::Meta::Path(path) => {
                    if let Ok(ident) = path.require_ident() {
                        match ident.to_string().as_str() {
                            "expose_automaton" => self.expose_automaton = true,
                            "clonable_state" => self.clonable_state = true,
                            "no_payload_default" => self.no_payload_default = Some(ident.span()),
                            other => {
                                let new_err = syn::Error::new(
                                    ident.span(),
                                    format!("unknown parser_cfg option `{other}`"),
                                );
                                if let Some(ref mut err) = err {
                                    err.combine(new_err);
                                } else {
                                    err = Some(new_err);
                                }
                            }
                        }
                    } else {
                        let new_err =
                            syn::Error::new(path.span(), format!("unknown parser_cfg option"));
                        if let Some(ref mut err) = err {
                            err.combine(new_err);
                        } else {
                            err = Some(new_err);
                        }
                    }
                }
                syn::Meta::NameValue(ref name_value) => {
                    let name_path = &name_value.path;
                    if let Ok(name) = name_path.require_ident() {
                        match name.to_string().to_lowercase().as_str() {
                            "payload_merge" => match &name_value.value {
                                syn::Expr::Path(path) => {
                                    self.payload_merge = Some(path.path.clone());
                                }
                                _ => {
                                    let new_err = syn::Error::new(
                                        name_value.span(),
                                        String::from("expecting a path as `payload_merge` value"),
                                    );
                                    if let Some(ref mut err) = err {
                                        err.combine(new_err);
                                    } else {
                                        err = Some(new_err);
                                    }
                                }
                            },
                            "payload_merge_strategy" => match &name_value.value {
                                syn::Expr::Path(path)
                                    if let Ok(path) = path.path.require_ident()
                                        && matches!(
                                            path.to_string().to_lowercase().as_str(),
                                            "first_last" | "fold" | "first" | "last"
                                        ) =>
                                {
                                    self.payload_merge_strategy =
                                        match path.to_string().to_lowercase().as_str() {
                                            "first_last" => MergeStrategy::FirstLast,
                                            "fold" => MergeStrategy::Fold,
                                            "first" => MergeStrategy::First,
                                            "last" => MergeStrategy::Last,
                                            _ => unreachable!(),
                                        }
                                }
                                _ => {
                                    let new_err = syn::Error::new(
                                        name_value.span(),
                                        String::from(
                                            "expecting one of `first_last`, `fold`, `first`, or \
                                            `last` for `payload_merge_strategy`.",
                                        ),
                                    );
                                    if let Some(ref mut err) = err {
                                        err.combine(new_err);
                                    } else {
                                        err = Some(new_err);
                                    }
                                }
                            },
                            "payload_default" => {
                                self.payload_default = Some(name_value.value.clone())
                            }
                            other => {
                                let new_err = syn::Error::new(
                                    name.span(),
                                    format!("unknown parser_cfg option `{other}`"),
                                );
                                if let Some(ref mut err) = err {
                                    err.combine(new_err);
                                } else {
                                    err = Some(new_err);
                                }
                            }
                        }
                    } else {
                        let new_err = syn::Error::new(
                            name_value.span(),
                            format!("unknown parser_cfg option"),
                        );
                        if let Some(ref mut err) = err {
                            err.combine(new_err);
                        } else {
                            err = Some(new_err);
                        }
                    }
                }
                syn::Meta::List(list) => {
                    let new_err =
                        syn::Error::new(list.span(), format!("unknown parser_cfg option"));
                    if let Some(ref mut err) = err {
                        err.combine(new_err);
                    } else {
                        err = Some(new_err);
                    }
                }
            }
        }

        if let Some(err) = err {
            Err(err)
        } else {
            Ok(())
        }
    }
}

pub(super) struct Grammar {
    pub(super) tokens_type: syn::Type,
    pub(super) payload_type: Option<syn::Type>,
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
        let mut payload_type = None;
        let mut precs = vec![];
        let mut rules = vec![];
        let mut cfg = ParserCfg::default();

        for attr in input.call(syn::Attribute::parse_inner)? {
            if attr.path().is_ident("parser_cfg") {
                cfg.apply(&attr)?;
            } else {
                return Err(syn::Error::new_spanned(
                    &attr,
                    "unknown inner attribute in grammar!",
                ));
            }
        }

        if let (Some(span), Some(_)) = (cfg.no_payload_default, &cfg.payload_default) {
            return Err(syn::Error::new(
                span,
                "`no_payload_default` cannot be combined with `payload_default = ...`",
            ));
        }

        loop {
            if input.is_empty() {
                break Ok::<(), syn::Error>(());
            }

            let attrs = input.call(syn::Attribute::parse_outer)?;

            if input.peek(syn::Token![type]) {
                input.parse::<syn::Token![type]>()?;
                let ident = input.parse::<syn::Ident>()?;
                input.parse::<syn::Token![=]>()?;
                let ty = input.parse::<syn::Type>()?;
                input.parse::<syn::Token![;]>()?;
                match ident.to_string().as_str() {
                    "Token" => token_type = Some(ty),
                    "Payload" => payload_type = Some(ty),
                    _ => {
                        return Err(syn::Error::new(
                            ident.span(),
                            "Only `Token` and `Payload` type aliases are allowed in the `grammar!` macro.",
                        ));
                    }
                }
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
            payload_type,
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
        let mutable = if input.peek(syn::Token![mut]) {
            input.parse::<syn::Token![mut]>()?;
            true
        } else {
            false
        };
        let ident = input.parse::<syn::Ident>()?;

        if input.peek(syn::Token![@]) {
            input.parse::<syn::Token![@]>()?;
            let sym = input.parse::<GrammarSymbol>()?;
            return Ok(GrammarSymbol::Bound {
                name: ident,
                mutable,
                sym: Box::new(sym),
            });
        }

        if mutable {
            return Err(input.error("`mut` is only valid before a binding: `mut name @ sym`"));
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
        } else if ident.to_string() == "error" {
            Ok(GrammarSymbol::ErrorToken)
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
        prec_by_name: &HashMap<syn::Ident, PrecLevel>,
    ) -> syn::Result<Production> {
        let rhs = self.flatten_symbols(nonterminals, terminals)?;
        let prec = self.resolve_prec(&rhs, prec_table, prec_by_name)?;
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
        prec_by_name: &HashMap<syn::Ident, PrecLevel>,
    ) -> syn::Result<Option<PrecLevel>> {
        if let Some(attr) = self.attrs.iter().find(|a| a.path().is_ident("prec")) {
            let ident: syn::Ident = attr.parse_args()?;
            return match prec_by_name.get(&ident) {
                Some(level) => Ok(Some(*level)),
                None => Err(syn::Error::new(
                    ident.span(),
                    format!(
                        "`#[prec({ident})]`: no precedence declared for `{ident}`; \
                         add it to a `#[left]`, `#[right]` or `#[nonassoc]` declaration"
                    ),
                )),
            };
        }
        Ok(rhs.iter().rev().find_map(|s| match s {
            Symbol::Terminal(id) => prec_table.get(id).map(|(l, _)| *l),
            _ => None,
        }))
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
                GrammarSymbol::ErrorToken => {
                    let ident = format_ident!("__error__");
                    let id = terminals.get(&ident).ok_or_else(|| {
                        syn::Error::new(
                            ident.span(),
                            format!("Unable to find the `__error__` symbol"),
                        )
                    })?;
                    Ok(Symbol::Terminal(*id))
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
