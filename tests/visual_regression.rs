use std::path::PathBuf;
use std::time::Instant;

use pptx_render::RenderOptions;

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixtures_dir() -> PathBuf {
    project_root().join("tests").join("fixtures")
}

fn public_fixtures_dir() -> PathBuf {
    fixtures_dir().join("public")
}

fn reference_dir() -> PathBuf {
    project_root().join("tests").join("reference")
}

fn output_dir() -> PathBuf {
    project_root().join("tests").join("output")
}

/// Compare two images and return the fraction of pixels that differ
/// beyond a per-channel threshold.
fn pixel_diff_ratio(a: &image::RgbaImage, b: &image::RgbaImage, threshold: u8) -> f64 {
    let (aw, ah) = a.dimensions();
    let (bw, bh) = b.dimensions();

    let b_resized;
    let b_ref = if (aw, ah) != (bw, bh) {
        b_resized = image::imageops::resize(b, aw, ah, image::imageops::FilterType::Lanczos3);
        &b_resized
    } else {
        b
    };

    let total = (aw as u64) * (ah as u64);
    if total == 0 {
        return 0.0;
    }

    let mut diff_count: u64 = 0;
    for (pa, pb) in a.pixels().zip(b_ref.pixels()) {
        let dr = (pa[0] as i16 - pb[0] as i16).unsigned_abs() as u8;
        let dg = (pa[1] as i16 - pb[1] as i16).unsigned_abs() as u8;
        let db = (pa[2] as i16 - pb[2] as i16).unsigned_abs() as u8;
        if dr > threshold || dg > threshold || db > threshold {
            diff_count += 1;
        }
    }

    diff_count as f64 / total as f64
}

/// Generate a diff image highlighting differing pixels in red.
fn generate_diff_image(
    a: &image::RgbaImage,
    b: &image::RgbaImage,
    threshold: u8,
) -> image::RgbaImage {
    let (aw, ah) = a.dimensions();
    let (bw, bh) = b.dimensions();

    let b_resized;
    let b_ref = if (aw, ah) != (bw, bh) {
        b_resized = image::imageops::resize(b, aw, ah, image::imageops::FilterType::Lanczos3);
        &b_resized
    } else {
        b
    };

    let mut diff = image::RgbaImage::new(aw, ah);
    for (x, y, pa) in a.enumerate_pixels() {
        let pb = b_ref.get_pixel(x, y);
        let dr = (pa[0] as i16 - pb[0] as i16).unsigned_abs() as u8;
        let dg = (pa[1] as i16 - pb[1] as i16).unsigned_abs() as u8;
        let db = (pa[2] as i16 - pb[2] as i16).unsigned_abs() as u8;
        if dr > threshold || dg > threshold || db > threshold {
            diff.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
        } else {
            diff.put_pixel(
                x,
                y,
                image::Rgba([pa[0] / 2, pa[1] / 2, pa[2] / 2, 255]),
            );
        }
    }
    diff
}

fn pixmap_to_rgba(pm: &tiny_skia::Pixmap) -> image::RgbaImage {
    let w = pm.width();
    let h = pm.height();
    let mut buf = Vec::with_capacity((w * h * 4) as usize);
    for pixel in pm.pixels() {
        let a = pixel.alpha();
        if a == 0 {
            buf.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            let r = ((pixel.red() as f32 / a as f32) * 255.0).round() as u8;
            let g = ((pixel.green() as f32 / a as f32) * 255.0).round() as u8;
            let b = ((pixel.blue() as f32 / a as f32) * 255.0).round() as u8;
            buf.extend_from_slice(&[r, g, b, a]);
        }
    }
    image::RgbaImage::from_raw(w, h, buf).expect("Failed to create RgbaImage")
}

/// Result of rendering a single PPTX fixture.
#[derive(Debug)]
#[allow(dead_code)]
struct RenderResult {
    name: String,
    slide_count: usize,
    duration_ms: u128,
    error: Option<String>,
    output_paths: Vec<PathBuf>,
}

/// Render a single PPTX file and save output PNGs.
fn render_fixture(pptx_path: &std::path::Path, dpi: f64, save_output: bool) -> RenderResult {
    let name = pptx_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let start = Instant::now();
    let opts = RenderOptions { dpi };
    let result = pptx_render::render_pptx(pptx_path, &opts);
    let duration_ms = start.elapsed().as_millis();

    match result {
        Ok(pixmaps) => {
            let mut output_paths = Vec::new();
            if save_output {
                // Derive output subdir from fixture path relative to fixtures dir
                let rel = pptx_path
                    .strip_prefix(fixtures_dir())
                    .unwrap_or(pptx_path);
                let out_subdir = output_dir().join(rel.with_extension(""));
                std::fs::create_dir_all(&out_subdir).ok();

                for (i, pm) in pixmaps.iter().enumerate() {
                    let out_path = out_subdir.join(format!("slide_{}.png", i + 1));
                    if let Err(e) = pm.save_png(&out_path) {
                        eprintln!("    Failed to save {}: {e}", out_path.display());
                    } else {
                        output_paths.push(out_path);
                    }
                }
            }

            RenderResult {
                name,
                slide_count: pixmaps.len(),
                duration_ms,
                error: None,
                output_paths,
            }
        }
        Err(e) => RenderResult {
            name,
            slide_count: 0,
            duration_ms,
            error: Some(format!("{e:#}")),
            output_paths: Vec::new(),
        },
    }
}

