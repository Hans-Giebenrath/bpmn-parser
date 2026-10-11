#![no_std]

extern crate alloc;

use core::fmt::Write as _;
use core::fmt::{self, Display, Formatter};

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
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
        diagnostics_pane_content: String,
        diagnostic_boxes_html: String,
        number_of_diagnostics: u32,
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
            log::error!("{error}");
            return serde_wasm_bindgen::to_value(&ReturnType::Failure {
                diagnostics_pane_content: Default::default(),
                diagnostic_boxes_html: Default::default(),
                number_of_diagnostics: Default::default(),
            })
            .unwrap();
        }
    };
    let mut import_data = ImportData::new(request.text.clone());
    let result = match run(request, &mut import_data) {
        Ok(result) => ReturnType::Success {
            diagram: result.diagram,
            pebpmd_visibility_table_html: to_html_table(&result.pebpmd_visibility_table),
        },
        Err(e) => into_error(&import_data.source, e),
    };
    serde_wasm_bindgen::to_value(&result).unwrap()
}

fn run(request: RequestType, import_data: &mut ImportData) -> Result<InnerReturnType, ParseError> {
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

    let mut graph = parse(import_data, &mut timer)?;

    let pebpmd_visibility_table = pebpmd_analysis(&mut graph, &mut timer)?;

    // Reuse the expensive font system between calls rather than constructing
    // FontCache for every compilation.
    let mut cache = FONT_CACHE.0.borrow_mut();
    let font_cache =
        cache.get_or_insert_with(|| timer.time_it("Initializing font system", FontCache::new));

    layout_graph(&mut graph, &mut timer, font_cache)?;
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

fn into_error(text: &str, errors: ParseError) -> ReturnType {
    let diagnostic_boxes_html = {
        let mut html = String::with_capacity(300);
        let mut is_first = true;
        for (idx, (error, _)) in errors.iter().enumerate() {
            if !is_first {
                write!(html, "<br>").unwrap();
            }
            is_first = false;
            write!(
                html,
                r#"<div class="diag-box" id="diag-box-{idx}">{}</div>"#,
                HtmlEscaped(error.as_str())
            )
            .unwrap();
        }
        html
    };

    let diagnostics_pane_content = render(text, &errors);

    ReturnType::Failure {
        diagnostics_pane_content,
        diagnostic_boxes_html,
        number_of_diagnostics: errors.len() as u32,
    }
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

pub struct HtmlEscaped<'a>(pub &'a str);

impl Display for HtmlEscaped<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut rest = self.0;

        while let Some(pos) = rest.find(['&', '<', '>', '"', '\'']) {
            f.write_str(&rest[..pos])?;

            let replacement = match rest.as_bytes()[pos] {
                b'&' => "&amp;",
                b'<' => "&lt;",
                b'>' => "&gt;",
                b'"' => "&quot;",
                b'\'' => "&#39;",
                _ => unreachable!(),
            };

            f.write_str(replacement)?;
            rest = &rest[pos + 1..];
        }

        f.write_str(rest)
    }
}

fn render(text: &str, tokens: &[(String, TokenCoordinate)]) -> String {
    let mut out = String::new();
    let mut pos = 0;
    let mut line_idx = 0;

    let tokens = {
        let mut tokens = tokens
            .iter()
            .enumerate()
            .map(|(idx, (_, tc))| (idx, *tc))
            .collect::<Vec<_>>();
        tokens.sort_unstable_by_key(|(_, tc)| (tc.start, tc.end));
        tokens
    };

    // Emit text, inserting line-end markers at newline boundaries.
    let emit = |segment: &str, out: &mut String, line_idx: &mut usize| {
        for part in segment.split_inclusive('\n') {
            if let Some(before_newline) = part.strip_suffix('\n') {
                write!(out, "{}", HtmlEscaped(before_newline)).unwrap();
                write!(
                    out,
                    "<span class=\"line-end\" id=\"line-end-{line_idx}\"></span>"
                )
                .unwrap();
                *line_idx += 1;
            } else {
                write!(out, "{}", HtmlEscaped(part)).unwrap();
            }
        }
    };

    for (idx, coord) in tokens {
        assert!(coord.start >= pos);
        assert!(coord.start <= coord.end);
        assert!(text.is_char_boundary(coord.start));
        assert!(text.is_char_boundary(coord.end));
        assert!(coord.end <= text.len());

        emit(&text[pos..coord.start], &mut out, &mut line_idx);

        write!(
            out,
            "<span class=\"{}\">",
            if idx == 0 { "error" } else { "hint" }
        )
        .unwrap();

        emit(&text[coord.start..coord.end], &mut out, &mut line_idx);

        write!(out, "</span>").unwrap();

        pos = coord.end;
    }

    emit(&text[pos..], &mut out, &mut line_idx);

    // Every line, including the final line without a newline,
    // receives a line-end marker.
    if !text.ends_with('\n') {
        write!(
            out,
            "<span class=\"line-end\" id=\"line-end-{line_idx}\"></span>"
        )
        .unwrap();
    }

    out
}
