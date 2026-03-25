use anyhow::Result;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;

use super::xml_types::*;

#[derive(Debug)]
pub struct Slide {
    pub shapes: Vec<Shape>,
    pub background_color: Option<ColorSpec>,
}

impl Slide {
    pub fn parse(xml: &str, rels: &HashMap<String, String>) -> Result<Self> {
        let mut reader = Reader::from_str(xml);
        let mut buf = Vec::new();
        let mut shapes = Vec::new();
        let mut bg_color = None;

        // We use a simple state machine approach.
        // Track element depth to know when we're inside a shape tree.
        let mut depth: Vec<String> = Vec::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    let local = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                    depth.push(local.clone());

                    if local == "bg" && depth.len() <= 3 {
                        // Try to parse slide background
                        bg_color = parse_background(&mut reader, &mut buf)?;
                        depth.pop();
                    } else if local == "sp" {
                        // Shape
                        let shape = parse_shape(&mut reader, &mut buf, rels)?;
                        if let Some(s) = shape {
                            shapes.push(s);
                        }
                        depth.pop();
                    } else if local == "pic" {
                        let shape = parse_picture(&mut reader, &mut buf, rels)?;
                        if let Some(s) = shape {
                            shapes.push(s);
                        }
                        depth.pop();
                    }
                }
                Ok(Event::End(_)) => {
                    depth.pop();
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(e.into()),
                _ => {}
            }
            buf.clear();
        }

        Ok(Slide {
            shapes,
            background_color: bg_color,
        })
    }
}