/// Core comparison logic. Returns true if the test passes.
fn compare_fixture(fixture_name: &str, dpi: f64, max_diff_ratio: f64, threshold: u8) -> bool {
    let pptx_path = fixtures_dir().join(format!("{fixture_name}.pptx"));
    if !pptx_path.exists() {
        eprintln!("Fixture not found: {}", pptx_path.display());
        return false;
    }

    let ref_dir = reference_dir().join(fixture_name);
    if !ref_dir.exists() {
        eprintln!(
            "Reference directory not found: {}. Run scripts/generate_reference.sh first.",
            ref_dir.display()
        );
        let opts = RenderOptions { dpi };
        let result = pptx_render::render_pptx(&pptx_path, &opts);
        assert!(result.is_ok(), "Rendering failed: {:?}", result.err());
        eprintln!("Rendering succeeded (no reference to compare against)");
        return true;
    }

    let opts = RenderOptions { dpi };
    let pixmaps = pptx_render::render_pptx(&pptx_path, &opts).expect("Rendering failed");

    let mut all_pass = true;
    for (i, pm) in pixmaps.iter().enumerate() {
        let slide_num = i + 1;
        let ref_path = ref_dir.join(format!("slide_{slide_num}.png"));
        if !ref_path.exists() {
            eprintln!("Reference image not found: {}", ref_path.display());
            continue;
        }

        let ref_img = image::open(&ref_path)
            .expect("Failed to open reference image")
            .to_rgba8();

        let rendered = pixmap_to_rgba(pm);
        let diff_ratio = pixel_diff_ratio(&rendered, &ref_img, threshold);
        eprintln!(
            "  slide_{slide_num}: {:.2}% pixels differ (threshold: per-channel={threshold}, max ratio={:.1}%)",
            diff_ratio * 100.0,
            max_diff_ratio * 100.0
        );

        if diff_ratio > max_diff_ratio {
            let debug_dir = project_root().join("tests").join("debug");
            std::fs::create_dir_all(&debug_dir).ok();
            let rendered_path =
                debug_dir.join(format!("{fixture_name}_slide_{slide_num}_rendered.png"));
            let diff_path = debug_dir.join(format!("{fixture_name}_slide_{slide_num}_diff.png"));
            rendered.save(&rendered_path).ok();
            let diff_img = generate_diff_image(&rendered, &ref_img, threshold);
            diff_img.save(&diff_path).ok();
            eprintln!("  FAIL: diff too large. See {}", debug_dir.display());
            all_pass = false;
        }
    }

    all_pass
}

/// Discover all .pptx files under a directory (recursively).
fn find_pptx_files(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if !dir.exists() {
        return files;
    }
    fn walk(dir: &std::path::Path, files: &mut Vec<PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, files);
                } else if path.extension().is_some_and(|e| e == "pptx") {
                    files.push(path);
                }
            }
        }
    }
    walk(dir, &mut files);
    files.sort();
    files
}

// ============================================================================
// Tests for our hand-crafted fixture
// ============================================================================

#[test]
fn test_basic_shapes_renders_without_crash() {
    let pptx_path = fixtures_dir().join("basic_shapes.pptx");
    let opts = RenderOptions { dpi: 150.0 };
    let result = pptx_render::render_pptx(&pptx_path, &opts);
    assert!(result.is_ok(), "Rendering failed: {:?}", result.err());
    let pixmaps = result.unwrap();
    assert_eq!(pixmaps.len(), 1, "Expected 1 slide");
    assert!(pixmaps[0].width() > 0);
    assert!(pixmaps[0].height() > 0);
}

#[test]
fn test_basic_shapes_visual_regression() {
    let pass = compare_fixture("basic_shapes", 150.0, 0.05, 30);
    assert!(pass, "Visual regression failed for basic_shapes");
}

