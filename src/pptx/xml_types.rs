/// EMU (English Metric Units) — 914400 EMU = 1 inch.
pub const EMU_PER_INCH: f64 = 914400.0;

/// Converts EMU to pixels at the given DPI.
pub fn emu_to_px(emu: i64, dpi: f64) -> f64 {
    (emu as f64 / EMU_PER_INCH) * dpi
}

/// OOXML half-points to points (font sizes are in half-points, i.e. 100ths of a point).
/// Actually, font sizes in DrawingML are in hundredths of a point.
pub fn hundredths_pt_to_pt(val: i32) -> f64 {
    val as f64 / 100.0
}

/// Position and extent of a shape on the slide.
#[derive(Debug, Clone, Default)]
pub struct Transform {
    pub off_x: i64, // EMU
    pub off_y: i64, // EMU
    pub ext_cx: i64, // EMU
    pub ext_cy: i64, // EMU
}

/// A color in RRGGBB hex or a theme reference (simplified).
#[derive(Debug, Clone)]
pub enum ColorSpec {
    SrgbClr(u8, u8, u8),
    /// A theme color name — for now we map a few common ones to defaults.
    SchemeClr(String),
    None,
}

impl Default for ColorSpec {
    fn default() -> Self {
        ColorSpec::None
    }
}

impl ColorSpec {
    pub fn to_rgba(&self) -> (u8, u8, u8, u8) {
        match self {
            ColorSpec::SrgbClr(r, g, b) => (*r, *g, *b, 255),
            ColorSpec::SchemeClr(name) => {
                // Very rough defaults for common theme colors
                match name.as_str() {
                    "tx1" | "dk1" => (0, 0, 0, 255),
                    "bg1" | "lt1" => (255, 255, 255, 255),
                    "accent1" => (68, 114, 196, 255),
                    "accent2" => (237, 125, 49, 255),
                    "accent3" => (165, 165, 165, 255),
                    "accent4" => (255, 192, 0, 255),
                    "accent5" => (91, 155, 213, 255),
                    "accent6" => (112, 173, 71, 255),
                    _ => (0, 0, 0, 255),
                }
            }
            ColorSpec::None => (0, 0, 0, 0),
        }
    }
}

/// Fill style for a shape.
#[derive(Debug, Clone, Default)]
pub enum FillStyle {
    Solid(ColorSpec),
    #[default]
    NoFill,
}

/// Outline/border of a shape.
#[derive(Debug, Clone)]
pub struct OutlineStyle {
    pub width_emu: i64,
    pub color: ColorSpec,
}

impl Default for OutlineStyle {
    fn default() -> Self {
        OutlineStyle {
            width_emu: 0,
            color: ColorSpec::None,
        }
    }
}

/// A text run — a contiguous span of text with uniform formatting.
#[derive(Debug, Clone)]
pub struct TextRun {
    pub text: String,
    pub font_size_hundredths_pt: i32, // 0 means inherit/default
    pub bold: bool,
    pub italic: bool,
    pub color: ColorSpec,
    pub font_family: Option<String>,
}

impl Default for TextRun {
    fn default() -> Self {
        TextRun {
            text: String::new(),
            font_size_hundredths_pt: 1800, // default 18pt
            bold: false,
            italic: false,
            color: ColorSpec::SchemeClr("tx1".into()),
            font_family: None,
        }
    }
}

/// Text alignment.
#[derive(Debug, Clone, Copy, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// A paragraph of text (one or more runs).
#[derive(Debug, Clone, Default)]
pub struct TextParagraph {
    pub runs: Vec<TextRun>,
    pub align: TextAlign,
}

/// A shape on a slide.
#[derive(Debug, Clone)]
pub enum ShapeKind {
    Rectangle,
    RoundedRectangle,
    Ellipse,
    Line,
    /// Preset geometry name from OOXML (e.g. "rightArrow").
    Preset(String),
    /// An image, with the relationship ID pointing into the media store.
    Picture { rel_id: String },
}

#[derive(Debug, Clone)]
pub struct Shape {
    pub kind: ShapeKind,
    pub transform: Transform,
    pub fill: FillStyle,
    pub outline: OutlineStyle,
    pub text: Vec<TextParagraph>,
}