fn parse_background(reader: &mut Reader<&[u8]>, buf: &mut Vec<u8>) -> Result<Option<ColorSpec>> {
    let mut bg_depth = 1i32;
    let mut color = None;

    loop {
        match reader.read_event_into(buf) {
            Ok(Event::Start(ref e)) => {
                bg_depth += 1;
                let local = e.local_name();
                if local.as_ref() == b"srgbClr" {
                    color = Some(parse_srgb_attr(e)?);
                } else if local.as_ref() == b"schemeClr" {
                    color = Some(parse_scheme_attr(e)?);
                }
            }
            Ok(Event::Empty(ref e)) => {
                let local = e.local_name();
                if local.as_ref() == b"srgbClr" {
                    color = Some(parse_srgb_attr(e)?);
                } else if local.as_ref() == b"schemeClr" {
                    color = Some(parse_scheme_attr(e)?);
                }
            }
            Ok(Event::End(_)) => {
                bg_depth -= 1;
                if bg_depth == 0 {
                    break;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(e.into()),
            _ => {}
        }
        buf.clear();
    }

    Ok(color)
}

fn parse_shape(
    reader: &mut Reader<&[u8]>,
    buf: &mut Vec<u8>,
    _rels: &HashMap<String, String>,
) -> Result<Option<Shape>> {
    let mut sp_depth = 1i32;
    let mut transform = Transform::default();
    let mut fill = FillStyle::NoFill;
    let mut outline = OutlineStyle::default();
    let mut text_paragraphs: Vec<TextParagraph> = Vec::new();
    let mut kind = ShapeKind::Rectangle;
    let mut has_fill = false;

    loop {
        match reader.read_event_into(buf) {
            Ok(Event::Start(ref e)) => {
                sp_depth += 1;
                let local = e.local_name();
                match local.as_ref() {
                    b"off" => {
                        parse_offset_attrs(e, &mut transform)?;
                    }
                    b"ext" => {
                        parse_extent_attrs(e, &mut transform)?;
                    }
                    b"prstGeom" => {
                        kind = parse_preset_geom(e)?;
                    }
                    b"solidFill" => {
                        let c = parse_fill_color(reader, buf, &mut sp_depth)?;
                        fill = FillStyle::Solid(c);
                        has_fill = true;
                    }
                    b"noFill" => {
                        has_fill = true;
                    }
                    b"ln" => {
                        // Extract width from ln element before releasing borrow on buf
                        let mut ln_width: i64 = 0;
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"w" {
                                if let Ok(val) = std::str::from_utf8(&attr.value) {
                                    ln_width = val.parse().unwrap_or(0);
                                }
                            }
                        }
                        outline = parse_outline(reader, buf, &mut sp_depth, ln_width)?;
                    }
                    b"p" if is_in_txbody(&[]) => {
                        // We handle <a:p> inside txBody below
                    }
                    b"txBody" => {
                        text_paragraphs = parse_text_body(reader, buf, &mut sp_depth)?;
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                let local = e.local_name();
                match local.as_ref() {
                    b"off" => {
                        parse_offset_attrs(e, &mut transform)?;
                    }
                    b"ext" => {
                        parse_extent_attrs(e, &mut transform)?;
                    }
                    b"prstGeom" => {
                        kind = parse_preset_geom(e)?;
                    }
                    b"noFill" => {
                        has_fill = true;
                    }
                    _ => {}
                }
            }
            Ok(Event::End(_)) => {
                sp_depth -= 1;
                if sp_depth == 0 {
                    break;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(e.into()),
            _ => {}
        }
        buf.clear();
    }

    if !has_fill && text_paragraphs.is_empty() {
        // Shape with no fill and no text — could be a placeholder, skip
    }

    Ok(Some(Shape {
        kind,
        transform,
        fill,
        outline,
        text: text_paragraphs,
    }))
}

fn parse_picture(
    reader: &mut Reader<&[u8]>,
    buf: &mut Vec<u8>,
    rels: &HashMap<String, String>,
) -> Result<Option<Shape>> {
    let mut depth = 1i32;
    let mut transform = Transform::default();
    let mut rel_id = String::new();

    loop {
        match reader.read_event_into(buf) {
            Ok(Event::Start(ref e)) => {
                depth += 1;
                let local = e.local_name();
                match local.as_ref() {
                    b"off" => parse_offset_attrs(e, &mut transform)?,
                    b"ext" => parse_extent_attrs(e, &mut transform)?,
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                let local = e.local_name();
                match local.as_ref() {
                    b"off" => parse_offset_attrs(e, &mut transform)?,
                    b"ext" => parse_extent_attrs(e, &mut transform)?,
                    b"blip" => {
                        for attr in e.attributes().flatten() {
                            // r:embed attribute
                            if attr.key.as_ref() == b"r:embed"
                                || attr.key.local_name().as_ref() == b"embed"
                            {
                                rel_id = std::str::from_utf8(&attr.value)?.to_string();
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(_)) => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(e.into()),
            _ => {}
        }
        buf.clear();
    }

    let resolved_path = rels.get(&rel_id).cloned().unwrap_or_default();

    Ok(Some(Shape {
        kind: ShapeKind::Picture {
            rel_id: resolved_path,
        },
        transform,
        fill: FillStyle::NoFill,
        outline: OutlineStyle::default(),
        text: Vec::new(),
    }))
}

fn parse_offset_attrs(e: &quick_xml::events::BytesStart, t: &mut Transform) -> Result<()> {
    for attr in e.attributes().flatten() {
        match attr.key.as_ref() {
            b"x" => t.off_x = std::str::from_utf8(&attr.value)?.parse()?,
            b"y" => t.off_y = std::str::from_utf8(&attr.value)?.parse()?,
            _ => {}
        }
    }
    Ok(())
}

fn parse_extent_attrs(e: &quick_xml::events::BytesStart, t: &mut Transform) -> Result<()> {
    for attr in e.attributes().flatten() {
        match attr.key.as_ref() {
            b"cx" => t.ext_cx = std::str::from_utf8(&attr.value)?.parse()?,
            b"cy" => t.ext_cy = std::str::from_utf8(&attr.value)?.parse()?,
            _ => {}
        }
    }
    Ok(())
}

fn parse_preset_geom(e: &quick_xml::events::BytesStart) -> Result<ShapeKind> {
    for attr in e.attributes().flatten() {
        if attr.key.as_ref() == b"prst" {
            let name = std::str::from_utf8(&attr.value)?.to_string();
            return Ok(match name.as_str() {
                "rect" => ShapeKind::Rectangle,
                "roundRect" => ShapeKind::RoundedRectangle,
                "ellipse" => ShapeKind::Ellipse,
                "line" => ShapeKind::Line,
                other => ShapeKind::Preset(other.to_string()),
            });
        }
    }
    Ok(ShapeKind::Rectangle)
}

fn parse_srgb_attr(e: &quick_xml::events::BytesStart) -> Result<ColorSpec> {
    for attr in e.attributes().flatten() {
        if attr.key.as_ref() == b"val" {
            let hex = std::str::from_utf8(&attr.value)?;
            let r = u8::from_str_radix(&hex[0..2], 16)?;
            let g = u8::from_str_radix(&hex[2..4], 16)?;
            let b = u8::from_str_radix(&hex[4..6], 16)?;
            return Ok(ColorSpec::SrgbClr(r, g, b));
        }
    }
    Ok(ColorSpec::None)
}

fn parse_scheme_attr(e: &quick_xml::events::BytesStart) -> Result<ColorSpec> {
    for attr in e.attributes().flatten() {
        if attr.key.as_ref() == b"val" {
            let name = std::str::from_utf8(&attr.value)?.to_string();
            return Ok(ColorSpec::SchemeClr(name));
        }
    }
    Ok(ColorSpec::None)
}

fn parse_fill_color(
    reader: &mut Reader<&[u8]>,
    buf: &mut Vec<u8>,
    depth: &mut i32,
) -> Result<ColorSpec> {
    let mut color = ColorSpec::None;
    let start_depth = *depth;

    loop {
        match reader.read_event_into(buf) {
            Ok(Event::Start(ref e)) => {
                *depth += 1;
                let local = e.local_name();
                if local.as_ref() == b"srgbClr" {
                    color = parse_srgb_attr(e)?;
                } else if local.as_ref() == b"schemeClr" {
                    color = parse_scheme_attr(e)?;
                }
            }
            Ok(Event::Empty(ref e)) => {
                let local = e.local_name();
                if local.as_ref() == b"srgbClr" {
                    color = parse_srgb_attr(e)?;
                } else if local.as_ref() == b"schemeClr" {
                    color = parse_scheme_attr(e)?;
                }
            }
            Ok(Event::End(_)) => {
                *depth -= 1;
                if *depth < start_depth {
                    break;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(e.into()),
            _ => {}
        }
        buf.clear();
    }

    Ok(color)
}

fn parse_outline(
    reader: &mut Reader<&[u8]>,
    buf: &mut Vec<u8>,
    depth: &mut i32,
    width_emu: i64,
) -> Result<OutlineStyle> {
    let mut outline = OutlineStyle::default();
    outline.width_emu = width_emu;
    let start_depth = *depth;

    loop {
        match reader.read_event_into(buf) {
            Ok(Event::Start(ref e)) => {
                *depth += 1;
                let local = e.local_name();
                if local.as_ref() == b"solidFill" {
                    outline.color = parse_fill_color(reader, buf, depth)?;
                }
            }
            Ok(Event::Empty(ref e)) => {
                let local = e.local_name();
                if local.as_ref() == b"srgbClr" {
                    outline.color = parse_srgb_attr(e)?;
                } else if local.as_ref() == b"schemeClr" {
                    outline.color = parse_scheme_attr(e)?;
                }
            }
            Ok(Event::End(_)) => {
                *depth -= 1;
                if *depth < start_depth {
                    break;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(e.into()),
            _ => {}
        }
        buf.clear();
    }

    Ok(outline)
}

fn parse_text_body(
    reader: &mut Reader<&[u8]>,
    buf: &mut Vec<u8>,
    depth: &mut i32,
) -> Result<Vec<TextParagraph>> {
    let mut paragraphs = Vec::new();
    let start_depth = *depth;

    loop {
        match reader.read_event_into(buf) {
            Ok(Event::Start(ref e)) => {
                *depth += 1;
                if e.local_name().as_ref() == b"p" {
                    let para = parse_paragraph(reader, buf, depth)?;
                    paragraphs.push(para);
                }
            }
            Ok(Event::End(_)) => {
                *depth -= 1;
                if *depth < start_depth {
                    break;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(e.into()),
            _ => {}
        }
        buf.clear();
    }

    Ok(paragraphs)
}

fn parse_paragraph(
    reader: &mut Reader<&[u8]>,
    buf: &mut Vec<u8>,
    depth: &mut i32,
) -> Result<TextParagraph> {
    let mut para = TextParagraph::default();
    let start_depth = *depth;

    loop {
        match reader.read_event_into(buf) {
            Ok(Event::Start(ref e)) => {
                *depth += 1;
                let local = e.local_name();
                match local.as_ref() {
                    b"pPr" => {
                        // Paragraph properties
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"algn" {
                                let val = std::str::from_utf8(&attr.value)?;
                                para.align = match val {
                                    "ctr" => TextAlign::Center,
                                    "r" => TextAlign::Right,
                                    _ => TextAlign::Left,
                                };
                            }
                        }
                    }
                    b"r" => {
                        let run = parse_text_run(reader, buf, depth)?;
                        para.runs.push(run);
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                if e.local_name().as_ref() == b"pPr" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"algn" {
                            let val = std::str::from_utf8(&attr.value)?;
                            para.align = match val {
                                "ctr" => TextAlign::Center,
                                "r" => TextAlign::Right,
                                _ => TextAlign::Left,
                            };
                        }
                    }
                }
            }
            Ok(Event::End(_)) => {
                *depth -= 1;
                if *depth < start_depth {
                    break;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(e.into()),
            _ => {}
        }
        buf.clear();
    }

    Ok(para)
}

fn parse_text_run(
    reader: &mut Reader<&[u8]>,
    buf: &mut Vec<u8>,
    depth: &mut i32,
) -> Result<TextRun> {
    let mut run = TextRun::default();
    let start_depth = *depth;

    loop {
        match reader.read_event_into(buf) {
            Ok(Event::Start(ref e)) => {
                *depth += 1;
                let local = e.local_name();
                match local.as_ref() {
                    b"rPr" => {
                        parse_run_props(e, &mut run)?;
                        // Parse children for color
                        parse_run_props_children(reader, buf, depth, &mut run)?;
                    }
                    b"t" => {
                        // Text content follows
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                if e.local_name().as_ref() == b"rPr" {
                    parse_run_props(e, &mut run)?;
                }
            }
            Ok(Event::Text(ref t)) => {
                run.text.push_str(&t.unescape()?);
            }
            Ok(Event::End(_)) => {
                *depth -= 1;
                if *depth < start_depth {
                    break;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(e.into()),
            _ => {}
        }
        buf.clear();
    }

    Ok(run)
}

fn parse_run_props(e: &quick_xml::events::BytesStart, run: &mut TextRun) -> Result<()> {
    for attr in e.attributes().flatten() {
        match attr.key.as_ref() {
            b"sz" => {
                run.font_size_hundredths_pt = std::str::from_utf8(&attr.value)?.parse()?;
            }
            b"b" => {
                run.bold = std::str::from_utf8(&attr.value)? == "1";
            }
            b"i" => {
                run.italic = std::str::from_utf8(&attr.value)? == "1";
            }
            _ => {}
        }
    }
    Ok(())
}

fn parse_run_props_children(
    reader: &mut Reader<&[u8]>,
    buf: &mut Vec<u8>,
    depth: &mut i32,
    run: &mut TextRun,
) -> Result<()> {
    let start_depth = *depth;

    loop {
        match reader.read_event_into(buf) {
            Ok(Event::Start(ref e)) => {
                *depth += 1;
                let local = e.local_name();
                if local.as_ref() == b"srgbClr" {
                    run.color = parse_srgb_attr(e)?;
                } else if local.as_ref() == b"schemeClr" {
                    run.color = parse_scheme_attr(e)?;
                } else if local.as_ref() == b"latin" || local.as_ref() == b"cs" || local.as_ref() == b"ea" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"typeface" {
                            run.font_family = Some(std::str::from_utf8(&attr.value)?.to_string());
                        }
                    }
                }
            }
            Ok(Event::Empty(ref e)) => {
                let local = e.local_name();
                if local.as_ref() == b"srgbClr" {
                    run.color = parse_srgb_attr(e)?;
                } else if local.as_ref() == b"schemeClr" {
                    run.color = parse_scheme_attr(e)?;
                } else if local.as_ref() == b"latin" || local.as_ref() == b"cs" || local.as_ref() == b"ea" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"typeface" {
                            run.font_family = Some(std::str::from_utf8(&attr.value)?.to_string());
                        }
                    }
                }
            }
            Ok(Event::End(_)) => {
                *depth -= 1;
                if *depth < start_depth {
                    break;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(e.into()),
            _ => {}
        }
        buf.clear();
    }

    Ok(())
}

fn is_in_txbody(_depth: &[String]) -> bool {
    false // placeholder, we handle txBody separately
}
