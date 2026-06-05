//! WIP cleaning up ChatGPT generated code.
//! I'd very much like to use the icons from bpmn.io, but the license requires me to include a
//! watermark on generated images. I don't want that. So I now have to go through all the icons and
//! create my own drawings, using the BPMN spec as a visual inspiration.
//! Look for `TODO continue here` mark to see which ChatGPT generated things to replace next.
//! Anyway, the idea is to have one big `defs` section which contains all icons, and then we can
//! reference it while drawing.

use cosmic_text::{Align, Attrs, Buffer, BufferLine, FontSystem, Metrics, Shaping, Wrap};
use std::fmt::Write as _;
const STROKE_WIDTH: &'static str = "0.8";

#[derive(Debug, Clone)]
pub struct EscapedSvgAttribute(String);

impl EscapedSvgAttribute {
    fn new(value: &str) -> Self {
        Self(esc_attr(&value))
    }
}

#[derive(Debug, Clone)]
pub struct SvgStyle {
    pub font_family: EscapedSvgAttribute,
    pub font_size: f32,
    pub line_height: f32,
    pub font_color: EscapedSvgAttribute,
    pub stroke: EscapedSvgAttribute,
    pub fill: EscapedSvgAttribute,
}

#[derive(Default, Debug, Clone)]
pub struct ElementSvgStyle {
    pub font_family: Option<EscapedSvgAttribute>,
    pub font_size: Option<f32>,
    pub line_height: Option<f32>,
    pub font_color: Option<EscapedSvgAttribute>,
    pub stroke: Option<EscapedSvgAttribute>,
    pub fill: Option<EscapedSvgAttribute>,
}

pub struct MergedSvgStyle<'a> {
    pub font_family: &'a str,
    pub font_size: f32,
    pub line_height: f32,
    pub font_color: &'a str,
    pub stroke: &'a str,
    pub fill: &'a str,
}

impl<'a> MergedSvgStyle<'a> {
    fn new(default_style: &'a SvgStyle, element_style: &'a ElementSvgStyle) -> Self {
        Self {
            font_family: element_style
                .font_family
                .as_ref()
                .map(|s| s.0.as_str())
                .unwrap_or(&default_style.font_family.0),
            font_size: element_style.font_size.unwrap_or(default_style.font_size),
            line_height: element_style
                .line_height
                .unwrap_or(default_style.line_height),
            font_color: element_style
                .font_color
                .as_ref()
                .map(|s| s.0.as_str())
                .unwrap_or(&default_style.font_color.0),
            stroke: element_style
                .stroke
                .as_ref()
                .map(|s| s.0.as_str())
                .unwrap_or(&default_style.stroke.0),
            fill: element_style
                .fill
                .as_ref()
                .map(|s| s.0.as_str())
                .unwrap_or(&default_style.fill.0),
        }
    }
}

impl Default for SvgStyle {
    fn default() -> Self {
        Self {
            font_family: EscapedSvgAttribute::new("Arial, Helvetica, sans-serif"),
            font_size: 12.0,
            line_height: 14.0,
            font_color: EscapedSvgAttribute::new("#111"),
            stroke: EscapedSvgAttribute::new("#222"),
            fill: EscapedSvgAttribute::new("none"),
        }
    }
}

/// SVG builder for BPMN diagrams.
///
/// Create one renderer, call draw_* methods, then call [`Svg::finish`].
pub struct Svg {
    width: usize,
    height: usize,
    body: String,
    style: SvgStyle,
}

impl Svg {
    pub fn new() -> Self {
        Self {
            width: 0,
            height: 0,
            body: String::new(),
            style: SvgStyle::default(),
        }
    }

    /// Returns complete SVG file content.
    pub fn finish(self) -> String {
        let mut out = String::new();
        writeln!(
                out,
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="{0}" height="{1}" viewBox="0 0 {0} {1}" role="img">"#,
                self.width, self.height,
            ).unwrap();
        writeln!(out, "{}", super::defs::defs(&self.style.stroke.0)).unwrap();
        writeln!(out, "{}", self.body).unwrap();
        writeln!(out, "</svg>").unwrap();
        out
    }

