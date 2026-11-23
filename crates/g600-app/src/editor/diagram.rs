//! Our own schematic drawing of the G600 (no Logitech artwork): a top view
//! with labelled boxes and leader lines, and the 3x4 thumb grid G9-G20.
//! Each box shows the button name and its current action. Left-click selects
//! a button; right-click selects it and starts key capture.

use eframe::egui::{self, pos2, vec2, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui};
use g600_core::{action::Action, profile::BUTTON_NAMES};

pub struct ButtonView<'a> {
    pub actions: &'a [Action; 20],
    /// Which buttons differ from the factory layout (highlighted).
    pub changed: [bool; 20],
    pub selected: Option<usize>,
    /// Which layer this view shows (keeps widget ids apart).
    pub shift: bool,
    /// The selected button is waiting for a key press.
    pub capturing: bool,
}

const BOX: egui::Vec2 = vec2(136.0, 42.0);

/// A click on a button box.
#[derive(Clone, Copy)]
pub struct Click {
    pub button: usize,
    /// Right-click: start key capture.
    pub capture: bool,
}

/// Top view. Returns a click on one of its buttons.
pub fn top_view(ui: &mut Ui, v: &ButtonView) -> Option<Click> {
    let (resp, painter) = ui.allocate_painter(vec2(520.0, 320.0), Sense::hover());
    let o = resp.rect.min;
    let at = |x: f32, y: f32| o + vec2(x, y);
    let visuals = ui.visuals().clone();
    let line = Stroke::new(1.5, visuals.weak_text_color());
    let groove = Stroke::new(2.0, visuals.weak_text_color());

    // Shape traced from a top-view photo of the mouse (~540x1030 px; the
    // cable enters at x=262, the front edge is at y=178). Coordinates below
    // are photo pixels; `p` scales them onto the canvas.
    const S: f32 = 0.33;
    let p = |x: f32, y: f32| at(260.0 + (x - 262.0) * S, 10.0 + (y - 178.0) * S);

    // Outline, front-centre first, then clockwise.
    let contour = smooth_closed(
        &[
            (262.0, 178.0), (330.0, 180.0), (400.0, 190.0), (440.0, 205.0), (500.0, 250.0),
            (530.0, 330.0), (536.0, 430.0), (534.0, 560.0), (530.0, 700.0), (516.0, 820.0),
            (490.0, 900.0), (440.0, 968.0), (360.0, 1004.0), (262.0, 1014.0), (170.0, 1000.0),
            (100.0, 960.0), (55.0, 880.0), (35.0, 790.0), (24.0, 680.0), (21.0, 560.0),
            (24.0, 440.0), (34.0, 330.0), (60.0, 258.0), (110.0, 214.0), (180.0, 188.0),
        ]
        .map(|(x, y)| p(x, y)),
        8,
    );
    fill_star_shaped(&painter, p(262.0, 560.0), &contour, visuals.extreme_bg_color);
    painter.add(egui::Shape::closed_line(contour, line));

    // Split between the left and right buttons (interrupted by wheel and G-keys).
    for (a, b) in [((258.0, 180.0), (258.0, 264.0)), ((258.0, 380.0), (258.0, 398.0)), ((260.0, 540.0), (262.0, 594.0))] {
        painter.line_segment([p(a.0, a.1), p(b.0, b.1)], line);
    }
    // Groove separating the ring-finger (G-Shift) button on the right.
    painter.add(egui::Shape::line(
        smooth_open(&[(432.0, 212.0), (430.0, 262.0), (426.0, 350.0), (418.0, 480.0), (408.0, 600.0), (398.0, 700.0)].map(|(x, y)| p(x, y)), 8),
        groove,
    ));
    // Wheel, then G8 (front) and G7 (behind it).
    let rect = |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_min_max(p(x0, y0), p(x1, y1));
    painter.rect_filled(rect(229.0, 266.0, 287.0, 378.0), CornerRadius::same(8), visuals.weak_text_color());
    for r in [rect(228.0, 400.0, 290.0, 456.0), rect(228.0, 462.0, 290.0, 538.0)] {
        painter.rect_stroke(r, CornerRadius::same(4), line, StrokeKind::Inside);
    }

    // (button index, dot on the body, box top-left)
    let layout: [(usize, Pos2, Pos2); 8] = [
        (0, p(150.0, 330.0), at(10.0, 10.0)),   // Left
        (3, p(205.0, 338.0), at(10.0, 70.0)),   // Tilt-L / Back
        (7, p(259.0, 428.0), at(10.0, 130.0)),  // G8 (front)
        (6, p(259.0, 500.0), at(10.0, 190.0)),  // G7
        (2, p(258.0, 300.0), at(374.0, 10.0)),  // Middle
        (1, p(370.0, 330.0), at(374.0, 70.0)),  // Right
        (4, p(311.0, 338.0), at(374.0, 130.0)), // Tilt-R / Forward
        (5, p(475.0, 560.0), at(374.0, 190.0)), // Ring (G-Shift)
    ];
    let mut clicked = None;
    for (i, dot, top_left) in layout {
        let rect = Rect::from_min_size(top_left, BOX);
        let anchor = if top_left.x < dot.x { rect.right_center() } else { rect.left_center() };
        painter.line_segment([anchor, dot], Stroke::new(1.0, visuals.weak_text_color()));
        painter.circle_filled(dot, 3.5, visuals.text_color());
        if let Some(c) = draw_box(ui, &painter, rect, i, v) {
            clicked = Some(c);
        }
    }
    clicked
}

