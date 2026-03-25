pub mod shape;
pub mod text;

use anyhow::Result;
use tiny_skia::{Color, Pixmap};

use crate::pptx::xml_types::*;
use crate::pptx::Presentation;

/// Render options.
#[derive(Debug, Clone)]
pub struct RenderOptions {
    pub dpi: f64,
}

impl Default for RenderOptions {
    fn default() -> Self {
        RenderOptions { dpi: 150.0 }
    }
}

/// Render all slides of a presentation to RGBA images.
pub fn render_presentation(
    pres: &Presentation,
    opts: &RenderOptions,
) -> Result<Vec<Pixmap>> {
    let mut results = Vec::new();
    let font_system = text::FontSystem::new();

    for slide in &pres.slides {
        let pixmap = render_slide(
            slide,
            pres.slide_width_emu,
            pres.slide_height_emu,
            &pres.images,
            opts,
            &font_system,
        )?;
        results.push(pixmap);
    }

    Ok(results)
}

fn render_slide(
    slide: &crate::pptx::slide::Slide,
    width_emu: i64,
    height_emu: i64,
    images: &std::collections::HashMap<String, Vec<u8>>,
    opts: &RenderOptions,
    font_system: &text::FontSystem,
) -> Result<Pixmap> {
    let w = emu_to_px(width_emu, opts.dpi).round() as u32;
    let h = emu_to_px(height_emu, opts.dpi).round() as u32;

    let mut pixmap = Pixmap::new(w, h)
        .ok_or_else(|| anyhow::anyhow!("Failed to create {w}x{h} pixmap"))?;

    // Fill background
    let bg = match &slide.background_color {
        Some(c) => {
            let (r, g, b, a) = c.to_rgba();
            Color::from_rgba8(r, g, b, a)
        }
        None => Color::WHITE,
    };
    pixmap.fill(bg);

    // Render each shape
    for s in &slide.shapes {
        shape::render_shape(&mut pixmap, s, opts.dpi, images, font_system)?;
    }

    Ok(pixmap)
}