    /// Draws a BPMN pool as a bordered container with a vertical header.
    pub fn draw_pool(
        &mut self,
        pool_header_width: usize,
        lane_header_width: usize,
        height: usize,
        content_width: usize, // already includes lane_header_width, but not pool_header_width.
        top_left_corner_xy: (usize, usize),
        title: &str,
        lanes: &[(
            /* title */ &str,
            /* height */ usize,
            &ElementSvgStyle,
        )],
        style: &ElementSvgStyle,
        _pool_id: usize,
    ) {
        let (x, y) = top_left_corner_xy;
        let total_width = pool_header_width + content_width;
        self.height = std::cmp::max(self.height, y + height);
        self.width = std::cmp::max(self.width, x + total_width);
        let merged = MergedSvgStyle::new(&self.style, style);

        writeln!(self.body, r#"<g transform="translate({x},{y})">"#,).unwrap();
        writeln!(
                self.body,
                r#"  <rect x="0" y="0" width="{total_width}" fill="{}" height="{height}" stroke="none" stroke-width="{STROKE_WIDTH}"/>"#,
                merged.fill,
            ).unwrap();

        if let &[(lane_title, lane_height, lane_style)] = &lanes[..]
            && lane_title.is_empty()
        {
            assert_eq!(lane_height, height);
            if let Some(fill) = &lane_style.fill {
                writeln!(
                        self.body,
                        r#"  <rect x="0" y="0" width="{total_width}" fill="{}" height="{height}" stroke="none" />"#,
                        fill.0,
                    ).unwrap();
            }
        } else {
            let mut cumulative_height = 0;
            for (_lane_title, lane_height, lane_style) in lanes {
                if let Some(fill) = &lane_style.fill {
                    writeln!(
                        self.body,
                        r#"  <rect x="{pool_header_width}" y="{cumulative_height}" width="{}" fill="{}" height="{lane_height}" stroke="none" />"#,
                        total_width - pool_header_width,
                        fill.0,
                    ).unwrap();
                }
                cumulative_height += lane_height;
            }
        }

        // Now write the borders, on top of the lane backgrounds.
        writeln!(
                self.body,
                r#"  <rect x="0" y="0" width="{total_width}" fill="none" height="{height}" stroke="{}" stroke-width="{STROKE_WIDTH}"/>"#,
                merged.stroke,
            ).unwrap();
        writeln!(
            self.body,
            r#"  <line x1="{pool_header_width}" y1="0" x2="{pool_header_width}" y2="{height}" stroke="{}" stroke-width="{STROKE_WIDTH}"/>"#,
            merged.stroke,
        )
        .unwrap();

        if let &[(lane_title, lane_height, lane_style)] = &lanes[..]
            && lane_title.is_empty()
        {
            assert_eq!(lane_height, height);
            if let Some(stroke) = &lane_style.stroke {
                writeln!(
                        self.body,
                        r#"  <rect x="0" y="0" width="{total_width}" fill="none" height="{height}" stroke="{}" stroke-width="{STROKE_WIDTH}"/>"#,
                        stroke.0,
                    ).unwrap();
            }
        } else {
            let mut cumulative_height = 0;
            for (_lane_title, lane_height, lane_style) in lanes {
                if let Some(stroke) = &lane_style.stroke {
                    writeln!(
                        self.body,
                        r#"  <rect x="{pool_header_width}" y="{cumulative_height}" width="{}" fill="none" height="{lane_height}" stroke="{}" stroke-width="{STROKE_WIDTH}"/>"#,
                        total_width - pool_header_width,
                        stroke.0,
                    ).unwrap();
                }
                cumulative_height += lane_height;
            }
        }

        // Rotation is done via CSS, hence the "pool-title" class.
        write_wrapped_text(
            &mut self.body,
            pool_header_width / 2,
            height / 2,
            title,
            Some(height),
            true,
            &merged,
            "pool-title",
        );

        if let &[(lane_title, _lane_height, _lane_style)] = &lanes[..]
            && lane_title.is_empty()
        {
        } else {
            let mut cumulative_height = 0;
            for (lane_title, lane_height, lane_style) in lanes {
                let merged = MergedSvgStyle::new(&self.style, lane_style);
                write_wrapped_text(
                    &mut self.body,
                    pool_header_width + lane_header_width / 2,
                    cumulative_height + lane_height / 2,
                    lane_title,
                    Some(*lane_height),
                    true,
                    &merged,
                    "lane-title",
                );
                cumulative_height += lane_height;
            }
        }
        writeln!(self.body, "</g>").unwrap();
    }

    //pub fn draw_task(
    //    &mut self,
    //    top_left_corner_xy: (usize, usize),
    //    width: usize,
    //    height: usize,
    //    text: &str,
    //    task_type: TaskType,
    //) {
    //    let (x, y) = top_left_corner_xy;
    //    let id = symbol_for_task(task_type);

    //    writeln!(
    //        self.body,
    //        r#"<g class="bpmn-task bpmn-task-{:?}" transform="translate({},{})">"#,
    //        task_type, x, y
    //    )
    //    .unwrap();
    //    writeln!(
    //            self.body,
    //            r#"  <rect x="0" y="0" width="{}" height="{}" rx="10" ry="10" fill="{}" stroke="{}" stroke-width="1.5"/>"#,
    //            width, height, self.style.fill, self.style.stroke
    //        ).unwrap();

    //    if let Some(symbol_id) = id {
    //        writeln!(
    //            self.body,
    //            r##"  <use href="#{}" x="10" y="8" width="18" height="18"/>"##,
    //            symbol_id
    //        )
    //        .unwrap();
    //    }

    //    self.write_wrapped_text(
    //        x + width / 2,
    //        y + height / 2,
    //        text,
    //        width.saturating_sub(18),
    //        true,
    //    );
    //    writeln!(self.body, "</g>").unwrap();
    //}

    ///// Draws a BPMN event. The event is positioned by its top-left bounding box.
    //pub fn draw_event(
    //    &mut self,
    //    top_left_corner_xy: (usize, usize),
    //    width: usize,
    //    height: usize,
    //    text: &str,
    //    event: EventSpec,
    //) {
    //    let (x, y) = top_left_corner_xy;
    //    let cx = x + width / 2;
    //    let cy = y + height / 2;
    //    let r = width.min(height) / 2;

    //    let stroke_width = match event.kind {
    //        EventKind::Start => 1.5,
    //        EventKind::Intermediate => 1.5,
    //        EventKind::End => 3.0,
    //    };

    //    writeln!(
    //        self.body,
    //        r#"<g class="bpmn-event bpmn-event-{:?} bpmn-event-{:?}">"#,
    //        event.kind, event.event_type
    //    )
    //    .unwrap();
    //    writeln!(
    //        self.body,
    //        r#"  <circle cx="{}" cy="{}" r="{}" fill="{}" stroke="{}" stroke-width="{}"/>"#,
    //        cx,
    //        cy,
    //        r.saturating_sub(2),
    //        self.style.fill,
    //        self.style.stroke,
    //        stroke_width
    //    )
    //    .unwrap();

    //    if matches!(event.kind, EventKind::Intermediate) {
    //        writeln!(
    //            self.body,
    //            r#"  <circle cx="{}" cy="{}" r="{}" fill="none" stroke="{}" stroke-width="1"/>"#,
    //            cx,
    //            cy,
    //            r.saturating_sub(6),
    //            self.style.stroke
    //        )
    //        .unwrap();
    //    }

    //    if let Some(symbol_id) = symbol_for_event(event.event_type) {
    //        let marker_size = r.saturating_sub(10).max(10) * 2;
    //        let mx = cx.saturating_sub(marker_size / 2);
    //        let my = cy.saturating_sub(marker_size / 2);
    //        let class = if event.filled_marker {
    //            "filled"
    //        } else {
    //            "outline"
    //        };
    //        writeln!(
    //            self.body,
    //            r##"  <use class="{}" href="#{}" x="{}" y="{}" width="{}" height="{}"/>"##,
    //            class, symbol_id, mx, my, marker_size, marker_size
    //        )
    //        .unwrap();
    //    }

    //    writeln!(self.body, "</g>").unwrap();

    //    if !text.is_empty() {
    //        self.write_wrapped_text(cx, y + height + 16, text, width.max(70), false);
    //    }
    //}

    ///// Draws a BPMN gateway diamond. Positioned by top-left bounding box.
    //pub fn draw_gateway(
    //    &mut self,
    //    top_left_corner_xy: (usize, usize),
    //    width: usize,
    //    height: usize,
    //    text: &str,
    //    gateway_type: GatewayType,
    //) {
    //    let (x, y) = top_left_corner_xy;
    //    let cx = x + width / 2;
    //    let cy = y + height / 2;
    //    let symbol_id = symbol_for_gateway(gateway_type);

    //    writeln!(
    //        self.body,
    //        r#"<g class="bpmn-gateway bpmn-gateway-{:?}">"#,
    //        gateway_type
    //    )
    //    .unwrap();
    //    writeln!(
    //            self.body,
    //            r#"  <polygon points="{},{} {},{} {},{} {},{}" fill="{}" stroke="{}" stroke-width="1.5"/>"#,
    //            cx, y,
    //            x + width, cy,
    //            cx, y + height,
    //            x, cy,
    //            self.style.fill,
    //            self.style.stroke
    //        ).unwrap();
    //    writeln!(
    //        self.body,
    //        r##"  <use href="#{}" x="{}" y="{}" width="{}" height="{}"/>"##,
    //        symbol_id,
    //        x + width / 4,
    //        y + height / 4,
    //        width / 2,
    //        height / 2
    //    )
    //    .unwrap();
    //    writeln!(self.body, "</g>").unwrap();

    //    if !text.is_empty() {
    //        self.write_wrapped_text(cx, y + height + 16, text, width.max(80), false);
    //    }
    //}

    ///// Draws BPMN data object/input/output/store.
    //pub fn draw_data(
    //    &mut self,
    //    top_left_corner_xy: (usize, usize),
    //    width: usize,
    //    height: usize,
    //    text: &str,
    //    data_type: DataType,
    //) {
    //    let (x, y) = top_left_corner_xy;
    //    let symbol_id = symbol_for_data(data_type);

    //    writeln!(
    //        self.body,
    //        r#"<g class="bpmn-data bpmn-data-{:?}" transform="translate({},{})">"#,
    //        data_type, x, y
    //    )
    //    .unwrap();
    //    writeln!(
    //        self.body,
    //        r##"  <use href="#{}" x="0" y="0" width="{}" height="{}"/>"##,
    //        symbol_id, width, height
    //    )
    //    .unwrap();
    //    writeln!(self.body, "</g>").unwrap();

    //    if !text.is_empty() {
    //        self.write_wrapped_text(x + width / 2, y + height + 15, text, width.max(80), false);
    //    }
    //}

    ///// Draws a sequence flow with a solid line and filled arrowhead.
    //pub fn draw_sequence_flow(&mut self, points: &[(usize, usize)], label: Option<&str>) {
    //    self.draw_flow(points, label, FlowKind::Sequence);
    //}

    ///// Draws a data association/data flow with a dotted line and open arrowhead.
    //pub fn draw_data_flow(&mut self, points: &[(usize, usize)], label: Option<&str>) {
    //    self.draw_flow(points, label, FlowKind::DataAssociation);
    //}

    ///// Draws a message flow with a dashed line, open circle start, and open arrowhead.
    //pub fn draw_message_flow(&mut self, points: &[(usize, usize)], label: Option<&str>) {
    //    self.draw_flow(points, label, FlowKind::Message);
    //}

    ///// Lower-level escape hatch for custom SVG snippets.
    ///// The caller is responsible for emitting valid SVG.
    //pub fn push_raw(&mut self, svg_fragment: &str) {
    //    self.body.push_str(svg_fragment);
    //    if !svg_fragment.ends_with('\n') {
    //        self.body.push('\n');
    //    }
    //}

    //fn draw_partition(
    //    &mut self,
    //    class_name: &str,
    //    header_width: usize,
    //    height: usize,
    //    content_width: usize,
    //    top_left_corner_xy: (usize, usize),
    //    title: &str,
    //    header_fill: &str,
    //) {
    //}

    //fn draw_flow(&mut self, points: &[(usize, usize)], label: Option<&str>, kind: FlowKind) {
    //    if points.len() < 2 {
    //        return;
    //    }

    //    let d = polyline_points(points);
    //    let (class_name, extra, marker_start, marker_end) = match kind {
    //        FlowKind::Sequence => (
    //            "bpmn-sequence-flow",
    //            "",
    //            "",
    //            r##" marker-end="url(#arrow-filled)""##,
    //        ),
    //        FlowKind::DataAssociation => (
    //            "bpmn-data-association",
    //            r#" stroke-dasharray="1 5" stroke-linecap="round""#,
    //            "",
    //            r##" marker-end="url(#arrow-open)""##,
    //        ),
    //        FlowKind::Message => (
    //            "bpmn-message-flow",
    //            r#" stroke-dasharray="8 5""#,
    //            r##" marker-start="url(#message-start)""##,
    //            r##" marker-end="url(#arrow-open)""##,
    //        ),
    //    };

    //    writeln!(
    //            self.body,
    //            r#"<polyline class="{}" points="{}" fill="none" stroke="{}" stroke-width="1.5"{}{}{} />"#,
    //            class_name,
    //            d,
    //            self.style.stroke,
    //            extra,
    //            marker_start,
    //            marker_end
    //        ).unwrap();

    //    if let Some(label) = label.filter(|s| !s.is_empty()) {
    //        let mid = points[points.len() / 2];
    //        self.write_wrapped_text(mid.0, mid.1.saturating_sub(6), label, 120, false);
    //    }
    //}
}

//fn symbol_for_task(task_type: TaskType) -> Option<&'static str> {
//    match task_type {
//        TaskType::None => None,
//        TaskType::User => Some("task-user"),
//        TaskType::Manual => Some("task-manual"),
//        TaskType::Service => Some("task-service"),
//        TaskType::Script => Some("task-script"),
//        TaskType::BusinessRule => Some("task-business-rule"),
//        TaskType::Send => Some("task-send"),
//        TaskType::Receive => Some("task-receive"),
//    }
//}
//
//fn symbol_for_event(event_type: EventType) -> Option<&'static str> {
//    match event_type {
//        EventType::None => None,
//        EventType::Message => Some("event-message"),
//        EventType::Timer => Some("event-timer"),
//        EventType::Error => Some("event-error"),
//        EventType::Signal => Some("event-signal"),
//        EventType::Conditional => Some("event-conditional"),
//        EventType::Escalation => Some("event-escalation"),
//        EventType::Link => Some("event-link"),
//        EventType::Terminate => Some("event-terminate"),
//    }
//}
//
//fn symbol_for_gateway(gateway_type: GatewayType) -> &'static str {
//    match gateway_type {
//        GatewayType::Exclusive => "gateway-exclusive",
//        GatewayType::Inclusive => "gateway-inclusive",
//        GatewayType::Parallel => "gateway-parallel",
//        GatewayType::EventBased => "gateway-event-based",
//        GatewayType::Complex => "gateway-complex",
//    }
//}
//
//fn symbol_for_data(data_type: DataType) -> &'static str {
//    match data_type {
//        DataType::DataObject => "data-object",
//        DataType::DataInput => "data-input",
//        DataType::DataOutput => "data-output",
//        DataType::DataStore => "data-store",
//    }
//}
//
fn polyline_points(points: &[(usize, usize)]) -> String {
    let mut out = String::new();
    for (i, (x, y)) in points.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        write!(out, "{},{}", x, y).unwrap();
    }
    out
}