#[test]
fn test_dpi_scaling() {
    let pptx_path = fixtures_dir().join("basic_shapes.pptx");

    let opts_72 = RenderOptions { dpi: 72.0 };
    let opts_300 = RenderOptions { dpi: 300.0 };

    let pm_72 = pptx_render::render_pptx(&pptx_path, &opts_72).unwrap();
    let pm_300 = pptx_render::render_pptx(&pptx_path, &opts_300).unwrap();

    let ratio_w = pm_300[0].width() as f64 / pm_72[0].width() as f64;
    let ratio_h = pm_300[0].height() as f64 / pm_72[0].height() as f64;

    assert!(
        (ratio_w - 300.0 / 72.0).abs() < 0.1,
        "Width ratio {ratio_w} doesn't match DPI ratio"
    );
    assert!(
        (ratio_h - 300.0 / 72.0).abs() < 0.1,
        "Height ratio {ratio_h} doesn't match DPI ratio"
    );
}

// ============================================================================
// Public fixture tests — render all downloaded PPTX files
// ============================================================================

/// Render every public fixture and report results.
/// This test ensures our renderer doesn't crash on real-world PPTX files.
/// Failures are collected and reported together so one bad file doesn't
/// prevent testing the rest.
#[test]
fn test_public_fixtures_render() {
    let public_dir = public_fixtures_dir();
    if !public_dir.exists() {
        eprintln!("Public fixtures not downloaded. Run: scripts/download_fixtures.sh");
        eprintln!("Skipping public fixture tests.");
        return;
    }

    let pptx_files = find_pptx_files(&public_dir);
    if pptx_files.is_empty() {
        eprintln!("No .pptx files found in {}", public_dir.display());
        return;
    }

    eprintln!("\n=== Public Fixture Render Test ({} files) ===\n", pptx_files.len());

    let dpi = 150.0;
    let mut results: Vec<RenderResult> = Vec::new();

    for pptx_path in &pptx_files {
        let rel_path = pptx_path
            .strip_prefix(&public_dir)
            .unwrap_or(pptx_path);
        eprint!("  {:<50} ", rel_path.display());

        let result = render_fixture(pptx_path, dpi, true);

        match &result.error {
            None => {
                eprintln!(
                    "OK  ({} slides, {}ms)",
                    result.slide_count, result.duration_ms
                );
            }
            Some(err) => {
                eprintln!("FAIL ({}ms): {}", result.duration_ms, err);
            }
        }

        results.push(result);
    }

    // Summary
    let total = results.len();
    let passed = results.iter().filter(|r| r.error.is_none()).count();
    let failed = total - passed;
    let total_slides: usize = results.iter().map(|r| r.slide_count).sum();
    let total_ms: u128 = results.iter().map(|r| r.duration_ms).sum();

    eprintln!("\n=== Summary ===");
    eprintln!("  Total files:  {total}");
    eprintln!("  Passed:       {passed}");
    eprintln!("  Failed:       {failed}");
    eprintln!("  Total slides: {total_slides}");
    eprintln!("  Total time:   {total_ms}ms");

    if failed > 0 {
        eprintln!("\nFailed files:");
        for r in &results {
            if let Some(err) = &r.error {
                eprintln!("  - {}: {err}", r.name);
            }
        }
    }

    // We consider the test passing if at least 80% of files render successfully.
    // This allows for some files that use features we haven't implemented yet.
    let pass_rate = passed as f64 / total as f64;
    assert!(
        pass_rate >= 0.80,
        "Only {passed}/{total} ({:.0}%) fixtures rendered successfully. Need at least 80%.",
        pass_rate * 100.0
    );
}

/// Render each public fixture at multiple DPI values to check consistency.
#[test]
fn test_public_fixtures_dpi_consistency() {
    let public_dir = public_fixtures_dir();
    if !public_dir.exists() {
        eprintln!("Public fixtures not downloaded. Skipping.");
        return;
    }

    let pptx_files = find_pptx_files(&public_dir);
    if pptx_files.is_empty() {
        return;
    }

    // Just test a subset to keep this fast
    let test_files: Vec<_> = pptx_files.iter().take(5).collect();

    eprintln!("\n=== DPI Consistency Test ({} files) ===\n", test_files.len());

    for pptx_path in test_files {
        let rel_path = pptx_path
            .strip_prefix(&public_dir)
            .unwrap_or(pptx_path);

        let opts_72 = RenderOptions { dpi: 72.0 };
        let opts_150 = RenderOptions { dpi: 150.0 };

        let pm_72 = match pptx_render::render_pptx(pptx_path, &opts_72) {
            Ok(p) if !p.is_empty() => p,
            _ => continue,
        };
        let pm_150 = match pptx_render::render_pptx(pptx_path, &opts_150) {
            Ok(p) if !p.is_empty() => p,
            _ => continue,
        };

        let ratio_w = pm_150[0].width() as f64 / pm_72[0].width() as f64;
        let expected = 150.0 / 72.0;

        eprintln!(
            "  {:<50} ratio={:.3} (expected {:.3})",
            rel_path.display(),
            ratio_w,
            expected
        );

        assert!(
            (ratio_w - expected).abs() < 0.15,
            "{}: DPI scaling ratio {ratio_w:.3} too far from expected {expected:.3}",
            rel_path.display()
        );
    }
}
