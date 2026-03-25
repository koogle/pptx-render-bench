pub mod pptx;
pub mod render;

use anyhow::Result;
use std::path::Path;

pub use render::RenderOptions;

/// Render all slides from a PPTX file to PNG images.
///
/// Returns a Vec of RGBA pixel buffers (one per slide) as tiny_skia::Pixmap.
pub fn render_pptx(
    path: &Path,
    opts: &RenderOptions,
) -> Result<Vec<tiny_skia::Pixmap>> {
    let pres = pptx::Presentation::open(path)?;
    render::render_presentation(&pres, opts)
}

/// Render all slides from a PPTX file and save them as PNGs.
///
/// Files are written as `{output_dir}/slide_{n}.png` (1-indexed).
pub fn render_pptx_to_files(
    pptx_path: &Path,
    output_dir: &Path,
    opts: &RenderOptions,
) -> Result<Vec<std::path::PathBuf>> {
    std::fs::create_dir_all(output_dir)?;
    let pixmaps = render_pptx(pptx_path, opts)?;
    let mut paths = Vec::new();

    for (i, pm) in pixmaps.iter().enumerate() {
        let out_path = output_dir.join(format!("slide_{}.png", i + 1));
        pm.save_png(&out_path)?;
        paths.push(out_path);
    }

    Ok(paths)
}
