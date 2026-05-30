use std::collections::HashSet;

use ferrolith_core::{
    automaton,
    first::FirstSet,
    grammar::{InternalGrammar, Symbol},
    table::{self, Action, Actions, Gotos},
};
use heck::ToPascalCase;
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::parse_macro_input;

use crate::grammar::{Grammar, GrammarSymbol};

mod grammar;
mod internalize;

#[proc_macro]
pub fn grammar(input: TokenStream) -> TokenStream {
    let grammar = parse_macro_input!(input as Grammar);

    let internal_grammar = match internalize::internalize(&grammar) {
        Ok(g) => g,
        Err(e) => return e.to_compile_error().into(),
    };

    let firsts = FirstSet::compute(&internal_grammar.productions);

    let automaton = match automaton::build_automaton(&internal_grammar, &firsts) {
        Ok(a) => a,
        Err(e) => return e.to_compile_error().into(),
    };

    let tables =
        match table::build_tables(automaton.states, automaton.transitions, &internal_grammar) {
            Ok(t) => t,
            Err(e) => return e.into_compile_error().into(),
        };

    let value_enum = gen_value_enum(&grammar, &internal_grammar);

    let token_enum = gen_token_enum(&grammar, &internal_grammar);

    let actions_fn = gen_action_function(&tables.actions);

    let gotos_fn = gen_goto_function(&tables.gotos);

    let reduce_fn = gen_reduce_function(&grammar, &internal_grammar);

    let parse_fn = gen_parse_function(&grammar, &internal_grammar);

    let incremental_api = gen_incremental_api(&grammar, &internal_grammar);

    quote! {
        #value_enum

        #token_enum

        #actions_fn

        #gotos_fn

        #reduce_fn

        #parse_fn

        #incremental_api

    }
    .into()
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

    quote! {
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
    }
}

