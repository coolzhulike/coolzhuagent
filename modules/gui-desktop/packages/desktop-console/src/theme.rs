use std::fs;

use eframe::egui::{self, Color32, FontFamily, FontId, Pos2, Rect, Stroke, Vec2};

pub const SKY: Color32 = Color32::from_rgb(92, 148, 252);
pub const CLOUD: Color32 = Color32::from_rgb(252, 252, 252);
pub const GROUND: Color32 = Color32::from_rgb(200, 76, 12);
pub const BRICK: Color32 = Color32::from_rgb(188, 92, 44);
pub const QUESTION: Color32 = Color32::from_rgb(252, 188, 60);
pub const PIPE: Color32 = Color32::from_rgb(0, 168, 0);
pub const PANEL: Color32 = Color32::from_rgb(32, 32, 52);
pub const PANEL_ALT: Color32 = Color32::from_rgb(18, 18, 30);
pub const PANEL_STROKE: Color32 = Color32::from_rgb(252, 188, 60);
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(252, 252, 252);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(196, 208, 255);
pub const USER_BUBBLE: Color32 = Color32::from_rgb(168, 32, 32);
pub const ASSISTANT_BUBBLE: Color32 = Color32::from_rgb(40, 88, 168);
pub const SUCCESS: Color32 = Color32::from_rgb(110, 200, 80);
pub const WARNING: Color32 = Color32::from_rgb(252, 144, 36);
pub const ERROR: Color32 = Color32::from_rgb(232, 64, 64);

const LOGO_LINES: [&str; 2] = ["COOLZHU", "CODE"];

pub fn apply_theme(ctx: &egui::Context) {
    install_windows_cjk_fallback(ctx);
    ctx.options_mut(|options| {
        options.warn_on_id_clash = false;
    });

    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = PANEL;
    style.visuals.window_fill = PANEL;
    style.visuals.extreme_bg_color = PANEL_ALT;
    style.visuals.widgets.noninteractive.bg_fill = PANEL_ALT;
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.5, TEXT_PRIMARY);
    style.visuals.widgets.inactive.bg_fill = PANEL_ALT;
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.5, TEXT_PRIMARY);
    style.visuals.widgets.hovered.bg_fill = BRICK;
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(2.0, TEXT_PRIMARY);
    style.visuals.widgets.active.bg_fill = QUESTION;
    style.visuals.widgets.active.fg_stroke = Stroke::new(2.0, PANEL_ALT);
    style.visuals.override_text_color = Some(TEXT_PRIMARY);
    style.text_styles.insert(
        egui::TextStyle::Heading,
        FontId::new(28.0, FontFamily::Monospace),
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        FontId::new(18.0, FontFamily::Monospace),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        FontId::new(18.0, FontFamily::Monospace),
    );
    style.text_styles.insert(
        egui::TextStyle::Monospace,
        FontId::new(17.0, FontFamily::Monospace),
    );
    ctx.set_style(style);
}

fn install_windows_cjk_fallback(ctx: &egui::Context) {
    let Some(bytes) = load_first_font_bytes(&[
        "C:\\Windows\\Fonts\\Deng.ttf",
        "C:\\Windows\\Fonts\\simhei.ttf",
        "C:\\Windows\\Fonts\\NotoSansSC-VF.ttf",
        "C:\\Windows\\Fonts\\simsunb.ttf",
    ]) else {
        return;
    };

    let mut fonts = egui::FontDefinitions::default();
    let font_name = "windows-cjk-fallback".to_string();

    fonts
        .font_data
        .insert(font_name.clone(), egui::FontData::from_owned(bytes).into());

    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push(font_name.clone());
    }

    ctx.set_fonts(fonts);
}

fn load_first_font_bytes(paths: &[&str]) -> Option<Vec<u8>> {
    paths.iter().find_map(|path| fs::read(path).ok())
}

