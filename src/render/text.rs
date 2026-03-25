use tiny_skia::{Paint, Pixmap, Transform};

/// Wrapper around fontdb + ab_glyph for text rendering.
pub struct FontSystem {
    db: fontdb::Database,
}

impl FontSystem {
    pub fn new() -> Self {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        // Also try common Linux font directories
        db.load_fonts_dir("/usr/share/fonts");
        db.load_fonts_dir("/usr/local/share/fonts");
        FontSystem { db }
    }

    /// Measure text width in pixels.
    pub fn measure_text(
        &self,
        text: &str,
        font_size_px: f32,
        bold: bool,
        italic: bool,
        family: Option<&str>,
    ) -> f32 {
        let face_id = self.find_face(bold, italic, family);
        let face_id = match face_id {
            Some(id) => id,
            None => return text.len() as f32 * font_size_px * 0.5, // rough fallback
        };

        self.db
            .with_face_data(face_id, |data, face_index| {
                let face = match ttf_parser::Face::parse(data, face_index) {
                    Ok(f) => f,
                    Err(_) => return text.len() as f32 * font_size_px * 0.5,
                };
                let scale = font_size_px / face.units_per_em() as f32;
                let mut width = 0.0f32;
                for ch in text.chars() {
                    if let Some(gid) = face.glyph_index(ch) {
                        let adv = face.glyph_hor_advance(gid).unwrap_or(0);
                        width += adv as f32 * scale;
                    } else {
                        width += font_size_px * 0.5;
                    }
                }
                width
            })
            .unwrap_or(text.len() as f32 * font_size_px * 0.5)
    }

    /// Draw text onto a pixmap and return the horizontal advance.
    pub fn draw_text(
        &self,
        pixmap: &mut Pixmap,
        text: &str,
        x: f32,
        y: f32,
        font_size_px: f32,
        color: (u8, u8, u8, u8),
        bold: bool,
        italic: bool,
        family: Option<&str>,
    ) -> f32 {
        let face_id = self.find_face(bold, italic, family);
        let face_id = match face_id {
            Some(id) => id,
            None => return self.measure_text(text, font_size_px, bold, italic, family),
        };

        self.db
            .with_face_data(face_id, |data, face_index| {
                let face = match ttf_parser::Face::parse(data, face_index) {
                    Ok(f) => f,
                    Err(_) => return self.measure_text(text, font_size_px, bold, italic, family),
                };

                let scale = font_size_px / face.units_per_em() as f32;
                let mut cursor_x = x;

                for ch in text.chars() {
                    let gid = match face.glyph_index(ch) {
                        Some(g) => g,
                        None => {
                            cursor_x += font_size_px * 0.5;
                            continue;
                        }
                    };

                    // Rasterize the glyph using tiny-skia path outline
                    if let Some(advance) = face.glyph_hor_advance(gid) {
                        // Build the glyph outline
                        let mut builder = GlyphPathBuilder::new(scale, cursor_x, y);
                        if face.outline_glyph(gid, &mut builder).is_some() {
                            if let Some(path) = builder.finish() {
                                let mut paint = Paint::default();
                                paint.set_color_rgba8(color.0, color.1, color.2, color.3);
                                paint.anti_alias = true;
                                pixmap.fill_path(
                                    &path,
                                    &paint,
                                    tiny_skia::FillRule::Winding,
                                    Transform::identity(),
                                    None,
                                );
                            }
                        }
                        cursor_x += advance as f32 * scale;
                    }
                }

                cursor_x - x
            })
            .unwrap_or_else(|| self.measure_text(text, font_size_px, bold, italic, family))
    }

    fn find_face(
        &self,
        bold: bool,
        italic: bool,
        family: Option<&str>,
    ) -> Option<fontdb::ID> {
        let weight = if bold {
            fontdb::Weight::BOLD
        } else {
            fontdb::Weight::NORMAL
        };
        let style = if italic {
            fontdb::Style::Italic
        } else {
            fontdb::Style::Normal
        };

        // Try requested family first
        if let Some(fam) = family {
            let query = fontdb::Query {
                families: &[fontdb::Family::Name(fam)],
                weight,
                style,
                ..fontdb::Query::default()
            };
            if let Some(id) = self.db.query(&query) {
                return Some(id);
            }
        }

        // Fallback chain
        let families = [
            fontdb::Family::Name("Calibri"),
            fontdb::Family::Name("Arial"),
            fontdb::Family::Name("Liberation Sans"),
            fontdb::Family::Name("DejaVu Sans"),
            fontdb::Family::SansSerif,
        ];

        let query = fontdb::Query {
            families: &families,
            weight,
            style,
            ..fontdb::Query::default()
        };
        self.db.query(&query)
    }
}

/// Builds a tiny-skia Path from ttf_parser glyph outlines.
struct GlyphPathBuilder {
    pb: tiny_skia::PathBuilder,
    scale: f32,
    offset_x: f32,
    baseline_y: f32,
}

impl GlyphPathBuilder {
    fn new(scale: f32, offset_x: f32, baseline_y: f32) -> Self {
        GlyphPathBuilder {
            pb: tiny_skia::PathBuilder::new(),
            scale,
            offset_x,
            baseline_y,
        }
    }

    fn finish(self) -> Option<tiny_skia::Path> {
        self.pb.finish()
    }

    fn tx(&self, x: f32) -> f32 {
        self.offset_x + x * self.scale
    }

    fn ty(&self, y: f32) -> f32 {
        // Font coordinates have Y up, screen has Y down
        self.baseline_y - y * self.scale
    }
}

impl ttf_parser::OutlineBuilder for GlyphPathBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.pb.move_to(self.tx(x), self.ty(y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.pb.line_to(self.tx(x), self.ty(y));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.pb
            .quad_to(self.tx(x1), self.ty(y1), self.tx(x), self.ty(y));
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.pb.cubic_to(
            self.tx(x1),
            self.ty(y1),
            self.tx(x2),
            self.ty(y2),
            self.tx(x),
            self.ty(y),
        );
    }

    fn close(&mut self) {
        self.pb.close();
    }
}
