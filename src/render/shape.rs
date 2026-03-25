use anyhow::Result;
use std::collections::HashMap;
use tiny_skia::*;

use super::text::FontSystem;
use crate::pptx::xml_types::{
    emu_to_px, hundredths_pt_to_pt, FillStyle, Shape, ShapeKind, TextAlign,
};

// Re-alias to avoid collision with tiny_skia::Transform
type SkTransform = tiny_skia::Transform;

pub fn render_shape(
    pixmap: &mut Pixmap,
    shape: &Shape,
    dpi: f64,
    images: &HashMap<String, Vec<u8>>,
    font_system: &FontSystem,
) -> Result<()> {
    match &shape.kind {
        ShapeKind::Rectangle | ShapeKind::Preset(_) => {
            draw_rect(pixmap, shape, dpi)?;
            draw_text(pixmap, shape, dpi, font_system)?;
        }
        ShapeKind::RoundedRectangle => {
            draw_rounded_rect(pixmap, shape, dpi)?;
            draw_text(pixmap, shape, dpi, font_system)?;
        }
        ShapeKind::Ellipse => {
            draw_ellipse(pixmap, shape, dpi)?;
            draw_text(pixmap, shape, dpi, font_system)?;
        }
        ShapeKind::Line => {
            draw_line(pixmap, shape, dpi)?;
        }
        ShapeKind::Picture { rel_id } => {
            draw_image(pixmap, shape, dpi, rel_id, images)?;
        }
    }
    Ok(())
}

fn draw_rect(pixmap: &mut Pixmap, shape: &Shape, dpi: f64) -> Result<()> {
    let x = emu_to_px(shape.transform.off_x, dpi) as f32;
    let y = emu_to_px(shape.transform.off_y, dpi) as f32;
    let w = emu_to_px(shape.transform.ext_cx, dpi) as f32;
    let h = emu_to_px(shape.transform.ext_cy, dpi) as f32;

    let rect = match Rect::from_xywh(x, y, w, h) {
        Some(r) => r,
        None => return Ok(()),
    };

    // Fill
    if let FillStyle::Solid(ref color) = shape.fill {
        let (r, g, b, a) = color.to_rgba();
        let mut paint = Paint::default();
        paint.set_color_rgba8(r, g, b, a);
        paint.anti_alias = true;
        pixmap.fill_rect(rect, &paint, SkTransform::identity(), None);
    }

    // Outline
    if shape.outline.width_emu > 0 {
        if let Some(path) = rect_to_path(rect) {
            let (r, g, b, a) = shape.outline.color.to_rgba();
            if a > 0 {
                let mut paint = Paint::default();
                paint.set_color_rgba8(r, g, b, a);
                paint.anti_alias = true;
                let width = emu_to_px(shape.outline.width_emu, dpi) as f32;
                let mut stroke = Stroke::default();
                stroke.width = width;
                pixmap.stroke_path(&path, &paint, &stroke, SkTransform::identity(), None);
            }
        }
    }

    Ok(())
}

fn draw_rounded_rect(pixmap: &mut Pixmap, shape: &Shape, dpi: f64) -> Result<()> {
    let x = emu_to_px(shape.transform.off_x, dpi) as f32;
    let y = emu_to_px(shape.transform.off_y, dpi) as f32;
    let w = emu_to_px(shape.transform.ext_cx, dpi) as f32;
    let h = emu_to_px(shape.transform.ext_cy, dpi) as f32;
    let radius = w.min(h) * 0.1; // 10% corner radius as default

    let path = match rounded_rect_path(x, y, w, h, radius) {
        Some(p) => p,
        None => return Ok(()),
    };

    if let FillStyle::Solid(ref color) = shape.fill {
        let (r, g, b, a) = color.to_rgba();
        let mut paint = Paint::default();
        paint.set_color_rgba8(r, g, b, a);
        paint.anti_alias = true;
        pixmap.fill_path(&path, &paint, FillRule::Winding, SkTransform::identity(), None);
    }

    if shape.outline.width_emu > 0 {
        let (r, g, b, a) = shape.outline.color.to_rgba();
        if a > 0 {
            let mut paint = Paint::default();
            paint.set_color_rgba8(r, g, b, a);
            paint.anti_alias = true;
            let width = emu_to_px(shape.outline.width_emu, dpi) as f32;
            let mut stroke = Stroke::default();
            stroke.width = width;
            pixmap.stroke_path(&path, &paint, &stroke, SkTransform::identity(), None);
        }
    }

    Ok(())
}

