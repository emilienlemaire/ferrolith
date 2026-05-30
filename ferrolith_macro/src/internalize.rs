use std::collections::HashMap;

use ferrolith_core::grammar::{InternalGrammar, Production, Symbol};
use proc_macro2::Span;
use quote::format_ident;
use syn::visit_mut::{self, VisitMut};

use crate::{
    Grammar,
    grammar::{Arm, GrammarSymbol, Rule},
};

struct TypeSubst<'a>(&'a [(syn::Ident, syn::Type)]);

type RuleSet = HashMap<syn::Ident, Rule>;

pub(super) fn internalize(grammar: &Grammar) -> syn::Result<InternalGrammar> {
    let mut terminals = HashMap::new();
    let mut nonterminals = HashMap::new();
    let mut rule_set = RuleSet::new();
    let mut actions: usize = 0;

    let mut terminal_id = 1usize;
    let mut nonterminal_id = 1usize;

    for rule in &grammar.rules {
        rule_set.entry(rule.name.clone()).or_insert(rule.clone());
        for arm in &rule.arms {
            for symbol in &arm.terms {
                let terms = get_symbol_terminal(symbol);
                for term in terms {
                    terminals.entry(term).or_insert_with(|| {
                        let curr_id = terminal_id;
                        terminal_id += 1;
                        curr_id
                    });
                }
            }
        }
    }

    desargue_applications(&mut rule_set)?;

    for rule in rule_set.values() {
        if rule.parameters.len() == 0 {
            nonterminals.entry(rule.name.clone()).or_insert_with(|| {
                let curr_id = nonterminal_id;
                nonterminal_id += 1;
                curr_id
            });
        }
    }

    let mut prods = vec![];

    let mut prec_table = HashMap::new();
    let mut prec_level = 1usize;

    for prec in &grammar.precedences {
        for token in &prec.tokens {
            match terminals.get(&token) {
                Some(id) => {
                    prec_table.entry(*id).or_insert_with(|| {
                        let curr_level = prec_level;
                        (curr_level, prec.associativity)
                    });
                }
                None => {
                    return Err(syn::Error::new(
                        token.span(),
                        format!(
                            "Trying to declare precedence for a non-terminal symbol: `{token}`"
                        ),
                    ));
                }
            }
        }
        prec_level += 1;
    }

    let mut actions_order = vec![];

    for rule in rule_set.values() {
        for (arm_idx, arm) in rule.arms.iter().enumerate() {
            if rule.parameters.len() == 0 {
                if let Some(nonterminal_id) = nonterminals.get(&rule.name) {
                    let prod = arm.into_production(
                        *nonterminal_id,
                        actions,
                        &nonterminals,
                        &terminals,
                        &prec_table,
                    )?;
                    actions_order.push((rule.name.clone(), arm_idx));
                    actions += 1;
                    prods.push(prod);
                } else {
                    return Err(syn::Error::new(
                        rule.name.span(),
                        format!("Failed to find non-terminal id for `{}`", rule.name),
                    ));
                }
            } else {
                continue;
            }
        }
    }

    let start_rule: Vec<_> = rule_set
        .values()
        .filter(|rule| {
            rule.attrs.iter().any(|attr| {
                attr.path()
                    .is_ident(&syn::Ident::new("start", rule.name.span()))
            })
        })
        .collect();

    if start_rule.len() != 1 {
        if start_rule.len() > 1 {
            return Err(syn::Error::new(
                Span::call_site(),
                "Multiple starting rule found, please only use one.",
            ));
        } else {
            return Err(syn::Error::new(
                Span::call_site(),
                "No starting rule found, please add the `#[start]` attribute to your starting rule.",
            ));
        }
    }

    let start_rule = start_rule.first().unwrap(); // Already checked it existence above

    if let Some(id) = nonterminals.get(&start_rule.name) {
        let start_prod = Production {
            lhs: 0,
            rhs: vec![Symbol::NonTerminal(*id)],
            action: None,
            prec: None,
        };
        prods.insert(0, start_prod);
    } else {
        return Err(syn::Error::new(
            start_rule.name.span(),
            "Failed to find starting rule id.",
        ));
    }

    let terminals = {
        let size = terminals.values().max().unwrap_or(&0usize);
        let mut terminal_vec = vec![None; *size + 1];
        terminal_vec[0] = Some(syn::Ident::new("__eof__", Span::call_site()));
        for (ident, id) in terminals {
            terminal_vec[id] = Some(ident);
        }

        let res_vec: syn::Result<Vec<_>> = terminal_vec
            .into_iter()
            .map(|terminal| match terminal {
                Some(id) => Ok(id),
                None => Err(syn::Error::new(
                    Span::call_site(),
                    "Somehow skipped a terminal, please report this error",
                )),
            })
            .collect();
        res_vec?
    };

    let mut ret_types;
    let nonterminals = {
        let size = nonterminals.values().max().unwrap_or(&0usize);
        let mut nonterminal_vec = vec![None; *size + 1];
        ret_types = vec![None; *size + 1];
        nonterminal_vec[0] = Some(syn::Ident::new("__start__", Span::call_site()));
        for (ident, id) in nonterminals {
            if let Some(rule) = rule_set.get(&ident) {
                ret_types[id] = rule.ret_type.clone();
            }
            nonterminal_vec[id] = Some(ident);
        }

        let res_vec: syn::Result<Vec<_>> = nonterminal_vec
            .into_iter()
            .map(|nonterminal| match nonterminal {
                Some(id) => Ok(id),
                None => Err(syn::Error::new(
                    Span::call_site(),
                    "Somehow skipped a nonterminal, please report this error",
                )),
            })
            .collect();
        res_vec?
    };

    Ok(InternalGrammar {
        terminals,
        nonterminals,
        productions: prods,
        start: 0usize,
        prec_table,
        ret_types,
        actions_order,
    })
}