fn esc_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn esc_attr(s: &str) -> String {
    esc_text(s).replace('"', "&quot;").replace('\'', "&apos;")
}

fn write_wrapped_text(
    body: &mut String,
    x: usize,
    y: usize,
    text: &str,
    max_width: Option<usize>,
    center_vertically: bool,
    merged: &MergedSvgStyle,
    class: &str,
) {
    let mut font_system = FontSystem::new();
    let metrics = Metrics::new(merged.font_size, merged.line_height);

    let mut buffer = Buffer::new(&mut font_system, metrics);

    let mut buffer = buffer.borrow_with(&mut font_system);

    // Add some text!
    buffer.set_wrap(Wrap::Word);
    buffer.set_text(
        // Don't escape just yet. We want to first inspect the text that will be visible.
        text,
        &Attrs::new().family(cosmic_text::Family::Name(merged.font_family)),
        Shaping::Advanced,
        Some(Align::Center),
    );

    // Set a size for the text buffer, in pixels
    buffer.set_size(max_width.map(|width| width as f32), None);

    // Perform shaping as desired
    buffer.shape_until_scroll(false /* not sure? */);
    let lines = &buffer.lines;
    if lines.is_empty() {
        return;
    }

    let (y, dominant_baseline) = if center_vertically {
        (
            y as f32 - (lines.len() as f32 * merged.line_height) / 2.0,
            "middle",
        )
    } else {
        (y as f32, "hanging")
    };

    writeln!(
                body,
                r#"<text class="{class}" y="{y}" text-anchor="middle" dominant-baseline="{dominant_baseline}" font-family="{}" font-size="{}" fill="{}">"#,
                merged.font_family,
                merged.font_size,
                merged.font_color
            ).unwrap();

    for (i, line) in lines.iter().enumerate() {
        let dy = if i == 0 { 0.0 } else { merged.line_height };
        writeln!(
            body,
            r#"  <tspan x="{x}" dy="{dy}">{}</tspan>"#,
            esc_text(line.text())
        )
        .unwrap();
    }
    writeln!(body, "</text>").unwrap();
}