fn draw_ellipse(pixmap: &mut Pixmap, shape: &Shape, dpi: f64) -> Result<()> {
    let x = emu_to_px(shape.transform.off_x, dpi) as f32;
    let y = emu_to_px(shape.transform.off_y, dpi) as f32;
    let w = emu_to_px(shape.transform.ext_cx, dpi) as f32;
    let h = emu_to_px(shape.transform.ext_cy, dpi) as f32;

    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    let rx = w / 2.0;
    let ry = h / 2.0;

    let path = match ellipse_path(cx, cy, rx, ry) {
        Some(p) => p,
        None => return Ok(()),
    };

    if let FillStyle::Solid(ref color) = shape.fill {
        let (r, g, b, a) = color.to_rgba();
        let mut paint = Paint::default();
        paint.set_color_rgba8(r, g, b, a);
        paint.anti_alias = true;
        pixmap.fill_path(&path, &paint, FillRule::Winding, SkTransform::identity(), None);
    }

    if shape.outline.width_emu > 0 {
        let (r, g, b, a) = shape.outline.color.to_rgba();
        if a > 0 {
            let mut paint = Paint::default();
            paint.set_color_rgba8(r, g, b, a);
            paint.anti_alias = true;
            let width = emu_to_px(shape.outline.width_emu, dpi) as f32;
            let mut stroke = Stroke::default();
            stroke.width = width;
            pixmap.stroke_path(&path, &paint, &stroke, SkTransform::identity(), None);
        }
    }

    Ok(())
}

fn draw_line(pixmap: &mut Pixmap, shape: &Shape, dpi: f64) -> Result<()> {
    let x1 = emu_to_px(shape.transform.off_x, dpi) as f32;
    let y1 = emu_to_px(shape.transform.off_y, dpi) as f32;
    let x2 = x1 + emu_to_px(shape.transform.ext_cx, dpi) as f32;
    let y2 = y1 + emu_to_px(shape.transform.ext_cy, dpi) as f32;

    let mut pb = PathBuilder::new();
    pb.move_to(x1, y1);
    pb.line_to(x2, y2);
    let path = match pb.finish() {
        Some(p) => p,
        None => return Ok(()),
    };

    let (r, g, b, a) = shape.outline.color.to_rgba();
    if a == 0 {
        return Ok(());
    }

    let mut paint = Paint::default();
    paint.set_color_rgba8(r, g, b, a);
    paint.anti_alias = true;
    let width = if shape.outline.width_emu > 0 {
        emu_to_px(shape.outline.width_emu, dpi) as f32
    } else {
        1.0
    };
    let mut stroke = Stroke::default();
    stroke.width = width;
    pixmap.stroke_path(&path, &paint, &stroke, SkTransform::identity(), None);

    Ok(())
}

fn draw_image(
    pixmap: &mut Pixmap,
    shape: &Shape,
    dpi: f64,
    rel_path: &str,
    images: &HashMap<String, Vec<u8>>,
) -> Result<()> {
    let data = match images.get(rel_path) {
        Some(d) => d,
        None => return Ok(()),
    };

    let img = match image::load_from_memory(data) {
        Ok(img) => img.to_rgba8(),
        Err(_) => return Ok(()),
    };

    let x = emu_to_px(shape.transform.off_x, dpi).round() as i32;
    let y = emu_to_px(shape.transform.off_y, dpi).round() as i32;
    let target_w = emu_to_px(shape.transform.ext_cx, dpi).round() as u32;
    let target_h = emu_to_px(shape.transform.ext_cy, dpi).round() as u32;

    if target_w == 0 || target_h == 0 {
        return Ok(());
    }

    // Resize image to fit
    let resized = image::imageops::resize(&img, target_w, target_h, image::imageops::FilterType::Lanczos3);

    let src_pixmap = PixmapRef::from_bytes(resized.as_raw(), target_w, target_h);
    if let Some(src) = src_pixmap {
        let mut paint = PixmapPaint::default();
        paint.opacity = 1.0;
        pixmap.draw_pixmap(
            x,
            y,
            src,
            &paint,
            SkTransform::identity(),
            None,
        );
    }

    Ok(())
}