fn gen_action_function(actions: &Actions) -> proc_macro2::TokenStream {
    fn action_to_quote(action: &Action) -> proc_macro2::TokenStream {
        match action {
            Action::Shift(id) => quote! { ::ferrolith::action::Action::Shift(#id) },
            Action::Reduce(id) => quote! { ::ferrolith::action::Action::Reduce(#id) },
            Action::Accept => quote! { ::ferrolith::action::Action::Accept },
            Action::Error => quote! { ::ferrolith::action::Action::Error },
        }
    }

    let action_arms: Vec<_> = actions
        .iter()
        .map(|((state_id, terminal_id), action)| {
            let action_toks = action_to_quote(action);

            quote! { (#state_id, #terminal_id) => #action_toks }
        })
        .collect();

    quote! {
        fn __action(state: usize, terminal: usize) -> ::ferrolith::action::Action {
            match (state, terminal) {
                #(#action_arms,)*
                _ => ::ferrolith::action::Action::Error
            }
        }
    }
}

fn gen_goto_function(gotos: &Gotos) -> proc_macro2::TokenStream {
    let goto_arms: Vec<_> = gotos
        .iter()
        .map(|((state_id, nonterminal_id), dest_id)| {
            quote! { (#state_id, #nonterminal_id) => #dest_id }
        })
        .collect();

    quote! {
        fn __goto(state: usize, nonterminal: usize) -> usize {
            match (state, nonterminal) {
                #(#goto_arms,)*
                _ => unreachable!("Automaton should be complete by construction")
            }
        }
    }
}

fn gen_reduce_function(
    grammar: &Grammar,
    internal_grammar: &InternalGrammar,
) -> proc_macro2::TokenStream {
    let mut prod_code = vec![];

    let tokens_type = &grammar.tokens_type.ty;

    for (prod_id, production) in internal_grammar.productions.iter().enumerate() {
        if let Some(action_id) = production.action {
            let (rule_ident, arm_idx) = &internal_grammar.actions_order[action_id];
            let arm = &grammar
                .rules
                .iter()
                .find(|rule| rule.name == *rule_ident)
                .unwrap()
                .arms[*arm_idx];
            let lhs_ident = &internal_grammar.nonterminals[production.lhs];
            let lhs_variant = format_ident!("N_{}", lhs_ident.to_string().to_pascal_case());
            let has_payload = internal_grammar.ret_types[production.lhs].is_some();

            let mut arms_assigns = vec![];
            for (idx, symbol) in arm.terms.iter().rev().enumerate() {
                #[allow(non_snake_case)]
                let vN = format_ident!("__v{}", idx);
                let assign = quote! { let #vN = value_stack.pop().unwrap(); };
                let symbol_quote = match symbol {
                    GrammarSymbol::Bound { name, sym } => match sym.as_ref() {
                        GrammarSymbol::Rule(ident) => {
                            let variant_ident =
                                format_ident!("N_{}", ident.to_string().to_pascal_case());
                            quote! {
                                let #name = match #vN {
                                    __Value::#variant_ident(v) => v,
                                    _ => unreachable!()
                                };
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
                        GrammarSymbol::Token(_) => {
                            quote! {
                                let #name = match #vN {
                                    __Value::T_Token(v) => v,
                                    _ => unreachable!(),
                                };
                            }
                        }
                        GrammarSymbol::App {
                            name: app_name,
                            args,
                        } => {
                            let synthetic_ident = internalize::app_ident(app_name, args);
                            let variant_ident =
                                format_ident!("N_{}", synthetic_ident.to_string().to_pascal_case());
                            quote! {
                                let #name = match #vN {
                                    __Value::#variant_ident(v) => v,
                                    _ => unreachable!(),
                                };
                            }
                        }
                        _ => quote! {
                            let _ = #vN;
                        },
                    },
                    _ => quote! {
                        let _ = #vN;
                    },
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

    let token_type = grammar.tokens_type.ty.as_ref();

    quote! {
        fn parse(
            mut tokens: impl Iterator<Item = #token_type>
        ) -> Result<#start_ret_type, ::ferrolith::ParseError<#token_type>> {
            let mut checkpoint = __start();
            loop {
                match checkpoint {
                    Checkpoint::InputNeeded(state) => {
                        let token = match tokens.next() {
                            Some(t) => __Token::Real(t),
                            None => __Token::Eof,
                        };
                        checkpoint = __feed(state, token);
                    }
                    Checkpoint::Accepted(v) => return Ok(v),
                    Checkpoint::Rejected(e) => return Err(e),
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

    let types = quote! {
        struct __ParserState {
            state_stack: Vec<usize>,
            value_stack: Vec<__Value>,
        }

        enum Checkpoint {
            InputNeeded(__ParserState),
            Accepted(#start_ret_type),
            Rejected(::ferrolith::ParseError<#tokens_type>),
        }
    };

    let start_fn = quote! {
        fn __start() -> Checkpoint {
            let state_stack = vec![0usize];
            let value_stack = vec![];
            Checkpoint::InputNeeded( __ParserState { state_stack, value_stack })
        }
    };

    let prod_lhs_fn = {
        let prod_arms = internal_grammar
            .productions
            .iter()
            .enumerate()
            .map(|(prod_id, prod)| {
                let prod_lhs = prod.lhs;
                quote! { #prod_id =>  #prod_lhs }
            });
        quote! {
            fn __prod_lhs(prod_id: usize) -> usize {
                match prod_id {
                    #(#prod_arms,)*
                    _ => unreachable!(),
                }
            }
        }
    };

    let prod_rhs_len_fn = {
        let prod_arms = internal_grammar
            .productions
            .iter()
            .enumerate()
            .map(|(prod_id, prod)| {
                let prod_len = prod.rhs.len();
                quote! { #prod_id => #prod_len }
            });
        quote! {
            fn __prod_rhs_len(prod_id: usize) -> usize {
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
        fn __feed(mut state: __ParserState, token: __Token) -> Checkpoint {
            let terminal_id = __token_id(&token);
            loop {
                let current_state = state.state_stack.last().unwrap();
                match __action(*current_state, terminal_id) {
                    ::ferrolith::action::Action::Shift(s) => {
                        state.state_stack.push(s);
                        if let __Token::Real(t) = token {
                            state.value_stack.push(__Value::T_Token(t));
                        } else {
                            panic!("Not a real token in shift action.");
                        }
                        return Checkpoint::InputNeeded(state);
                    }
                    ::ferrolith::action::Action::Reduce(p) => {
                        let lhs = __prod_lhs(p);
                        let result = __reduce(p, &mut state.value_stack);
                        let rhs_len = __prod_rhs_len(p);
                        for _ in 0..rhs_len {
                            state.state_stack.pop();
                        }
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
                        match token {
                            __Token::Real(t) =>
                                return Checkpoint::Rejected(
                                    ::ferrolith::ParseError::UnexpectedToken(t)
                                ),
                            __Token::Eof =>
                                return Checkpoint::Rejected(
                                    ::ferrolith::ParseError::UnexpectedEof
                                ),
                        }
                    }
                }
            }
        }
    };

    quote! {
        #types

        #start_fn

        #prod_lhs_fn

        #prod_rhs_len_fn

        #feed_fn
    }
}
