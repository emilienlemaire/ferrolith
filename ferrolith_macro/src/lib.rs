use std::{collections::HashSet, usize};

use ferrolith_core::{
    automaton,
    first::FirstSet,
    grammar::{InternalGrammar, Symbol},
    table::{self, Action, Actions, Gotos, ShiftReduceConflict},
};
use heck::ToPascalCase;
use proc_macro::TokenStream;
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

    let actions_fn = gen_action_function(&actions, n_states, n_terminals, &defaults);

    let gotos_fn = gen_goto_function(&gotos, n_states, n_nonterminals);

    let reduce_fn = gen_reduce_function(&grammar, &internal_grammar, &rule_set);

    let parse_fn = gen_parse_function(&grammar, &internal_grammar, error_id);

    let incremental_api = gen_incremental_api(&grammar, &internal_grammar);

    let automaton_view = grammar
        .cfg
        .expose_automaton
        .then(|| gen_automaton_view(&internal_grammar, n_states, &incoming));

    quote! {
        #(#warnings)*

        #value_enum

        #token_enum

        #expected_fn

        #actions_fn

        #default_reductions_fn

        #gotos_fn

        #reduce_fn

        #parse_fn

        #incremental_api

        #automaton_view
    }
    .into()
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
    let token_ty = &grammar.tokens_type;

    let captures = collect_capture_terminals(grammar);

    let tokens_arms: Vec<_> = internal_grammar
        .terminals
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(id, _)| Some(*id) != error_terminal_id)
        .map(|(id, ident)| {
            if captures.contains(ident) {
                quote! { #token_ty::#ident(..) => #id, }
            } else {
                quote! { #token_ty::#ident => #id, }
            }
        })
        .collect();

    quote! {
        enum __Token {
            Real(#token_ty),
            Eof,
        }

        fn __token_id(token: &__Token) -> usize {
            match token {
                __Token::Eof => 0,
                __Token::Real(t) => {
                    match t {
                        #(#tokens_arms)*
                        _ => unreachable!()
                    }
                }
            }
        }

        fn __token_id_opt(token: Option<&#token_ty>) -> usize {
            match token {
                None => 0,
                Some(t) => match t {
                    #(#tokens_arms)*
                    _ => unreachable!()
                }
            }
        }
    }
}

fn gen_action_function(
    actions: &Actions,
    n_states: usize,
    n_terminals: usize,
    defaults: &[usize],
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
        let default_prod = defaults[state];
        rows.push((state, vec![None; n_terminals]));
        for ((state_id, terminal), action) in actions {
            if *state_id != state || *action == Action::Error {
                continue;
            }

            if let Action::Reduce(prod) = action
                && *prod == default_prod
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

    quote! {
        static __ACTION_DISP: [isize; #n_states] = [ #(#disp),* ];
        static __ACTION_PACKED: [::ferrolith::action::Action; #pack_len] = [ #(#packed),* ];
        static __ACTION_CHECK: [usize; #pack_len] = [ #(#check),* ];

        fn __action(state: usize, terminal: usize) -> ::ferrolith::action::Action {
            let index = __ACTION_DISP[state] + terminal as isize;
            if index >= 0 && (index as usize) < #pack_len
                && __ACTION_CHECK[index as usize] == state {
                __ACTION_PACKED[index as usize]
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
                    Some(Action::Shift(_)) | Some(Action::Accept) => {
                        has_non_reduce = true;
                        break;
                    }
                    _ => {}
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

    let tokens_type = &grammar.tokens_type;

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
                    if let Some(ref ty) = grammar.payload_type {
                        let payload_default = if let Some(ref def) = grammar.cfg.payload_default {
                            quote! { #def }
                        } else {
                            quote! { <#ty>::default() }
                        };
                        Some(quote! {
                            let #payload_id = payload_stack.pop().unwrap_or_else(|| #payload_default );
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
                                    __Value::T_Token(#tokens_type::#variant(#(#fields),*)) => (#(#fields),*),
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
                                __Value::T_Token(#tokens_type::#variant(#(#fields),*)) => (#(#fields),*),
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
                if let Some(ref ty) = grammar.payload_type {
                    if rhs_len == 0 {
                        let payload_default = if let Some(ref def) = grammar.cfg.payload_default {
                            quote! { #def }
                        } else {
                            quote! { <#ty>::default() }
                        };
                        Some(quote! {
                            _payload = #payload_default;
                        })
                    } else if rhs_len == 1 {
                        Some(quote! { _payload = _1_payload; })
                    } else {
                        match grammar.cfg.payload_merge_strategy {
                            grammar::MergeStrategy::FirstLast => {
                                let first = format_ident!("_1_payload");
                                let last = format_ident!("_{}_payload", rhs_len);
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
                                    let last = format_ident!("_{}_payload", rhs_len);
                                    let ob_last = format_ident!("_{}_payload", rhs_len - 1);
                                    gen_merge(ob_last.to_token_stream(), last.to_token_stream())
                                };
                                for id in (1..rhs_len - 1).rev() {
                                    let ident = format_ident!("_{}_payload", id);
                                    merge = gen_merge(ident.to_token_stream(), merge);
                                }
                                Some(quote! {
                                    _payload = #merge;
                                })
                            }
                            grammar::MergeStrategy::First => {
                                let ident = format_ident!("_1_payload");
                                Some(quote! {
                                    _payload = #ident;
                                })
                            }
                            grammar::MergeStrategy::Last => {
                                let ident = format_ident!("_{}_payload", rhs_len);
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

    let parse_ret_ty = if let Some(ref ty) = grammar.payload_type {
        quote! {
            (#token_type, #ty)
        }
    } else {
        quote! { #token_type }
    };

    let token_id_expr = if grammar.payload_type.is_some() {
        quote! {
            __token_id_opt(token.as_ref().map(|(t, _)| t));
        }
    } else {
        quote! {
            __token_id_opt(token.as_ref());
        }
    };

    let payload_stack_trunc = if grammar.payload_type.is_some() {
        Some(quote! {
            payload_stack.truncate(new_len.saturating_sub(1));
        })
    } else {
        None
    };

    let err_handling = if let Some(err_id) = error_terminal_id {
        quote! {
            let error_expected = __expected_tokens(state);

            let mut pop_count = 0;
            let recovery_state = state_stack.iter().rev().cloned().find(|state|
                if let ::ferrolith::action::Action::Shift(_) = __action(*state, #err_id) {
                    true
                } else {
                    pop_count += 1;
                    false
                }
            );

            if let Some(candidate) = recovery_state {
                let new_len = state_stack.len() - pop_count;
                let shift_target = match __action(candidate, #err_id) {
                    ::ferrolith::action::Action::Shift(s) => s,
                    _ => unreachable!(),
                };

                state_stack.truncate(new_len);
                value_stack.truncate(new_len.saturating_sub(1));
                #payload_stack_trunc

                state_stack.push(shift_target);
                value_stack.push(__Value::T_Unit);

                while
                    let ::ferrolith::action::Action::Error = __action(shift_target, terminal_id)
                    && terminal_id != 0 {
                        token = tokens.next();
                        terminal_id = #token_id_expr;
                }

                if let ::ferrolith::action::Action::Error = __action(shift_target, terminal_id) {
                    return Err(match token {
                        Some(t) => ::ferrolith::ParseError::UnexpectedToken {
                            token: t,
                            expected: error_expected,
                        },
                        None => ::ferrolith::ParseError::UnexpectedEof {
                            expected: error_expected,
                        },
                    });
                }

                continue;

            } else {
                return Err(match token {
                    Some(t) => ::ferrolith::ParseError::UnexpectedToken {
                        token: t,
                        expected: error_expected,
                    },
                    None => ::ferrolith::ParseError::UnexpectedEof {
                        expected: error_expected,
                    },
                });
            }

        }
    } else {
        quote! {
            return Err(match token {
                Some(t) => ::ferrolith::ParseError::UnexpectedToken {
                    token: t,
                    expected: __expected_tokens(state)
                },
                None => ::ferrolith::ParseError::UnexpectedEof {
                    expected: __expected_tokens(state)
                },
            });
        }
    };

    let iter_type = if let Some(ref ty) = grammar.payload_type {
        quote! {
            impl Iterator<Item = (#token_type, #ty)>
        }
    } else {
        quote! {
            impl Iterator<Item = #token_type>
        }
    };

    let payload_stack = if let Some(ref ty) = grammar.payload_type {
        Some(quote! {
            let mut payload_stack: Vec<#ty> = Vec::with_capacity(64);
        })
    } else {
        None
    };

    let reduce_call = if grammar.payload_type.is_some() {
        quote! {
            let (result, payload_result) = __reduce(p, &mut value_stack, &mut payload_stack);
        }
    } else {
        quote! {
            let result = __reduce(p, &mut value_stack);
        }
    };

    let reduce_call_default = if grammar.payload_type.is_some() {
        quote! {
            let (result, payload_result) =
                __reduce(default_prod, &mut value_stack, &mut payload_stack);
        }
    } else {
        quote! {
            let result = __reduce(default_prod, &mut value_stack);
        }
    };

    let reduce_payload_push = if grammar.payload_type.is_some() {
        Some(quote! {
            payload_stack.push(payload_result);
        })
    } else {
        None
    };

    let shift_body = if grammar.payload_type.is_some() {
        quote! {
            let (tok, pay) = token.unwrap();
            value_stack.push(__Value::T_Token(tok));
            payload_stack.push(pay);
        }
    } else {
        quote! {
            value_stack.push(__Value::T_Token(token.unwrap()));
        }
    };

    quote! {
        pub fn parse(
            mut tokens: #iter_type
        ) -> Result<#start_ret_type, ::ferrolith::ParseError<#parse_ret_ty>> {
            let mut state_stack: Vec<usize> = Vec::with_capacity(64);
            let mut value_stack: Vec<__Value> = Vec::with_capacity(64);
            #payload_stack
            state_stack.push(0usize);

            let mut token = tokens.next();
            let mut terminal_id = #token_id_expr;

            loop {
                let state = *state_stack.last().unwrap();
                match __action(state, terminal_id) {
                    ::ferrolith::action::Action::Shift(s) => {
                        state_stack.push(s);
                        #shift_body
                        token = tokens.next();
                        terminal_id = #token_id_expr;
                    }
                    ::ferrolith::action::Action::Reduce(p) => {
                        let (lhs, rhs_len) = __prod_info(p);
                        #reduce_call
                        state_stack.truncate(state_stack.len() - rhs_len);
                        let next = __goto(*state_stack.last().unwrap(), lhs);
                        state_stack.push(next);
                        value_stack.push(result);
                        #reduce_payload_push
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
                            let (lhs, rhs_len) = __prod_info(default_prod);
                            #reduce_call_default
                            state_stack.truncate(state_stack.len() - rhs_len);
                            let next = __goto(*state_stack.last().unwrap(), lhs);
                            state_stack.push(next);
                            value_stack.push(result);
                            #reduce_payload_push
                        } else {
                            #err_handling
                        }
                    }
                }
            }
        }
    }
}

fn gen_incremental_api(
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
        pub struct ParserState {
            pub state_stack: Vec<usize>,
            pub value_stack: Vec<__Value>,
            #payload_stack_field
        }

        impl ParserState {
            /// Returns the numeric LR state the parser is currently in.
            pub fn current_state(&self) -> usize {
                *self.state_stack.last().unwrap()
            }

            /// Returns the names of the terminals that would advance the parse
            /// without error in the current state.
            ///
            /// Equivalent to the `expected` field on [`::ferrolith::ParseError`].
            pub fn expected_terminals(&self) -> Vec<&'static str> {
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
        pub enum Checkpoint {
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

    let start_fn_body = if grammar.payload_type.is_some() {
        quote! {
            let mut state_stack = Vec::with_capacity(64);
            let value_stack = Vec::with_capacity(64);
            state_stack.push(0usize);
            let payload_stack = Vec::with_capacity(64);
            Checkpoint::InputNeeded(ParserState { state_stack, value_stack, payload_stack })
        }
    } else {
        quote! {
            let mut state_stack = Vec::with_capacity(64);
            let value_stack = Vec::with_capacity(64);
            state_stack.push(0usize);
            Checkpoint::InputNeeded(ParserState { state_stack, value_stack })
        }
    };

    let start_fn = quote! {
        /// Creates a fresh parser in its initial state.
        ///
        /// Returns [`Checkpoint::InputNeeded`] containing the initial [`ParserState`].
        /// Feed the first token with [`feed`].
        pub fn start() -> Checkpoint {
            #start_fn_body
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

    let start_variant = format_ident!(
        "N_{}",
        internal_grammar.nonterminals[start_nt_id]
            .to_string()
            .to_pascal_case()
    );

    let token_ty = if let Some(ref ty) = grammar.payload_type {
        quote! {
            (#tokens_type, #ty)
        }
    } else {
        quote! {
            #tokens_type
        }
    };

    let token_id_expr = if grammar.payload_type.is_some() {
        quote! {
            __token_id_opt(token.as_ref().map(|(t, _)| t))
        }
    } else {
        quote! {
            __token_id_opt(token.as_ref())
        }
    };

    let shift_arm = if grammar.payload_type.is_some() {
        quote! {
            let (tok, pay) = token.expect("shift on EOF is a parser bug");
            state.value_stack.push(__Value::T_Token(tok));
            state.payload_stack.push(pay);
        }
    } else {
        quote! {
            state.value_stack.push(
                __Value::T_Token(
                    token.expect("shift on EOF is a parser bug")
                )
            );
        }
    };

    let reduce_call = if grammar.payload_type.is_some() {
        quote! {
            let (result, payload_result) = __reduce(
                p,
                &mut state.value_stack,
                &mut state.payload_stack);
        }
    } else {
        quote! {
            let result = __reduce(p, &mut state.value_stack);
        }
    };

    let reduce_call_default = if grammar.payload_type.is_some() {
        quote! {
            let (result, payload_result) = __reduce(
                default_prod,
                &mut state.value_stack,
                &mut state.payload_stack);
        }
    } else {
        quote! {
            let result = __reduce(default_prod, &mut state.value_stack);
        }
    };

    let reduce_payload_push = if grammar.payload_type.is_some() {
        Some(quote! {
            state.payload_stack.push(payload_result);
        })
    } else {
        None
    };

    let feed_fn = quote! {
        /// Advances the parser by one token.
        ///
        /// Pass `Some(token)` to supply the next token, or `None` to signal end of
        /// input. The returned [`Checkpoint`] describes the outcome.
        ///
        /// Only call this with a [`ParserState`] obtained from
        /// [`Checkpoint::InputNeeded`]; calling it after acceptance or rejection is
        /// a logic error.
        pub fn feed(mut state: ParserState, token: Option<#token_ty>) -> Checkpoint {
            let terminal_id = #token_id_expr;
            loop {
                let current_state = *state.state_stack.last().unwrap();
                match __action(current_state, terminal_id) {
                    ::ferrolith::action::Action::Shift(s) => {
                        state.state_stack.push(s);
                        #shift_arm
                        return Checkpoint::InputNeeded(state);
                    }
                    ::ferrolith::action::Action::Reduce(p) => {
                        let (lhs, rhs_len) = __prod_info(p);
                        #reduce_call
                        state.state_stack.truncate(state.state_stack.len() - rhs_len);
                        let next = __goto(*state.state_stack.last().unwrap(), lhs);
                        #reduce_payload_push
                        state.state_stack.push(next);
                        state.value_stack.push(result);
                    }
                    ::ferrolith::action::Action::Accept => {
                        let v = state.value_stack.pop().unwrap();
                        let res = match v {
                            __Value::#start_variant(inner) => inner,
                            _ => unreachable!()
                        };
                        return Checkpoint::Accepted(res);
                    }
                    ::ferrolith::action::Action::Error => {
                        let default_prod = __DEFAULT_REDUCTIONS[current_state];
                        if default_prod != usize::MAX {
                            let (lhs, rhs_len) = __prod_info(default_prod);
                            #reduce_call_default
                            state.state_stack.truncate(state.state_stack.len() - rhs_len);
                            let next = __goto(*state.state_stack.last().unwrap(), lhs);
                            #reduce_payload_push
                            state.state_stack.push(next);
                            state.value_stack.push(result);
                        } else {
                            let expected = __expected_tokens(current_state);
                            return Checkpoint::Rejected(state, match token {
                                Some(t) => ::ferrolith::ParseError::UnexpectedToken {
                                    token: t,
                                    expected
                                },
                                None => ::ferrolith::ParseError::UnexpectedEof {
                                    expected
                                },
                            });
                        }
                    }
                }
            }
        }
    };

    quote! {
        #types

        #start_fn

        #prod_info_fn

        #feed_fn
    }
}

fn symbol_to_tokens(sym: &Symbol) -> proc_macro2::TokenStream {
    match sym {
        Symbol::Terminal(id) => quote!(::ferrolith::Symbol::Terminal(#id)),
        Symbol::NonTerminal(id) => quote!(::ferrolith::Symbol::Nonterminal(#id)),
    }
}

fn gen_automaton_view(
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

        pub static AUTOMATON: ::ferrolith::AutomatonView = ::ferrolith::AutomatonView {
            n_states: #n_states,
            terminals: &[#(#terminal_names,)*],
            nonterminals: &[#(#nonterminal_names,)*],
            productions: &[#(#production_infos,)*],
            states: &[#(#state_infos,)*],
        };

        pub fn action(state: usize, terminal: usize) -> ::ferrolith::action::Action {
            __action(state, terminal)
        }

        pub fn goto(state: usize, nonterminal: usize) -> usize {
            __goto(state, nonterminal)
        }
    }
}