/// Thumb grid G9-G20 (3 columns x 4 rows, as on the mouse). Returns a click on one of its buttons.
pub fn side_view(ui: &mut Ui, v: &ButtonView) -> Option<Click> {
    let gap = 8.0;
    let size = vec2(3.0 * BOX.x + 2.0 * gap + 24.0, 4.0 * BOX.y + 3.0 * gap + 24.0);
    let (resp, painter) = ui.allocate_painter(size, Sense::hover());
    let visuals = ui.visuals().clone();
    painter.rect(resp.rect, CornerRadius::same(18), visuals.extreme_bg_color, Stroke::new(1.5, visuals.weak_text_color()), StrokeKind::Inside);
    let mut clicked = None;
    for n in 0..12 {
        let (row, col) = (n / 3, n % 3);
        let min = resp.rect.min + vec2(12.0 + col as f32 * (BOX.x + gap), 12.0 + row as f32 * (BOX.y + gap));
        let i = 8 + n; // G9 is index 8
        if let Some(c) = draw_box(ui, &painter, Rect::from_min_size(min, BOX), i, v) {
            clicked = Some(c);
        }
    }
    clicked
}

fn draw_box(ui: &mut Ui, painter: &egui::Painter, rect: Rect, i: usize, v: &ButtonView) -> Option<Click> {
    let resp = ui.interact(rect, egui::Id::unique(("g600-button", v.shift, i)), Sense::click());
    let visuals = ui.visuals();
    let selected = v.selected == Some(i);
    let fill = if selected {
        visuals.selection.bg_fill
    } else if v.changed[i] {
        visuals.selection.bg_fill.gamma_multiply(0.35)
    } else if resp.hovered() {
        visuals.widgets.hovered.weak_bg_fill
    } else {
        visuals.widgets.inactive.weak_bg_fill
    };
    let stroke = if resp.hovered() || selected {
        Stroke::new(1.5, visuals.selection.stroke.color)
    } else {
        Stroke::new(1.0, visuals.widgets.inactive.bg_stroke.color)
    };
    painter.rect(rect, CornerRadius::same(6), fill, stroke, StrokeKind::Inside);
    let text = if selected { visuals.selection.stroke.color } else { visuals.text_color() };
    let name = BUTTON_NAMES[i];
    painter.text(rect.min + vec2(8.0, 5.0), Align2::LEFT_TOP, name, FontId::proportional(11.0), visuals.weak_text_color());
    painter.text(
        pos2(rect.min.x + 8.0, rect.max.y - 5.0),
        Align2::LEFT_BOTTOM,
        if selected && v.capturing { "press a key…".to_string() } else { ellipsize(&v.actions[i].to_string(), 20) },
        FontId::proportional(14.0),
        if v.changed[i] && !selected { Color32::from_rgb(0x40, 0xC0, 0x70) } else { text },
    );
    let resp = resp.on_hover_text(format!("{name}: {}\nRight-click to capture a key", v.actions[i]));
    if resp.secondary_clicked() {
        Some(Click { button: i, capture: true })
    } else if resp.clicked() {
        Some(Click { button: i, capture: false })
    } else {
        None
    }
}

/// Catmull-Rom spline through `pts` (closed loop), `steps` points per segment.
fn smooth_closed(pts: &[Pos2], steps: usize) -> Vec<Pos2> {
    let n = pts.len();
    (0..n)
        .flat_map(|i| {
            let p = |k: usize| pts[(i + n + k - 1) % n];
            spline_segment(p(0), p(1), p(2), p(3), steps)
        })
        .collect()
}

/// Catmull-Rom spline through `pts` (open curve, ends included).
fn smooth_open(pts: &[Pos2], steps: usize) -> Vec<Pos2> {
    let n = pts.len();
    let mut out: Vec<Pos2> = (0..n - 1)
        .flat_map(|i| {
            let p = |k: isize| pts[(i as isize + k).clamp(0, n as isize - 1) as usize];
            spline_segment(p(-1), p(0), p(1), p(2), steps)
        })
        .collect();
    out.push(pts[n - 1]);
    out
}

/// Points from p1 towards p2 (p2 excluded) on a Catmull-Rom segment.
fn spline_segment(p0: Pos2, p1: Pos2, p2: Pos2, p3: Pos2, steps: usize) -> Vec<Pos2> {
    (0..steps)
        .map(|s| {
            let t = s as f32 / steps as f32;
            let (t2, t3) = (t * t, t * t * t);
            let f = |a: f32, b: f32, c: f32, d: f32| {
                0.5 * (2.0 * b + (c - a) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2 + (3.0 * b - a - 3.0 * c + d) * t3)
            };
            pos2(f(p0.x, p1.x, p2.x, p3.x), f(p0.y, p1.y, p2.y, p3.y))
        })
        .collect()
}

/// Fill a closed outline that every point of which can be "seen" from
/// `center` (egui only fills convex polygons on its own).
fn fill_star_shaped(painter: &egui::Painter, center: Pos2, outline: &[Pos2], color: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(center, color);
    for &p in outline {
        mesh.colored_vertex(p, color);
    }
    let n = outline.len() as u32;
    for k in 0..n {
        mesh.add_triangle(0, 1 + k, 1 + (k + 1) % n);
    }
    painter.add(egui::Shape::mesh(mesh));
}

fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    }
}