fn draw_text(
    pixmap: &mut Pixmap,
    shape: &Shape,
    dpi: f64,
    font_system: &FontSystem,
) -> Result<()> {
    if shape.text.is_empty() {
        return Ok(());
    }

    let x = emu_to_px(shape.transform.off_x, dpi) as f32;
    let y = emu_to_px(shape.transform.off_y, dpi) as f32;
    let w = emu_to_px(shape.transform.ext_cx, dpi) as f32;
    let _h = emu_to_px(shape.transform.ext_cy, dpi) as f32;

    // Simple text layout: each paragraph on a new line
    let padding = 4.0 * (dpi / 96.0) as f32;
    let mut cursor_y = y + padding;

    for para in &shape.text {
        let mut cursor_x = x + padding;
        let line_height = para
            .runs
            .iter()
            .map(|r| hundredths_pt_to_pt(r.font_size_hundredths_pt) as f32 * (dpi as f32 / 72.0))
            .fold(0.0f32, f32::max)
            .max(12.0);

        // Calculate total text width for alignment
        let total_width: f32 = para
            .runs
            .iter()
            .map(|r| {
                let font_size_px = hundredths_pt_to_pt(r.font_size_hundredths_pt) as f32 * (dpi as f32 / 72.0);
                font_system.measure_text(&r.text, font_size_px, r.bold, r.italic, r.font_family.as_deref())
            })
            .sum();

        let available = w - 2.0 * padding;
        match para.align {
            TextAlign::Center => cursor_x = x + (available - total_width) / 2.0 + padding,
            TextAlign::Right => cursor_x = x + available - total_width + padding,
            TextAlign::Left => {}
        }

        cursor_y += line_height;

        for run in &para.runs {
            if run.text.is_empty() {
                continue;
            }

            let font_size_px =
                hundredths_pt_to_pt(run.font_size_hundredths_pt) as f32 * (dpi as f32 / 72.0);
            let (r, g, b, a) = run.color.to_rgba();

            let advance = font_system.draw_text(
                pixmap,
                &run.text,
                cursor_x,
                cursor_y,
                font_size_px,
                (r, g, b, a),
                run.bold,
                run.italic,
                run.font_family.as_deref(),
            );

            cursor_x += advance;
        }
    }

    Ok(())
}

fn rect_to_path(rect: Rect) -> Option<Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(rect.left(), rect.top());
    pb.line_to(rect.right(), rect.top());
    pb.line_to(rect.right(), rect.bottom());
    pb.line_to(rect.left(), rect.bottom());
    pb.close();
    pb.finish()
}

fn rounded_rect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    let r = r.min(w / 2.0).min(h / 2.0);
    let mut pb = PathBuilder::new();
    // top edge
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    // top-right corner
    pb.quad_to(x + w, y, x + w, y + r);
    // right edge
    pb.line_to(x + w, y + h - r);
    // bottom-right corner
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    // bottom edge
    pb.line_to(x + r, y + h);
    // bottom-left corner
    pb.quad_to(x, y + h, x, y + h - r);
    // left edge
    pb.line_to(x, y + r);
    // top-left corner
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish()
}

fn ellipse_path(cx: f32, cy: f32, rx: f32, ry: f32) -> Option<Path> {
    // Approximate ellipse with 4 cubic bezier curves
    let k: f32 = 0.5522848;
    let ox = rx * k;
    let oy = ry * k;

    let mut pb = PathBuilder::new();
    pb.move_to(cx - rx, cy);
    pb.cubic_to(cx - rx, cy - oy, cx - ox, cy - ry, cx, cy - ry);
    pb.cubic_to(cx + ox, cy - ry, cx + rx, cy - oy, cx + rx, cy);
    pb.cubic_to(cx + rx, cy + oy, cx + ox, cy + ry, cx, cy + ry);
    pb.cubic_to(cx - ox, cy + ry, cx - rx, cy + oy, cx - rx, cy);
    pb.close();
    pb.finish()
}
