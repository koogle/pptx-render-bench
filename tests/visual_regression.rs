use std::path::PathBuf;

use pptx_render::RenderOptions;

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixtures_dir() -> PathBuf {
    project_root().join("tests").join("fixtures")
}

fn reference_dir() -> PathBuf {
    project_root().join("tests").join("reference")
}

/// Compare two images and return the fraction of pixels that differ
/// beyond a per-channel threshold.
fn pixel_diff_ratio(a: &image::RgbaImage, b: &image::RgbaImage, threshold: u8) -> f64 {
    // If dimensions differ, resize b to match a
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
            // Dimmed version of original
            diff.put_pixel(
                x,
                y,
                image::Rgba([pa[0] / 2, pa[1] / 2, pa[2] / 2, 255]),
            );
        }
    }
    diff
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
        // If no reference, just test that rendering doesn't crash
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

        // Convert pixmap to image::RgbaImage
        let rendered = pixmap_to_rgba(pm);

        let diff_ratio = pixel_diff_ratio(&rendered, &ref_img, threshold);
        eprintln!(
            "  slide_{slide_num}: {:.2}% pixels differ (threshold: per-channel={threshold}, max ratio={:.1}%)",
            diff_ratio * 100.0,
            max_diff_ratio * 100.0
        );

        if diff_ratio > max_diff_ratio {
            // Save rendered and diff for debugging
            let debug_dir = project_root().join("tests").join("debug");
            std::fs::create_dir_all(&debug_dir).ok();
            let rendered_path = debug_dir.join(format!("{fixture_name}_slide_{slide_num}_rendered.png"));
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

fn pixmap_to_rgba(pm: &tiny_skia::Pixmap) -> image::RgbaImage {
    let w = pm.width();
    let h = pm.height();
    // tiny_skia stores premultiplied RGBA, we need to unpremultiply
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
    // We allow this to pass even without reference images for now
    assert!(pass, "Visual regression failed for basic_shapes");
}

#[test]
fn test_dpi_scaling() {
    let pptx_path = fixtures_dir().join("basic_shapes.pptx");

    let opts_72 = RenderOptions { dpi: 72.0 };
    let opts_300 = RenderOptions { dpi: 300.0 };

    let pm_72 = pptx_render::render_pptx(&pptx_path, &opts_72).unwrap();
    let pm_300 = pptx_render::render_pptx(&pptx_path, &opts_300).unwrap();

    // 300 DPI should be ~4.17x larger than 72 DPI in each dimension
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