pub fn paint_backdrop(painter: &egui::Painter, rect: Rect, time: f32) {
    painter.rect_filled(rect, 0.0, SKY);
    paint_cloud(
        painter,
        Pos2::new(rect.left() + 90.0 + time.sin() * 6.0, rect.top() + 48.0),
    );
    paint_cloud(
        painter,
        Pos2::new(
            rect.left() + 360.0 + (time * 0.7).cos() * 8.0,
            rect.top() + 88.0,
        ),
    );
    paint_cloud(
        painter,
        Pos2::new(
            rect.right() - 280.0 + (time * 0.8).sin() * 10.0,
            rect.top() + 62.0,
        ),
    );

    paint_hills(painter, rect);
    paint_brick_logo(painter, rect);

    let ground_height = 112.0;
    let ground_top = rect.bottom() - ground_height;
    painter.rect_filled(
        Rect::from_min_max(
            Pos2::new(rect.left(), ground_top),
            Pos2::new(rect.right(), rect.bottom()),
        ),
        0.0,
        GROUND,
    );

    let mut x = rect.left();
    while x < rect.right() + 40.0 {
        paint_brick_tile(painter, Pos2::new(x, ground_top + 18.0), BRICK);
        x += 34.0;
    }

    paint_pipe(painter, Pos2::new(rect.right() - 220.0, ground_top - 68.0));
    paint_question_block(painter, Pos2::new(rect.left() + 88.0, ground_top - 154.0));
    paint_question_block(painter, Pos2::new(rect.left() + 124.0, ground_top - 154.0));
}

pub fn logo_reserve_height(rect: Rect) -> f32 {
    let layout = compute_logo_layout(rect);
    (layout.start_y - rect.top()) + layout.total_height + 36.0
}

pub fn pixel_frame(fill: Color32) -> egui::Frame {
    egui::Frame::group(&egui::Style::default())
        .fill(fill)
        .inner_margin(egui::Margin::same(8))
        .stroke(Stroke::new(3.0, PANEL_STROKE))
}

pub fn hud_frame(fill: Color32) -> egui::Frame {
    egui::Frame::group(&egui::Style::default())
        .fill(fill)
        .inner_margin(egui::Margin::same(10))
        .stroke(Stroke::new(3.0, PANEL_STROKE))
}

pub fn compact_frame(fill: Color32) -> egui::Frame {
    egui::Frame::group(&egui::Style::default())
        .fill(fill)
        .inner_margin(egui::Margin::same(6))
        .stroke(Stroke::new(2.0, PANEL_STROKE))
}

pub fn heading(text: &str) -> egui::RichText {
    egui::RichText::new(text)
        .monospace()
        .size(26.0)
        .strong()
        .color(TEXT_PRIMARY)
}

pub fn label(text: &str) -> egui::RichText {
    egui::RichText::new(text)
        .monospace()
        .size(16.0)
        .color(TEXT_MUTED)
}

pub fn status_dot(painter: &egui::Painter, center: Pos2, color: Color32) {
    painter.circle_filled(center, 5.0, color);
    painter.circle_stroke(center, 6.5, Stroke::new(1.0, PANEL_ALT));
}

pub fn paint_top_plaque(painter: &egui::Painter, rect: Rect, title: &str) {
    let width = rect.width().min(520.0);
    let height = 60.0;
    let plaque = Rect::from_center_size(rect.center(), Vec2::new(width, height));
    painter.rect_filled(plaque, 0.0, PANEL_ALT);
    painter.rect_stroke(
        plaque,
        0.0,
        Stroke::new(4.0, BRICK),
        egui::StrokeKind::Inside,
    );

    let mut x = plaque.left();
    while x < plaque.right() {
        paint_brick_tile(painter, Pos2::new(x, plaque.top()), BRICK);
        paint_brick_tile(painter, Pos2::new(x, plaque.bottom() - 32.0), BRICK);
        x += 34.0;
    }

    painter.text(
        plaque.center(),
        egui::Align2::CENTER_CENTER,
        title,
        FontId::new(28.0, FontFamily::Monospace),
        QUESTION,
    );
}

pub fn paint_question_corner(painter: &egui::Painter, rect: Rect) {
    let size = 28.0;
    let top_left = Pos2::new(rect.right() - size - 3.0, rect.top() + 3.0);
    let block = Rect::from_min_size(top_left, Vec2::splat(size));
    painter.rect_filled(block, 0.0, QUESTION);
    painter.rect_stroke(
        block,
        0.0,
        Stroke::new(2.0, PANEL_ALT),
        egui::StrokeKind::Inside,
    );
    painter.text(
        block.center(),
        egui::Align2::CENTER_CENTER,
        "?",
        FontId::new(18.0, FontFamily::Monospace),
        PANEL_ALT,
    );
}

