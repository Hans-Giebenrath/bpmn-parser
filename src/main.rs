#![feature(map_try_insert)]
#![feature(never_type)]
#![feature(array_windows)]

mod common;
mod layout;
mod lexer;
mod node_id_matcher;
mod parser;
mod pe_bpmn;
mod pool_id_matcher;
mod to_xml;
use clap::Parser;
use layout::all_crossing_minimization::reduce_all_crossings;
use layout::dummy_node_generation::generate_dummy_nodes;
use layout::edge_routing::edge_routing;
use layout::fixup_gateway_ports::fixup_gateway_ports;
use layout::port_assignment::port_assignment;
use layout::replace_dummy_nodes::replace_dummy_nodes;
use layout::solve_layer_assignment::solve_layer_assignment;
use layout::straight_edge_routing::find_straight_edges;
use layout::try_move_nodes_into_half_layer::try_move_nodes_into_half_layer;
use layout::xy_ilp::assign_xy_ilp;

use crate::common::graph::{Graph, sort_lanes_by_layer};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Input DSL file. If missing, the input file is read from STDIN.
    #[arg(short, long, value_name = "IN_FILE")]
    input: Option<std::path::PathBuf>,

    /// Output XML file. If missing, the xml data will be written to STDOUT.
    #[arg(short, long, value_name = "OUT_FILE")]
    output: Option<std::path::PathBuf>,

    /// Output visibility table to this file (CSV format).
    #[arg(short, long, value_name = "VISIBILITY_FILE")]
    visibility_table: Option<std::path::PathBuf>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let bpmd = cli.input.map_or_else(
        || std::io::read_to_string(std::io::stdin()),
        std::fs::read_to_string,
    )?;

    let mut graph: Graph = parser::parse(bpmd)?;

    if let Some(visibility_path) = &cli.visibility_table {
        let visibility_data = pe_bpmn::visibility_table::generate_visibility_table(&graph)?;
        std::fs::write(visibility_path, visibility_data)?;
    };

    layout_graph(&mut graph);
    let bpmn = to_xml::generate_bpmn(&graph);

    match cli.output {
        Some(pb) => std::fs::write(pb, bpmn)?,
        None => print!("{bpmn}"),
    };

    Ok(())
}

pub fn layout_graph(graph: &mut Graph) {
    // Phase 2
    solve_layer_assignment(graph);
    generate_dummy_nodes(graph);
    sort_lanes_by_layer(graph);

    // Phase 3
    reduce_all_crossings(graph);
    port_assignment(graph);

    // Phase 4
    assign_xy_ilp(graph);

    // Phase 5
    fixup_gateway_ports(graph); // TODO write this one
    try_move_nodes_into_half_layer(graph);
    find_straight_edges(graph);
    edge_routing(graph);
    replace_dummy_nodes(graph);
}
