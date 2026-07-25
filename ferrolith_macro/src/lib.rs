use std::collections::HashSet;

use ferrolith_core::{
    automaton,
    first::FirstSet,
    grammar::{InternalGrammar, Symbol},
    table::{self, Action, Actions, Gotos, ShiftReduceConflict},
};
use heck::ToPascalCase;
use proc_macro::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use syn::parse_macro_input;

use crate::{grammar::{Grammar, GrammarSymbol, Rule}, internalize::app_ident};
use crate::internalize::RuleSet;

mod grammar;
mod internalize;

#[proc_macro]
pub fn grammar(input: TokenStream) -> TokenStream {
    let grammar = parse_macro_input!(input as Grammar);

    let (internal_grammar, rule_set) = match internalize::internalize(&grammar) {
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

    let token_enum = gen_token_enum(&grammar, &internal_grammar);

    let expected_fn = gen_expected_tokens_fn(&internal_grammar);

    let actions_fn = gen_action_function(&actions, n_states, n_terminals);

    let gotos_fn = gen_goto_function(&gotos, n_states, n_nonterminals);

    let reduce_fn = gen_reduce_function(&grammar, &internal_grammar, &rule_set);

    let parse_fn = gen_parse_function(&grammar, &internal_grammar);

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

        #gotos_fn

        #reduce_fn

        #parse_fn

        #incremental_api

        #automaton_view
    }
    .into()
}

