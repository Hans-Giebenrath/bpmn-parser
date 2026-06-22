//! WIP cleaning up ChatGPT generated code.
//! I'd very much like to use the icons from bpmn.io, but the license requires me to include a
//! watermark on generated images. I don't want that. So I now have to go through all the icons and
//! create my own drawings, using the BPMN spec as a visual inspiration.
//! Look for `TODO continue here` mark to see which ChatGPT generated things to replace next.
//! Anyway, the idea is to have one big `defs` section which contains all icons, and then we can
//! reference it while drawing.

use cosmic_text::{
    Align, Attrs, Buffer, Command, FontSystem, LayoutRun, Metrics, Shaping, SwashCache, Wrap,
};

use std::fmt::Write as _;

use crate::{
    common::{
        bpmn_node::{
            ActivityMarker, ActivityType, BoundaryEventType, EventVisual, InterruptKind, TaskType,
        },
        edge::FlowType,
        graph::{
            ACTIVITY_NODE_HEIGHT, ACTIVITY_NODE_WIDTH, DATAOBJECT_NODE_HEIGHT,
            DATAOBJECT_NODE_WIDTH, DATASTORE_NODE_HEIGHT, DATASTORE_NODE_WIDTH, EVENT_NODE_HEIGHT,
            EVENT_NODE_WIDTH, GATEWAY_NODE_HEIGHT, GATEWAY_NODE_WIDTH, MAX_NODE_WIDTH,
        },
    },
    lexer::{DataType, EventType, GatewayType},
};
pub const STROKE_WIDTH: f64 = 2.;
pub const FLOW_CORNER_RADIUS: usize = 7;
pub const MESSAGE_FLOW_START_MARKER_RADIUS: f64 = 4.;
pub const MESSAGE_FLOW_END_MARKER_WIDTH: f64 = 10.;
pub const ACTIVITY_MARKER_DIMENSION: usize = 20;

#[derive(Debug, Clone)]
/// TODO make this a Cow, as most of the time no escaping is needed.
pub struct EscapedSvgAttribute(String);

impl EscapedSvgAttribute {
    fn new(value: &str) -> Self {
        Self(esc_attr(value))
    }
}

impl From<String> for EscapedSvgAttribute {
    fn from(value: String) -> Self {
        Self(esc_attr(&value))
    }
}

impl From<&String> for EscapedSvgAttribute {
    fn from(value: &String) -> Self {
        Self(esc_attr(value))
    }
}

impl From<&str> for EscapedSvgAttribute {
    fn from(value: &str) -> Self {
        Self(esc_attr(value))
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
            fill: EscapedSvgAttribute::new("#fff"),
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
    font_system: FontSystem,
    embed_font: bool,
}

impl Svg {
    pub fn new(embed_font: bool, width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            body: String::new(),
            style: SvgStyle::default(),
            font_system: FontSystem::new(),
            embed_font,
        }
    }

