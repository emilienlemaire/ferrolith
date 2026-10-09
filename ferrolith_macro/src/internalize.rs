use std::collections::{HashMap, HashSet};

use ferrolith_core::grammar::{Assoc, InternalGrammar, PrecLevel, Production, Symbol, TerminalId};
use proc_macro2::Span;
use quote::format_ident;
use syn::visit_mut::{self, VisitMut};

use crate::{
    Grammar,
    grammar::{Arm, GrammarSymbol, Rule},
};

struct TypeSubst<'a>(&'a [(syn::Ident, syn::Type)]);

const BUILTIN_SOURCES: [(&str, &str); 5] = [
    (
        "star",
        "
     fn star(x) -> Vec<x> {
        () => vec![],
        (mut xs @ star(x), v @ x) => { xs.push(v); xs },
     }
     ",
    ),
    (
        "plus",
        "
        fn plus(x) -> Vec<x> {
            (v @ x) => vec![v],
            (mut xs @ plus(x), v @ x) => { xs.push(v); xs },
        }
     ",
    ),
    (
        "opt",
        "
        fn opt(x) -> Option<x> {
            () => None,
            (v @ x) => Some(v),
        }
      ",
    ),
    (
        "sep",
        "
     fn sep(s, x) -> Vec<x> {
        (v @ x) => vec![v],
        (mut xs @ sep(s, x), s, v @ x) => { xs.push(v); xs },
     }
     ",
    ),
    (
        "sep_opt",
        "
      fn sep_opt(s, x) -> Vec<x> {
        () => vec![],
        (v @ x) => vec![v],
        (mut xs @ sep_opt(s, x), s, v @ x) => { xs.push(v); xs },
      }
      ",
    ),
];

pub(super) type RuleSet = HashMap<syn::Ident, Rule>;

pub(super) fn internalize(
    grammar: &Grammar,
) -> syn::Result<(InternalGrammar, RuleSet, Option<usize>)> {
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

    let error_terminal_id = terminals.get(&format_ident!("__error__")).map(|id| *id);

    inject_builtins(&mut rule_set)?;
    check_empty_arms_without_payload_default(grammar, &rule_set)?;
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

    let mut prec_table: HashMap<TerminalId, (PrecLevel, Assoc)> = HashMap::new();
    let mut prec_by_name: HashMap<syn::Ident, PrecLevel> = HashMap::new();
    let mut pseudo_token: Vec<syn::Ident> = vec![];
    let mut prec_level = 1usize;

    for prec in &grammar.precedences {
        for token in &prec.tokens {
            prec_by_name.entry(token.clone()).or_insert(prec_level);
            match terminals.get(token) {
                Some(id) => {
                    prec_table
                        .entry(*id)
                        .or_insert((prec_level, prec.associativity));
                }
                None => pseudo_token.push(token.clone()),
            }
        }
        prec_level += 1;
    }

    validate(&rule_set, &pseudo_token)?;

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
                        &prec_by_name,
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

    Ok((
        InternalGrammar {
            terminals,
            nonterminals,
            productions: prods,
            start: 0usize,
            prec_table,
            ret_types,
            actions_order,
        },
        rule_set,
        error_terminal_id,
    ))
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
        GrammarSymbol::ErrorToken => {
            terms.push(format_ident!("__error__"));
        }
    };
    terms
}