fn get_symbol_terminal(symbol: &GrammarSymbol) -> Vec<syn::Ident> {
    let mut terms = vec![];
    match symbol {
        GrammarSymbol::Bound { sym, .. } => terms.extend(get_symbol_terminal(sym)),
        GrammarSymbol::Rule(_) => (),
        GrammarSymbol::Token(ident) => terms.push(ident.clone()),
        GrammarSymbol::Capture { variant, .. } => terms.push(variant.clone()),
        GrammarSymbol::App { args, .. } => {
            for arg in args {
                terms.extend(get_symbol_terminal(arg));
            }
        }
    };
    terms
}

fn desargue_applications(rules: &mut RuleSet) -> syn::Result<()> {
    let mut top_level_symbols = vec![];

    for rule in rules.values() {
        for arm in &rule.arms {
            for symbol in &arm.terms {
                top_level_symbols.push(symbol.clone());
            }
        }
    }

    for symbol in top_level_symbols {
        desargue_symbol(rules, &symbol)?;
    }

    Ok(())
}

fn desargue_symbol(rules: &mut RuleSet, symbol: &GrammarSymbol) -> syn::Result<()> {
    match symbol {
        GrammarSymbol::Bound { sym, .. } => desargue_symbol(rules, sym),
        GrammarSymbol::App { name, args } => {
            for symbol in args {
                desargue_symbol(rules, symbol)?;
            }
            let new_name = app_ident(&name, &args);
            let new_rule = expand_rule(rules, &new_name, &name, &args)?;
            rules.entry(new_name).or_insert(new_rule);
            Ok(())
        }
        _ => Ok(()),
    }
}

