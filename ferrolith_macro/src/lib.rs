use std::{collections::HashSet, usize};

use ferrolith_core::{
    automaton,
    first::FirstSet,
    grammar::{InternalGrammar, Symbol},
    table::{self, Action, Actions, Gotos, ShiftReduceConflict},
};
use heck::ToPascalCase;
use proc_macro::TokenStream;
use proc_macro2::TokenTree;
use quote::{ToTokens, format_ident, quote, quote_spanned};
use syn::parse_macro_input;

use crate::internalize::RuleSet;
use crate::{
    grammar::{Grammar, GrammarSymbol, Rule},
    internalize::app_ident,
};

mod grammar;
mod internalize;

#[proc_macro]
pub fn grammar(input: TokenStream) -> TokenStream {
    let grammar = parse_macro_input!(input as Grammar);

    let api_vis = grammar
        .rules
        .iter()
        .find(|r| r.attrs.iter().any(|a| a.path().is_ident("start")))
        .map(|r| r.vis.clone())
        .unwrap_or(syn::Visibility::Inherited);

    let (internal_grammar, rule_set, error_id) = match internalize::internalize(&grammar) {
        Ok(r) => r,
        Err(e) => return e.to_compile_error().into(),
    };

    let firsts = FirstSet::compute(&internal_grammar.productions);

    let automaton = match automaton::build_automaton(&internal_grammar, &firsts) {
        Ok(a) => a,
        Err(e) => return e.to_compile_error().into(),
    };

    let (actions, gotos, n_states, incoming, conflicts) =
        match table::build_tables(automaton, &internal_grammar) {
            Ok(t) => t,
            Err(e) => return e.into_compile_error().into(),
        };

    let warnings: Vec<_> = conflicts
        .iter()
        .enumerate()
        .map(|(i, c)| gen_shift_reduce_warnings(i, c))
        .collect();

    let n_terminals = internal_grammar.terminals.len();
    let n_nonterminals = internal_grammar.nonterminals.len();

    let value_enum = gen_value_enum(&grammar, &internal_grammar);

    let token_enum = gen_token_enum(&grammar, &internal_grammar, error_id);

    let expected_fn = gen_expected_tokens_fn(&internal_grammar, error_id);

    let (default_reductions_fn, defaults) =
        gen_default_reductions_fn(&actions, n_states, n_terminals);

    let actions_fn = gen_action_function(&actions, n_states, n_terminals, Some(&defaults));

    let exact_actions_fn = grammar
        .cfg
        .expose_automaton
        .then(|| gen_action_function(&actions, n_states, n_terminals, None));

    let gotos_fn = gen_goto_function(&gotos, n_states, n_nonterminals);

    let reduce_fn = gen_reduce_function(&grammar, &internal_grammar, &rule_set);

    let parse_fn = gen_parse_function(&api_vis, &grammar, &internal_grammar, error_id);

    let incremental_api = gen_incremental_api(&api_vis, &grammar, &internal_grammar);

    let automaton_view = grammar
        .cfg
        .expose_automaton
        .then(|| gen_automaton_view(&api_vis, &internal_grammar, n_states, &incoming));

    let tokens_type = &grammar.tokens_type;

    quote! {
        #(#warnings)*

        #[allow(non_camel_case_types)]
        type __UserToken = #tokens_type;

        #value_enum

        #token_enum

        #expected_fn

        #actions_fn

        #exact_actions_fn

        #default_reductions_fn

        #gotos_fn

        #reduce_fn

        #parse_fn

        #incremental_api

        #automaton_view
    }
    .into()
}

/// The fallback payload expression, or `None` if there is no payload type or
/// `no_payload_default` is set.
fn payload_default_expr(grammar: &Grammar) -> Option<proc_macro2::TokenStream> {
    let ty = grammar.payload_type.as_ref()?;
    if grammar.cfg.no_payload_default.is_some() {
        return None;
    }
    Some(match grammar.cfg.payload_default {
        Some(ref def) => quote! { #def },
        None => quote! {<#ty>::default() },
    })
}

fn gen_expected_tokens_fn(
    internal_grammar: &InternalGrammar,
    error_terminal_id: Option<usize>,
) -> proc_macro2::TokenStream {
    let len = internal_grammar.terminals.len();
    let names: Vec<String> = internal_grammar
        .terminals
        .iter()
        .enumerate()
        .map(|(idx, i)| {
            if idx == 0 {
                "<eof>".into()
            } else {
                i.to_string()
            }
        })
        .collect();

    if let Some(err_id) = error_terminal_id {
        quote! {
            static __TERMINAL_NAMES: [&str; #len] = [#(#names,)*];

            fn __expected_tokens(state: usize) -> Vec<&'static str> {
                let mut expected = vec![];
                for t in 0..#len {
                    if t == #err_id {
                        continue;
                    } else if __action(state, t) != ::ferrolith::action::Action::Error {
                        expected.push(__TERMINAL_NAMES[t]);
                    }
                }

                expected
            }
        }
    } else {
        quote! {
            static __TERMINAL_NAMES: [&str; #len] = [#(#names,)*];

            fn __expected_tokens(state: usize) -> Vec<&'static str> {
                let mut expected = vec![];
                for t in 0..#len {
                    if __action(state, t) != ::ferrolith::action::Action::Error {
                        expected.push(__TERMINAL_NAMES[t]);
                    }
                }

                expected
            }
        }
    }
}