fn desargue_applications(rules: &mut RuleSet) -> syn::Result<()> {
    let mut top_level_symbols = vec![];

    for rule in rules.values() {
        if !rule.parameters.is_empty() {
            continue;
        }
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

fn inject_builtins(rules: &mut RuleSet) -> syn::Result<()> {
    for (name, src) in BUILTIN_SOURCES {
        let rule = syn::parse_str::<Rule>(src)?;
        rules.entry(format_ident!("{}", name)).or_insert(rule);
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
        GrammarSymbol::Bound {
            name,
            mutable,
            sym: inner,
        } => GrammarSymbol::Bound {
            name: name.clone(),
            mutable: *mutable,
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
        GrammarSymbol::ErrorToken => String::from("__error_"),
    }
}

fn direct_rule_name_refs(symbol: &GrammarSymbol) -> Vec<&syn::Ident> {
    match symbol {
        GrammarSymbol::Rule(ident) => vec![ident],
        GrammarSymbol::App { name, .. } => vec![name],
        GrammarSymbol::Bound { sym, .. } => direct_rule_name_refs(sym),
        _ => vec![],
    }
}

fn nonterm_refs_in_symbol(symbol: &GrammarSymbol) -> Vec<syn::Ident> {
    match symbol {
        GrammarSymbol::Rule(ident) => vec![ident.clone()],
        GrammarSymbol::App { name, args } => vec![app_ident(name, args)],
        GrammarSymbol::Bound { sym, .. } => nonterm_refs_in_symbol(sym),
        _ => vec![],
    }
}

/// With `no_payload_default`, an empty alternative has no symbol to take its payload
/// from. Reject empty alternatives in user rules, and uses of the built-ins that have
/// one (`star`, `opt`, `sep_opt`) unless the user redefined them.
fn check_empty_arms_without_payload_default(grammar: &Grammar, rules: &RuleSet) -> syn::Result<()> {
    use syn::spanned::Spanned;

    if grammar.payload_type.is_none() || grammar.cfg.no_payload_default.is_none() {
        return Ok(());
    }

    let user_rules: HashSet<&syn::Ident> = grammar.rules.iter().map(|r| &r.name).collect();
    let nullable_builtins: HashSet<&syn::Ident> = rules
        .iter()
        .filter(|(name, rule)| {
            !user_rules.contains(name) && rule.arms.iter().any(|arm| arm.terms.is_empty())
        })
        .map(|(name, _)| name)
        .collect();

    fn combine(acc: &mut Option<syn::Error>, e: syn::Error) {
        match acc {
            Some(acc) => acc.combine(e),
            None => *acc = Some(e),
        }
    }

    fn check_symbol(
        symbol: &GrammarSymbol,
        nullable_builtins: &HashSet<&syn::Ident>,
        acc: &mut Option<syn::Error>,
    ) {
        match symbol {
            GrammarSymbol::Bound { sym, .. } => check_symbol(sym, nullable_builtins, acc),
            GrammarSymbol::App { name, args } => {
                if nullable_builtins.contains(name) {
                    combine(
                        acc,
                        syn::Error::new(
                            name.span(),
                            format!(
                                "`{name}` can match nothing, which needs a default payload, \
                                 but `no_payload_default` is set"
                            ),
                        ),
                    );
                }
                for arg in args {
                    check_symbol(arg, nullable_builtins, acc);
                }
            }
            _ => (),
        }
    }

    let mut acc = None;
    for rule in &grammar.rules {
        for arm in &rule.arms {
            if arm.terms.is_empty() {
                combine(
                    &mut acc,
                    syn::Error::new(
                        arm.action.span(),
                        format!(
                            "empty alternative in `{}` needs a default payload, but \
                             `no_payload_default` is set",
                            rule.name
                        ),
                    ),
                );
            }
            for symbol in &arm.terms {
                check_symbol(symbol, &nullable_builtins, &mut acc);
            }
        }
    }

    acc.map_or(Ok(()), Err)
}

fn collect_prec_refs(rules: &RuleSet) -> HashSet<syn::Ident> {
    let mut refs = HashSet::new();
    for rule in rules.values() {
        for arm in &rule.arms {
            for attr in arm.attrs.iter().filter(|a| a.path().is_ident("prec")) {
                if let Ok(ident) = attr.parse_args::<syn::Ident>() {
                    refs.insert(ident);
                }
            }
        }
    }
    refs
}

fn check_unused_pseudo_tokens(pseudo: &[syn::Ident], rules: &RuleSet) -> syn::Result<()> {
    let refs = collect_prec_refs(rules);
    let mut err: Option<syn::Error> = None;
    for tok in pseudo.iter().filter(|t| !refs.contains(*t)) {
        let e = syn::Error::new(
            tok.span(),
            format!(
                "precedence declared for `{tok}`, which is neither a token used in a rule \
                nor referenced by a `#[prec(...)]` attribute"
            ),
        );
        match err {
            Some(ref mut acc) => acc.combine(e),
            None => err = Some(e),
        }
    }
    err.map_or(Ok(()), Err)
}

fn check_unreachable_rules(rules: &RuleSet, start: &syn::Ident) -> syn::Result<()> {
    let mut reachable = HashSet::new();
    let mut worklist = vec![start.clone()];

    while !worklist.is_empty() {
        let name = worklist.pop().unwrap();

        if reachable.contains(&name) {
            continue;
        }

        reachable.insert(name.clone());

        if let Some(rule) = rules.get(&name) {
            for arm in &rule.arms {
                for symbol in &arm.terms {
                    for ident in nonterm_refs_in_symbol(&symbol) {
                        worklist.push(ident);
                    }
                }
            }
        }
    }

    let mut acc: Option<syn::Error> = None;

    for rule in rules.values() {
        if rule.parameters.is_empty() {
            if !reachable.contains(&rule.name) {
                let message = format!("rule `{}` is unreachable from the start rule", &rule.name);
                let new_err = syn::Error::new_spanned(&rule.name, message);
                if let Some(ref mut err) = acc {
                    err.combine(new_err);
                } else {
                    acc = Some(new_err);
                }
            }
        }
    }

    acc.map(Err).unwrap_or(Ok(()))
}

fn check_nonproductive_rules(rules: &RuleSet) -> syn::Result<()> {
    let mut productive = HashSet::new();

    loop {
        let mut changed = false;

        for rule in rules.values() {
            if rule.parameters.is_empty() {
                if productive.contains(&rule.name) {
                    continue;
                }

                for arm in &rule.arms {
                    let arm_is_productive = arm.terms.iter().all(|symbol| {
                        nonterm_refs_in_symbol(symbol)
                            .iter()
                            .all(|i| productive.contains(i))
                    });
                    if arm_is_productive {
                        productive.insert(rule.name.clone());
                        changed = true;
                        break;
                    }
                }
            }
        }

        if !changed {
            break;
        }
    }

    let mut acc: Option<syn::Error> = None;

    for rule in rules.values() {
        if rule.parameters.is_empty() {
            if !productive.contains(&rule.name) {
                let message = format!("rule `{}` can never produce a value", &rule.name);
                let new_err = syn::Error::new_spanned(&rule.name, message);
                if let Some(ref mut err) = acc {
                    err.combine(new_err);
                } else {
                    acc = Some(new_err);
                }
            }
        }
    }

    acc.map(Err).unwrap_or(Ok(()))
}

fn check_undefined_references(rules: &RuleSet) -> syn::Result<()> {
    let mut acc: Option<syn::Error> = None;

    for rule in rules.values() {
        if !rule.parameters.is_empty() {
            continue;
        }

        for arm in &rule.arms {
            for symbol in &arm.terms {
                for ident in direct_rule_name_refs(&symbol) {
                    if !rules.contains_key(ident) {
                        let message = format!("undefined rule `{ident}`");
                        let new_err = syn::Error::new_spanned(&ident, message);
                        if let Some(ref mut err) = acc {
                            err.combine(new_err);
                        } else {
                            acc = Some(new_err);
                        }
                    }
                }
            }
        }
    }

    acc.map(Err).unwrap_or(Ok(()))
}

fn validate(rules: &RuleSet, pseudo: &[syn::Ident]) -> syn::Result<()> {
    let starts: Vec<_> = rules
        .values()
        .filter(|r| r.attrs.iter().any(|attr| attr.path().is_ident("start")))
        .collect();
    let start = if starts.len() == 1 {
        Some(&starts[0].name)
    } else {
        None
    };

    let mut acc: Option<syn::Error> = None;

    match check_undefined_references(rules) {
        Ok(()) => (),
        Err(e) => {
            if let Some(ref mut err) = acc {
                err.combine(e);
            } else {
                acc = Some(e);
            }
        }
    };

    match check_nonproductive_rules(rules) {
        Ok(()) => (),
        Err(e) => {
            if let Some(ref mut err) = acc {
                err.combine(e);
            } else {
                acc = Some(e);
            }
        }
    };

    if let Some(start) = start {
        match check_unreachable_rules(rules, &start) {
            Ok(()) => (),
            Err(e) => {
                if let Some(ref mut err) = acc {
                    err.combine(e);
                } else {
                    acc = Some(e);
                }
            }
        }
    }

    match check_unused_pseudo_tokens(pseudo, rules) {
        Ok(()) => (),
        Err(e) => {
            if let Some(ref mut err) = acc {
                err.combine(e);
            } else {
                acc = Some(e);
            }
        }
    }

    acc.map(Err).unwrap_or(Ok(()))
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