fn expand_rule(
    rules: &RuleSet,
    new_name: &syn::Ident,
    name: &syn::Ident,
    args: &[GrammarSymbol],
) -> syn::Result<Rule> {
    match rules.get(name) {
        Some(rule) => {
            let substitutions: Vec<_> = rule
                .parameters
                .iter()
                .zip(args.iter())
                .filter_map(|(param, arg)| match arg {
                    GrammarSymbol::Rule(ident) => rules
                        .get(ident)?
                        .ret_type
                        .clone()
                        .map(|ty| (param.clone(), ty)),
                    _ => None,
                })
                .collect();
            let new_ret_type = rule.ret_type.clone().map(|mut ty| {
                TypeSubst(&substitutions).visit_type_mut(&mut ty);
                ty
            });

            if rule.parameters.len() == args.len() {
                let mut new_rule = Rule {
                    attrs: rule.attrs.clone(),
                    name: new_name.clone(),
                    parameters: vec![],
                    ret_type: new_ret_type,
                    arms: rule.arms.clone(),
                };
                for arm in new_rule.arms.iter_mut() {
                    *arm = replace_symbols(arm, &rule.parameters, args);
                }
                Ok(new_rule)
            } else {
                Err(syn::Error::new(
                    name.span(),
                    format!(
                        "Invalid number of arguments for `{name}`, expecting {} found {}",
                        rule.parameters.len(),
                        args.len()
                    ),
                ))
            }
        }
        None => Err(syn::Error::new(
            name.span(),
            format!("Failed to find `{name}` in available applicative rules"),
        )),
    }
}

fn replace_symbol(
    sym: &GrammarSymbol,
    param: &syn::Ident,
    replacement: &GrammarSymbol,
) -> GrammarSymbol {
    match sym {
        GrammarSymbol::Bound { name, sym: inner } => GrammarSymbol::Bound {
            name: name.clone(),
            sym: Box::new(replace_symbol(inner, param, replacement)),
        },
        GrammarSymbol::App { name, args } => GrammarSymbol::App {
            name: name.clone(),
            args: args
                .iter()
                .map(|a| replace_symbol(a, param, replacement))
                .collect(),
        },
        GrammarSymbol::Rule(ident) if ident == param => replacement.clone(),
        GrammarSymbol::Token(ident) if ident == param => replacement.clone(),
        _ => sym.clone(),
    }
}

fn replace_symbols(arm: &Arm, params: &[syn::Ident], args: &[GrammarSymbol]) -> Arm {
    let mut new_arm = arm.clone();
    for (param, replacement) in params.iter().zip(args.iter()) {
        new_arm.terms = new_arm
            .terms
            .iter()
            .map(|term| replace_symbol(term, param, replacement))
            .collect();
    }
    new_arm
}

pub(crate) fn app_ident(name: &syn::Ident, args: &[GrammarSymbol]) -> syn::Ident {
    let args_str = args.iter().map(symbol_str).collect::<Vec<_>>().join("_");
    format_ident!("__{}__{}", name, args_str, span = name.span())
}

fn symbol_str(sym: &GrammarSymbol) -> String {
    match sym {
        GrammarSymbol::Rule(ident) => ident.to_string(),
        GrammarSymbol::Token(ident) => ident.to_string(),
        GrammarSymbol::Capture { variant, .. } => variant.to_string(),
        GrammarSymbol::Bound { sym, .. } => symbol_str(sym),
        GrammarSymbol::App { name, args } => {
            let args_str = args.iter().map(symbol_str).collect::<Vec<_>>().join("_");
            format!("{}__{}", name, args_str)
        }
    }
}

impl VisitMut for TypeSubst<'_> {
    fn visit_type_mut(&mut self, ty: &mut syn::Type) {
        if let syn::Type::Path(p) = ty {
            if p.qself.is_none() {
                if let Some(ident) = p.path.get_ident() {
                    if let Some((_, replacement)) = self.0.iter().find(|(param, _)| param == ident)
                    {
                        *ty = replacement.clone();
                        return;
                    }
                }
            }
        }
        visit_mut::visit_type_mut(self, ty);
    }
}
