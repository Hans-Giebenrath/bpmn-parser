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
        config::Config,
        edge::FlowType,
        graph::{
            ACTIVITY_NODE_HEIGHT, ACTIVITY_NODE_WIDTH, DATAOBJECT_NODE_HEIGHT,
            DATAOBJECT_NODE_WIDTH, DATASTORE_NODE_HEIGHT, DATASTORE_NODE_WIDTH, EVENT_NODE_HEIGHT,
            EVENT_NODE_WIDTH, GATEWAY_NODE_HEIGHT, GATEWAY_NODE_WIDTH, MAX_NODE_WIDTH,
        },
        node::{Dimension, Side},
    },
    layout::{
        collision_grid::Grid,
        set_display_text_location_candidates::{
            Alignment, DisplayTextLocationCandidate, activity_display_text_location_candidates,
            data_display_text_location_candidates, edge_display_text_location_candidates,
            event_display_text_location_candidates, gateway_display_text_location_candidates,
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
            font_family: EscapedSvgAttribute::new("sans-serif"),
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
    swash_cache: SwashCache,
    embed_font: bool,
    grid: Grid,
    config: Config,
}

impl Svg {
    pub fn new(embed_font: bool, width: usize, height: usize, grid: Grid, config: &Config) -> Self {
        let mut font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        font_system
            .db_mut()
            .load_font_data(include_bytes!("../../../inter-font/Inter-Regular.ttf").to_vec());
        font_system
            .db_mut()
            .load_font_data(include_bytes!("../../../inter-font/Inter-SemiBold.ttf").to_vec());
        font_system
            .db_mut()
            .load_font_data(include_bytes!("../../../inter-font/Inter-Italic.ttf").to_vec());
        Self {
            width,
            height,
            body: String::new(),
            style: SvgStyle::default(),
            font_system,
            swash_cache,
            embed_font,
            grid,
            // Just clone it to avoid lifetimes. It is big, but whatever. Just one clone.
            config: config.clone(),
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
        writeln!(out, r#"<rect x="0" y="0" width="{}" height="{}" fill="white" stroke="white" stroke-width="{STROKE_WIDTH}" />"#, self.width, self.height).unwrap();
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

        write_rotated_text(
            &mut self.body,
            pool_header_width / 2,
            height / 2,
            PreparedText::new(
                &mut self.font_system,
                &mut self.swash_cache,
                title,
                Some(height),
                &merged,
                self.embed_font,
            ),
            &merged,
            "pool-title",
        );

        let mut cumulative_height = 0;
        for (lane_title, lane_height, lane_style) in lanes {
            if lane_title.is_empty() {
                continue;
            }
            let merged = MergedSvgStyle::new(&self.style, lane_style);
            write_rotated_text(
                &mut self.body,
                pool_header_width + lane_header_width / 2,
                cumulative_height + lane_height / 2,
                PreparedText::new(
                    &mut self.font_system,
                    &mut self.swash_cache,
                    lane_title,
                    Some(*lane_height),
                    &merged,
                    self.embed_font,
                ),
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
            r##"<g class="task">
            <use href="#task-box" x="{x}" y="{y}" stroke="{}" fill="{}" width="{ACTIVITY_NODE_WIDTH}" height="{ACTIVITY_NODE_HEIGHT}" stroke-width="{STROKE_WIDTH}" />
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
                r##"  <use href="#{activity_type_href}" x="{}" y="{}" width="20" height="20" stroke="{}" fill="{}" />"##,
                x + 3, y + 3,
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
            let start_y = y + ACTIVITY_NODE_HEIGHT - box_width - y_padding;
            let mut start_x = x + ACTIVITY_NODE_WIDTH / 2
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

        if !text.is_empty() {
            let text = PreparedText::new(
                &mut self.font_system,
                &mut self.swash_cache,
                text,
                Some((ACTIVITY_NODE_WIDTH - STROKE_WIDTH as usize) - 4),
                &merged,
                self.embed_font,
            );
            let text_dims = text.dims();
            write_text_at(
                &mut self.body,
                activity_display_text_location_candidates(
                    text_dims,
                    Dimension {
                        x,
                        y,
                        width: ACTIVITY_NODE_WIDTH,
                        height: ACTIVITY_NODE_HEIGHT,
                    },
                ),
                &mut self.grid,
                text,
                &merged,
                "",
            );
        }
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
        sequence_flow_coming_in_from: Side,
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
            let text = PreparedText::new(
                &mut self.font_system,
                &mut self.swash_cache,
                text,
                Some(MAX_NODE_WIDTH),
                &merged,
                self.embed_font,
            );
            let text_dims = text.dims();
            let position = event_display_text_location_candidates(
                &self.config,
                text_dims,
                Dimension {
                    x,
                    y,
                    width: EVENT_NODE_WIDTH,
                    height: EVENT_NODE_HEIGHT,
                },
                sequence_flow_coming_in_from,
                &|e: &DisplayTextLocationCandidate| {
                    self.grid.box_intersection_weight((e.x, e.y), text_dims)
                },
            );
            write_text_at(&mut self.body, position, &mut self.grid, text, &merged, "");
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
        sequence_flow_coming_in_from: Side,
    ) {
        let merged = MergedSvgStyle::new(&self.style, element_style);

        let symbol = match gateway_type {
            GatewayType::Exclusive => "exclusive",
            GatewayType::Parallel => "parallel",
            GatewayType::Inclusive => "inclusive",
            GatewayType::Event => "event",
        };

        writeln!(self.body, r##"<g class="gateway gateway-{symbol}">"##,).unwrap();
        writeln!(
            self.body,
            r##"
<use href="#gateway-box" x="{x}" y="{y}" stroke="{0}" fill="{1}" width="{GATEWAY_NODE_WIDTH}" height="{GATEWAY_NODE_HEIGHT}" stroke-width="{STROKE_WIDTH}" />
<use href="#gateway-{symbol}" x="{x}" y="{y}" stroke="{0}" fill="{0}" width="{GATEWAY_NODE_WIDTH}" height="{GATEWAY_NODE_HEIGHT}" stroke-width="{STROKE_WIDTH}" />
"##,
merged.stroke, merged.fill
        )
        .unwrap();

        if !text.is_empty() {
            let text = PreparedText::new(
                &mut self.font_system,
                &mut self.swash_cache,
                text,
                Some(MAX_NODE_WIDTH),
                &merged,
                self.embed_font,
            );
            let text_dims = text.dims();
            let position = gateway_display_text_location_candidates(
                &self.config,
                text_dims,
                Dimension {
                    x,
                    y,
                    width: GATEWAY_NODE_WIDTH,
                    height: GATEWAY_NODE_HEIGHT,
                },
                sequence_flow_coming_in_from,
                &|e: &DisplayTextLocationCandidate| {
                    self.grid.box_intersection_weight((e.x, e.y), text_dims)
                },
            );
            write_text_at(&mut self.body, position, &mut self.grid, text, &merged, "");
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
        data_flow_coming_in_from: Side,
    ) {
        let merged = MergedSvgStyle::new(&self.style, style);
        let (symbol, width, height) = match data_type {
            DataType::Store => ("data-store", DATASTORE_NODE_WIDTH, DATASTORE_NODE_HEIGHT),
            DataType::Object => ("data-object", DATAOBJECT_NODE_WIDTH, DATAOBJECT_NODE_HEIGHT),
        };

        writeln!(self.body, r#"<g class="{symbol}">"#,).unwrap();

        writeln!(
            self.body,
            r##"  <use href="#{symbol}" x="{x}" y="{y}" stroke-width="{STROKE_WIDTH}" width="{width}" height="{height}" fill="{}" stroke="{}" />"##,
            merged.fill, merged.stroke,
        )
        .unwrap();

        if !text.is_empty() {
            let text = PreparedText::new(
                &mut self.font_system,
                &mut self.swash_cache,
                text,
                Some(MAX_NODE_WIDTH),
                &merged,
                self.embed_font,
            );
            let text_dims = text.dims();
            let position = data_display_text_location_candidates(
                &self.config,
                text_dims,
                Dimension {
                    x,
                    y,
                    width,
                    height,
                },
                data_flow_coming_in_from,
                &|e: &DisplayTextLocationCandidate| {
                    self.grid.box_intersection_weight((e.x, e.y), text_dims)
                },
            );
            write_text_at(&mut self.body, position, &mut self.grid, text, &merged, "");
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
            if matches!(flow_type, FlowType::DataFlow(..)) {
                // Data flows are ideally straight, so the fine logic for orthogonal edges won't work.
                let mid = if (points.len() & 1) == 1 {
                    // Uneven, so just take middle point.
                    points[points.len() / 2]
                } else {
                    let a = points[points.len() / 2];
                    let b = points[(points.len() / 2) + 1];
                    ((a.0 + b.0) / 2, (a.1 + b.1) / 2)
                };
                let text = PreparedText::new(
                    &mut self.font_system,
                    &mut self.swash_cache,
                    label,
                    Some(MAX_NODE_WIDTH),
                    &merged,
                    self.embed_font,
                );
                write_text_at(
                    &mut self.body,
                    DisplayTextLocationCandidate {
                        alignment: Alignment::Center,
                        x: mid.0,
                        y: mid.1.saturating_sub(merged.line_height as usize / 2),
                    },
                    &mut self.grid,
                    text,
                    &merged,
                    "",
                );
            } else {
                let text = PreparedText::new(
                    &mut self.font_system,
                    &mut self.swash_cache,
                    label,
                    Some(MAX_NODE_WIDTH),
                    &merged,
                    self.embed_font,
                );
                let text_dims = text.dims();
                let position = edge_display_text_location_candidates(
                    &self.config,
                    text_dims,
                    points,
                    &|e: &DisplayTextLocationCandidate| {
                        self.grid.box_intersection_weight((e.x, e.y), text_dims)
                    },
                );
                write_text_at(&mut self.body, position, &mut self.grid, text, &merged, "");
            }
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

fn write_rotated_text(
    body: &mut String,
    x: usize,
    y: usize,
    text: PreparedText,
    merged: &MergedSvgStyle,
    class: &str,
) {
    if text.embed {
        for line in text.buffer.layout_runs() {
            // Came up with this using trial and error. No idea why it works but it works.
            let y_offset = y as f32 + {
                if let Some(max_width) = text.max_width {
                    max_width / 2.
                } else {
                    text.width / 2.
                }
            };
            let x_offset = x as f32 - text.height / 2.;

            writeln!(
            body,
            r#"
            <path transform="translate({x_offset}, {y_offset}) rotate(-90)" d="{}" fill="{}" stroke="none" />
            "#,
            run_to_svg_path(text.font_system, text.swash_cache, &line, 0.0, 0.0),
            merged.font_color
        )
        .unwrap();
        }
    }

    let fill_opacity = if text.embed {
        " fill-opacity=\"0\""
    } else {
        ""
    };

    write!(
        body,
        r#"<text class="{class}" transform="translate({}, {}) rotate(-90)" text-anchor="middle" dominant-baseline="middle" font-family="{}" fill="{}" font-size="{}"{fill_opacity}>"#,
        x as f32 - text.height / 2. - merged.line_height / 2., // So `dy` in the loop above can always be line height.
        y as f32,
        merged.font_family,
        merged.font_color,
        merged.font_size,
    ).unwrap();

    for line in text.buffer.layout_runs() {
        let dy = merged.line_height;
        let start = line.glyphs.first().map(|g| g.start).unwrap_or(0);
        let end = line.glyphs.last().map(|g| g.end).unwrap_or(start);
        write!(
            body,
            r#"<tspan x="0" dy="{dy}">{}</tspan>"#,
            esc_text(&line.text[start..end])
        )
        .unwrap();
    }
    writeln!(body, "</text>").unwrap();
}

struct PreparedText<'a> {
    buffer: Buffer,
    height: f32,
    width: f32,
    font_system: &'a mut FontSystem,
    swash_cache: &'a mut SwashCache,
    max_width: Option<f32>,
    /// If true, the text will be inserted as pre-rendered <path ...>, and overlaid transparently
    /// with an invisible <text ...> for copy support.
    embed: bool,
}

impl<'a> PreparedText<'a> {
    fn new(
        font_system: &'a mut FontSystem,
        swash_cache: &'a mut SwashCache,
        text: &str,
        max_width: Option<usize>,
        merged: &MergedSvgStyle,
        embed: bool,
    ) -> Self {
        let metrics = Metrics::new(merged.font_size, merged.line_height);

        let mut buffer = Buffer::new(font_system, metrics);

        buffer.set_wrap(Wrap::Word);
        buffer.set_size(max_width.map(|width| width as f32), None);
        buffer.set_text(
            // Don't escape just yet. We want to first inspect the text that will be visible.
            text,
            &Attrs::new().family(cosmic_text::Family::Name(merged.font_family)),
            Shaping::Advanced,
            // Can only have Center here, since we don't know where it will be finally positioned at.
            Some(Align::Center),
        );

        // Perform shaping as desired
        buffer.shape_until_scroll(font_system, false /* not sure? */);
        let count = buffer.layout_runs().count().max(1); // Always have at least one.
        let height = count as f32 * merged.line_height;
        let width = buffer.layout_runs().fold(0.0, |state, line| {
            if state < line.line_w {
                line.line_w
            } else {
                state
            }
        });
        PreparedText {
            buffer,
            height,
            width,
            font_system,
            swash_cache,
            max_width: max_width.map(|x| x as f32),
            embed,
        }
    }

    fn dims(&self) -> (usize, usize) {
        (self.width as usize, self.height as usize)
    }
}

/// Writes both the pathified text for display, and an invisible <text> on top of it, so
/// one can select and copy it. Done for portability and user experience (at the cost of file size).
fn write_text_at(
    body: &mut String,
    candidate: DisplayTextLocationCandidate,
    grid: &mut Grid,
    text: PreparedText,
    merged: &MergedSvgStyle,
    class: &str,
) {
    let x = candidate.x as f32;
    let y = candidate.y as f32;

    if text.embed {
        for line in text.buffer.layout_runs() {
            // Came up with this using trial and error. No idea why it works but it works.
            let x = match candidate.alignment {
                Alignment::Left => x,
                Alignment::Center => {
                    if let Some(max_width) = text.max_width {
                        (x + text.width / 2.) - max_width / 2.
                    } else {
                        x - text.width / 2.
                    }
                }
                Alignment::Right => {
                    (if let Some(max_width) = text.max_width {
                        (x + text.width / 2.) - max_width / 2.
                    } else {
                        x - text.width / 2.
                    }) + (text.width - line.line_w) / 2.
                }
            };

            writeln!(
                body,
                r#"<path d="{}" fill="{}" stroke="none" />"#,
                run_to_svg_path(text.font_system, text.swash_cache, &line, x, y),
                merged.font_color
            )
            .unwrap();
        }
    }

    {
        let fill_opacity = if text.embed {
            " fill-opacity=\"0\""
        } else {
            ""
        };
        // Write the invisible text, so it can be selected.
        // This does mean that text is duplicated, but should provide maximum portability and
        // usability.
        write!(
            body,
            r#"<text class="{class}" x="{x}" y="{}" text-anchor="start" dominant-baseline="hanging" font-family="{}" fill="{}" font-size="{}"{fill_opacity}>"#,
            y - merged.line_height, // subtract line height here, so in the loop we can
                                    // unconditionally set `dy` to line height (otherwise first
                                    // iteration must use 0.0).
            merged.font_family,
            merged.font_color,
            merged.font_size,
        ).unwrap();

        for line in text.buffer.layout_runs() {
            let start = line.glyphs.first().map(|g| g.start).unwrap_or(0);
            let end = line.glyphs.last().map(|g| g.end).unwrap_or(start);
            let dy = merged.line_height;
            let x = match candidate.alignment {
                Alignment::Left => x,
                Alignment::Center => x + (text.width - line.line_w) / 2.,
                Alignment::Right => x + (text.width - line.line_w),
            };
            write!(
                body,
                r#"<tspan x="{x}" dy="{dy}">{}</tspan>"#,
                esc_text(&line.text[start..end])
            )
            .unwrap();
        }
        writeln!(body, "</text>").unwrap();
    }

    grid.insert_quadrangle(
        (x.trunc() as usize, y.trunc() as usize),
        ((x + text.width).ceil() as usize, y.trunc() as usize),
        (
            (x + text.width).ceil() as usize,
            (y + text.height).ceil() as usize,
        ),
        (x.trunc() as usize, (y + text.height).ceil() as usize),
        10,
    );
}

fn run_to_svg_path(
    font_system: &mut FontSystem,
    swash_cache: &mut SwashCache,
    run: &LayoutRun<'_>,
    x: f32,
    y: f32,
) -> String {
    // Generated by ChatGPT, some smaller additions made manually.
    let mut d = String::new();
    for glyph in run.glyphs.iter() {
        let physical = glyph.physical((0.0, 0.0), 1.0);

        let Some(commands) = swash_cache.get_outline_commands(font_system, physical.cache_key)
        else {
            continue;
        };

        let dx = x + glyph.x + glyph.x_offset;
        let dy = y + run.line_y + glyph.y + glyph.y_offset;

        for cmd in commands {
            match *cmd {
                Command::MoveTo(p) => {
                    d.push_str(&format!("M {} {} ", p.x + dx, -p.y + dy));
                }
                Command::LineTo(p) => {
                    d.push_str(&format!("L {} {} ", p.x + dx, -p.y + dy));
                }
                Command::QuadTo(p1, p2) => {
                    d.push_str(&format!(
                        "Q {} {} {} {} ",
                        p1.x + dx,
                        -p1.y + dy,
                        p2.x + dx,
                        -p2.y + dy,
                    ));
                }
                Command::CurveTo(p1, p2, p3) => {
                    d.push_str(&format!(
                        "C {} {} {} {} {} {} ",
                        p1.x + dx,
                        -p1.y + dy,
                        p2.x + dx,
                        -p2.y + dy,
                        p3.x + dx,
                        -p3.y + dy,
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
