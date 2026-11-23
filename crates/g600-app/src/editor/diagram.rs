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
    let (resp, painter) = ui.allocate_painter(vec2(520.0, 300.0), Sense::hover());
    let o = resp.rect.min;
    let at = |x: f32, y: f32| o + vec2(x, y);
    let visuals = ui.visuals().clone();

    // Body outline and wheel.
    let body = Rect::from_min_max(at(205.0, 15.0), at(315.0, 290.0));
    painter.rect(body, CornerRadius::same(50), visuals.extreme_bg_color, Stroke::new(1.5, visuals.weak_text_color()), StrokeKind::Inside);
    painter.line_segment([at(260.0, 15.0), at(260.0, 110.0)], Stroke::new(1.0, visuals.weak_text_color()));
    painter.rect_filled(Rect::from_center_size(at(260.0, 60.0), vec2(14.0, 34.0)), CornerRadius::same(6), visuals.weak_text_color());

    // (button index, dot on the body, box top-left)
    let layout: [(usize, Pos2, Pos2); 8] = [
        (0, at(232.0, 45.0), at(10.0, 10.0)),    // Left
        (3, at(250.0, 60.0), at(10.0, 70.0)),    // Tilt-L / Back
        (6, at(260.0, 130.0), at(10.0, 130.0)),  // G7
        (7, at(260.0, 160.0), at(10.0, 190.0)),  // G8
        (2, at(260.0, 45.0), at(374.0, 10.0)),   // Middle
        (1, at(288.0, 45.0), at(374.0, 70.0)),   // Right
        (4, at(270.0, 60.0), at(374.0, 130.0)),  // Tilt-R / Forward
        (5, at(313.0, 200.0), at(374.0, 190.0)), // Ring (G-Shift)
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

fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    }
}
