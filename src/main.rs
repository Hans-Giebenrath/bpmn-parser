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
use crate::lexer::TokenCoordinate;
use annotate_snippets::AnnotationKind;
use annotate_snippets::Level;
use std::path::PathBuf;

use annotate_snippets::Snippet;
use annotate_snippets::renderer::{DecorStyle, Renderer};
use clap::Parser;
use layout::all_crossing_minimization::reduce_all_crossings;
use layout::dummy_node_generation::generate_dummy_nodes;
use layout::edge_routing::edge_routing;
use layout::port_assignment::port_assignment;
use layout::postprocess_ports_and_vertical_edges::postprocess_ports_and_vertical_edges;
use layout::replace_dummy_nodes::replace_dummy_nodes;
use layout::solve_layer_assignment::solve_layer_assignment;
use layout::straight_edge_routing::find_straight_edges;
use layout::try_move_nodes_into_half_layer::try_move_nodes_into_half_layer;
use layout::xy_ilp::assign_xy_ilp;

use crate::{
    common::graph::{Graph, sort_lanes_by_layer},
    parser::ParseError,
};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Input DSL file. If missing, the input file is read from standard input.
    #[arg(short, long, value_name = "IN_FILE")]
    input: Option<std::path::PathBuf>,

    /// Output XML file. If missing, the xml data will be written to standard output.
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

    let mut bpmd_source_files = Vec::new();
    let mut graph: Graph =
        parser::parse(bpmd, &mut bpmd_source_files).bpmd_format_err(&bpmd_source_files)?;

    if let Some(visibility_path) = &cli.visibility_table {
        pebpmn_analysis(&mut graph, visibility_path, &bpmd_source_files)?;
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
    postprocess_ports_and_vertical_edges(graph);
    try_move_nodes_into_half_layer(graph);
    find_straight_edges(graph);
    edge_routing(graph);
    replace_dummy_nodes(graph);
}

pub fn pebpmn_analysis(
    graph: &mut Graph,
    visibility_path: &PathBuf,
    bpmd_source_files: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    pe_bpmn::analysis::analyse(graph).bpmd_format_err(bpmd_source_files)?;
    let visibility_data = pe_bpmn::visibility_table::generate_visibility_table(graph)?;
    std::fs::write(visibility_path, visibility_data)?;
    Ok(())
}

// Workaround since `String` cannot be used for `Box<dyn std::err::Error>`.
pub(crate) struct BpmdParseError(pub String);

impl std::fmt::Display for BpmdParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::fmt::Debug for BpmdParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for BpmdParseError {}

trait ParseErrorMapToBoxError<T> {
    fn bpmd_format_err(self, source_files: &[String]) -> Result<T, Box<dyn std::error::Error>>;
}

impl<T> ParseErrorMapToBoxError<T> for Result<T, ParseError> {
    fn bpmd_format_err(self, source_files: &[String]) -> Result<T, Box<dyn std::error::Error>> {
        self.map_err(|annotations| {
            Box::new(BpmdParseError(render_snippet_report(
                source_files,
                annotations,
            )))
            .into()
        })
    }
}

fn render_snippet_report(
    source_files: &[String],
    annotations: Vec<(String, TokenCoordinate)>,
) -> String {
    assert!(!annotations.is_empty());

    let report = annotations
        .iter()
        .take(1)
        .map(|e| {
            Level::ERROR.clone().primary_title(e.0.clone()).element(
                Snippet::source(&source_files[annotations[0].1.source_file_idx])
                    .line_start(1)
                    .fold(true)
                    .annotation(AnnotationKind::Primary.span(e.1.start..e.1.end)),
            )
        })
        .chain(annotations.iter().skip(1).map(|e| {
            Level::INFO.clone().secondary_title(e.0.clone()).element(
                Snippet::source(&source_files[annotations[0].1.source_file_idx])
                    .line_start(1)
                    .fold(true)
                    .annotation(AnnotationKind::Context.span(e.1.start..e.1.end)),
            )
        }))
        .collect::<Vec<_>>();

    let renderer = Renderer::styled().decor_style(DecorStyle::Unicode);
    renderer.render(&report).to_string()
}
