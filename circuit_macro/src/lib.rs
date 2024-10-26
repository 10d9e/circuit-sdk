extern crate proc_macro;
use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, BinOp, Expr, ExprBinary, ExprIf, ExprUnary, FnArg, ItemFn, Pat, PatType,
};

#[proc_macro_attribute]
pub fn circuit(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mode = parse_macro_input!(attr as syn::Ident).to_string(); // Retrieve the mode (e.g., "compile" or "execute")
    generate_macro(item, &mode)
}

/// Generates the macro code based on the mode (either "compile" or "execute")
fn generate_macro(item: TokenStream, mode: &str) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn);
    let fn_name = &input_fn.sig.ident; // Function name
    let inputs = &input_fn.sig.inputs; // Function input parameters

    // get the type of the first input parameter
    let type_name = if let FnArg::Typed(PatType { ty, .. }) = &inputs[0] {
        quote! {#ty}
    } else {
        panic!("Expected typed argument");
    };

    // get the type of the first output parameter
    let output_type = if let syn::ReturnType::Type(_, ty) = &input_fn.sig.output {
        quote! {#ty}
    } else {
        panic!("Expected typed return type");
    };

    // We need to extract each input's identifier
    let mapped_inputs = inputs.iter().map(|input| {
        if let FnArg::Typed(PatType { pat, .. }) = input {
            if let Pat::Ident(pat_ident) = &**pat {
                let var_name = &pat_ident.ident;
                quote! {
                    let #var_name = &context.input(&#var_name.clone().into());
                }
            } else {
                quote! {}
            }
        } else {
            quote! {}
        }
    });

    // Replace "+" with context.add and handle if/else in the function body
    let transformed_block = modify_body(*input_fn.block);

    // Collect parameter names dynamically
    let param_names: Vec<_> = inputs
        .iter()
        .map(|input| {
            if let FnArg::Typed(PatType { pat, .. }) = input {
                if let Pat::Ident(pat_ident) = &**pat {
                    pat_ident.ident.clone()
                } else {
                    panic!("Expected identifier pattern");
                }
            } else {
                panic!("Expected typed argument");
            }
        })
        .collect();

    // Dynamically generate the `generate` function calls using the parameter names
    let match_arms = quote! {
        match std::any::type_name::<#type_name>() {
            "u8" => generate::<8, #type_name>(#(#param_names),*),
            "u16" => generate::<16, #type_name>(#(#param_names),*),
            "u32" => generate::<32, #type_name>(#(#param_names),*),
            "u64" => generate::<64, #type_name>(#(#param_names),*),
            "u128" => generate::<128, #type_name>(#(#param_names),*),
            _ => panic!("Unsupported type"),
        }
    };

    // Set the output type and operation logic based on mode
    let output_type = if mode == "compile" {
        quote! {(Circuit, Vec<bool>)}
    } else {
        quote! {#output_type}
    };

    let operation = if mode == "compile" {
        quote! {
            (context.compile(&output), context.inputs().to_vec())
        }
    } else {
        quote! {
            let compiled_circuit = context.compile(&output.into());
            let result = context.execute::<N>(&compiled_circuit).expect("Execution failed");
            result.into()
        }
    };

    // Build the function body with circuit context, compile, and execute
    let expanded = quote! {
        #[allow(non_camel_case_types)]
        fn #fn_name<#type_name>(#inputs) -> #output_type
        where
        #type_name: Into<GarbledUint<8>> + From<GarbledUint<8>>
                + Into<GarbledUint<16>> + From<GarbledUint<16>>
                + Into<GarbledUint<32>> + From<GarbledUint<32>>
                + Into<GarbledUint<64>> + From<GarbledUint<64>>
                + Into<GarbledUint<128>> + From<GarbledUint<128>>
                + Clone,
        {
            fn generate<const N: usize, #type_name>(#inputs) -> #output_type
            where
                #type_name: Into<GarbledUint<N>> + From<GarbledUint<N>> + Clone,
            {
                let mut context = CircuitBuilder::default();
                #(#mapped_inputs)*

                // Use the transformed function block (with context.add and if/else replacements)
                let output = { #transformed_block };

                #operation
            }

            #match_arms
        }
    };

    // Print the expanded code to stderr
    println!("Generated code:\n{}", expanded);

    TokenStream::from(expanded)
}

/// Traverse and transform the function body, replacing binary operators and if/else expressions.
fn modify_body(block: syn::Block) -> syn::Block {
    let stmts = block
        .stmts
        .into_iter()
        .map(|stmt| {
            match stmt {
                syn::Stmt::Expr(expr, semi_opt) => {
                    syn::Stmt::Expr(replace_expressions(expr), semi_opt)
                }
                syn::Stmt::Local(mut local) => {
                    if let Some(local_init) = &mut local.init {
                        // Replace the initializer expression
                        local_init.expr = Box::new(replace_expressions(*local_init.expr.clone()));
                    }
                    syn::Stmt::Local(local)
                }
                other => other,
            }
        })
        .collect();

    syn::Block {
        stmts,
        brace_token: syn::token::Brace::default(),
    }
}

/// Replaces binary operators and if/else expressions with appropriate context calls.
fn replace_expressions(expr: Expr) -> Expr {
    match expr {
        Expr::Binary(ExprBinary {
            left,
            right,
            op: BinOp::Eq(_),
            ..
        }) => {
            let left = replace_expressions(*left);
            let right = replace_expressions(*right);
            syn::parse_quote! {{
                &context.eq(&#left, &#right)
            }}
        }
        Expr::Binary(ExprBinary {
            left,
            right,
            op: BinOp::Ne(_),
            ..
        }) => {
            let left = replace_expressions(*left);
            let right = replace_expressions(*right);
            syn::parse_quote! {{
                &context.ne(&#left, &#right)
            }}
        }
        Expr::Binary(ExprBinary {
            left,
            right,
            op: BinOp::Gt(_),
            ..
        }) => {
            let left = replace_expressions(*left);
            let right = replace_expressions(*right);
            syn::parse_quote! {{
                &context.gt(&#left, &#right)
            }}
        }
        Expr::Binary(ExprBinary {
            left,
            right,
            op: BinOp::Ge(_),
            ..
        }) => {
            let left = replace_expressions(*left);
            let right = replace_expressions(*right);
            syn::parse_quote! {{
                &context.ge(&#left, &#right)
            }}
        }
        Expr::Binary(ExprBinary {
            left,
            right,
            op: BinOp::Lt(_),
            ..
        }) => {
            let left = replace_expressions(*left);
            let right = replace_expressions(*right);
            syn::parse_quote! {{
                &context.lt(&#left, &#right)
            }}
        }
        Expr::Binary(ExprBinary {
            left,
            right,
            op: BinOp::Le(_),
            ..
        }) => {
            let left = replace_expressions(*left);
            let right = replace_expressions(*right);
            syn::parse_quote! {{
                &context.le(&#left, &#right)
            }}
        }
        Expr::Binary(ExprBinary {
            left,
            right,
            op: BinOp::Add(_),
            ..
        }) => {
            syn::parse_quote! {{
                &context.add(&#left, &#right)
            }}
        }
        // subtraction
        Expr::Binary(ExprBinary {
            left,
            right,
            op: BinOp::Sub(_),
            ..
        }) => {
            syn::parse_quote! {{
                &context.sub(&#left, &#right)
            }}
        }
        // multiplication
        Expr::Binary(ExprBinary {
            left,
            right,
            op: BinOp::Mul(_),
            ..
        }) => {
            syn::parse_quote! {{
                &context.mul(&#left, &#right)
            }}
        }
        // division - TODO: Implement division
        Expr::Binary(ExprBinary {
            left,
            right,
            op: BinOp::Div(_),
            ..
        }) => {
            syn::parse_quote! {{
                &context.div(&#left, &#right)
            }}
        }
        // modulo - TODO: Implement modulo
        Expr::Binary(ExprBinary {
            left,
            right,
            op: BinOp::Rem(_),
            ..
        }) => {
            syn::parse_quote! {{
                &context.rem(&#left, &#right)
            }}
        }
        // bitwise AND
        Expr::Binary(ExprBinary {
            left,
            right,
            op: BinOp::BitAnd(_),
            ..
        }) => {
            syn::parse_quote! {{
                &context.and(&#left, &#right)
            }}
        }
        // bitwise OR
        Expr::Binary(ExprBinary {
            left,
            right,
            op: BinOp::BitOr(_),
            ..
        }) => {
            syn::parse_quote! {{
                &context.or(&#left, &#right)
            }}
        }
        // bitwise XOR
        Expr::Binary(ExprBinary {
            left,
            right,
            op: BinOp::BitXor(_),
            ..
        }) => {
            syn::parse_quote! {{
                &context.xor(&#left, &#right)
            }}
        }
        // bitwise NOT
        Expr::Unary(ExprUnary {
            op: syn::UnOp::Not(_),
            expr,
            ..
        }) => {
            syn::parse_quote! {{
                &context.not(&#expr)
            }}
        }
        // Handle if/else by translating to context.mux
        // Handle if/else by translating to context.mux
        Expr::If(ExprIf {
            cond,
            then_branch,
            else_branch,
            ..
        }) => {
            if let Some((_, else_branch)) = else_branch {
                let then_expr = modify_body(then_branch.clone());

                let else_expr = match *else_branch {
                    syn::Expr::Block(syn::ExprBlock { block, .. }) => modify_body(block.clone()),
                    _ => panic!("Expected a block in else branch"),
                };

                let cond = replace_expressions(*cond.clone());

                syn::parse_quote! {{
                    let if_true = #then_expr;
                    let if_false = #else_expr;
                    let cond = #cond;
                    &context.mux(cond, if_true, if_false)
                }}
            } else {
                panic!("Expected else branch for if expression");
            }
        }

        other => other,
    }
}
