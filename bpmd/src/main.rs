#![allow(clippy::too_many_arguments)]

use annotate_snippets::AnnotationKind;
use annotate_snippets::Level;
use bpmd_graph::*;
use bpmd_layout::*;
use bpmd_parse::*;
use bpmd_pebpmd_analysis::pebpmd_analysis;
use bpmd_to_bpmn::*;
use bpmd_to_svg::*;
use bpmd_util::timer::Timer;
use std::fmt::Display;
use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Instant;

use annotate_snippets::Snippet;
use annotate_snippets::renderer::{DecorStyle, Renderer};
use clap::{Parser, ValueEnum};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Input DSL file. If missing, the input file is read from standard input.
    #[arg(short, long, value_name = "IN_FILE")]
    input: Option<std::path::PathBuf>,

    /// Output file. If missing, the data will be written to standard output.
    #[arg(short, long, value_name = "OUT_FILE")]
    output: Option<std::path::PathBuf>,

    /// Root directory for importing.
    #[arg(short, long, value_name = "ROOT_DIRECTORY")]
    root: Option<std::path::PathBuf>,

    #[arg(short = 'f', long, value_name = "FORMAT", default_value_t = OutputFormat::Svg)]
    output_format: OutputFormat,

    /// By default, SVG fonts are embedded for maximum portability. The result is that text cannot
    /// be copied from the SVG, but this should usually not be necessary anyway.
    #[arg(short = 'E', long, default_value_t = false)]
    no_svg_embed_font: bool,

    /// Output visibility table to this file (CSV format).
    #[arg(short, long, value_name = "VISIBILITY_FILE")]
    visibility_table: Option<std::path::PathBuf>,
}

#[derive(ValueEnum, Clone)]
enum OutputFormat {
    Bpmn,
    Svg,
    Png,
}

impl Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputFormat::Bpmn => write!(f, "bpmn"),
            OutputFormat::Svg => write!(f, "svg"),
            OutputFormat::Png => write!(f, "png"),
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let origin = Instant::now();
    let mut timer = Timer::new(
        Box::new(move || origin.elapsed()),
        Box::new(|start, end| end - start),
        Box::new(|s| println!("{s}")),
    );

    let bpmd = cli.input.as_ref().map_or_else(
        || std::io::read_to_string(std::io::stdin()),
        std::fs::read_to_string,
    )?;

    let root = if let Some(root) = cli.root {
        Some(root)
    } else if let Some(input) = &cli.input {
        input.parent().map(|p| p.to_owned())
    } else {
        None
    };
    let mut import_data = ImportData::new(root);
    match &cli.input {
        None => import_data.push_from_stdin(bpmd),
        Some(input) => match import_data.push(
            &PathBuf::from_str(".").unwrap(),
            input.to_string_lossy().to_string(),
            TokenCoordinate::default(),
        ) {
            Ok(stack) => stack,
            Err(e) => {
                Result::<(), ParseError>::Err(e).bpmd_format_err(&import_data)?;
                unreachable!();
            }
        },
    }

    let mut graph = parse(&mut import_data, &mut timer).bpmd_format_err(&import_data)?;
    import_data.pop();

    {
        let visibility_table =
            pebpmd_analysis(&mut graph, &mut timer).bpmd_format_err(&import_data)?;
        if let Some(visibility_path) = &cli.visibility_table {
            std::fs::write(visibility_path, visibility_table)?;
        };
    }

    // This takes quite some time :( Would be cool if that could be a `const` method, but requires
    // upstream support and I don't believe this is easily achievable.
    let mut font_cache = timer.time_it("Initializing font system", FontCache::new);

    let result = catch_unwind(AssertUnwindSafe(
        || -> Result<String, Box<dyn std::error::Error>> {
            layout_graph(&mut graph, &mut timer, &mut font_cache).bpmd_format_err(&import_data)?;
            Ok(match cli.output_format {
                OutputFormat::Bpmn => timer.time_it("XML export", || generate_bpmn(&graph)),
                OutputFormat::Svg => timer.time_it("SVG export", || {
                    to_svg(&graph, &mut font_cache, !cli.no_svg_embed_font)
                }),
                OutputFormat::Png => {
                    let _svg =
                        timer.time_it("SVG export", || to_svg(&graph, &mut font_cache, true));
                    timer.time_it("Png export", || todo!())
                }
            })
        },
    ));

    let output_data = match result {
        Ok(r) => r?, // propagate Result errors as before
        Err(payload) => {
            eprintln!("Panic! Debug-printing the graph: {graph:#?}");
            std::panic::resume_unwind(payload);
        }
    };

    match cli.output {
        Some(pb) => std::fs::write(pb, output_data)?,
        None => print!("{output_data}"),
    };

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
    fn bpmd_format_err(self, source_files: &ImportData) -> Result<T, Box<dyn std::error::Error>>;
}