fn paint_cloud(painter: &egui::Painter, top_left: Pos2) {
    let size = 18.0;
    let pixels = [
        (1.0, 0.0),
        (2.0, 0.0),
        (0.0, 1.0),
        (1.0, 1.0),
        (2.0, 1.0),
        (3.0, 1.0),
        (0.0, 2.0),
        (1.0, 2.0),
        (2.0, 2.0),
        (3.0, 2.0),
        (1.0, 3.0),
        (2.0, 3.0),
    ];
    for (x, y) in pixels {
        let min = Pos2::new(top_left.x + x * size, top_left.y + y * size);
        let max = Pos2::new(min.x + size, min.y + size);
        painter.rect_filled(Rect::from_min_max(min, max), 0.0, CLOUD);
    }
}

fn paint_hills(painter: &egui::Painter, rect: Rect) {
    let ground_top = rect.bottom() - 112.0;
    for (x, width, height, color) in [
        (
            rect.left() + 120.0,
            240.0,
            110.0,
            Color32::from_rgb(44, 124, 76),
        ),
        (
            rect.right() - 360.0,
            300.0,
            132.0,
            Color32::from_rgb(42, 142, 84),
        ),
    ] {
        let points = vec![
            Pos2::new(x, ground_top),
            Pos2::new(x + width * 0.5, ground_top - height),
            Pos2::new(x + width, ground_top),
        ];
        painter.add(egui::Shape::convex_polygon(
            points,
            color,
            Stroke::new(2.0, Color32::from_rgb(24, 82, 54)),
        ));
    }
}

fn paint_brick_tile(painter: &egui::Painter, top_left: Pos2, color: Color32) {
    let rect = Rect::from_min_size(top_left, Vec2::new(32.0, 32.0));
    painter.rect_filled(rect, 0.0, color);
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(2.0, PANEL_ALT),
        egui::StrokeKind::Inside,
    );
    painter.line_segment(
        [
            Pos2::new(rect.left(), rect.center().y),
            Pos2::new(rect.right(), rect.center().y),
        ],
        Stroke::new(2.0, PANEL_ALT),
    );
    painter.line_segment(
        [
            Pos2::new(rect.center().x, rect.top()),
            Pos2::new(rect.center().x, rect.center().y),
        ],
        Stroke::new(2.0, PANEL_ALT),
    );
}

fn paint_question_block(painter: &egui::Painter, top_left: Pos2) {
    let rect = Rect::from_min_size(top_left, Vec2::new(32.0, 32.0));
    painter.rect_filled(rect, 0.0, QUESTION);
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(2.0, PANEL_ALT),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "?",
        FontId::new(22.0, FontFamily::Monospace),
        PANEL_ALT,
    );
}

fn paint_pipe(painter: &egui::Painter, top_left: Pos2) {
    let rim = Rect::from_min_size(top_left, Vec2::new(88.0, 22.0));
    let body = Rect::from_min_size(
        Pos2::new(top_left.x + 10.0, top_left.y + 18.0),
        Vec2::new(68.0, 78.0),
    );
    painter.rect_filled(rim, 0.0, PIPE);
    painter.rect_filled(body, 0.0, PIPE);
    painter.rect_stroke(
        rim,
        0.0,
        Stroke::new(2.0, PANEL_ALT),
        egui::StrokeKind::Inside,
    );
    painter.rect_stroke(
        body,
        0.0,
        Stroke::new(2.0, PANEL_ALT),
        egui::StrokeKind::Inside,
    );
}

fn paint_brick_logo(painter: &egui::Painter, rect: Rect) {
    let layout = compute_logo_layout(rect);
    let mut baseline_y = layout.start_y;

    for (line_index, line) in LOGO_LINES.iter().enumerate() {
        let width = line_width(line, layout.tile, layout.gap);
        let start_x = rect.center().x - width / 2.0;
        paint_logo_line(painter, line, start_x, baseline_y, layout.tile, layout.gap);
        baseline_y += block_height(layout.tile, layout.gap);
        if line_index + 1 < LOGO_LINES.len() {
            baseline_y += layout.line_gap;
        }
    }
}

