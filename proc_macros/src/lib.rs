//! These macros are primarily helpers to have succinct syntax.
//! Cannot use inherent functions (a function of Graph, etc) as
//! Rust does not have partial borrows (at least today). So instead
//! we simply let Rust write the verbose syntax out for us.
//!
use proc_macro::TokenStream;
use quote::quote;
use syn::{Expr, parse_macro_input};

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

/// Usage: `edges!(node_expr)`
/// Expands to: `*{match direction { Direction::Forward => &node_expr.outgoing, Direction::Backward
/// => &node_expr.incoming}}`
/// XXX: Requires `direction` to be available at the caller side.
#[proc_macro]
pub fn edges(input: TokenStream) -> TokenStream {
    let node_expr: Expr = parse_macro_input!(input as Expr);

    // Note: Not sure if the brackets `()` are required.
    // Use *{..&incoming} to allow on the caller side to use `&mut edges(node)`.
    let expanded = quote! {
        *{match direction {
            Direction::Forward => &(#node_expr).outgoing,
            Direction::Backward => &(#node_expr).incoming,
        }}
    };

    TokenStream::from(expanded)
}

/// Usage: `follow!(edge_expr)`
/// Expands to: `match direction { Direction::Forward => edge_expr.to, Direction::Backward
/// => edge_expr.from}`
/// XXX: Requires `direction` to be available at the caller side.
#[proc_macro]
pub fn follow(input: TokenStream) -> TokenStream {
    let edge_expr: Expr = parse_macro_input!(input as Expr);

    // Note: Not sure if the brackets `()` are required.
    let expanded = quote! {
        match direction {
            Direction::Forward => (#edge_expr).to,
            Direction::Backward => (#edge_expr).from,
        }
    };

    TokenStream::from(expanded)
}

/// Usage: `target_nodes!(node_expr)`
/// Expands to: `*{match direction { Direction::Forward => &node_expr.outgoing, Direction::Backward
/// => &node_expr.incoming}}`
/// XXX: Requires `direction` to be available at the caller side.
#[proc_macro]
pub fn target_nodes(input: TokenStream) -> TokenStream {
    let node_expr: Expr = parse_macro_input!(input as Expr);

    // Note: Not sure if the brackets `()` are required.
    // Use *{..&incoming} to allow on the caller side to use `&mut edges(node)`.
    let expanded = quote! {
        match direction {
            Direction::Forward => #node_expr.outgoing.iter().map(|edge_id| graph.nodes[graph.edges[edge_id].to]),
            Direction::Backward => #node_expr.incoming.iter().map(|edge_id| graph.nodes[graph.edges[edge_id].from]),
        }
    };

    TokenStream::from(expanded)
}