    /// Returns complete SVG file content.
    pub fn finish(self) -> String {
        let mut out = String::new();
        // XXX Omit width and height, so the renderer can decide how to display it.
        // (Recommended by asciidoctor, and it also looks nicer in the browser this way)
        writeln!(
                out,
                r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="-{2} -{2} {0} {1}" role="img" stroke-width="{STROKE_WIDTH}" stroke-linecap="round" stroke-linejoin="round" stroke-dasharray="none" >"#,
                self.width as f64 + STROKE_WIDTH, self.height as f64 + STROKE_WIDTH,
                STROKE_WIDTH / 2.0,
            ).unwrap();
        writeln!(out, "{}", super::defs::defs()).unwrap();
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
        multiple: bool,
        lanes: &[(
            /* title */ &str,
            /* height */ usize,
            ElementSvgStyle,
        )],
        style: &ElementSvgStyle,
    ) {
        let (x, y) = top_left_corner_xy;
        let total_width = pool_header_width + content_width;
        let merged = MergedSvgStyle::new(&self.style, style);

        writeln!(self.body, r#"<g transform="translate({x},{y})">"#,).unwrap();
        writeln!(
                self.body,
                r#"  <rect x="0" y="0" width="{total_width}" fill="{}" height="{height}" stroke="none" stroke-width="{STROKE_WIDTH}"/>"#,
                merged.fill,
            ).unwrap();

        if let [(lane_title, lane_height, lane_style)] = lanes
            && lane_title.is_empty()
        {
            assert_eq!(*lane_height, height);
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

        if let [(lane_title, lane_height, lane_style)] = lanes
            && lane_title.is_empty()
        {
            assert_eq!(*lane_height, height);
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
                writeln!(
                        self.body,
                        r#"  <rect class="lane-border" x="{pool_header_width}" y="{cumulative_height}" width="{}" fill="none" height="{lane_height}" stroke="{}" stroke-width="{STROKE_WIDTH}"/>"#,
                        total_width - pool_header_width,
                        lane_style.stroke.as_ref().map(|s|s.0.as_str()).unwrap_or(merged.stroke),
                    ).unwrap();
                cumulative_height += lane_height;
            }
        }

        // Rotation is done via CSS, hence the "pool-title" class.
        write_wrapped_text(
            &mut self.body,
            &mut self.font_system,
            pool_header_width / 2,
            height / 2,
            title,
            Some(height),
            true,
            true,
            &merged,
            "pool-title",
        );

        let mut cumulative_height = 0;
        for (lane_title, lane_height, lane_style) in lanes {
            if lane_title.is_empty() {
                continue;
            }
            let merged = MergedSvgStyle::new(&self.style, lane_style);
            write_wrapped_text(
                &mut self.body,
                &mut self.font_system,
                pool_header_width + lane_header_width / 2,
                cumulative_height + lane_height / 2,
                lane_title,
                Some(*lane_height),
                true,
                true,
                &merged,
                "lane-title",
            );
            cumulative_height += lane_height;
        }
        writeln!(self.body, "</g>").unwrap();

        if multiple {
            let start_x = content_width / 2 + pool_header_width - ACTIVITY_MARKER_DIMENSION / 2;
            let y_padding = 3;
            let start_y = height - y_padding - ACTIVITY_MARKER_DIMENSION;
            writeln!(
                    self.body,
                    r##"  <use href="#tm-multiple" x="{start_x}" y="{start_y}" width="{ACTIVITY_MARKER_DIMENSION}" height="{ACTIVITY_MARKER_DIMENSION}" stroke="{}" fill="none" />"##,
                    merged.stroke
                )
                .unwrap();
        }
    }

    pub fn draw_task(
        &mut self,
        (x, y): (usize, usize),
        text: &str,
        element_style: &ElementSvgStyle,
        activity_type: ActivityType,
        activity_marker: ActivityMarker,
    ) {
        let merged = MergedSvgStyle::new(&self.style, element_style);
        writeln!(
            self.body,
            r##"<g class="task" transform="translate({x},{y})">
            <use href="#task-box" x="0" y="0" stroke="{}" fill="{}" width="{ACTIVITY_NODE_WIDTH}" height="{ACTIVITY_NODE_HEIGHT}" stroke-width="{STROKE_WIDTH}" />
            "##,
            merged.stroke, merged.fill
        )
        .unwrap();

        let activity_type_href = match activity_type {
            ActivityType::Task(TaskType::None) => "",
            ActivityType::Task(TaskType::Send) => "task-send",
            ActivityType::Task(TaskType::Receive) => "task-receive",
            ActivityType::Task(TaskType::Manual) => "task-manual",
            ActivityType::Task(TaskType::User) => "task-user",
            ActivityType::Task(TaskType::Script) => "task-script",
            ActivityType::Task(TaskType::Service) => "task-service",
            ActivityType::Task(TaskType::Businessrule) => "task-business-rule",
            ActivityType::Subprocess => todo!(),
            ActivityType::CallActivity => todo!(),
            ActivityType::EventSubprocess => todo!(),
            ActivityType::Transaction => todo!(),
        };
        if !activity_type_href.is_empty() {
            writeln!(
                self.body,
                r##"  <use href="#{activity_type_href}" x="3" y="3" width="20" height="20" stroke="{}" fill="{}" />"##,
                merged.stroke,
                if matches!(activity_type, ActivityType::Task(TaskType::Send)) { merged.stroke } else { merged.fill }
            )
            .unwrap();
        }

        {
            // Activity Marker
            // Rename to `am` so formatting is a bit nicer.
            let am = &activity_marker;
            // The order is the same as they will appear in the bottom of the activity box.
            let arr = [
                (am.compensation, "tm-compensation", merged.stroke, "#fff"),
                (am.multiple, "tm-multiple", merged.stroke, "none"),
                (am.r#loop, "tm-loop", merged.stroke, "none"),
                (am.plus_in_a_box, "tm-plus-box", merged.stroke, "none"),
                (am.adhoc, "tm-adhoc", "none", merged.stroke),
            ];
            let symbol_count = arr.iter().filter(|tpl| tpl.0).count();
            let y_padding = 3;
            let x_padding = 2;
            // TODO the activity markers need to be reworked to fit into the 15 x 15 space, looks better.
            // 20x20 is too large.
            let box_width = ACTIVITY_MARKER_DIMENSION;
            let box_width = 15;
            let start_y = ACTIVITY_NODE_HEIGHT - box_width - y_padding;
            let mut start_x = ACTIVITY_NODE_WIDTH / 2
                - ((box_width + x_padding) * symbol_count) / 2
                + x_padding / 2;

            for (_, href, stroke, fill) in arr.iter().filter(|tpl| tpl.0) {
                writeln!(
                    self.body,
                    r##"  <use href="#{href}" x="{start_x}" y="{start_y}" width="{box_width}" height="{box_width}" stroke="{stroke}" fill="{fill}" />"##,
                )
                .unwrap();
                start_x += box_width + x_padding;
            }
        }

        write_wrapped_text(
            &mut self.body,
            &mut self.font_system,
            ACTIVITY_NODE_WIDTH / 2,
            ACTIVITY_NODE_HEIGHT / 2,
            text,
            Some((ACTIVITY_NODE_WIDTH - STROKE_WIDTH as usize) - 4),
            true,
            false,
            &merged,
            "",
        );
        writeln!(self.body, "</g>").unwrap();
    }

    ///// Draws a BPMN event. The event is positioned by its top-left bounding box.
    pub fn draw_event(
        &mut self,
        (x, y): (usize, usize),
        text: &str,
        event_type: EventType,
        event_visual: EventVisual,
        style: &ElementSvgStyle,
    ) {
        let merged = MergedSvgStyle::new(&self.style, style);

        let (event_outer_symbol, event_inner_symbol_fill_kind, stroke, fill) = match event_visual {
            EventVisual::Catch(InterruptKind::NonInterrupting) => {
                ("event-dashed-dashed", "catching", merged.stroke, "none")
            }
            EventVisual::Catch(InterruptKind::Interrupting) => {
                ("event-solid-solid", "catching", merged.stroke, "none")
            }
            EventVisual::Start(InterruptKind::NonInterrupting) => {
                ("event-dashed", "catching", merged.stroke, "none")
            }
            EventVisual::Start(InterruptKind::Interrupting) => {
                ("event-solid", "catching", merged.stroke, "none")
            }
            EventVisual::Throw => ("event-solid-solid", "throwing", "none", merged.stroke),
            EventVisual::End => ("event-thick", "throwing", "none", merged.stroke),
        };

        let event_inner_symbol = match event_type {
            EventType::Blank => "blank",
            EventType::Message => "message",
            EventType::Timer => "timer",
            EventType::Conditional => "conditional",
            EventType::Link => "link",
            EventType::Signal => "signal",
            EventType::Error => "error",
            EventType::Escalation => "escalation",
            EventType::Termination => "termination",
            EventType::Compensation => "compensation",
            EventType::Cancel => "cancel",
            EventType::Multiple => "multiple",
            EventType::MultipleParallel => "multiple-parallel",
        };

        writeln!(
            self.body,
            r##"
<g class="event {event_outer_symbol} event-{event_inner_symbol_fill_kind} event-{event_inner_symbol}">
  <use href="#{event_outer_symbol}" x="{x}" y="{y}" width="{EVENT_NODE_WIDTH}" height="{EVENT_NODE_HEIGHT}" stroke="{}" fill="{}"/>
  <use href="#event-{event_inner_symbol}-{event_inner_symbol_fill_kind}" x="{x}" y="{y}" width="{EVENT_NODE_WIDTH}" height="{EVENT_NODE_HEIGHT}" stroke="{stroke}" fill="{fill}"/>
"##,
merged.stroke, merged.fill
        )
        .unwrap();

        if !text.is_empty() {
            write_wrapped_text(
                &mut self.body,
                &mut self.font_system,
                x + EVENT_NODE_WIDTH / 2,
                y + EVENT_NODE_HEIGHT + 10,
                text,
                Some(MAX_NODE_WIDTH),
                false,
                false,
                &merged,
                "",
            );
        }
        writeln!(self.body, "</g>").unwrap();
    }

    pub fn draw_boundary_event(
        &mut self,
        (x, y): (usize, usize),
        event_type: BoundaryEventType,
        interrupt_kind: InterruptKind,
        style: &ElementSvgStyle,
    ) {
        let merged = MergedSvgStyle::new(&self.style, style);

        let event_outer_symbol = match interrupt_kind {
            InterruptKind::NonInterrupting => "event-dashed-dashed",
            InterruptKind::Interrupting => "event-solid-solid",
        };

        let event_inner_symbol = match event_type {
            BoundaryEventType::Message => "message",
            BoundaryEventType::Timer => "timer",
            BoundaryEventType::Conditional => "conditional",
            BoundaryEventType::Signal => "signal",
            BoundaryEventType::Error => "error",
            BoundaryEventType::Escalation => "escalation",
            BoundaryEventType::Compensation => "compensation",
            BoundaryEventType::Cancel => "cancel",
            BoundaryEventType::Multiple => "multiple",
            BoundaryEventType::MultipleParallel => "multiple-parallel",
        };

        writeln!(
            self.body,
            r##"
<g class="event {event_outer_symbol} event-boundary event-{event_inner_symbol}">
  <use href="#{event_outer_symbol}" x="{x}" y="{y}" width="{EVENT_NODE_WIDTH}" height="{EVENT_NODE_HEIGHT}" stroke="{0}" fill="{1}"/>
  <use href="#event-{event_inner_symbol}-catching" x="{x}" y="{y}" width="{EVENT_NODE_WIDTH}" height="{EVENT_NODE_HEIGHT}" stroke="{0}" fill="{1}"/>
</g>"##,
merged.stroke, merged.fill
        )
        .unwrap();
    }

    pub fn draw_gateway(
        &mut self,
        (x, y): (usize, usize),
        text: &str,
        element_style: &ElementSvgStyle,
        gateway_type: GatewayType,
    ) {
        let merged = MergedSvgStyle::new(&self.style, element_style);

        let symbol = match gateway_type {
            GatewayType::Exclusive => "exclusive",
            GatewayType::Parallel => "parallel",
            GatewayType::Inclusive => "inclusive",
            GatewayType::Event => "event",
        };

        writeln!(
            self.body,
            r##"<g class="gateway gateway-{symbol}" transform="translate({x},{y})">"##,
        )
        .unwrap();
        writeln!(
            self.body,
            r##"
<use href="#gateway-box" x="0" y="0" stroke="{0}" fill="{1}" width="{GATEWAY_NODE_WIDTH}" height="{GATEWAY_NODE_HEIGHT}" stroke-width="{STROKE_WIDTH}" />
<use href="#gateway-{symbol}" x="0" y="0" stroke="{0}" fill="{0}" width="{GATEWAY_NODE_WIDTH}" height="{GATEWAY_NODE_HEIGHT}" stroke-width="{STROKE_WIDTH}" />
"##,
merged.stroke, merged.fill
        )
        .unwrap();

        if !text.is_empty() {
            write_wrapped_text(
                &mut self.body,
                &mut self.font_system,
                x + GATEWAY_NODE_WIDTH / 2,
                y + GATEWAY_NODE_HEIGHT + 10,
                text,
                Some(MAX_NODE_WIDTH),
                false,
                false,
                &merged,
                "",
            );
        }
        writeln!(self.body, "</g>").unwrap();
    }

    ///// Draws BPMN data object/input/output/store.
    pub fn draw_data(
        &mut self,
        (x, y): (usize, usize),
        text: &str,
        data_type: DataType,
        style: &ElementSvgStyle,
    ) {
        let merged = MergedSvgStyle::new(&self.style, style);
        let (symbol, width, height) = match data_type {
            DataType::Store => ("data-store", DATASTORE_NODE_WIDTH, DATASTORE_NODE_HEIGHT),
            DataType::Object => ("data-object", DATAOBJECT_NODE_WIDTH, DATAOBJECT_NODE_HEIGHT),
        };

        writeln!(
            self.body,
            r#"<g class="{symbol}" transform="translate({x},{y})">"#,
        )
        .unwrap();

        writeln!(
            self.body,
            r##"  <use href="#{symbol}" x="0" y="0" stroke-width="{STROKE_WIDTH}" width="{width}" height="{height}" fill="{}" stroke="{}" />"##,
            merged.fill, merged.stroke,
        )
        .unwrap();

        if !text.is_empty() {
            write_wrapped_text(
                &mut self.body,
                &mut self.font_system,
                width / 2,
                height + 10,
                text,
                Some(MAX_NODE_WIDTH),
                false,
                false,
                &merged,
                "",
            );
        }

        writeln!(self.body, "</g>").unwrap();
    }

    pub fn draw_flow(
        &mut self,
        points: &[(usize, usize)],
        label: &Option<String>,
        flow_type: &FlowType,
        element_style: &ElementSvgStyle,
    ) {
        if points.len() < 2 {
            return;
        }

        let merged = MergedSvgStyle::new(&self.style, element_style);

        let (d, attributes, stroke_width) = match flow_type {
            FlowType::SequenceFlow => (
                flow_points(points, 0., 3. + 2. * STROKE_WIDTH),
                r##"class="sequence-flow" marker-end="url(#sequence-flow-head)""##,
                STROKE_WIDTH,
            ),
            FlowType::DataFlow(..) => (
                flow_points(points, 0., STROKE_WIDTH / 2.),
                r#" class="data-association" stroke-dasharray="1 7" stroke-linecap="round" marker-end="url(#data-association-head)" "#,
                // Kinda looks like the data associations have a finer stroke in the BPMN spec.
                // To me personally it looks nicer this way actually.
                1.6,
            ),
            FlowType::MessageFlow(..) => (
                flow_points(
                    points,
                    MESSAGE_FLOW_START_MARKER_RADIUS / 2.,
                    MESSAGE_FLOW_END_MARKER_WIDTH + 1.0 * STROKE_WIDTH,
                ),
                r##"class="message-flow" stroke-dasharray="10 8" marker-start="url(#message-start)" marker-end="url(#message-flow-head)""##,
                STROKE_WIDTH,
            ),
        };

        writeln!(
            self.body,
            r#"<path d="{d}" fill="none" stroke="{}" stroke-width="{stroke_width}" {attributes} />"#,
            merged.stroke,
        )
        .unwrap();

        if let Some(label) = label.as_ref().filter(|s| !s.is_empty()) {
            let mid = if (points.len() & 1) == 1 {
                // Uneven, so just take middle point.
                points[points.len() / 2]
            } else {
                let a = points[points.len() / 2];
                let b = points[(points.len() / 2) + 1];
                ((a.0 + b.0) / 2, (a.1 + b.1) / 2)
            };
            write_wrapped_text(
                &mut self.body,
                &mut self.font_system,
                mid.0,
                mid.1.saturating_sub(merged.line_height as usize / 2),
                label,
                None,
                false,
                false,
                &merged,
                "",
            );
        }
    }
}

fn move_towards(a: f64, b: f64, k: f64) -> f64 {
    a + (b - a).clamp(-k, k)
}

fn flow_points(points: &[(usize, usize)], first_offset: f64, last_offset: f64) -> String {
    let mut out = String::new();
    if let [(x1, y1), (x2, y2), ..] = points {
        write!(
            out,
            "M{} {} ",
            move_towards(*x1 as f64, *x2 as f64, first_offset),
            move_towards(*y1 as f64, *y2 as f64, first_offset),
        )
        .unwrap();
    }
    for &[prev, cur, next] in points.array_windows() {
        if prev.1 == cur.1 {
            if prev.0 < cur.0 {
                // `prev` to `cur` is goes right.
                write!(out, "L{} {} ", cur.0 - FLOW_CORNER_RADIUS, cur.1).unwrap();
                if cur.1 < next.1 {
                    // line goes downwards
                    write!(
                        out,
                        "A {FLOW_CORNER_RADIUS} {FLOW_CORNER_RADIUS} 0 0 1 {} {} ",
                        cur.0,
                        cur.1 + FLOW_CORNER_RADIUS,
                    )
                    .unwrap();
                } else {
                    assert!(cur.1 > next.1);
                    // line goes upwards
                    write!(
                        out,
                        "A {FLOW_CORNER_RADIUS} {FLOW_CORNER_RADIUS} 0 0 0 {} {} ",
                        cur.0,
                        cur.1 - FLOW_CORNER_RADIUS,
                    )
                    .unwrap();
                }
            } else {
                assert!(prev.0 > cur.0);
                // `prev` to `cur` is goes left.
                write!(out, "L{} {} ", cur.0 + FLOW_CORNER_RADIUS, cur.1).unwrap();
                if cur.1 < next.1 {
                    // line goes downwards
                    write!(
                        out,
                        "A {FLOW_CORNER_RADIUS} {FLOW_CORNER_RADIUS} 0 0 0 {} {} ",
                        cur.0,
                        cur.1 + FLOW_CORNER_RADIUS,
                    )
                    .unwrap();
                } else {
                    assert!(cur.1 > next.1);
                    // line goes upwards
                    write!(
                        out,
                        "A {FLOW_CORNER_RADIUS} {FLOW_CORNER_RADIUS} 0 0 1 {} {} ",
                        cur.0,
                        cur.1 - FLOW_CORNER_RADIUS,
                    )
                    .unwrap();
                }
            }
        } else if prev.1 < cur.1 {
            // `prev` to `cur` goes downwards
            write!(out, "L{} {} ", cur.0, cur.1 - FLOW_CORNER_RADIUS).unwrap();
            if cur.0 > next.0 {
                // line goes left
                write!(
                    out,
                    "A {FLOW_CORNER_RADIUS} {FLOW_CORNER_RADIUS} 0 0 1 {} {} ",
                    cur.0 - FLOW_CORNER_RADIUS,
                    cur.1,
                )
                .unwrap();
            } else {
                assert!(cur.0 < next.0);
                // line goes right
                write!(
                    out,
                    "A {FLOW_CORNER_RADIUS} {FLOW_CORNER_RADIUS} 0 0 0 {} {} ",
                    cur.0 + FLOW_CORNER_RADIUS,
                    cur.1,
                )
                .unwrap();
            }
        } else {
            assert!(prev.1 > cur.1);
            // `prev` to `cur` goes upwards
            write!(out, "L{} {} ", cur.0, cur.1 + FLOW_CORNER_RADIUS).unwrap();
            if cur.0 > next.0 {
                // line goes left
                write!(
                    out,
                    "A {FLOW_CORNER_RADIUS} {FLOW_CORNER_RADIUS} 0 0 0 {} {} ",
                    cur.0 - FLOW_CORNER_RADIUS,
                    cur.1,
                )
                .unwrap();
            } else {
                assert!(cur.0 < next.0);
                // line goes right
                write!(
                    out,
                    "A {FLOW_CORNER_RADIUS} {FLOW_CORNER_RADIUS} 0 0 1 {} {}",
                    cur.0 + FLOW_CORNER_RADIUS,
                    cur.1,
                )
                .unwrap();
            }
        }
    }
    if let [.., (x2, y2), (x1, y1)] = points {
        write!(
            out,
            "L{} {} ",
            move_towards(*x1 as f64, *x2 as f64, last_offset),
            move_towards(*y1 as f64, *y2 as f64, last_offset),
        )
        .unwrap();
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
    font_system: &mut FontSystem,
    x: usize,
    y: usize,
    text: &str,
    max_width: Option<usize>,
    center_vertically: bool,
    rotate: bool,
    merged: &MergedSvgStyle,
    class: &str,
) {
    let metrics = Metrics::new(merged.font_size, merged.line_height);

    let mut buffer = Buffer::new(font_system, metrics);

    buffer.set_wrap(Wrap::Word);
    buffer.set_size(max_width.map(|width| width as f32), None);
    buffer.set_text(
        // Don't escape just yet. We want to first inspect the text that will be visible.
        text,
        &Attrs::new().family(cosmic_text::Family::Name(merged.font_family)),
        Shaping::Advanced,
        Some(Align::Center),
    );

    // Perform shaping as desired
    buffer.shape_until_scroll(font_system, false /* not sure? */);
    let count = buffer.layout_runs().count();
    let lines = buffer.layout_runs();
    if count == 0 {
        return;
    }
    let (start_y, dominant_baseline) = if center_vertically {
        (
            y as f32 - ((count - 1) as f32 * merged.line_height) / 2.0,
            "middle",
        )
    } else {
        (y as f32, "hanging")
    };

    if rotate {
        assert!(center_vertically); // Otherwise, the center rotation center is wrong
        write!(
                body,
                r#"<text class="{class}" transform="translate({x}, {y}) rotate(-90)" text-anchor="middle" dominant-baseline="middle" font-family="{}" font-size="{}" fill="{}">"#,
                merged.font_family,
                merged.font_size,
                merged.font_color
            ).unwrap();
    } else {
        write!(
                body,
                r#"<text class="{class}" x="{x}" y="{start_y}" text-anchor="middle" dominant-baseline="{dominant_baseline}" font-family="{}" font-size="{}" fill="{}">"#,
                merged.font_family,
                merged.font_size,
                merged.font_color
            ).unwrap();
    }

    // When rotating, the `x` transformation is already applied. In that case only `0` needs to be
    // hardcoded to reset the x state.
    let tspan_x = if rotate { 0 } else { x };
    for (i, line) in lines.enumerate() {
        let dy = if i == 0 { 0.0 } else { merged.line_height };
        let start = line.glyphs.first().map(|g| g.start).unwrap_or(0);
        let end = line.glyphs.last().map(|g| g.end).unwrap_or(start);
        write!(
            body,
            r#"<tspan x="{tspan_x}" dy="{dy}">{}</tspan>"#,
            esc_text(&line.text[start..end])
        )
        .unwrap();
    }
    writeln!(body, "</text>").unwrap();
}

/// TODO! Untested, received form ChatGPT.
fn run_to_svg_path(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    run: &LayoutRun<'_>,
) -> String {
    let mut d = String::new();

    for glyph in run.glyphs.iter() {
        let physical = glyph.physical((0.0, 0.0), 1.0);

        let Some(commands) = swash_cache.get_outline_commands(font_system, physical.cache_key)
        else {
            continue;
        };

        let dx = glyph.x + glyph.x_offset;
        let dy = run.line_y + glyph.y + glyph.y_offset;

        for cmd in commands {
            match *cmd {
                Command::MoveTo(p) => {
                    d.push_str(&format!("M {} {} ", p.x + dx, p.y + dy));
                }
                Command::LineTo(p) => {
                    d.push_str(&format!("L {} {} ", p.x + dx, p.y + dy));
                }
                Command::QuadTo(p1, p2) => {
                    d.push_str(&format!(
                        "Q {} {} {} {} ",
                        p1.x + dx,
                        p1.y + dy,
                        p2.x + dx,
                        p2.y + dy,
                    ));
                }
                Command::CurveTo(p1, p2, p3) => {
                    d.push_str(&format!(
                        "C {} {} {} {} {} {} ",
                        p1.x + dx,
                        p1.y + dy,
                        p2.x + dx,
                        p2.y + dy,
                        p3.x + dx,
                        p3.y + dy,
                    ));
                }
                Command::Close => {
                    d.push_str("Z ");
                }
            }
        }
    }

    d
}