impl<T> ParseErrorMapToBoxError<T> for Result<T, ParseError> {
    fn bpmd_format_err(self, source_files: &ImportData) -> Result<T, Box<dyn std::error::Error>> {
        self.map_err(|annotations| {
            Box::new(BpmdParseError(render_snippet_report(
                &source_files.bpmd_source_files,
                annotations,
            )))
            .into()
        })
    }
}

pub struct BpmdSourceFile {
    // Standard input, file path, or URL in include, or whatever.
    pub location: String,
    pub canonicalized_location: PathBuf,
    pub content: String,
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

struct ImportData {
    root: Option<PathBuf>,
    bpmd_source_files: Vec<BpmdSourceFile>,
    import_stack: Vec<ImportStackElement>,
}

impl ImportData {
    fn new(root: Option<PathBuf>) -> Self {
        Self {
            root,
            bpmd_source_files: vec![],
            import_stack: vec![],
        }
    }

    fn push(
        &mut self,
        current_processed_file_location: &Path,
        location: String,
        tc: TokenCoordinate,
    ) -> Result<(), ParseError> {
        let canonicalized_location = match std::fs::canonicalize(
            current_processed_file_location.join(&location),
        ) {
            Ok(v) => v,
            Err(e) => {
                return Err(vec![(
                    format!(
                        "The imported file seems to not exist at the given path: {location} (looking relative to {}, underlying error: {e})",
                        current_processed_file_location.to_string_lossy()
                    ),
                    tc,
                )]);
            }
        };
        if self.import_stack.iter().any(|e| {
            self.bpmd_source_files[e.bpmn_source_file_index].canonicalized_location
                == canonicalized_location
        }) {
            return Err(vec![(
                format!(
                    "There is a cyclic [import ...] happening, in order: {:?}. The last attempted import is here.",
                    self.bpmd_source_files
                        .iter()
                        .map(|e| e.location.as_str())
                        .chain(std::iter::once(location.as_str()))
                        .collect::<Vec<_>>()
                ),
                tc,
            )]);
        }
        if let Some(previously_imported) = self
            .bpmd_source_files
            .iter()
            .position(|a| a.canonicalized_location == canonicalized_location)
        {
            self.import_stack.push(ImportStackElement {
                bpmn_source_file_index: previously_imported,
            });
        } else {
            validate_import(&canonicalized_location, &self.root, tc)?;
            let content = match std::fs::read_to_string(&canonicalized_location) {
                Ok(content) => content,
                Err(err) => {
                    return Err(vec![(
                        format!("The requested file at <{location}> cannot be read: {err}"),
                        tc,
                    )]);
                }
            };
            self.bpmd_source_files.push(BpmdSourceFile {
                location,
                content,
                canonicalized_location,
            });
            self.import_stack.push(ImportStackElement {
                bpmn_source_file_index: self.bpmd_source_files.len().strict_sub(1),
            });
        }
        Ok(())
    }

    fn pop(&mut self) {
        self.import_stack.pop().expect("Called too often?");
    }

    fn push_from_stdin(&mut self, content: String) {
        self.bpmd_source_files.push(BpmdSourceFile {
            location: "(source read from standard input)".to_string(),
            canonicalized_location: "(source read from standard input)".into(),
            content,
        });
        self.import_stack.push(ImportStackElement {
            bpmn_source_file_index: 0,
        });
    }
}

struct ImportStackElement {
    pub bpmn_source_file_index: usize,
}

impl ImportHandler for ImportData {
    fn push(&mut self, location: String, tc: TokenCoordinate) -> Result<(), ParseError> {
        let current_file_location = self.bpmd_source_files
            [self.import_stack.last().unwrap().bpmn_source_file_index]
            .canonicalized_location
            // `.clone()` for the borrow checker.
            .clone();
        self.push(current_file_location.parent().unwrap(), location, tc)?;
        Ok(())
    }

    fn pop(&mut self) {
        self.pop();
    }

    fn current_source_file_content_and_index(&self) -> (String, usize) {
        let idx = self.import_stack.last().unwrap().bpmn_source_file_index;
        (self.bpmd_source_files[idx].content.clone(), idx)
    }
}

fn validate_import(
    canonicalized_location: &Path,
    root: &Option<PathBuf>,
    tc: TokenCoordinate,
) -> Result<(), ParseError> {
    let Some(root) = &root else {
        return Err(vec![("It looks like you are reading a .bpmd diagram from STDIN which wants to `[import ..]` some file. However, this is forbidden unless you specify the `--root some/path` argument.".to_string(),
        tc)
        ]);
    };

    if canonicalized_location.starts_with(root) {
        Ok(())
    } else {
        Err(vec![(
            format!(
                "The imported file is outside of the current root. The imported file path resolves to: {}, the root to: {}",
                canonicalized_location.to_string_lossy(),
                root.to_string_lossy()
            ),
            tc,
        )])
    }
}
