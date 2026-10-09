#![no_std]

extern crate alloc;

use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use annotate_snippets::AnnotationKind;
use annotate_snippets::Level;
use annotate_snippets::Snippet;
use annotate_snippets::renderer::{DecorStyle, Renderer};
use web_sys::js_sys;

use bpmd_graph::*;
use bpmd_layout::layout_graph;
use bpmd_parse::*;
use bpmd_pebpmd_analysis::{VisibilityTable, pebpmd_analysis};
use bpmd_to_bpmn::generate_bpmn;
use bpmd_to_svg::to_svg;
use bpmd_util::timer::Timer;

use core::cell::RefCell;
use core::time::Duration;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::syntax_highlighting::highlight_text_for_inner_html;

mod syntax_highlighting;

struct Cache(RefCell<Option<FontCache>>);

// WASM currently executes this module on one thread. The wrapper is needed
// because RefCell itself is not Sync.
unsafe impl Sync for Cache {}

static FONT_CACHE: Cache = Cache(RefCell::new(None));

#[derive(Serialize, Deserialize)]
struct RequestType {
    /// BPMD source code.
    text: String,

    format: Format,
}

#[derive(Serialize, Deserialize)]
enum Format {
    SvgEmbed,
    SvgNoEmbed,
    Bpmn,
    HighlightedInnerHtml,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum ReturnType {
    Success {
        /// Format was specified in the request argument.
        diagram: String,
        pebpmd_visibility_table_html: String,
    },
    Failure {
        error_message: String,
    },
}

#[derive(Default)]
struct InnerReturnType {
    diagram: String,
    pebpmd_visibility_table: VisibilityTable,
}

/// Import handler for the browser build.
///
/// There is no filesystem available to the WASM module, so currently this
/// contains exactly one source file. `[import ...]` produces an error.
struct ImportData {
    source: String,
}

impl ImportData {
    fn new(source: String) -> Self {
        Self { source }
    }
}

impl ImportHandler for ImportData {
    fn push(&mut self, _location: String, tc: TokenCoordinate) -> Result<(), ParseError> {
        Err(vec![(
            "[import ...] is not supported in the playground.".to_string(),
            tc,
        )])
    }

    fn pop(&mut self) {
        unreachable!()
    }

    fn current_source_file_content_and_index(&self) -> (String, usize) {
        (self.source.clone(), 0)
    }
}

#[wasm_bindgen]
pub fn init_bpmd() {
    wasm_logger::init(wasm_logger::Config::default());
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub fn compile_bpmd(text: JsValue) -> JsValue {
    let request: RequestType = match serde_wasm_bindgen::from_value(text) {
        Ok(request) => request,
        Err(error) => {
            return serde_wasm_bindgen::to_value(&ReturnType::Failure {
                error_message: format!("Invalid compile request: {error}"),
            })
            .unwrap();
        }
    };
    let result = match run(request) {
        Ok(result) => ReturnType::Success {
            diagram: result.diagram,
            pebpmd_visibility_table_html: to_html_table(&result.pebpmd_visibility_table),
        },
        Err(e) => ReturnType::Failure {
            error_message: e.to_string(),
        },
    };
    serde_wasm_bindgen::to_value(&result).unwrap()
}

fn run(request: RequestType) -> Result<InnerReturnType, Box<dyn core::error::Error>> {
    let global = js_sys::global();
    let worker: web_sys::WorkerGlobalScope = global.unchecked_into();

    let performance = worker.performance().expect("performance API unavailable");
    let mut timer = Timer::new(
        Box::new(move || Duration::from_secs_f64(performance.now() / 1000.0)),
        Box::new(|start, end| end.saturating_sub(start)),
        Box::new(|s| web_sys::console::log_1(&s.into())),
    );

    if matches!(request.format, Format::HighlightedInnerHtml) {
        return Ok(InnerReturnType {
            diagram: timer.time_it("Highlighting text", || {
                highlight_text_for_inner_html(&request.text)
            }),
            ..Default::default()
        });
    }

    let mut import_data = ImportData::new(request.text);
    let mut graph = parse(&mut import_data, &mut timer).bpmd_format_err(&import_data)?;

    let pebpmd_visibility_table =
        pebpmd_analysis(&mut graph, &mut timer).bpmd_format_err(&import_data)?;

    // Reuse the expensive font system between calls rather than constructing
    // FontCache for every compilation.
    let mut cache = FONT_CACHE.0.borrow_mut();
    let font_cache =
        cache.get_or_insert_with(|| timer.time_it("Initializing font system", FontCache::new));

    layout_graph(&mut graph, &mut timer, font_cache).bpmd_format_err(&import_data)?;
    let diagram = match request.format {
        Format::Bpmn => timer.time_it("XML export", || generate_bpmn(&graph)),
        Format::SvgNoEmbed => timer.time_it("SVG export (without font embedding)", || {
            to_svg(&graph, font_cache, false)
        }),
        Format::SvgEmbed => timer.time_it("SVG export (with font embedding)", || {
            to_svg(&graph, font_cache, true)
        }),
        Format::HighlightedInnerHtml => unreachable!("Previously handled"),
    };

    Ok(InnerReturnType {
        diagram,
        pebpmd_visibility_table,
    })
}
// XXX Don't use `String.into()` instead of this, as otherwise it will verbatim print all the
// terminal color escape codes, instead of printing colored output.
pub(crate) struct BpmdParseError(pub String);

impl core::fmt::Display for BpmdParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl core::fmt::Debug for BpmdParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl core::error::Error for BpmdParseError {}

trait ParseErrorMapToBoxError<T> {
    fn bpmd_format_err(self, source_files: &ImportData) -> Result<T, Box<dyn core::error::Error>>;
}

impl<T> ParseErrorMapToBoxError<T> for Result<T, ParseError> {
    fn bpmd_format_err(self, source_files: &ImportData) -> Result<T, Box<dyn core::error::Error>> {
        self.map_err(|annotations| {
            Box::new(BpmdParseError(render_snippet_report(
                &source_files.source,
                annotations,
            )))
            .into()
        })
    }
}

fn render_snippet_report(
    editor_content: &str,
    annotations: Vec<(String, TokenCoordinate)>,
) -> String {
    assert!(!annotations.is_empty());

    let report = annotations
        .iter()
        .take(1)
        .map(|e| {
            Level::ERROR.clone().primary_title("Error").element(
                Snippet::source(editor_content)
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
                Snippet::source(editor_content)
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

    let renderer = Renderer::plain().decor_style(DecorStyle::Unicode);
    renderer.render(&report).to_string()
}

pub fn to_html_table(visibility_table: &VisibilityTable) -> String {
    fn push_html_escaped(out: &mut String, value: &str) {
        for c in value.chars() {
            match c {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                '\'' => out.push_str("&#39;"),
                _ => out.push(c),
            }
        }
    }
    let mut html = String::from("<table>\n\t<thead>\n\t\t<tr>");

    for header in &visibility_table.header_row {
        html.push_str("\n\t\t\t<th>");
        push_html_escaped(&mut html, header);
        html.push_str("</th>");
    }

    html.push_str("\n\t\t</tr>\n\t</thead>\n\t<tbody>");

    for row in &visibility_table.rows {
        html.push_str("\n\t\t<tr>");

        for cell in row {
            html.push_str("\n\t\t\t<td>");
            push_html_escaped(&mut html, cell);
            html.push_str("</td>");
        }

        html.push_str("\n\t\t</tr>");
    }

    html.push_str("\n\t</tbody>\n</table>");
    html
}
