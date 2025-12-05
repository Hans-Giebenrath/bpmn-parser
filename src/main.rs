#![feature(map_try_insert)]
#![feature(never_type)]
#![feature(array_windows)]

mod analysis;
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

pub struct BpmdSourceFile {
    // Standard input, file path, or URL in include, or whatever.
    location: String,
    content: String,
}

#[derive(Default)]
struct Timer {
    measurements: Vec<(&'static str, std::time::Duration)>,
}

impl Timer {
    fn time_it<F, R>(&mut self, label: &'static str, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let t = std::time::Instant::now();
        let r = f();
        self.measurements.push((label, t.elapsed()));
        r
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        if self.measurements.is_empty() {
            return;
        }
        let longest_label = self
            .measurements
            .iter()
            .max_by_key(|(label, _)| label.len())
            .expect("Just checked before")
            .0
            .len();
        for (label, duration) in &mut self.measurements {
            let padding = std::iter::repeat_n(' ', longest_label - label.len()).collect::<String>();
            println!("{padding}{label} took {}ms", duration.as_millis());
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let mut timer = Timer::default();

    let bpmd = cli.input.as_ref().map_or_else(
        || std::io::read_to_string(std::io::stdin()),
        std::fs::read_to_string,
    )?;

    let mut bpmd_source_files = vec![BpmdSourceFile {
        location: cli.input.map_or_else(
            || "(source read from standard input)".to_string(),
            |pathbuf| pathbuf.to_string_lossy().to_string(),
        ),
        content: bpmd.clone(),
    }];
    let mut graph: Graph = timer.time_it("Parsing", || {
        parser::parse(bpmd, &mut bpmd_source_files).bpmd_format_err(&bpmd_source_files)
    })?;

    timer.time_it("Create transported data", || {
        analysis::create_transported_data(&mut graph);
    });

    if let Some(visibility_path) = &cli.visibility_table {
        timer.time_it("PE-BPMN analysis", || {
            pebpmn_analysis(&mut graph, visibility_path, &bpmd_source_files)
        })?;
    };

    layout_graph(&mut graph, &mut timer);
    let bpmn = timer.time_it("XML export", || to_xml::generate_bpmn(&graph));

    match cli.output {
        Some(pb) => std::fs::write(pb, bpmn)?,
        None => print!("{bpmn}"),
    };

    Ok(())
}

fn layout_graph(graph: &mut Graph, timer: &mut Timer) {
    // Phase 2
    timer.time_it("solve_layer_assignment", || solve_layer_assignment(graph));
    timer.time_it("generate_dummy_nodes", || generate_dummy_nodes(graph));
    timer.time_it("sort_lanes_by_layer", || sort_lanes_by_layer(graph));

    // Phase 3
    timer.time_it("reduce_all_crossings", || reduce_all_crossings(graph));
    timer.time_it("port_assignment", || port_assignment(graph));

    // Phase 4
    timer.time_it("assign_xy_ilp", || assign_xy_ilp(graph));

    // Phase 5
    timer.time_it("postprocess_ports_and_vertical_edges", || {
        postprocess_ports_and_vertical_edges(graph)
    });
    timer.time_it("try_move_nodes_into_half_layer", || {
        try_move_nodes_into_half_layer(graph)
    });
    timer.time_it("find_straight_edges", || find_straight_edges(graph));
    timer.time_it("edge_routing", || edge_routing(graph));
    timer.time_it("replace_dummy_nodes", || replace_dummy_nodes(graph));
}

pub fn pebpmn_analysis(
    graph: &mut Graph,
    visibility_path: &PathBuf,
    bpmd_source_files: &[BpmdSourceFile],
) -> Result<(), Box<dyn std::error::Error>> {
    pe_bpmn::analysis::analyse(graph).bpmd_format_err(bpmd_source_files)?;
    let visibility_data = pe_bpmn::visibility_table::generate_visibility_table(graph)?;
    std::fs::write(visibility_path, visibility_data)?;
    Ok(())
}

// XXX Don't use `String.into()` instead of this, as otherwise it will verbatim print all the
// terminal color escape codes, instead of printing colored output.
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
    fn bpmd_format_err(
        self,
        source_files: &[BpmdSourceFile],
    ) -> Result<T, Box<dyn std::error::Error>>;
}

impl<T> ParseErrorMapToBoxError<T> for Result<T, ParseError> {
    fn bpmd_format_err(
        self,
        source_files: &[BpmdSourceFile],
    ) -> Result<T, Box<dyn std::error::Error>> {
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
    source_files: &[BpmdSourceFile],
    annotations: Vec<(String, TokenCoordinate)>,
) -> String {
    assert!(!annotations.is_empty());

    let report = annotations
        .iter()
        .take(1)
        .map(|e| {
            Level::ERROR.clone().primary_title("Error").element(
                Snippet::source(&source_files[e.1.source_file_idx].content)
                    .path(&source_files[e.1.source_file_idx].location)
                    .line_start(1)
                    .fold(true)
                    .annotation(
                        AnnotationKind::Primary
                            .span(e.1.start..e.1.end)
                            .label(e.0.clone()),
                    ),
            )
        })
        .chain(annotations.iter().skip(1).map(|e| {
            Level::HELP.clone().secondary_title("").element(
                Snippet::source(&source_files[e.1.source_file_idx].content)
                    .path(&source_files[e.1.source_file_idx].location)
                    .line_start(1)
                    .fold(true)
                    .annotation(
                        AnnotationKind::Context
                            .span(e.1.start..e.1.end)
                            .label(e.0.clone()),
                    ),
            )
        }))
        .collect::<Vec<_>>();

    let renderer = Renderer::styled().decor_style(DecorStyle::Unicode);
    renderer.render(&report).to_string()
}
