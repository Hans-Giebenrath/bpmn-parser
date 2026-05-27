//! WIP cleaning up ChatGPT generated code.
//! I'd very much like to use the icons from bpmn.io, but the license requires me to include a
//! watermark on generated images. I don't want that. So I now have to go through all the icons and
//! create my own drawings, using the BPMN spec as a visual inspiration.
//! Look for `TODO continue here` mark to see which ChatGPT generated things to replace next.
//! Anyway, the idea is to have one big `defs` section which contains all icons, and then we can
//! reference it while drawing.

use std::fmt::Write as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskType {
    None,
    User,
    Manual,
    Service,
    Script,
    BusinessRule,
    Send,
    Receive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Start,
    Intermediate,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    None,
    Message,
    Timer,
    Error,
    Signal,
    Conditional,
    Escalation,
    Link,
    Terminate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventSpec {
    pub kind: EventKind,
    pub event_type: EventType,
    /// If true, the inner event marker is filled instead of stroked.
    /// BPMN commonly fills throwing/end event markers.
    pub filled_marker: bool,
}

impl EventSpec {
    pub const fn start(event_type: EventType) -> Self {
        Self {
            kind: EventKind::Start,
            event_type,
            filled_marker: false,
        }
    }

    pub const fn intermediate(event_type: EventType) -> Self {
        Self {
            kind: EventKind::Intermediate,
            event_type,
            filled_marker: false,
        }
    }

    pub const fn end(event_type: EventType) -> Self {
        Self {
            kind: EventKind::End,
            event_type,
            filled_marker: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayType {
    Exclusive,
    Inclusive,
    Parallel,
    EventBased,
    Complex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    DataObject,
    DataInput,
    DataOutput,
    DataStore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowKind {
    Sequence,
    DataAssociation,
    Message,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAnchor {
    Start,
    Middle,
    End,
}

#[derive(Debug, Clone)]
pub struct SvgStyle {
    pub font_family: String,
    pub font_size: usize,
    pub stroke: String,
    pub fill: String,
    pub text_fill: String,
    pub pool_header_fill: String,
    pub lane_header_fill: String,
}

impl Default for SvgStyle {
    fn default() -> Self {
        Self {
            font_family: "Arial, Helvetica, sans-serif".to_owned(),
            font_size: 12,
            stroke: "#222".to_owned(),
            fill: "#fff".to_owned(),
            text_fill: "#111".to_owned(),
            pool_header_fill: "#f3f3f3".to_owned(),
            lane_header_fill: "#fafafa".to_owned(),
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
    pub fn new(width: usize, height: usize) -> Self {
        Self::with_style(width, height, SvgStyle::default())
    }

    pub fn with_style(width: usize, height: usize, style: SvgStyle) -> Self {
        let mut result = Self {
            width,
            height,
            body: String::new(),
            style,
        };
        result
    }

    /// Returns complete SVG file content.
    pub fn finish(mut self) -> String {
        let mut out = String::new();
        writeln!(
                out,
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}" role="img">"#,
                self.width, self.height, self.width, self.height
            ).unwrap();
        writeln!(out, "{}", defs(&self.style)).unwrap();
        writeln!(out, "{}", self.body).unwrap();
        writeln!(out, "</svg>").unwrap();
        out
    }

    /// Draws a BPMN pool as a bordered container with a vertical header.
    pub fn draw_pool(
        &mut self,
        header_width: usize,
        height: usize,
        content_width: usize,
        top_left_corner_xy: (usize, usize),
        title: &str,
    ) {
        self.draw_partition(
            "pool",
            header_width,
            height,
            content_width,
            top_left_corner_xy,
            title,
            &self.style.pool_header_fill.clone(),
            2,
        );
    }

    /// Draws a BPMN lane as a bordered row with a vertical header.
    pub fn draw_lane(
        &mut self,
        header_width: usize,
        height: usize,
        content_width: usize,
        top_left_corner_xy: (usize, usize),
        title: &str,
    ) {
        self.draw_partition(
            "lane",
            header_width,
            height,
            content_width,
            top_left_corner_xy,
            title,
            &self.style.lane_header_fill.clone(),
            1,
        );
    }

    pub fn draw_task(
        &mut self,
        top_left_corner_xy: (usize, usize),
        width: usize,
        height: usize,
        text: &str,
        task_type: TaskType,
    ) {
        let (x, y) = top_left_corner_xy;
        let id = symbol_for_task(task_type);

        writeln!(
            self.body,
            r#"<g class="bpmn-task bpmn-task-{:?}" transform="translate({},{})">"#,
            task_type, x, y
        )
        .unwrap();
        writeln!(
                self.body,
                r#"  <rect x="0" y="0" width="{}" height="{}" rx="10" ry="10" fill="{}" stroke="{}" stroke-width="1.5"/>"#,
                width, height, self.style.fill, self.style.stroke
            ).unwrap();

        if let Some(symbol_id) = id {
            writeln!(
                self.body,
                r##"  <use href="#{}" x="10" y="8" width="18" height="18"/>"##,
                symbol_id
            )
            .unwrap();
        }

        self.write_wrapped_text(
            x + width / 2,
            y + height / 2,
            text,
            width.saturating_sub(18),
            TextAnchor::Middle,
            true,
        );
        writeln!(self.body, "</g>").unwrap();
    }

    /// Draws a BPMN event. The event is positioned by its top-left bounding box.
    pub fn draw_event(
        &mut self,
        top_left_corner_xy: (usize, usize),
        width: usize,
        height: usize,
        text: &str,
        event: EventSpec,
    ) {
        let (x, y) = top_left_corner_xy;
        let cx = x + width / 2;
        let cy = y + height / 2;
        let r = width.min(height) / 2;

        let stroke_width = match event.kind {
            EventKind::Start => 1.5,
            EventKind::Intermediate => 1.5,
            EventKind::End => 3.0,
        };

        writeln!(
            self.body,
            r#"<g class="bpmn-event bpmn-event-{:?} bpmn-event-{:?}">"#,
            event.kind, event.event_type
        )
        .unwrap();
        writeln!(
            self.body,
            r#"  <circle cx="{}" cy="{}" r="{}" fill="{}" stroke="{}" stroke-width="{}"/>"#,
            cx,
            cy,
            r.saturating_sub(2),
            self.style.fill,
            self.style.stroke,
            stroke_width
        )
        .unwrap();

        if matches!(event.kind, EventKind::Intermediate) {
            writeln!(
                self.body,
                r#"  <circle cx="{}" cy="{}" r="{}" fill="none" stroke="{}" stroke-width="1"/>"#,
                cx,
                cy,
                r.saturating_sub(6),
                self.style.stroke
            )
            .unwrap();
        }

        if let Some(symbol_id) = symbol_for_event(event.event_type) {
            let marker_size = r.saturating_sub(10).max(10) * 2;
            let mx = cx.saturating_sub(marker_size / 2);
            let my = cy.saturating_sub(marker_size / 2);
            let class = if event.filled_marker {
                "filled"
            } else {
                "outline"
            };
            writeln!(
                self.body,
                r##"  <use class="{}" href="#{}" x="{}" y="{}" width="{}" height="{}"/>"##,
                class, symbol_id, mx, my, marker_size, marker_size
            )
            .unwrap();
        }

        writeln!(self.body, "</g>").unwrap();

        if !text.is_empty() {
            self.write_wrapped_text(
                cx,
                y + height + 16,
                text,
                width.max(70),
                TextAnchor::Middle,
                false,
            );
        }
    }

    /// Draws a BPMN gateway diamond. Positioned by top-left bounding box.
    pub fn draw_gateway(
        &mut self,
        top_left_corner_xy: (usize, usize),
        width: usize,
        height: usize,
        text: &str,
        gateway_type: GatewayType,
    ) {
        let (x, y) = top_left_corner_xy;
        let cx = x + width / 2;
        let cy = y + height / 2;
        let symbol_id = symbol_for_gateway(gateway_type);

        writeln!(
            self.body,
            r#"<g class="bpmn-gateway bpmn-gateway-{:?}">"#,
            gateway_type
        )
        .unwrap();
        writeln!(
                self.body,
                r#"  <polygon points="{},{} {},{} {},{} {},{}" fill="{}" stroke="{}" stroke-width="1.5"/>"#,
                cx, y,
                x + width, cy,
                cx, y + height,
                x, cy,
                self.style.fill,
                self.style.stroke
            ).unwrap();
        writeln!(
            self.body,
            r##"  <use href="#{}" x="{}" y="{}" width="{}" height="{}"/>"##,
            symbol_id,
            x + width / 4,
            y + height / 4,
            width / 2,
            height / 2
        )
        .unwrap();
        writeln!(self.body, "</g>").unwrap();

        if !text.is_empty() {
            self.write_wrapped_text(
                cx,
                y + height + 16,
                text,
                width.max(80),
                TextAnchor::Middle,
                false,
            );
        }
    }

    /// Draws BPMN data object/input/output/store.
    pub fn draw_data(
        &mut self,
        top_left_corner_xy: (usize, usize),
        width: usize,
        height: usize,
        text: &str,
        data_type: DataType,
    ) {
        let (x, y) = top_left_corner_xy;
        let symbol_id = symbol_for_data(data_type);

        writeln!(
            self.body,
            r#"<g class="bpmn-data bpmn-data-{:?}" transform="translate({},{})">"#,
            data_type, x, y
        )
        .unwrap();
        writeln!(
            self.body,
            r##"  <use href="#{}" x="0" y="0" width="{}" height="{}"/>"##,
            symbol_id, width, height
        )
        .unwrap();
        writeln!(self.body, "</g>").unwrap();

        if !text.is_empty() {
            self.write_wrapped_text(
                x + width / 2,
                y + height + 15,
                text,
                width.max(80),
                TextAnchor::Middle,
                false,
            );
        }
    }

    /// Draws a sequence flow with a solid line and filled arrowhead.
    pub fn draw_sequence_flow(&mut self, points: &[(usize, usize)], label: Option<&str>) {
        self.draw_flow(points, label, FlowKind::Sequence);
    }

    /// Draws a data association/data flow with a dotted line and open arrowhead.
    pub fn draw_data_flow(&mut self, points: &[(usize, usize)], label: Option<&str>) {
        self.draw_flow(points, label, FlowKind::DataAssociation);
    }

    /// Draws a message flow with a dashed line, open circle start, and open arrowhead.
    pub fn draw_message_flow(&mut self, points: &[(usize, usize)], label: Option<&str>) {
        self.draw_flow(points, label, FlowKind::Message);
    }

    /// Lower-level escape hatch for custom SVG snippets.
    /// The caller is responsible for emitting valid SVG.
    pub fn push_raw(&mut self, svg_fragment: &str) {
        self.body.push_str(svg_fragment);
        if !svg_fragment.ends_with('\n') {
            self.body.push('\n');
        }
    }

    fn draw_partition(
        &mut self,
        class_name: &str,
        header_width: usize,
        height: usize,
        content_width: usize,
        top_left_corner_xy: (usize, usize),
        title: &str,
        header_fill: &str,
        stroke_width: usize,
    ) {
        let (x, y) = top_left_corner_xy;
        let total_width = header_width + content_width;
        writeln!(
            self.body,
            r#"<g class="bpmn-{}" transform="translate({},{})">"#,
            class_name, x, y
        )
        .unwrap();
        writeln!(
                self.body,
                r#"  <rect x="0" y="0" width="{}" height="{}" fill="{}" stroke="{}" stroke-width="{}"/>"#,
                total_width, height, self.style.fill, self.style.stroke, stroke_width
            ).unwrap();
        writeln!(
                self.body,
                r#"  <rect x="0" y="0" width="{}" height="{}" fill="{}" stroke="{}" stroke-width="1"/>"#,
                header_width, height, header_fill, self.style.stroke
            ).unwrap();
        writeln!(
            self.body,
            r#"  <line x1="{}" y1="0" x2="{}" y2="{}" stroke="{}" stroke-width="1"/>"#,
            header_width, header_width, height, self.style.stroke
        )
        .unwrap();

        // BPMN pool/lane headers are often vertical. Rotate around header center.
        let tx = header_width / 2;
        let ty = height / 2;
        writeln!(
                self.body,
                r#"  <text x="{}" y="{}" text-anchor="middle" dominant-baseline="middle" transform="rotate(-90 {} {})" font-family="{}" font-size="{}" fill="{}">{}</text>"#,
                tx,
                ty,
                tx,
                ty,
                esc_attr(&self.style.font_family),
                self.style.font_size,
                self.style.text_fill,
                esc_text(title)
            ).unwrap();
        writeln!(self.body, "</g>").unwrap();
    }

    fn draw_flow(&mut self, points: &[(usize, usize)], label: Option<&str>, kind: FlowKind) {
        if points.len() < 2 {
            return;
        }

        let d = polyline_points(points);
        let (class_name, extra, marker_start, marker_end) = match kind {
            FlowKind::Sequence => (
                "bpmn-sequence-flow",
                "",
                "",
                r##" marker-end="url(#arrow-filled)""##,
            ),
            FlowKind::DataAssociation => (
                "bpmn-data-association",
                r#" stroke-dasharray="1 5" stroke-linecap="round""#,
                "",
                r##" marker-end="url(#arrow-open)""##,
            ),
            FlowKind::Message => (
                "bpmn-message-flow",
                r#" stroke-dasharray="8 5""#,
                r##" marker-start="url(#message-start)""##,
                r##" marker-end="url(#arrow-open)""##,
            ),
        };

        writeln!(
                self.body,
                r#"<polyline class="{}" points="{}" fill="none" stroke="{}" stroke-width="1.5"{}{}{} />"#,
                class_name,
                d,
                self.style.stroke,
                extra,
                marker_start,
                marker_end
            ).unwrap();

        if let Some(label) = label.filter(|s| !s.is_empty()) {
            let mid = points[points.len() / 2];
            self.write_wrapped_text(
                mid.0,
                mid.1.saturating_sub(6),
                label,
                120,
                TextAnchor::Middle,
                false,
            );
        }
    }

    fn write_wrapped_text(
        &mut self,
        x: usize,
        y: usize,
        text: &str,
        max_width: usize,
        anchor: TextAnchor,
        center_vertically: bool,
    ) {
        let lines = wrap_text_roughly(text, max_width, self.style.font_size);
        if lines.is_empty() {
            return;
        }

        let line_height = (self.style.font_size as f32 * 1.25).ceil() as isize;
        let total_height = line_height * lines.len() as isize;
        let first_y = if center_vertically {
            y as isize - total_height / 2 + line_height / 2
        } else {
            y as isize
        };
        let anchor = match anchor {
            TextAnchor::Start => "start",
            TextAnchor::Middle => "middle",
            TextAnchor::End => "end",
        };

        writeln!(
                self.body,
                r#"<text x="{}" y="{}" text-anchor="{}" dominant-baseline="middle" font-family="{}" font-size="{}" fill="{}">"#,
                x,
                first_y,
                anchor,
                esc_attr(&self.style.font_family),
                self.style.font_size,
                self.style.text_fill
            ).unwrap();

        for (i, line) in lines.iter().enumerate() {
            let dy = if i == 0 { 0 } else { line_height };
            writeln!(
                self.body,
                r#"  <tspan x="{}" dy="{}">{}</tspan>"#,
                x,
                dy,
                esc_text(line)
            )
            .unwrap();
        }
        writeln!(self.body, "</text>").unwrap();
    }
}

fn symbol_for_task(task_type: TaskType) -> Option<&'static str> {
    match task_type {
        TaskType::None => None,
        TaskType::User => Some("task-user"),
        TaskType::Manual => Some("task-manual"),
        TaskType::Service => Some("task-service"),
        TaskType::Script => Some("task-script"),
        TaskType::BusinessRule => Some("task-business-rule"),
        TaskType::Send => Some("task-send"),
        TaskType::Receive => Some("task-receive"),
    }
}

fn symbol_for_event(event_type: EventType) -> Option<&'static str> {
    match event_type {
        EventType::None => None,
        EventType::Message => Some("event-message"),
        EventType::Timer => Some("event-timer"),
        EventType::Error => Some("event-error"),
        EventType::Signal => Some("event-signal"),
        EventType::Conditional => Some("event-conditional"),
        EventType::Escalation => Some("event-escalation"),
        EventType::Link => Some("event-link"),
        EventType::Terminate => Some("event-terminate"),
    }
}

fn symbol_for_gateway(gateway_type: GatewayType) -> &'static str {
    match gateway_type {
        GatewayType::Exclusive => "gateway-exclusive",
        GatewayType::Inclusive => "gateway-inclusive",
        GatewayType::Parallel => "gateway-parallel",
        GatewayType::EventBased => "gateway-event-based",
        GatewayType::Complex => "gateway-complex",
    }
}

fn symbol_for_data(data_type: DataType) -> &'static str {
    match data_type {
        DataType::DataObject => "data-object",
        DataType::DataInput => "data-input",
        DataType::DataOutput => "data-output",
        DataType::DataStore => "data-store",
    }
}

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

fn wrap_text_roughly(text: &str, max_width: usize, font_size: usize) -> Vec<String> {
    let max_chars = ((max_width as f32) / (font_size as f32 * 0.58)).max(4.0) as usize;
    let mut lines = Vec::new();

    for paragraph in text.split('\n') {
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            if current.is_empty() {
                current.push_str(word);
            } else if current.chars().count() + 1 + word.chars().count() <= max_chars {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(current);
                current = word.to_owned();
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }

    lines
}

fn esc_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn esc_attr(s: &str) -> String {
    esc_text(s).replace('"', "&quot;").replace('\'', "&apos;")
}

fn defs(style: &SvgStyle) -> String {
    format!(
        r##"<defs>
  <style><![CDATA[
    .filled * {{ fill: {stroke}; stroke: {stroke}; }}
    .outline * {{ fill: none; stroke: {stroke}; }}
    text {{ user-select: none; }}
  ]]></style>

  <!-- use with fill="{{stroke}}" -->
  <marker id="arrow-filled" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse">
    <path d="M 0 0 L 10 5 L 0 10 z" />
  </marker>

  <!-- use with fill="none" and stroke="{{stroke}}" -->
  <marker id="arrow-open" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse">
    <path d="M 1 1 L 9 5 L 1 9" stroke-width="1.5" />
  </marker>

  <!-- use with fill="white" and stroke="{{stroke}}" -->
  <marker id="message-start" viewBox="0 0 10 10" refX="5" refY="5" markerWidth="7" markerHeight="7" orient="auto">
    <circle cx="5" cy="5" r="3" stroke-width="1.5" />
  </marker>

  <!-- Task icons. All are authored in a 20x20 viewport. -->
  <symbol id="task-user" viewBox="0 0 20 20">
    <circle cx="10" cy="5" r="3" fill="none" stroke-width="1" />
    <path d="M 17,17 C 17,7 3,7 3,17 Z" fill="none" stroke="{stroke}" stroke-width="1" />
    <path fill="none" stroke="{stroke}" d="M 6,17 C 6,15 6,14 7,13" stroke-width="0.7" />
    <path fill="none" stroke="{stroke}" d="m 14,17 c 0,-2 0,-3 -1,-4" stroke-width="0.7" />
  </symbol>

  <symbol id="task-manual" viewBox="0 0 20 20">
  <path style="fill:none;stroke:{stroke};stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
     d="M 2,6 3,5 h 7 c 0.273837,1.0273837 0,1.3333333 -2,2 H 5 15 c 1,0 1,2 0,2 h -7 8 c 1,0 1,2 0,2 h -7 7 c 1,0 1,1.939827 0,1.939827 H 9 L 14,13 c 1,0 1,1.803733 0,1.803733 0,0 -7.00008,-0.02384 -9,0 C 3.6785895,14.819482 2,14 2,14" />
  </symbol>

  <symbol id="task-service" viewBox="0 0 20 20">
    <g
       transform="translate(0.041016,0.43847664)">
      <path
         id="path23"
         style="fill:#cccccc;stroke:{stroke};stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
         d="M 9.066406,2.0898437 C 8.785746,2.5974027 8.406338,3 8,3 7.595606,3 7.239756,2.6017747 6.980468,2.0976562 A 6,6 0 0 0 4.6191402,3.0761718 C 4.7917279,3.6153969 4.82095,4.1477996 4.5351559,4.4335937 4.2480065,4.720743 3.6972927,4.7048591 3.1406246,4.5449218 A 6,6 0 0 0 2.1386715,6.9375 c 0.5041758,0.259293 0.9023437,0.6170582 0.9023437,1.0214843 0,0.4063849 -0.4025156,0.7857368 -0.9101562,1.0664063 a 6,6 0 0 0 0.984375,2.4003904 c 0.2631149,-0.08457 0.5254974,-0.134733 0.7578125,-0.130859 0.2439954,0.0041 0.4551159,0.0684 0.6015625,0.214843 0.2871649,0.287166 0.2712897,0.839307 0.1113281,1.396485 A 6,6 0 0 0 6.980468,13.908203 C 7.239723,13.404452 7.595814,13.005859 8,13.005859 c 0.406132,0 0.785789,0.40101 1.066406,0.908203 a 6,6 0 0 0 2.435547,-1.007812 c -0.160414,-0.557144 -0.177774,-1.109336 0.109375,-1.396485 0.146446,-0.146446 0.357567,-0.210775 0.601562,-0.214843 0.232391,-0.0039 0.494342,0.04624 0.757813,0.130859 a 6,6 0 0 0 0.984375,-2.4003904 c -0.507136,-0.2806103 -0.908203,-0.660307 -0.908203,-1.0664063 0,-0.4041392 0.398675,-0.7602839 0.902343,-1.0195312 A 6,6 0 0 0 12.947265,4.5449218 C 12.390166,4.7048219 11.837907,4.7207192 11.550781,4.4335937 11.265003,4.1478157 11.293759,3.6153658 11.466797,3.0761718 A 6,6 0 0 0 9.066406,2.0898437 Z m -1.023438,2.4375 A 3.4752038,3.4752038 0 0 1 11.517578,8.0019531 3.4752038,3.4752038 0 0 1 8.042968,11.476562 3.4752038,3.4752038 0 0 1 4.568359,8.0019531 3.4752038,3.4752038 0 0 1 8.042968,4.5273437 Z" />
      <path
         id="path23-2"
         style="fill:#cccccc;stroke:{stroke};stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
         d="m 12.898437,5.2089849 c -0.28066,0.507559 -0.660068,0.9101563 -1.066406,0.9101563 -0.404394,0 -0.760244,-0.3982253 -1.019532,-0.9023438 A 6,6 0 0 0 8.451171,6.195313 C 8.623759,6.7345381 8.652981,7.2669408 8.367191,7.5527349 8.080042,7.8398842 7.529328,7.8240003 6.97266,7.664063 a 6,6 0 0 0 -1.0019575,2.392578 c 0.5041755,0.259293 0.9023435,0.617058 0.9023435,1.021484 0,0.406385 -0.402515,0.785737 -0.910156,1.066407 a 6,6 0 0 0 0.984375,2.40039 c 0.263115,-0.08457 0.525497,-0.134733 0.757813,-0.130859 0.243995,0.0041 0.455115,0.0684 0.601562,0.214843 0.287165,0.287166 0.27129,0.839307 0.111328,1.396485 a 6,6 0 0 0 2.394531,1.001953 C 11.071754,16.523593 11.427845,16.125 11.832031,16.125 c 0.406132,0 0.785789,0.40101 1.066406,0.908203 a 6,6 0 0 0 2.435547,-1.007812 c -0.160414,-0.557144 -0.177774,-1.109336 0.109375,-1.396485 0.146446,-0.146446 0.357567,-0.210775 0.601562,-0.214843 0.232391,-0.0039 0.494342,0.04624 0.757813,0.130859 a 6,6 0 0 0 0.984375,-2.40039 c -0.507136,-0.280611 -0.908203,-0.660307 -0.908203,-1.066407 0,-0.404139 0.398675,-0.760283 0.902343,-1.019531 A 6,6 0 0 0 16.779296,7.664063 C 16.222197,7.8239631 15.669938,7.8398604 15.382812,7.5527349 15.097034,7.2669569 15.12579,6.734507 15.298828,6.195313 A 6,6 0 0 0 12.898437,5.2089849 Z m -1.023438,2.4375 a 3.4752038,3.4752038 0 0 1 3.47461,3.4746091 3.4752038,3.4752038 0 0 1 -3.47461,3.474609 3.4752038,3.4752038 0 0 1 -3.474609,-3.474609 3.4752038,3.4752038 0 0 1 3.474609,-3.4746091 z" />
    </g>
  </symbol>

  <symbol id="task-script" viewBox="0 0 20 20">
      <path
         style="fill:#cccccc;stroke:{stroke};stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
         d="M 6.1728012,4 H 13.694508"
         />
      <path
         style="fill:#cccccc;stroke:{stroke};stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
         d="M 5.8271988,7 H 13.348908"
         />
      <path
         style="fill:#cccccc;stroke:{stroke};stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
         d="M 7.8238944,13 H 15.3456"
          />
      <path
         style="fill:#cccccc;stroke:{stroke};stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
         d="M 7.3054908,16 H 14.8272"
          />
      <path
         style="fill:#cccccc;stroke:{stroke};stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
         d="m 6.807143,10 h 7.521711"
          />
  </symbol>

  <symbol id="task-business-rule" viewBox="0 0 20 20">
      <rect
         style="fill:none;stroke:{stroke};stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
         width="16"
         height="16"
         x="2"
         y="2" />
      <rect
         style="fill:#cccccc;stroke:{stroke};stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
         width="16"
         height="4"
         x="2"
         y="2" />
      <path
         style="fill:none;stroke:{stroke};stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
         d="M 2,10.000001 H 18" />
      <path
         style="fill:none;stroke:{stroke};stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
         d="M 2,14.000004 H 18" />
      <path
         style="fill:none;stroke:{stroke};stroke-width:0.8;stroke-linecap:round;stroke-linejoin:round"
         d="M 6,6 V 18" />
  </symbol>

  <symbol id="task-send" viewBox="0 0 20 20">
      <path
         d="m 3,6 h 14 v 9 H 3 Z"
         fill="none"
         stroke="{stroke}"
         style="stroke-width:1;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
         />
      <path
         d="m 3,6 7,4 7,-4"
         fill="none"
         stroke="{stroke}"
         style="stroke-width:1;stroke-linecap:round;stroke-linejoin:round;stroke-dasharray:none"
         />
  </symbol>

  <symbol id="task-receive" viewBox="0 0 20 20">
  <path
     style="baseline-shift:baseline;display:inline;overflow:visible;vector-effect:none;stroke-linecap:round;stroke-linejoin:round;enable-background:accumulate;stop-color:{stroke}"
     d="M 3 5.5 C 2.7238691 5.5000276 2.5000276 5.7238691 2.5 6 L 2.5 15 C 2.5000276 15.276131 2.7238691 15.499972 3 15.5 L 17 15.5 C 17.276131 15.499972 17.499972 15.276131 17.5 15 L 17.5 6 C 17.499972 5.7238691 17.276131 5.5000276 17 5.5 L 3 5.5 z M 16.800781 5.6621094 A 0.5 0.5 0 0 1 17.232422 5.9160156 A 0.5 0.5 0 0 1 17.042969 6.5976562 L 10.246094 10.435547 A 0.50005 0.50005 0 0 1 9.7539062 10.435547 L 2.9570312 6.5976562 A 0.5 0.5 0 0 1 2.7675781 5.9160156 A 0.5 0.5 0 0 1 3.0703125 5.6816406 A 0.5 0.5 0 0 1 3.4492188 5.7265625 L 10 9.4257812 L 16.550781 5.7265625 A 0.5 0.5 0 0 1 16.800781 5.6621094 z " />
  </symbol>

  TODO continue here
  <!-- Event markers. All are authored in a 20x20 viewport. -->
  <symbol id="event-message" viewBox="0 0 20 20">
    <path d="M3 6 h14 v9 H3 z" fill="none" stroke="{stroke}" stroke-width="1.5"/>
    <path d="M3 6 l7 5 l7-5" fill="none" stroke="{stroke}" stroke-width="1.5"/>
  </symbol>

  <symbol id="event-timer" viewBox="0 0 20 20">
    <circle cx="10" cy="10" r="7" fill="none" stroke="{stroke}" stroke-width="1.5"/>
    <path d="M10 10 V5 M10 10 L14 12" fill="none" stroke="{stroke}" stroke-width="1.5" stroke-linecap="round"/>
    <path d="M10 3 V1 M7 1 h6" stroke="{stroke}" stroke-width="1.2"/>
  </symbol>

  <symbol id="event-error" viewBox="0 0 20 20">
    <path d="M5 16 L9 4 L11 12 L15 4 L11 16 L9 8 z" fill="none" stroke="{stroke}" stroke-width="1.5" stroke-linejoin="round"/>
  </symbol>

  <symbol id="event-signal" viewBox="0 0 20 20">
    <path d="M10 3 L17 16 H3 z" fill="none" stroke="{stroke}" stroke-width="1.5" stroke-linejoin="round"/>
  </symbol>

  <symbol id="event-conditional" viewBox="0 0 20 20">
    <path d="M5 4 h10 v12 H5 z" fill="none" stroke="{stroke}" stroke-width="1.4"/>
    <path d="M7 8 h6 M7 11 h6 M7 14 h4" stroke="{stroke}" stroke-width="1.2"/>
  </symbol>

  <symbol id="event-escalation" viewBox="0 0 20 20">
    <path d="M10 3 L16 16 L10 12 L4 16 z" fill="none" stroke="{stroke}" stroke-width="1.5" stroke-linejoin="round"/>
  </symbol>

  <symbol id="event-link" viewBox="0 0 20 20">
    <path d="M4 10 h10 M10 5 l5 5 l-5 5" fill="none" stroke="{stroke}" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/>
  </symbol>

  <symbol id="event-terminate" viewBox="0 0 20 20">
    <circle cx="10" cy="10" r="6" fill="{stroke}" stroke="{stroke}" stroke-width="1.5"/>
  </symbol>

  <!-- Gateway markers. -->
  <symbol id="gateway-exclusive" viewBox="0 0 20 20">
    <path d="M5 5 L15 15 M15 5 L5 15" stroke="{stroke}" stroke-width="3" stroke-linecap="round"/>
  </symbol>

  <symbol id="gateway-inclusive" viewBox="0 0 20 20">
    <circle cx="10" cy="10" r="6" fill="none" stroke="{stroke}" stroke-width="2.3"/>
  </symbol>

  <symbol id="gateway-parallel" viewBox="0 0 20 20">
    <path d="M10 4 V16 M4 10 H16" stroke="{stroke}" stroke-width="3" stroke-linecap="round"/>
  </symbol>

  <symbol id="gateway-event-based" viewBox="0 0 20 20">
    <circle cx="10" cy="10" r="7" fill="none" stroke="{stroke}" stroke-width="1.4"/>
    <circle cx="10" cy="10" r="5" fill="none" stroke="{stroke}" stroke-width="1.2"/>
    <path d="M10 4 L11.8 8.2 L16.2 8.6 L12.9 11.5 L13.9 16 L10 13.7 L6.1 16 L7.1 11.5 L3.8 8.6 L8.2 8.2 z" fill="none" stroke="{stroke}" stroke-width="1"/>
  </symbol>

  <symbol id="gateway-complex" viewBox="0 0 20 20">
    <path d="M10 3 V17 M3 10 H17 M5 5 L15 15 M15 5 L5 15" stroke="{stroke}" stroke-width="2" stroke-linecap="round"/>
  </symbol>

  <!-- Data shapes authored in a 60x70 viewport. -->
  <symbol id="data-object" viewBox="0 0 60 70">
    <path d="M8 2 h32 l12 12 v54 H8 z" fill="white" stroke="{stroke}" stroke-width="1.5"/>
    <path d="M40 2 v12 h12" fill="none" stroke="{stroke}" stroke-width="1.5"/>
  </symbol>

  <symbol id="data-input" viewBox="0 0 60 70">
    <path d="M8 2 h32 l12 12 v54 H8 z" fill="white" stroke="{stroke}" stroke-width="1.5"/>
    <path d="M40 2 v12 h12" fill="none" stroke="{stroke}" stroke-width="1.5"/>
    <path d="M17 35 h22 M31 27 l8 8 l-8 8" fill="none" stroke="{stroke}" stroke-width="2"/>
  </symbol>

  <symbol id="data-output" viewBox="0 0 60 70">
    <path d="M8 2 h32 l12 12 v54 H8 z" fill="white" stroke="{stroke}" stroke-width="1.5"/>
    <path d="M40 2 v12 h12" fill="none" stroke="{stroke}" stroke-width="1.5"/>
    <path d="M39 35 H17 M25 27 l-8 8 l8 8" fill="none" stroke="{stroke}" stroke-width="2"/>
  </symbol>

  <symbol id="data-store" viewBox="0 0 70 60">
    <ellipse cx="35" cy="10" rx="25" ry="8" fill="white" stroke="{stroke}" stroke-width="1.5"/>
    <path d="M10 10 v38 c0 5 50 5 50 0 V10" fill="white" stroke="{stroke}" stroke-width="1.5"/>
    <path d="M10 27 c0 5 50 5 50 0 M10 43 c0 5 50 5 50 0" fill="none" stroke="{stroke}" stroke-width="1.2"/>
  </symbol>
</defs>"##,
        stroke = esc_attr(&style.stroke)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_svg_content() {
        let mut doc = Svg::new(900, 420);
        doc.draw_pool(40, 320, 760, (40, 40), "Sales");
        doc.draw_lane(40, 160, 760, (40, 40), "Agent");
        doc.draw_lane(40, 160, 760, (40, 200), "System");

        doc.draw_event(
            (110, 95),
            36,
            36,
            "Request received",
            EventSpec::start(EventType::Message),
        );
        doc.draw_task((200, 75), 130, 72, "Check request", TaskType::User);
        doc.draw_gateway((390, 83), 56, 56, "Valid?", GatewayType::Exclusive);
        doc.draw_task((505, 75), 130, 72, "Send offer", TaskType::Send);
        doc.draw_event((700, 95), 36, 36, "Done", EventSpec::end(EventType::None));

        doc.draw_sequence_flow(&[(146, 113), (200, 113)], None);
        doc.draw_sequence_flow(&[(330, 113), (390, 113)], None);
        doc.draw_sequence_flow(&[(446, 113), (505, 113)], Some("yes"));
        doc.draw_sequence_flow(&[(635, 113), (700, 113)], None);

        doc.draw_data((230, 250), 54, 64, "Customer data", DataType::DataObject);
        doc.draw_data_flow(&[(265, 250), (265, 147)], None);
        doc.draw_message_flow(
            &[(128, 95), (128, 40), (760, 40), (760, 95)],
            Some("external msg"),
        );

        let svg = doc.finish();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("<defs>"));
        assert!(svg.contains("task-user"));
        assert!(svg.contains("Request received"));
    }
}
