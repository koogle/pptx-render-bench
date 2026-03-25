pub mod slide;
pub mod xml_types;

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

/// Represents a parsed PPTX file.
pub struct Presentation {
    pub slide_width_emu: i64,
    pub slide_height_emu: i64,
    pub slides: Vec<slide::Slide>,
    pub images: HashMap<String, Vec<u8>>,
}

impl Presentation {
    pub fn open(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path)
            .with_context(|| format!("Failed to open {}", path.display()))?;
        let mut archive = zip::ZipArchive::new(file)?;

        // Parse presentation.xml for slide size
        let (width, height) = parse_presentation_size(&mut archive)?;

        // Discover slide files via relationships
        let slide_paths = discover_slides(&mut archive)?;

        // Parse each slide
        let mut slides = Vec::new();
        for slide_path in &slide_paths {
            let slide_rels = load_slide_rels(&mut archive, slide_path)?;
            let xml = read_archive_file(&mut archive, slide_path)?;
            let parsed = slide::Slide::parse(&xml, &slide_rels)?;
            slides.push(parsed);
        }

        // Extract embedded images
        let mut images = HashMap::new();
        let file_names: Vec<String> = archive.file_names().map(String::from).collect();
        for name in file_names {
            if name.starts_with("ppt/media/") {
                let data = read_archive_bytes(&mut archive, &name)?;
                images.insert(name, data);
            }
        }

        Ok(Presentation {
            slide_width_emu: width,
            slide_height_emu: height,
            slides,
            images,
        })
    }
}

fn read_archive_file(archive: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Result<String> {
    let mut file = archive.by_name(name).with_context(|| format!("Missing {name}"))?;
    let mut buf = String::new();
    file.read_to_string(&mut buf)?;
    Ok(buf)
}

fn read_archive_bytes(archive: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Result<Vec<u8>> {
    let mut file = archive.by_name(name)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    Ok(buf)
}

fn parse_presentation_size(archive: &mut zip::ZipArchive<std::fs::File>) -> Result<(i64, i64)> {
    let xml = read_archive_file(archive, "ppt/presentation.xml")?;
    let doc = quick_xml::Reader::from_str(&xml);
    let mut reader = doc;
    let mut buf = Vec::new();
    let mut width: i64 = 9144000; // default 10 inches
    let mut height: i64 = 6858000; // default 7.5 inches

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Empty(ref e)) | Ok(quick_xml::events::Event::Start(ref e)) => {
                let local_name = e.local_name();
                if local_name.as_ref() == b"sldSz" {
                    for attr in e.attributes().flatten() {
                        match attr.key.as_ref() {
                            b"cx" => {
                                width = std::str::from_utf8(&attr.value)?.parse()?;
                            }
                            b"cy" => {
                                height = std::str::from_utf8(&attr.value)?.parse()?;
                            }
                            _ => {}
                        }
                    }
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(e) => return Err(e.into()),
            _ => {}
        }
        buf.clear();
    }

    Ok((width, height))
}

fn discover_slides(archive: &mut zip::ZipArchive<std::fs::File>) -> Result<Vec<String>> {
    let rels_xml = read_archive_file(archive, "ppt/_rels/presentation.xml.rels")?;
    let mut reader = quick_xml::Reader::from_str(&rels_xml);
    let mut buf = Vec::new();
    let mut slides: Vec<(i32, String)> = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Empty(ref e)) | Ok(quick_xml::events::Event::Start(ref e)) => {
                if e.local_name().as_ref() == b"Relationship" {
                    let mut target = String::new();
                    let mut rel_type = String::new();
                    for attr in e.attributes().flatten() {
                        match attr.key.as_ref() {
                            b"Target" => target = std::str::from_utf8(&attr.value)?.to_string(),
                            b"Type" => rel_type = std::str::from_utf8(&attr.value)?.to_string(),
                            _ => {}
                        }
                    }
                    if rel_type.ends_with("/slide") {
                        // Extract slide number for sorting
                        let num: i32 = target
                            .trim_start_matches("slides/slide")
                            .trim_end_matches(".xml")
                            .parse()
                            .unwrap_or(0);
                        let full_path = if target.starts_with('/') {
                            target[1..].to_string()
                        } else {
                            format!("ppt/{target}")
                        };
                        slides.push((num, full_path));
                    }
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(e) => return Err(e.into()),
            _ => {}
        }
        buf.clear();
    }

    slides.sort_by_key(|(n, _)| *n);
    Ok(slides.into_iter().map(|(_, p)| p).collect())
}

fn load_slide_rels(
    archive: &mut zip::ZipArchive<std::fs::File>,
    slide_path: &str,
) -> Result<HashMap<String, String>> {
    // e.g. ppt/slides/slide1.xml -> ppt/slides/_rels/slide1.xml.rels
    let file_name = slide_path
        .rsplit('/')
        .next()
        .unwrap_or(slide_path);
    let dir = slide_path.trim_end_matches(file_name);
    let rels_path = format!("{dir}_rels/{file_name}.rels");

    let mut map = HashMap::new();
    let xml = match read_archive_file(archive, &rels_path) {
        Ok(x) => x,
        Err(_) => return Ok(map), // no rels file is fine
    };

    let mut reader = quick_xml::Reader::from_str(&xml);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Empty(ref e)) | Ok(quick_xml::events::Event::Start(ref e)) => {
                if e.local_name().as_ref() == b"Relationship" {
                    let mut id = String::new();
                    let mut target = String::new();
                    for attr in e.attributes().flatten() {
                        match attr.key.as_ref() {
                            b"Id" => id = std::str::from_utf8(&attr.value)?.to_string(),
                            b"Target" => target = std::str::from_utf8(&attr.value)?.to_string(),
                            _ => {}
                        }
                    }
                    if !id.is_empty() {
                        // Resolve relative paths
                        let full = if target.starts_with('/') {
                            target[1..].to_string()
                        } else if target.starts_with("../") {
                            // Relative to ppt/slides/ -> ppt/
                            let base = Path::new(dir);
                            let resolved = base.join(&target);
                            // Normalize the path
                            let s = resolved.to_string_lossy().to_string();
                            normalize_path(&s)
                        } else {
                            format!("{dir}{target}")
                        };
                        map.insert(id, full);
                    }
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(e) => return Err(e.into()),
            _ => {}
        }
        buf.clear();
    }

    Ok(map)
}

fn normalize_path(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            ".." => { parts.pop(); }
            "." | "" => {}
            _ => parts.push(part),
        }
    }
    parts.join("/")
}