fn gen_shift_reduce_warnings(idx: usize, c: &ShiftReduceConflict) -> proc_macro2::TokenStream {
    let msg = format!(
        "shift/reduce conflict: shifting `{}` wins over reducing `{}`",
        c.terminal_name, c.reduce_name
    );

    let reduce_item = format_ident!("__ferrolith_sr_r{}", idx, span = c.reduce_span);
    let terminal_item = format_ident!("__ferrolith_sr_t{}", idx, span = c.terminal_span);

    let define_reduce = quote! {
        #[deprecated = #msg]
        #[doc(hidden)]
        #[allow(non_upper_case_globals)]
        const #reduce_item: () = ();
    };

    let use_reduce = quote_spanned! { c.reduce_span => const _ : () = #reduce_item; };

    let define_terminal = quote! {
        #[deprecated = #msg]
        #[doc(hidden)]
        #[allow(non_upper_case_globals)]
        const #terminal_item: () = ();
    };

    let use_terminal = quote_spanned! { c.terminal_span => const _ : () = #terminal_item; };

    quote! {
        #define_reduce
        #use_reduce
        #define_terminal
        #use_terminal
    }
}

fn gen_value_enum(
    grammar: &Grammar,
    internal_grammar: &InternalGrammar,
) -> proc_macro2::TokenStream {
    let non_term_variants: Vec<_> = internal_grammar
        .nonterminals
        .iter()
        .enumerate()
        .map(|(idx, ident)| {
            if let Some(Some(ret_type)) = internal_grammar.ret_types.get(idx) {
                let ident = format_ident!("N_{}", ident.to_string().to_pascal_case());
                quote! { #ident(#ret_type)  }
            } else {
                let ident = format_ident!("N_{}", ident.to_string().to_pascal_case());
                quote! { #ident }
            }
        })
        .collect();

    let tokens_type = &grammar.tokens_type;

    let derive = grammar
        .cfg
        .clonable_state
        .then(|| quote! { #[derive(Clone)] });
    quote! {
        #derive
        enum __Value {
            #(#non_term_variants,)*
            T_Token(#tokens_type),
            T_Unit,
        }
    }
}

fn collect_symbol(symbol: &GrammarSymbol, captures: &mut HashSet<syn::Ident>) {
    match symbol {
        GrammarSymbol::Bound { sym, .. } => {
            collect_symbol(sym, captures);
        }
        GrammarSymbol::Capture { variant, .. } => {
            captures.insert(variant.clone());
        }
        GrammarSymbol::App { args, .. } => {
            for arg in args {
                collect_symbol(arg, captures);
            }
        }
        _ => (),
    }
}

fn collect_capture_terminals(grammar: &Grammar) -> HashSet<syn::Ident> {
    let mut captures = HashSet::new();

    for rule in &grammar.rules {
        for arm in &rule.arms {
            for symbol in &arm.terms {
                collect_symbol(&symbol, &mut captures);
            }
        }
    }

    captures
}

fn gen_token_enum(
    grammar: &Grammar,
    internal_grammar: &InternalGrammar,
    error_terminal_id: Option<usize>,
) -> proc_macro2::TokenStream {
    let captures = collect_capture_terminals(grammar);

    let tokens_arms: Vec<_> = internal_grammar
        .terminals
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(id, _)| Some(*id) != error_terminal_id)
        .map(|(id, ident)| {
            if captures.contains(ident) {
                quote! { __UserToken::#ident(..) => #id, }
            } else {
                quote! { __UserToken::#ident => #id, }
            }
        })
        .collect();

    let unknown_id = internal_grammar.terminals.len();

    quote! {
        fn __token_id_opt(token: Option<&__UserToken>) -> usize {
            match token {
                None => 0,
                Some(t) => match t {
                    #(#tokens_arms)*
                    _ => #unknown_id,
                }
            }
        }
    }
}

/// Emits the packed action table and its lookup function.
///
/// With `Some(defaults)`, entries equal to their state's default reduction are
/// left out (the parser falls back to `__DEFAULT_REDUCTIONS`) and the function
/// is `__action`. With `None`, every entry is kept and the function is
/// `__action_exact`, the table behind the `action` function of `expose_automaton`.
fn gen_action_function(
    actions: &Actions,
    n_states: usize,
    n_terminals: usize,
    defaults: Option<&[usize]>,
) -> proc_macro2::TokenStream {
    fn action_to_tokens(action: Action) -> proc_macro2::TokenStream {
        match action {
            Action::Shift(n) => quote! { ::ferrolith::action::Action::Shift(#n) },
            Action::Reduce(n) => quote! { ::ferrolith::action::Action::Reduce(#n) },
            Action::Accept => quote! { ::ferrolith::action::Action::Accept },
            Action::Error => quote! { ::ferrolith::action::Action::Error },
        }
    }

    let mut rows = vec![];

    for state in 0..n_states {
        let default_prod = defaults.map(|d| d[state]);
        rows.push((state, vec![None; n_terminals]));
        for ((state_id, terminal), action) in actions {
            if *state_id != state || *action == Action::Error {
                continue;
            }

            if let Action::Reduce(prod) = action
                && Some(*prod) == default_prod
            {
                continue;
            }

            rows[state].1[*terminal] = Some(action.clone());
        }
    }

    let mut packed = vec![];
    let mut check: Vec<usize> = vec![];
    let mut disp = vec![0isize; n_states];

    rows.sort_by(|l, r| {
        r.1.iter()
            .filter(|r| r.is_some())
            .count()
            .cmp(&l.1.iter().filter(|l| l.is_some()).count())
    });

    for (state, row) in rows {
        if row.iter().all(Option::is_none) {
            disp[state] = 0;
            continue;
        }

        let terminals: Vec<_> = row
            .iter()
            .enumerate()
            .filter_map(|(idx, action)| {
                if let Some(action) = action {
                    Some((idx, action))
                } else {
                    None
                }
            })
            .collect();

        let min_t = terminals[0].0 as isize;

        let mut d = -min_t;
        loop {
            let mut fit = true;
            for (terminal, _) in &terminals {
                if (d + *terminal as isize) as usize >= check.len()
                    || check[(d + *terminal as isize) as usize] == usize::MAX
                {
                    continue;
                }
                fit = false;
                break;
            }
            if fit {
                break;
            }
            d += 1;
        }

        disp[state] = d;

        for (terminal, action) in terminals {
            let slot = (d + terminal as isize) as usize;
            if slot >= packed.len() {
                while slot + 1 >= packed.len() {
                    packed.push(action_to_tokens(Action::Error));
                    check.push(usize::MAX);
                }
            }
            packed[slot] = action_to_tokens(action.clone());
            check[slot] = state;
        }
    }

    let pack_len = packed.len();

    let (disp_name, packed_name, check_name, fn_name) = if defaults.is_some() {
        (
            "__ACTION_DISP",
            "__ACTION_PACKED",
            "__ACTION_CHECK",
            "__action",
        )
    } else {
        (
            "__EXACT_ACTION_DISP",
            "__EXACT_ACTION_PACKED",
            "__EXACT_ACTION_CHECK",
            "__action_exact",
        )
    };
    let disp_name = format_ident!("{}", disp_name);
    let packed_name = format_ident!("{}", packed_name);
    let check_name = format_ident!("{}", check_name);
    let fn_name = format_ident!("{}", fn_name);

    quote! {
        static #disp_name: [isize; #n_states] = [ #(#disp),* ];
        static #packed_name: [::ferrolith::action::Action; #pack_len] = [ #(#packed),* ];
        static #check_name: [usize; #pack_len] = [ #(#check),* ];

        fn #fn_name(state: usize, terminal: usize) -> ::ferrolith::action::Action {
            let index = #disp_name[state] + terminal as isize;
            if index >= 0 && (index as usize) < #pack_len
                && #check_name[index as usize] == state {
                #packed_name[index as usize]
            } else {
                ::ferrolith::action::Action::Error
            }
        }
    }
}

fn gen_default_reductions_fn(
    actions: &Actions,
    n_states: usize,
    n_terminals: usize,
) -> (proc_macro2::TokenStream, Vec<usize>) {
    let defaults: Vec<_> = (0..n_states)
        .map(|state| {
            let mut default_prod: Option<usize> = None;
            let mut has_non_reduce = false;
            for t in 0..n_terminals {
                match actions.get(&(state, t)) {
                    Some(Action::Reduce(p)) => match default_prod {
                        None => default_prod = Some(*p),
                        Some(q) if q == *p => {}
                        _ => {
                            has_non_reduce = true;
                            break;
                        }
                    },
                    // An explicit `Error` is a `nonassoc` tie: reducing by default
                    // there would accept the token the tie must reject.
                    Some(Action::Shift(_)) | Some(Action::Accept) | Some(Action::Error) => {
                        has_non_reduce = true;
                        break;
                    }
                    None => {}
                }
            }
            if has_non_reduce {
                usize::MAX
            } else {
                default_prod.unwrap_or(usize::MAX)
            }
        })
        .collect();

    (
        quote! {
            static __DEFAULT_REDUCTIONS: [usize; #n_states] = [#(#defaults,)*];
        },
        defaults.clone(),
    )
}

fn gen_goto_function(
    gotos: &Gotos,
    n_states: usize,
    n_nonterminals: usize,
) -> proc_macro2::TokenStream {
    let mut rows = vec![];

    for state in 0..n_states {
        rows.push((state, vec![None; n_nonterminals]));
        for ((state_id, nonterminal), dest) in gotos {
            if *state_id == state {
                rows[state].1[*nonterminal] = Some(dest);
            }
        }
    }

    let mut packed = vec![];
    let mut check = vec![];
    let mut disp = vec![0isize; n_states];

    rows.sort_by(|l, r| {
        r.1.iter()
            .filter(|r| r.is_some())
            .count()
            .cmp(&l.1.iter().filter(|l| l.is_some()).count())
    });

    for (state, row) in rows {
        if row.iter().all(Option::is_none) {
            disp[state] = 0;
            continue;
        }

        let nonterminals: Vec<_> = row
            .iter()
            .enumerate()
            .filter_map(|(idx, dest)| {
                if let Some(dest) = dest {
                    Some((idx, **dest))
                } else {
                    None
                }
            })
            .collect();

        let min_nt = nonterminals[0].0 as isize;

        let mut d = -min_nt;

        loop {
            let mut fit = true;
            for (nonterminal, _) in &nonterminals {
                if (d + *nonterminal as isize) as usize >= check.len()
                    || check[(d + *nonterminal as isize) as usize] == usize::MAX
                {
                    continue;
                }
                fit = false;
                break;
            }

            if fit {
                break;
            }
            d += 1;
        }

        disp[state] = d;

        for (nonterminal, dest) in nonterminals {
            let slot = (d + nonterminal as isize) as usize;
            if slot >= packed.len() {
                while slot + 1 >= packed.len() {
                    packed.push(usize::MAX);
                    check.push(usize::MAX);
                }
            }

            packed[slot] = dest;
            check[slot] = state;
        }
    }

    let pack_len = packed.len();

    quote! {
        static __GOTO_DISP: [isize; #n_states] = [ #(#disp),* ];
        static __GOTO_PACKED: [usize; #pack_len] = [ #(#packed),* ];
        static __GOTO_CHECK: [usize; #pack_len] = [ #(#check),* ];

        fn __goto(state: usize, nonterminal: usize) -> usize {
            let index = __GOTO_DISP[state] + nonterminal as isize;
            if index >= 0 && (index as usize) < #pack_len
                && __GOTO_CHECK[index as usize] == state {
                    __GOTO_PACKED[index as usize]
            } else {
                usize::MAX
            }
        }

    }
}

fn gen_reduce_function(
    grammar: &Grammar,
    internal_grammar: &InternalGrammar,
    rule_set: &RuleSet,
) -> proc_macro2::TokenStream {
    let mut prod_code = vec![];

    for (prod_id, production) in internal_grammar.productions.iter().enumerate() {
        if let Some(action_id) = production.action {
            let (rule_ident, arm_idx) = &internal_grammar.actions_order[action_id];
            let rule: &Rule = grammar
                .rules
                .iter()
                .find(|rule| rule.name == *rule_ident)
                .or_else(|| rule_set.get(rule_ident))
                .unwrap();
            let arm = &rule.arms[*arm_idx];
            let lhs_ident = &internal_grammar.nonterminals[production.lhs];
            let lhs_variant = format_ident!("N_{}", lhs_ident.to_string().to_pascal_case());
            let has_payload = internal_grammar.ret_types[production.lhs].is_some();

            let mut arms_assigns = vec![];
            let rhs_len = arm.terms.len();
            for (idx, symbol) in arm.terms.iter().rev().enumerate() {
                #[allow(non_snake_case)]
                let vN = format_ident!("__v{}", idx);
                let payload_id = format_ident!("_{}_payload", rhs_len - idx);
                let assign = quote! { let #vN = value_stack.pop().unwrap(); };
                let pos = format_ident!("_{}", rhs_len - idx);
                let payload_pop = {
                    if grammar.payload_type.is_some() {
                        Some(quote! {
                            let #payload_id = payload_stack.pop().unwrap();
                        })
                    } else {
                        None
                    }
                };
                let symbol_quote = match symbol {
                    GrammarSymbol::Bound { name, mutable, sym } => {
                        // Capture-in-Bound (e.g. `n @ Tok(x)`): the outer name is
                        // unused — fields are destructured directly, same as the
                        // non-Bound Capture path. `mut` is not applicable here.
                        if let GrammarSymbol::Capture { variant, fields } = sym.as_ref() {
                            quote! {
                                let (#(#fields),*) = match #vN {
                                    __Value::T_Token(__UserToken::#variant(#(#fields),*)) => (#(#fields),*),
                                    _ => unreachable!(),
                                };
                            }
                        } else {
                            let value_expr = match sym.as_ref() {
                                GrammarSymbol::Rule(ident) => {
                                    let variant_ident =
                                        format_ident!("N_{}", ident.to_string().to_pascal_case());
                                    quote! {
                                        match #vN {
                                            __Value::#variant_ident(v) => v,
                                            _ => unreachable!()
                                        }
                                    }
                                }
                                GrammarSymbol::Token(_) => {
                                    quote! {
                                        match #vN {
                                            __Value::T_Token(v) => v,
                                            _ => unreachable!(),
                                        }
                                    }
                                }
                                GrammarSymbol::App {
                                    name: app_name,
                                    args,
                                } => {
                                    let synthetic_ident = internalize::app_ident(app_name, args);
                                    let variant_ident = format_ident!(
                                        "N_{}",
                                        synthetic_ident.to_string().to_pascal_case()
                                    );
                                    quote! {
                                        match #vN {
                                            __Value::#variant_ident(v) => v,
                                            _ => unreachable!(),
                                        }
                                    }
                                }
                                _ => quote! { #vN },
                            };
                            if *mutable {
                                quote! {
                                    let mut #pos = #value_expr;
                                    let mut #name = #pos;
                                }
                            } else {
                                quote! {
                                    let #pos = #value_expr;
                                    let #name = #pos;
                                }
                            }
                        }
                    }
                    GrammarSymbol::Capture { variant, fields } => {
                        quote! {
                            let (#(#fields),*) = match #vN {
                                __Value::T_Token(__UserToken::#variant(#(#fields),*)) => (#(#fields),*),
                                _ => unreachable!(),
                            };
                        }
                    }
                    GrammarSymbol::Rule(ident) => {
                        let nonterm_idx = internal_grammar
                            .nonterminals
                            .iter()
                            .position(|id| id == ident);
                        let ret_type = nonterm_idx
                            .map(|idx| {
                                internal_grammar
                                    .ret_types
                                    .get(idx)
                                    .map(|o| o.clone())
                                    .flatten()
                            })
                            .flatten();
                        match ret_type {
                            Some(_) => {
                                let variant_ident =
                                    format_ident!("N_{}", ident.to_string().to_pascal_case());
                                quote! {
                                    let #pos = match #vN {
                                        __Value::#variant_ident(v) => v,
                                        _ => unreachable!(),
                                    };
                                }
                            }
                            None => {
                                quote! {
                                    let _ = #vN;
                                }
                            }
                        }
                    }
                    GrammarSymbol::Token(_) => {
                        quote! {
                            let #pos = match #vN {
                                __Value::T_Token(v) => v,
                                _ => unreachable!(),
                            };
                        }
                    }
                    GrammarSymbol::App {
                        name: app_name,
                        args,
                    } => {
                        let synthetic = app_ident(app_name, args);
                        let variant_ident =
                            format_ident!("N_{}", synthetic.to_string().to_pascal_case());
                        quote! {
                            let #pos = match #vN {
                                __Value::#variant_ident(v) => v,
                                _ => unreachable!(),
                            };
                        }
                    }
                    GrammarSymbol::ErrorToken => {
                        quote! {
                            let _ = #vN;
                        }
                    }
                };
                arms_assigns.push(quote! {
                    #assign
                    #payload_pop
                    #symbol_quote
                });
            }
            let payload_decl = {
                if let Some(ref ty) = grammar.payload_type {
                    Some(quote! {
                        let mut _payload : #ty;
                    })
                } else {
                    None
                }
            };
            let payload_merge = {
                let action_tokens = arm.action.to_token_stream();
                let payload_arg = |k: usize| -> proc_macro2::TokenStream {
                    let id = format_ident!("_{}_payload", k);
                    if tokens_mention(action_tokens.clone(), &id) {
                        quote! { #id.clone() }
                    } else {
                        quote! { #id }
                    }
                };
                if grammar.payload_type.is_some() {
                    if rhs_len == 0 {
                        // Empty alternatives are rejected during internalization
                        // when `no_payload_default` is set.
                        let def = payload_default_expr(grammar)
                            .expect("empty alternative without a default payload");
                        Some(quote! {
                            _payload = #def;
                        })
                    } else if rhs_len == 1 {
                        let arg = payload_arg(1);
                        Some(quote! { _payload = #arg; })
                    } else {
                        match grammar.cfg.payload_merge_strategy {
                            grammar::MergeStrategy::FirstLast => {
                                let first = payload_arg(1);
                                let last = payload_arg(rhs_len);
                                if let Some(ref merge) = grammar.cfg.payload_merge {
                                    Some(quote! {
                                        _payload = #merge(#first, #last);
                                    })
                                } else {
                                    Some(syn::Error::new(
                                        proc_macro2::Span::call_site(),
                                        "`first_last` merge strategy requires a `payload_merge` setting.",
                                    ).to_compile_error())
                                }
                            }
                            grammar::MergeStrategy::Fold => {
                                let gen_merge =
                                    |left: proc_macro2::TokenStream,
                                     right: proc_macro2::TokenStream| {
                                        if let Some(ref merge) = grammar.cfg.payload_merge {
                                            quote! {
                                                #merge(#left, #right)
                                            }
                                        } else {
                                            syn::Error::new(
                                                proc_macro2::Span::call_site(),
                                                "`fold` merge strategy requires a `payload_merge` setting.",
                                            ).to_compile_error()
                                        }
                                };
                                let mut merge = {
                                    let last = payload_arg(rhs_len);
                                    let ob_last = payload_arg(rhs_len - 1);
                                    gen_merge(ob_last.to_token_stream(), last.to_token_stream())
                                };
                                for id in (1..rhs_len - 1).rev() {
                                    let ident = payload_arg(id);
                                    merge = gen_merge(ident.to_token_stream(), merge);
                                }
                                Some(quote! {
                                    _payload = #merge;
                                })
                            }
                            grammar::MergeStrategy::First => {
                                let ident = payload_arg(1);
                                Some(quote! {
                                    _payload = #ident;
                                })
                            }
                            grammar::MergeStrategy::Last => {
                                let ident = payload_arg(rhs_len);
                                Some(quote! {
                                    _payload = #ident;
                                })
                            }
                        }
                    }
                } else {
                    None
                }
            };
            let action = &arm.action;

            let ret_toks = match (grammar.payload_type.is_some(), has_payload) {
                (true, true) => {
                    quote! {
                        #payload_decl
                        #payload_merge
                        (__Value::#lhs_variant(#action), _payload)
                    }
                }
                (true, false) => {
                    quote! {
                        #payload_decl
                        #payload_merge
                        #action;
                        (__Value::#lhs_variant, _payload)
                    }
                }
                (false, true) => {
                    quote! {
                        __Value::#lhs_variant(#action)
                    }
                }
                (false, false) => {
                    quote! {
                        #action;
                        __Value::#lhs_variant
                    }
                }
            };

            prod_code.push((
                prod_id,
                quote! {
                    #(#arms_assigns)*
                    #ret_toks
                },
            ));
        } else {
            continue;
        }
    }

    let (prod_id, prod_code): (Vec<_>, Vec<_>) = prod_code.into_iter().unzip();

    if let Some(ref ty) = grammar.payload_type {
        quote! {
            fn __reduce(
                prod_id: usize,
                value_stack: &mut Vec<__Value>,
                payload_stack: &mut Vec<#ty>
            ) -> (__Value, #ty) {
                match prod_id {
                    #(#prod_id => { #prod_code },)*
                    _ => unreachable!(),
                }
            }
        }
    } else {
        quote! {
            fn __reduce(prod_id: usize, value_stack: &mut Vec<__Value>) -> __Value {
                match prod_id {
                    #(#prod_id => { #prod_code },)*
                    _ => unreachable!(),
                }
            }
        }
    }
}

fn gen_parse_function(
    api_vis: &syn::Visibility,
    grammar: &Grammar,
    internal_grammar: &InternalGrammar,
    error_terminal_id: Option<usize>,
) -> proc_macro2::TokenStream {
    let start_nt_id = match internal_grammar.productions[0].rhs[0] {
        Symbol::NonTerminal(id) => id,
        _ => unreachable!(),
    };

    let start_ret_type = match &internal_grammar.ret_types[start_nt_id] {
        Some(ty) => ty.clone(),
        None => syn::parse_quote!(()),
    };

    let start_variant = format_ident!(
        "N_{}",
        internal_grammar.nonterminals[start_nt_id]
            .to_string()
            .to_pascal_case()
    );

    let token_type = &grammar.tokens_type;

    // What `parse` reads from its iterator and `feed` receives: the token, paired
    // with its payload when there is one.
    let tok_ty = if let Some(ref ty) = grammar.payload_type {
        quote! { (#token_type, #ty) }
    } else {
        quote! { #token_type }
    };

    let token_id_expr = if grammar.payload_type.is_some() {
        quote! { __token_id_opt(token.as_ref().map(|(t, _)| t)) }
    } else {
        quote! { __token_id_opt(token.as_ref()) }
    };

    let shift_body = if grammar.payload_type.is_some() {
        quote! {
            let (tok, pay) = token.expect("shift on EOF is a parser bug");
            state.value_stack.push(__Value::T_Token(tok));
            state.payload_stack.push(pay);
        }
    } else {
        quote! {
            state.value_stack.push(__Value::T_Token(token.expect("shift on EOF is a parser bug")));
        }
    };

    let reduce = |prod: proc_macro2::TokenStream| {
        if grammar.payload_type.is_some() {
            quote! {
                let (lhs, rhs_len) = __prod_info(#prod);
                let (result, payload_result) =
                    __reduce(#prod, &mut state.value_stack, &mut state.payload_stack);
                state.state_stack.truncate(state.state_stack.len() - rhs_len);
                let next = __goto(*state.state_stack.last().unwrap(), lhs);
                state.state_stack.push(next);
                state.value_stack.push(result);
                state.payload_stack.push(payload_result);
            }
        } else {
            quote! {
                let (lhs, rhs_len) = __prod_info(#prod);
                let result = __reduce(#prod, &mut state.value_stack);
                state.state_stack.truncate(state.state_stack.len() - rhs_len);
                let next = __goto(*state.state_stack.last().unwrap(), lhs);
                state.state_stack.push(next);
                state.value_stack.push(result);
            }
        }
    };
    let reduce_explicit = reduce(quote! { p });
    let reduce_default = reduce(quote! { default_prod });

    // `parse`'s fast path keeps its stacks in locals rather than behind
    // `&mut ParserState`, which is measurably faster on error-free input.
    let local_reduce = |prod: proc_macro2::TokenStream| {
        if grammar.payload_type.is_some() {
            quote! {
                let (lhs, rhs_len) = __prod_info(#prod);
                let (result, payload_result) =
                    __reduce(#prod, &mut value_stack, &mut payload_stack);
                state_stack.truncate(state_stack.len() - rhs_len);
                let next = __goto(*state_stack.last().unwrap(), lhs);
                state_stack.push(next);
                value_stack.push(result);
                payload_stack.push(payload_result);
            }
        } else {
            quote! {
                let (lhs, rhs_len) = __prod_info(#prod);
                let result = __reduce(#prod, &mut value_stack);
                state_stack.truncate(state_stack.len() - rhs_len);
                let next = __goto(*state_stack.last().unwrap(), lhs);
                state_stack.push(next);
                value_stack.push(result);
            }
        }
    };
    let local_reduce_explicit = local_reduce(quote! { p });
    let local_reduce_default = local_reduce(quote! { default_prod });

    let local_payload_stack = grammar.payload_type.as_ref().map(|ty| {
        quote! { let mut payload_stack: Vec<#ty> = Vec::with_capacity(64); }
    });

    let local_shift = if grammar.payload_type.is_some() {
        quote! {
            let (tok, pay) = token.expect("shift on EOF is a parser bug");
            value_stack.push(__Value::T_Token(tok));
            payload_stack.push(pay);
        }
    } else {
        quote! {
            value_stack.push(__Value::T_Token(token.expect("shift on EOF is a parser bug")));
        }
    };

    // First error with no default reduction: hand the stacks over to `__step`,
    // which performs recovery exactly as `feed` does, for the rest of the input.
    let fast_path_error = if error_terminal_id.is_some() {
        let payload_field = grammar
            .payload_type
            .as_ref()
            .map(|_| quote! { payload_stack, });
        quote! {
            let mut slow = ParserState {
                state_stack,
                value_stack,
                #payload_field
                recovering: false,
            };
            let mut next = token;
            loop {
                match __step(&mut slow, next) {
                    __Step::InputNeeded => {}
                    __Step::Accepted(v) => return Ok(v),
                    __Step::Rejected(e) => return Err(e),
                }
                next = tokens.next();
            }
        }
    } else {
        quote! {
            return Err(match token {
                Some(t) => ::ferrolith::ParseError::UnexpectedToken {
                    token: t,
                    expected: __expected_tokens(state),
                },
                None => ::ferrolith::ParseError::UnexpectedEof {
                    expected: __expected_tokens(state),
                },
            });
        }
    };

    let reject = |expected: proc_macro2::TokenStream| {
        quote! {
            return __Step::Rejected(match token {
                Some(t) => ::ferrolith::ParseError::UnexpectedToken { token: t, expected: #expected },
                None => ::ferrolith::ParseError::UnexpectedEof { expected: #expected },
            });
        }
    };

    // While recovering, input is discarded until a token can be consumed; the
    // recovery itself starts in the `Error` arm below.
    let (recovering_check, err_handling) = if let Some(err_id) = error_terminal_id {
        let error_payload_push = grammar.payload_type.as_ref().map(|_| {
            // At end of input there is no token to take a payload from: use the
            // default, or give up on recovery when `no_payload_default` is set.
            let eof_payload = payload_default_expr(grammar).unwrap_or_else(|| {
                quote! {
                    return __Step::Rejected(::ferrolith::ParseError::UnexpectedEof {
                        expected: error_expected,
                    })
                }
            });
            quote! {
                let __error_payload = match token {
                    Some((_, ref p)) => p.clone(),
                    None => #eof_payload,
                };
                state.payload_stack.push(__error_payload);
            }
        });

        let payload_stack_trunc = grammar.payload_type.as_ref().map(|_| {
            quote! { state.payload_stack.truncate(new_len - 1); }
        });

        let reject_original = reject(quote! { error_expected });

        let recovering_check = quote! {
            if state.recovering {
                if !__can_consume(&state.state_stack, terminal_id) {
                    if terminal_id == 0 {
                        // End of input before any token that ends recovery.
                        return __Step::Rejected(::ferrolith::ParseError::UnexpectedEof {
                            expected: state.expected_terminals(),
                        });
                    }
                    // Discard this token (and its payload).
                    return __Step::InputNeeded;
                }
                state.recovering = false;
            }
        };

        let err_handling = quote! {
            let error_expected = __expected_tokens(current_state);

            let mut pop_count = 0;
            let recovery_state = state.state_stack.iter().rev().cloned().find(|s|
                if let ::ferrolith::action::Action::Shift(_) = __action(*s, #err_id) {
                    true
                } else {
                    pop_count += 1;
                    false
                }
            );

            if let Some(candidate) = recovery_state {
                let new_len = state.state_stack.len() - pop_count;
                let shift_target = match __action(candidate, #err_id) {
                    ::ferrolith::action::Action::Shift(s) => s,
                    _ => unreachable!(),
                };

                state.state_stack.truncate(new_len);
                state.value_stack.truncate(new_len - 1);
                #payload_stack_trunc

                state.state_stack.push(shift_target);
                state.value_stack.push(__Value::T_Unit);
                #error_payload_push

                if __can_consume(&state.state_stack, terminal_id) {
                    // The offending token is itself a sync token.
                    continue;
                }
                if terminal_id == 0 {
                    return __Step::Rejected(::ferrolith::ParseError::UnexpectedEof {
                        expected: error_expected,
                    });
                }
                // Discard the offending token and wait for the next one.
                state.recovering = true;
                return __Step::InputNeeded;
            }

            #reject_original
        };

        (Some(recovering_check), err_handling)
    } else {
        (None, reject(quote! { __expected_tokens(current_state) }))
    };

    let iter_type = quote! { impl Iterator<Item = #tok_ty> };

    quote! {
        /// Outcome of feeding one token to [`__step`].
        enum __Step {
            InputNeeded,
            Accepted(#start_ret_type),
            Rejected(::ferrolith::ParseError<#tok_ty>),
        }

        /// Advances the parser by one token (`None` for end of input). Shared by
        /// `parse` and `feed`, including error recovery.
        fn __step(state: &mut ParserState, token: Option<#tok_ty>) -> __Step {
            let terminal_id = #token_id_expr;

            #recovering_check

            loop {
                let current_state = *state.state_stack.last().unwrap();
                match __action(current_state, terminal_id) {
                    ::ferrolith::action::Action::Shift(s) => {
                        state.state_stack.push(s);
                        #shift_body
                        return __Step::InputNeeded;
                    }
                    ::ferrolith::action::Action::Reduce(p) => {
                        #reduce_explicit
                    }
                    ::ferrolith::action::Action::Accept => {
                        return __Step::Accepted(match state.value_stack.pop().unwrap() {
                            __Value::#start_variant(v) => v,
                            _ => unreachable!(),
                        });
                    }
                    ::ferrolith::action::Action::Error => {
                        let default_prod = __DEFAULT_REDUCTIONS[current_state];
                        if default_prod != usize::MAX {
                            #reduce_default
                        } else {
                            #err_handling
                        }
                    }
                }
            }
        }

        #api_vis fn parse(
            mut tokens: #iter_type
        ) -> Result<#start_ret_type, ::ferrolith::ParseError<#tok_ty>> {
            let mut state_stack: Vec<usize> = Vec::with_capacity(64);
            let mut value_stack: Vec<__Value> = Vec::with_capacity(64);
            #local_payload_stack
            state_stack.push(0usize);

            let mut token = tokens.next();
            let mut terminal_id = #token_id_expr;

            loop {
                let state = *state_stack.last().unwrap();
                match __action(state, terminal_id) {
                    ::ferrolith::action::Action::Shift(s) => {
                        state_stack.push(s);
                        #local_shift
                        token = tokens.next();
                        terminal_id = #token_id_expr;
                    }
                    ::ferrolith::action::Action::Reduce(p) => {
                        #local_reduce_explicit
                    }
                    ::ferrolith::action::Action::Accept => {
                        return Ok(match value_stack.pop().unwrap() {
                            __Value::#start_variant(v) => v,
                            _ => unreachable!(),
                        });
                    }
                    ::ferrolith::action::Action::Error => {
                        let default_prod = __DEFAULT_REDUCTIONS[state];
                        if default_prod != usize::MAX {
                            #local_reduce_default
                        } else {
                            #fast_path_error
                        }
                    }
                }
            }
        }
    }
}

fn gen_incremental_api(
    api_vis: &syn::Visibility,
    grammar: &Grammar,
    internal_grammar: &InternalGrammar,
) -> proc_macro2::TokenStream {
    let start_nt_id = match internal_grammar.productions[0].rhs[0] {
        Symbol::NonTerminal(id) => id,
        _ => unreachable!(),
    };

    let start_ret_type = match &internal_grammar.ret_types[start_nt_id] {
        Some(ty) => ty.clone(),
        None => syn::parse_quote!(()),
    };

    let tokens_type = &grammar.tokens_type;

    let derive = grammar
        .cfg
        .clonable_state
        .then(|| quote! { #[derive(Clone)] });

    let payload_stack_field = if let Some(ref ty) = grammar.payload_type {
        Some(quote! {
            pub payload_stack: Vec<#ty>,
        })
    } else {
        None
    };

    let checkpoint_err_ty = if let Some(ref ty) = grammar.payload_type {
        quote! {
            (#tokens_type, #ty)
        }
    } else {
        quote! {
            #tokens_type
        }
    };

    let types = quote! {
        /// The intermediate parse state, carried inside [`Checkpoint::InputNeeded`].
        ///
        /// Obtain it by calling [`start`] or by unwrapping [`Checkpoint::InputNeeded`].
        /// Pass it to [`feed`] together with the next token to advance the parse.
        #derive
        #api_vis struct ParserState {
            pub state_stack: Vec<usize>,
            pub value_stack: Vec<__Value>,
            #payload_stack_field
            /// Discarding input after a syntax error, until a token lets the
            /// parser resume. See [`ParserState::is_recovering`].
            recovering: bool,
        }

        impl ParserState {
            /// Whether the parser is discarding input after a syntax error: an
            /// `error` alternative matched, and tokens are dropped until one of
            /// the tokens that can follow it arrives. Discarded tokens still
            /// return [`Checkpoint::InputNeeded`].
            #api_vis fn is_recovering(&self) -> bool {
                self.recovering
            }

            /// Returns the numeric LR state the parser is currently in.
            #api_vis fn current_state(&self) -> usize {
                *self.state_stack.last().unwrap()
            }

            /// Returns the names of the terminals that would advance the parse
            /// without error in the current state.
            ///
            /// Equivalent to the `expected` field on [`::ferrolith::ParseError`].
            #api_vis fn expected_terminals(&self) -> Vec<&'static str> {
                let mut states = self.state_stack.clone();
                loop {
                    let state = *states.last().unwrap();
                    let default_prod = __DEFAULT_REDUCTIONS[state];
                    if default_prod == usize::MAX {
                        return __expected_tokens(state)
                    }

                    let (lhs, rhs_len) = __prod_info(default_prod);
                    let trunc_len = states.len() - rhs_len;
                    states.truncate(trunc_len);
                    states.push(__goto(*states.last().unwrap(), lhs));
                }
            }
        }

        /// The result of a single parse step, returned by [`start`] and [`feed`].
        #api_vis enum Checkpoint {
            /// The parser consumed the last token and is ready for the next one.
            /// Call [`feed`] with `Some(token)` for the next token, or `None` to
            /// signal end of input.
            InputNeeded(ParserState),
            /// The grammar was fully satisfied. The inner value is the result of
            /// the start rule's action expression.
            Accepted(#start_ret_type),
            /// The parser encountered an error it could not recover from. The first
            /// field is the parser state at the point of failure (preserved for
            /// diagnostics); the second describes what went wrong.
            Rejected(ParserState, ::ferrolith::ParseError<#checkpoint_err_ty>),
        }
    };

    let payload_stack_init = grammar
        .payload_type
        .as_ref()
        .map(|_| quote! { payload_stack: Vec::with_capacity(64), });

    let initial_state_fn = quote! {
        fn __initial_state() -> ParserState {
            let mut state_stack = Vec::with_capacity(64);
            state_stack.push(0usize);
            ParserState {
                state_stack,
                value_stack: Vec::with_capacity(64),
                #payload_stack_init
                recovering: false,
            }
        }
    };

    let start_fn = quote! {
        /// Creates a fresh parser in its initial state.
        ///
        /// Returns [`Checkpoint::InputNeeded`] containing the initial [`ParserState`].
        /// Feed the first token with [`feed`].
        #api_vis fn start() -> Checkpoint {
            Checkpoint::InputNeeded(__initial_state())
        }
    };

    let prod_info_fn = {
        let prod_arms = internal_grammar
            .productions
            .iter()
            .enumerate()
            .map(|(prod_id, prod)| {
                let lhs = prod.lhs;
                let rhs_len = prod.rhs.len();
                quote! { #prod_id => (#lhs, #rhs_len) }
            });
        quote! {
            fn __prod_info(prod_id: usize) -> (usize, usize) {
                match prod_id {
                    #(#prod_arms,)*
                    _ => unreachable!(),
                }
            }
        }
    };

    let token_ty = if let Some(ref ty) = grammar.payload_type {
        quote! { (#tokens_type, #ty) }
    } else {
        quote! { #tokens_type }
    };

    let feed_fn = quote! {
        /// Advances the parser by one token.
        ///
        /// Pass `Some(token)` to supply the next token, or `None` to signal end of
        /// input. The returned [`Checkpoint`] describes the outcome. Error recovery
        /// works as in `parse`: while [`ParserState::is_recovering`], discarded
        /// tokens return [`Checkpoint::InputNeeded`].
        ///
        /// Only call this with a [`ParserState`] obtained from
        /// [`Checkpoint::InputNeeded`]; calling it after acceptance or rejection is
        /// a logic error.
        #api_vis fn feed(mut state: ParserState, token: Option<#token_ty>) -> Checkpoint {
            match __step(&mut state, token) {
                __Step::InputNeeded => Checkpoint::InputNeeded(state),
                __Step::Accepted(v) => Checkpoint::Accepted(v),
                __Step::Rejected(e) => Checkpoint::Rejected(state, e),
            }
        }
    };

    let can_consume_fn = quote! {
        fn __can_consume(stack: &[usize], t: usize) -> bool {
            let mut states = stack.to_vec();
            loop {
                let s = *states.last().unwrap();
                let prod = match __action(s, t) {
                    ::ferrolith::action::Action::Shift(_) | ::ferrolith::action::Action::Accept => return true,
                    ::ferrolith::action::Action::Reduce(p) => p,
                    ::ferrolith::action::Action::Error => match __DEFAULT_REDUCTIONS[s] {
                        usize::MAX => return false,
                        p => p,
                    },
                };
                let (lhs, rhs_len) = __prod_info(prod);
                states.truncate(states.len() - rhs_len);
                states.push(__goto(*states.last().unwrap(), lhs));
            }
        }
    };

    quote! {
        #types

        #initial_state_fn

        #start_fn

        #prod_info_fn

        #can_consume_fn

        #feed_fn
    }
}

fn tokens_mention(ts: proc_macro2::TokenStream, name: &syn::Ident) -> bool {
    for tk in ts {
        match tk {
            TokenTree::Ident(i) if i == *name => return true,
            TokenTree::Group(g) if tokens_mention(g.stream(), name) => return true,
            _ => (),
        }
    }
    false
}

fn symbol_to_tokens(sym: &Symbol) -> proc_macro2::TokenStream {
    match sym {
        Symbol::Terminal(id) => quote!(::ferrolith::Symbol::Terminal(#id)),
        Symbol::NonTerminal(id) => quote!(::ferrolith::Symbol::Nonterminal(#id)),
    }
}

fn gen_automaton_view(
    api_vis: &syn::Visibility,
    internal_grammar: &InternalGrammar,
    n_states: usize,
    incoming: &[Option<Symbol>],
) -> proc_macro2::TokenStream {
    let terminal_names: Vec<_> = internal_grammar
        .terminals
        .iter()
        .map(|t| t.to_string())
        .collect();
    let nonterminal_names: Vec<_> = internal_grammar
        .nonterminals
        .iter()
        .map(|nt| nt.to_string())
        .collect();

    // One static array per production holding its RHS symbols.
    let rhs_statics: Vec<_> = internal_grammar
        .productions
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let name = format_ident!("__PROD_RHS_{}", i);
            let len = p.rhs.len();
            let syms: Vec<_> = p.rhs.iter().map(symbol_to_tokens).collect();
            quote! {
                static #name: [::ferrolith::Symbol; #len] = [#(#syms,)*];
            }
        })
        .collect();

    let production_infos: Vec<_> = internal_grammar
        .productions
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let name = format_ident!("__PROD_RHS_{}", i);
            let lhs = p.lhs;
            quote! {
                ::ferrolith::ProductionInfo { lhs: #lhs, rhs: &#name }
            }
        })
        .collect();

    let state_infos: Vec<_> = incoming
        .iter()
        .map(|sym| match sym {
            None => quote!(::ferrolith::StateInfo { incoming: None }),
            Some(s) => {
                let s = symbol_to_tokens(s);
                quote!(::ferrolith::StateInfo { incoming: Some(#s) })
            }
        })
        .collect();

    quote! {
        #(#rhs_statics)*

        #api_vis static AUTOMATON: ::ferrolith::AutomatonView = ::ferrolith::AutomatonView {
            n_states: #n_states,
            terminals: &[#(#terminal_names,)*],
            nonterminals: &[#(#nonterminal_names,)*],
            productions: &[#(#production_infos,)*],
            states: &[#(#state_infos,)*],
        };

        /// The LR(1) action for `terminal` in `state`.
        ///
        /// This is the exact table entry: in a state where the parser reduces
        /// by default, it is `Reduce` only for the valid lookaheads and `Error`
        /// for the others. See `default_reduction` for what the parser does.
        #api_vis fn action(state: usize, terminal: usize) -> ::ferrolith::action::Action {
            __action_exact(state, terminal)
        }

        /// The production the parser reduces by in `state` whatever the next
        /// token is, if any.
        ///
        /// In such a state the parser reduces without looking at the token; an
        /// invalid token is reported by a later state, before it is shifted.
        #api_vis fn default_reduction(state: usize) -> Option<usize> {
            match __DEFAULT_REDUCTIONS[state] {
                usize::MAX => None,
                p => Some(p),
            }
        }

        #api_vis fn goto(state: usize, nonterminal: usize) -> usize {
            __goto(state, nonterminal)
        }
    }
}