fn gen_expected_tokens_fn(internal_grammar: &InternalGrammar) -> proc_macro2::TokenStream {
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

    let tokens_type = &grammar.tokens_type.ty;

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
        GrammarSymbol::Rule(_) => (),
        GrammarSymbol::Token(_) => (),
        GrammarSymbol::Capture { variant, .. } => {
            captures.insert(variant.clone());
        }
        GrammarSymbol::App { args, .. } => {
            for arg in args {
                collect_symbol(arg, captures);
            }
        }
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
) -> proc_macro2::TokenStream {
    let token_type = grammar.tokens_type.ty.as_ref();

    let captures = collect_capture_terminals(grammar);

    let tokens_arms: Vec<_> = internal_grammar
        .terminals
        .iter()
        .enumerate()
        .skip(1)
        .map(|(id, ident)| {
            if captures.contains(ident) {
                quote! { #token_type::#ident(..) => #id, }
            } else {
                quote! { #token_type::#ident => #id, }
            }
        })
        .collect();

    quote! {
        enum __Token {
            Real(#token_type),
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

        fn __token_id_opt(token: Option<&#token_type>) -> usize {
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
) -> proc_macro2::TokenStream {
    let table_size = n_states * n_terminals;
    let mut table = vec![quote!(::ferrolith::action::Action::Error); table_size];
    for ((state_id, terminal_id), action) in actions {
        let entry = match action {
            Action::Error => quote!(::ferrolith::action::Action::Error),
            Action::Accept => quote!(::ferrolith::action::Action::Accept),
            Action::Shift(n) => quote!(::ferrolith::action::Action::Shift(#n)),
            Action::Reduce(n) => quote!(::ferrolith::action::Action::Reduce(#n)),
        };
        table[state_id * n_terminals + terminal_id] = entry;
    }

    quote! {
        const __N_TERMINALS: usize = #n_terminals;
        static __ACTION_TABLE: [::ferrolith::action::Action; #table_size] = [#(#table,)*];

        fn __action(state: usize, terminal: usize) -> ::ferrolith::action::Action {
            __ACTION_TABLE[state * __N_TERMINALS + terminal]
        }
    }
}

fn gen_goto_function(
    gotos: &Gotos,
    n_states: usize,
    n_nonterminals: usize,
) -> proc_macro2::TokenStream {
    let table_size = n_states * n_nonterminals;
    let mut table = vec![usize::MAX; table_size];
    for ((state_id, nonterminal_id), dest_id) in gotos {
        table[state_id * n_nonterminals + nonterminal_id] = *dest_id;
    }

    quote! {
        const __N_NONTERMINALS: usize = #n_nonterminals;
        static __GOTO_TABLE: [usize; #table_size] = [#(#table,)*];

        fn __goto(state: usize, nonterminal: usize) -> usize {
            __GOTO_TABLE[state * __N_NONTERMINALS + nonterminal]
        }
    }
}

fn gen_reduce_function(
    grammar: &Grammar,
    internal_grammar: &InternalGrammar,
    rule_set: &RuleSet,
) -> proc_macro2::TokenStream {
    let mut prod_code = vec![];

    let tokens_type = &grammar.tokens_type.ty;

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
                let assign = quote! { let #vN = value_stack.pop().unwrap(); };
                let pos = format_ident!("_{}", rhs_len - idx);
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
                        let ret_type = nonterm_idx.map(|idx| {
                            internal_grammar
                                .ret_types
                                .get(idx)
                                .map(|o| o.clone())
                                .flatten()
                        }).flatten();
                        match ret_type {
                            Some(_) => {
                                let variant_ident = format_ident!(
                                    "N_{}",
                                    ident.to_string().to_pascal_case()
                                );
                                quote! {
                                    let #pos = match #vN {
                                        __Value::#variant_ident(v) => v,
                                        _ => unreachable!(),
                                    };
                                }
                            }
                            None => {
                                quote! {
                                    let _ = vN;
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
                    GrammarSymbol::App { name: app_name, args } => {
                        let synthetic = app_ident(app_name, args);
                        let variant_ident = format_ident!("N_{}", synthetic.to_string().to_pascal_case());
                        quote! {
                            let #pos = match #vN {
                                __Value::#variant_ident(v) = v,
                                _ => unreachable!(),
                            };
                        }
                    }
                };
                arms_assigns.push(quote! {
                    #assign
                    #symbol_quote
                });
            }

            let action = &arm.action;

            let ret_toks = if has_payload {
                quote! { __Value::#lhs_variant(#action) }
            } else {
                quote! {
                    #action;
                    __Value::#lhs_variant
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

    quote! {
        fn __reduce(prod_id: usize, value_stack: &mut Vec<__Value>) -> __Value {
            match prod_id {
                #(#prod_id => { #prod_code },)*
                _ => unreachable!(),
            }
        }
    }
}

fn gen_parse_function(
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

    let start_variant = format_ident!(
        "N_{}",
        internal_grammar.nonterminals[start_nt_id]
            .to_string()
            .to_pascal_case()
    );

    let token_type = grammar.tokens_type.ty.as_ref();

    quote! {
        pub fn parse(
            mut tokens: impl Iterator<Item = #token_type>
        ) -> Result<#start_ret_type, ::ferrolith::ParseError<#token_type>> {
            let mut state_stack: Vec<usize> = Vec::with_capacity(64);
            let mut value_stack: Vec<__Value> = Vec::with_capacity(64);
            state_stack.push(0usize);

            let mut token = tokens.next();
            let mut terminal_id = __token_id_opt(token.as_ref());

            loop {
                let state = *state_stack.last().unwrap();
                match __action(state, terminal_id) {
                    ::ferrolith::action::Action::Shift(s) => {
                        state_stack.push(s);
                        value_stack.push(__Value::T_Token(token.unwrap()));
                        token = tokens.next();
                        terminal_id = __token_id_opt(token.as_ref());
                    }
                    ::ferrolith::action::Action::Reduce(p) => {
                        let (lhs, rhs_len) = __prod_info(p);
                        let result = __reduce(p, &mut value_stack);
                        state_stack.truncate(state_stack.len() - rhs_len);
                        let next = __goto(*state_stack.last().unwrap(), lhs);
                        state_stack.push(next);
                        value_stack.push(result);
                    }
                    ::ferrolith::action::Action::Accept => {
                        return Ok(match value_stack.pop().unwrap() {
                            __Value::#start_variant(v) => v,
                            _ => unreachable!(),
                        });
                    }
                    ::ferrolith::action::Action::Error => {
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

    let tokens_type = grammar.tokens_type.ty.as_ref();

    let derive = grammar
        .cfg
        .clonable_state
        .then(|| quote! { #[derive(Clone)] });

    let types = quote! {
        #derive
        pub struct ParserState {
            state_stack: Vec<usize>,
            value_stack: Vec<__Value>,
        }

        impl ParserState {
            pub fn current_state(&self) -> usize {
                *self.state_stack.last().unwrap()
            }

            pub fn expected_terminals(&self) -> Vec<&'static str> {
                __expected_tokens(self.current_state())
            }
        }

        pub enum Checkpoint {
            InputNeeded(ParserState),
            Accepted(#start_ret_type),
            Rejected(ParserState, ::ferrolith::ParseError<#tokens_type>),
        }
    };

    let start_fn = quote! {
        pub fn start() -> Checkpoint {
            let mut state_stack = Vec::with_capacity(64);
            let value_stack = Vec::with_capacity(64);
            state_stack.push(0usize);
            Checkpoint::InputNeeded(ParserState { state_stack, value_stack })
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

    let feed_fn = quote! {
        pub fn feed(mut state: ParserState, token: Option<#tokens_type>) -> Checkpoint {
            let terminal_id = __token_id_opt(token.as_ref());
            loop {
                let current_state = *state.state_stack.last().unwrap();
                match __action(current_state, terminal_id) {
                    ::ferrolith::action::Action::Shift(s) => {
                        state.state_stack.push(s);
                        state.value_stack.push(__Value::T_Token(
                            token.expect("shift on EOF is a parser bug"),
                        ));
                        return Checkpoint::InputNeeded(state);
                    }
                    ::ferrolith::action::Action::Reduce(p) => {
                        let (lhs, rhs_len) = __prod_info(p);
                        let result = __reduce(p, &mut state.value_stack);
                        state.state_stack.truncate(state.state_stack.len() - rhs_len);
                        let next = __goto(*state.state_stack.last().unwrap(), lhs);
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
