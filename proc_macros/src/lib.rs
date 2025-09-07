use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Expr};

/// Usage: `from!(edge_expr)`
/// Expands to: `graph.nodes[graph.edges[edge_expr].from]`
/// XXX: Requires `graph` to be available at the caller side. The whole point of this macro being a
/// procedural macro is to avoid typing `graph` so often.
#[proc_macro]
pub fn from(input: TokenStream) -> TokenStream {
    let edge_expr: Expr = parse_macro_input!(input as Expr);

    let expanded = quote! {
        graph.nodes[ graph.edges[#edge_expr].from ]
    };

    TokenStream::from(expanded)
}

/// Usage: `to!(edge_expr)`
/// Expands to: `graph.nodes[graph.edges[edge_expr].to]`
/// XXX: Requires `graph` to be available at the caller side. The whole point of this macro being a
/// procedural macro is to avoid typing `graph` so often.
#[proc_macro]
pub fn to(input: TokenStream) -> TokenStream {
    let edge_expr: Expr = parse_macro_input!(input as Expr);

    let expanded = quote! {
        graph.nodes[ graph.edges[#edge_expr].to ]
    };

    TokenStream::from(expanded)
}

/// Usage: `n!(node_id_expr)`
/// Expands to: `graph.nodes[node_id_expr]`
/// XXX: Requires `graph` to be available at the caller side. The whole point of this macro being a
/// procedural macro is to avoid typing `graph` so often.
#[proc_macro]
pub fn n(input: TokenStream) -> TokenStream {
    let node_id_expr: Expr = parse_macro_input!(input as Expr);

    let expanded = quote! {
        graph.nodes[ #node_id_expr ]
    };

    TokenStream::from(expanded)
}

/// Usage: `e!(edge_id_expr)`
/// Expands to: `graph.edges[edge_id_expr]`
/// XXX: Requires `graph` to be available at the caller side. The whole point of this macro being a
/// procedural macro is to avoid typing `graph` so often.
#[proc_macro]
pub fn e(input: TokenStream) -> TokenStream {
    let edge_id_expr: Expr = parse_macro_input!(input as Expr);

    let expanded = quote! {
        graph.edges[ #edge_id_expr ]
    };

    TokenStream::from(expanded)
}