fn paint_logo_line(
    painter: &egui::Painter,
    line: &str,
    start_x: f32,
    start_y: f32,
    tile: f32,
    gap: f32,
) {
    let mut cursor_x = start_x;
    for (letter_index, letter) in line.chars().enumerate() {
        let pattern = letter_pattern(letter);
        for (row_index, row) in pattern.iter().enumerate() {
            for (column_index, cell) in row.chars().enumerate() {
                if cell != '#' {
                    continue;
                }
                let top_left = Pos2::new(
                    cursor_x + column_index as f32 * (tile + gap),
                    start_y + row_index as f32 * (tile + gap),
                );
                paint_banner_brick(painter, top_left, tile);
            }
        }
        cursor_x += pattern[0].len() as f32 * (tile + gap);
        if letter_index + 1 < line.chars().count() {
            cursor_x += tile + gap;
        }
    }
}

fn paint_banner_brick(painter: &egui::Painter, top_left: Pos2, tile: f32) {
    let shadow = Rect::from_min_size(
        Pos2::new(top_left.x + 3.0, top_left.y + 3.0),
        Vec2::new(tile, tile),
    );
    painter.rect_filled(shadow, 0.0, Color32::from_rgba_premultiplied(0, 0, 0, 40));

    let rect = Rect::from_min_size(top_left, Vec2::new(tile, tile));
    painter.rect_filled(rect, 0.0, BRICK);
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.5, PANEL_ALT),
        egui::StrokeKind::Inside,
    );
    painter.line_segment(
        [
            Pos2::new(rect.left(), rect.center().y),
            Pos2::new(rect.right(), rect.center().y),
        ],
        Stroke::new(1.0, PANEL_ALT),
    );
}

fn compute_logo_layout(rect: Rect) -> LogoLayout {
    let max_width = (rect.width() - 72.0).max(340.0);
    let mut tile = 18.0;
    let mut gap = 5.0;

    for candidate in (9..=20).rev() {
        let candidate_tile = candidate as f32;
        let candidate_gap = (candidate_tile * 0.28).round().max(3.0);
        if line_width(LOGO_LINES[0], candidate_tile, candidate_gap) <= max_width {
            tile = candidate_tile;
            gap = candidate_gap;
            break;
        }
    }

    let line_gap = tile * 1.45;
    let total_height = block_height(tile, gap) * LOGO_LINES.len() as f32 + line_gap;

    LogoLayout {
        tile,
        gap,
        line_gap,
        start_y: rect.top() + 54.0,
        total_height,
    }
}

fn line_width(line: &str, tile: f32, gap: f32) -> f32 {
    let columns = line
        .chars()
        .enumerate()
        .map(|(index, letter)| {
            letter_pattern(letter)[0].len() + usize::from(index + 1 < line.len())
        })
        .sum::<usize>();
    columns as f32 * (tile + gap) - gap
}

fn block_height(tile: f32, gap: f32) -> f32 {
    5.0 * (tile + gap) - gap
}

fn letter_pattern(letter: char) -> [&'static str; 5] {
    match letter {
        'C' => [" ### ", "#    ", "#    ", "#    ", " ### "],
        'O' => [" ### ", "#   #", "#   #", "#   #", " ### "],
        'L' => ["#    ", "#    ", "#    ", "#    ", "#####"],
        'Z' => ["#####", "   # ", "  #  ", " #   ", "#####"],
        'H' => ["#   #", "#   #", "#####", "#   #", "#   #"],
        'U' => ["#   #", "#   #", "#   #", "#   #", " ### "],
        'D' => ["#### ", "#   #", "#   #", "#   #", "#### "],
        'E' => ["#####", "#    ", "#### ", "#    ", "#####"],
        _ => ["#####", "#####", "#####", "#####", "#####"],
    }
}

struct LogoLayout {
    tile: f32,
    gap: f32,
    line_gap: f32,
    start_y: f32,
    total_height: f32,
}
