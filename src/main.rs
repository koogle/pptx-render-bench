use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;

use pptx_render::RenderOptions;

#[derive(Parser, Debug)]
#[command(name = "pptx-render", about = "Render PPTX slides as PNG images")]
struct Args {
    /// Path to the input .pptx file
    input: PathBuf,

    /// Output directory for rendered PNG files
    #[arg(short, long, default_value = "output")]
    output: PathBuf,

    /// Render DPI (dots per inch)
    #[arg(long, default_value_t = 150.0)]
    dpi: f64,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let opts = RenderOptions { dpi: args.dpi };

    eprintln!(
        "Rendering {} at {} DPI...",
        args.input.display(),
        opts.dpi
    );

    let paths = pptx_render::render_pptx_to_files(&args.input, &args.output, &opts)
        .with_context(|| format!("Failed to render {}", args.input.display()))?;

    for p in &paths {
        println!("{}", p.display());
    }

    eprintln!("Rendered {} slide(s)", paths.len());
    Ok(())
}
