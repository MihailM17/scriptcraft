//! Fade In `.fadein` import/export (Open Screenplay Format in a ZIP).
//!
//! A `.fadein` file is a ZIP archive whose `document.xml` entry is an OSF
//! document: `<document>` → `<settings>`, `<styles>`, `<paragraphs>`,
//! `<titlepage>` and `<lists>`. Paragraphs name a base style
//! (`<style basestylename="Dialogue"/>`), scene numbers ride on
//! `<para number="12">`, notes on `<para note="…">`, and the first cue of a
//! dual-dialogue pair carries `dualdialogue="1"` on its style. Parentheticals
//! are stored without their outer parentheses.
//!
//! We read both `basestylename` (Fade In) and `basestyle` (early OSF 1.2
//! files) and write `basestylename`. The ZIP reader/writer is deliberately
//! small: one deflated entry, no ZIP64, no encryption.

use crate::kitsp::Metadata;
use crate::model::{
    speech_unit_end, DualMark, DualSide, ElementType, InlineStyle, Project, ScriptBlock, StyleSpan,
};
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;
use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};
use std::io::{Read, Write};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum FadeInError {
    #[error("zip: {0}")]
    Zip(String),
    #[error("xml: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("not a Fade In document (document.xml with a <document> root missing)")]
    NotFadeIn,
}

/// Paragraph type names used by Fade In / OSF.
fn osf_style(t: ElementType) -> &'static str {
    match t {
        ElementType::SceneHeading => "Scene Heading",
        ElementType::SceneCharacters => "Cast List",
        ElementType::Action => "Action",
        ElementType::Character => "Character",
        ElementType::Dialogue => "Dialogue",
        ElementType::Parenthetical => "Parenthetical",
        ElementType::Shot => "Shot",
        ElementType::Transition => "Transition",
        ElementType::NonPrintable => "General",
        ElementType::Lyrics => "Lyrics",
    }
}

/// Inverse of [`osf_style`]. Unknown styles become Action, like Fountain.
fn element_type(kind: &str) -> Option<ElementType> {
    match kind.trim() {
        "Scene Heading" | "Scene" => Some(ElementType::SceneHeading),
        "Cast List" | "Scene Characters" => Some(ElementType::SceneCharacters),
        "Action" | "Normal Text" | "Center" | "Right" => Some(ElementType::Action),
        "Character" => Some(ElementType::Character),
        "Dialogue" => Some(ElementType::Dialogue),
        "Parenthetical" => Some(ElementType::Parenthetical),
        "Shot" => Some(ElementType::Shot),
        "Transition" => Some(ElementType::Transition),
        "General" | "Note" | "Synopsis" => Some(ElementType::NonPrintable),
        "Lyrics" => Some(ElementType::Lyrics),
        _ => None,
    }
}

/// Frozen scene numbers (`locked_scene_numbers`), if any.
fn locked_labels(meta: &Metadata) -> Option<Vec<String>> {
    let raw = meta.locked_scene_numbers.trim();
    if raw.is_empty() {
        return None;
    }
    let raw = raw.strip_prefix('*').unwrap_or(raw);
    let labels: Vec<String> = raw
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    (!labels.is_empty()).then_some(labels)
}

/// The number written onto a scene heading: frozen label when numbering is
/// locked, otherwise prefix + start (same rule as the editor and PDF).
fn scene_number(meta: &Metadata, i: usize) -> String {
    if let Some(label) = locked_labels(meta).and_then(|v| v.get(i).cloned()) {
        return label;
    }
    let start: u32 = meta.scene_number_start.trim().parse().unwrap_or(1);
    format!("{}{}", meta.scene_number_prefix.trim(), start + i as u32)
}

/// Fade In stores parentheticals without the surrounding parentheses. Strip
/// one outer pair only when the inner text is balanced, so `(angry) (sotto)`
/// survives intact.
fn strip_outer_parentheses(text: &str) -> String {
    let t = text.trim();
    if t.len() >= 2 && t.starts_with('(') && t.ends_with(')') {
        let mut depth = 0i32;
        let mut closes_at_end = false;
        for (i, ch) in t.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        closes_at_end = i + ch.len_utf8() == t.len();
                        break;
                    }
                }
                _ => {}
            }
        }
        if closes_at_end {
            return t[1..t.len() - 1].to_string();
        }
    }
    t.to_string()
}

/// Inverse of [`strip_outer_parentheses`]: Fade In parentheticals arrive
/// without parentheses; our model stores the printed form.
fn wrap_parenthetical(text: &str) -> String {
    let t = text.trim();
    if t.starts_with('(') || t.ends_with(')') {
        t.to_string()
    } else {
        format!("({t})")
    }
}

fn esc_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn esc_attr(s: &str) -> String {
    esc_text(s).replace('"', "&quot;")
}

fn unescape_text(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// Build the OSF `document.xml` text for a project.
pub fn to_osf(p: &Project, meta: &Metadata) -> String {
    let title = if meta.title.trim().is_empty() {
        p.title.trim()
    } else {
        meta.title.trim()
    };
    let locked = locked_labels(meta).is_some();
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <document type=\"Open Screenplay Format document\" version=\"30\">\n\
         <info uuid=\"",
    );
    out.push_str(&Uuid::new_v4().to_string().to_uppercase());
    out.push_str("\" pagecount=\"1\"/>\n");

    // Settings: A4 screenplay in Fade In's 1/100 cm units, scene numbers
    // locked because every heading below carries an explicit `number`.
    out.push_str(
        "<settings page_width=\"2159\" page_height=\"2794\" margin_top=\"317\" \
         margin_bottom=\"220\" margin_left=\"317\" margin_right=\"317\" \
         normal_linesperinch=\"6.0\" element_spacing=\"1.00\" \
         break_on_sentences=\"true\" dialogue_continues=\"true\" \
         dialogue_pagebreaks=\"true\" cont_text=\"(cont'd)\" more_text=\"(MORE)\" \
         scenes_continue=\"true\" continued_text=\"CONTINUED\" \
         number_continued=\"true\" auto_omit_scenes=\"false\" \
         omitted_text=\"OMITTED\" scene_time_separator=\" - \" \
         page_header=\"#.\" page_footer=\"\" header_alignment=\"3\" \
         footer_alignment=\"3\" header_first_page=\"false\" \
         footer_first_page=\"false\" pages_locked=\"false\" \
         pagenumber_start=\"1\" pagenumber_mode=\"1AB\" scene_numbering=\"true\" \
         scenenumber_mode=\"1AB\" scenenumber_skip_io=\"false\" \
         scenenumber_start=\"1\" scenenumber_format=\"#\" \
         scenenumber_position=\"3\" scenes_locked=\"",
    );
    out.push_str(if locked { "true" } else { "false" });
    out.push_str(
        "\" dialogue_numbering=\"false\" dialoguenumber_start=\"1\" \
         dialoguenumber_format=\"(#)\" dialogue_locked=\"false\" revision=\"0\" \
         show_revisions=\"true\" show_all_revisions=\"true\"/>\n",
    );

    out.push_str(
        "<styles>\n\
         <style name=\"Normal Text\" builtin=\"1\" builtin_index=\"0\" \
         label=\"Normal Text\" font=\"Courier\" size=\"12\"/>\n\
         <style name=\"Scene Heading\" builtin=\"1\" builtin_index=\"1\" \
         label=\"Scene Heading\" basestylename=\"Normal Text\" style_enter=\"Action\" \
         font=\"Courier\" size=\"12\" spacebefore=\"2.0\" keepwithnext=\"1\" allcaps=\"1\"/>\n\
         <style name=\"Action\" builtin=\"1\" builtin_index=\"2\" label=\"Action\" \
         basestylename=\"Normal Text\" style_tab_before=\"Character\" \
         style_tab_after=\"Character\" font=\"Courier\" size=\"12\" spacebefore=\"1.0\"/>\n\
         <style name=\"Character\" builtin=\"1\" builtin_index=\"3\" label=\"Character\" \
         basestylename=\"Normal Text\" style_enter=\"Dialogue\" \
         style_tab_before=\"Action\" style_tab_after=\"Action\" font=\"Courier\" \
         size=\"12\" spacebefore=\"1.0\" keepwithnext=\"1\" leftindent=\"635\" allcaps=\"1\"/>\n\
         <style name=\"Parenthetical\" builtin=\"1\" builtin_index=\"4\" \
         label=\"Parenthetical\" basestylename=\"Normal Text\" style_enter=\"Dialogue\" \
         style_tab_before=\"Dialogue\" style_tab_after=\"Dialogue\" font=\"Courier\" \
         size=\"12\" keepwithnext=\"1\" leftindent=\"508\" rightindent=\"508\"/>\n\
         <style name=\"Dialogue\" builtin=\"1\" builtin_index=\"5\" label=\"Dialogue\" \
         basestylename=\"Normal Text\" style_enter=\"Action\" \
         style_tab_before=\"Parenthetical\" style_tab_after=\"Parenthetical\" \
         font=\"Courier\" size=\"12\" leftindent=\"330\" rightindent=\"254\"/>\n\
         <style name=\"Transition\" builtin=\"1\" builtin_index=\"6\" \
         label=\"Transition\" basestylename=\"Normal Text\" style_enter=\"Scene Heading\" \
         style_tab_after=\"Action\" font=\"Courier\" size=\"12\" spacebefore=\"1.0\" \
         align=\"right\" leftindent=\"1016\" rightindent=\"127\" allcaps=\"1\"/>\n\
         <style name=\"Shot\" builtin=\"1\" builtin_index=\"7\" label=\"Shot\" \
         basestylename=\"Normal Text\" style_enter=\"Action\" font=\"Courier\" \
         size=\"12\" spacebefore=\"1.0\" keepwithnext=\"1\" allcaps=\"1\"/>\n\
         <style name=\"Cast List\" label=\"Cast List\" basestylename=\"Action\" \
         font=\"Courier\" size=\"12\"/>\n\
         <style name=\"Lyrics\" label=\"Lyrics\" basestylename=\"Action\" \
         font=\"Courier\" size=\"12\" italic=\"1\"/>\n\
         <header_style basestylename=\"Normal Text\"/>\n\
         <footer_style basestylename=\"Normal Text\"/>\n\
         </styles>\n\
         <paragraphs>\n",
    );

    let pairs = p.dual_pairs();
    let mut left_cues = vec![false; p.blocks.len()];
    for pair in &pairs {
        left_cues[pair.left] = true;
    }
    let mut scene_i = 0usize;
    for (bi, b) in p.blocks.iter().enumerate() {
        let style = osf_style(b.element_type);
        let mut attrs = String::new();
        if b.element_type == ElementType::SceneHeading {
            attrs.push_str(&format!(
                " number=\"{}\"",
                esc_attr(&scene_number(meta, scene_i))
            ));
            scene_i += 1;
        }
        let text = b.text.trim();
        if b.element_type == ElementType::NonPrintable {
            out.push_str(&format!(
                "  <para note=\"{}\">\n    <style basestylename=\"{style}\"/>\n    <text></text>\n  </para>\n",
                esc_attr(text)
            ));
            continue;
        }
        if text.is_empty() {
            continue;
        }
        let body = match b.element_type {
            ElementType::Parenthetical => strip_outer_parentheses(text),
            // One OSF paragraph is one printed paragraph; embedded breaks
            // become spaces, exactly like the FDX exporter.
            _ => text.replace(['\r', '\n'], " "),
        };
        out.push_str(&format!(
            "  <para{attrs}>\n    <style basestylename=\"{style}\""
        ));
        if left_cues[bi] {
            out.push_str(" dualdialogue=\"1\"");
        }
        out.push_str("/>\n");
        if b.styles.is_empty() {
            out.push_str("    <text>");
            out.push_str(&esc_text(&body));
            out.push_str("</text>\n  </para>\n");
        } else {
            let leading_trim = b.text.chars().count() - b.text.trim_start().chars().count();
            let paren_strip = usize::from(
                b.element_type == ElementType::Parenthetical
                    && text.starts_with('(')
                    && body.len() + 2 == text.len(),
            );
            let base_off = leading_trim + paren_strip;
            let body_chars: Vec<char> = body.chars().collect();
            let runs = b.style_runs_in(base_off, base_off + body_chars.len());
            for (a, z, bold, italic, underline) in runs {
                let slice: String = body_chars[a - base_off..z - base_off].iter().collect();
                let mut parts: Vec<&'static str> = Vec::new();
                if bold {
                    parts.push("bold");
                }
                if italic {
                    parts.push("italic");
                }
                if underline {
                    parts.push("underline");
                }
                if parts.is_empty() {
                    out.push_str("    <text>");
                } else {
                    out.push_str(&format!("    <text style=\"{}\">", parts.join("+")));
                }
                out.push_str(&esc_text(&slice));
                out.push_str("</text>\n");
            }
            out.push_str("  </para>\n");
        }
    }
    out.push_str("  </paragraphs>\n");

    out.push_str("  <titlepage>\n");
    let mut title_para = |bookmark: &str, text: &str, center: bool| {
        if text.trim().is_empty() {
            return;
        }
        out.push_str(&format!(
            "    <para bookmark=\"{bookmark}\">\n      <style basestylename=\"Normal Text\""
        ));
        if center {
            out.push_str(" align=\"center\"");
        }
        out.push_str("/>\n      <text>");
        out.push_str(&esc_text(text.trim()));
        out.push_str("</text>\n    </para>\n");
    };
    title_para("Title", title, true);
    title_para("Credit", &meta.genre, true);
    title_para("Author", &meta.author, true);
    title_para("Source", &meta.additional_info, false);
    title_para("Draft", &meta.year, false);
    title_para("Contact", &meta.contacts, false);
    out.push_str("  </titlepage>\n");
    out.push_str("  <spelling language=\"en_US\"/>\n");
    out.push_str("</document>\n");
    out
}

/// Serialize a project to a `.fadein` ZIP archive.
pub fn to_fadein_bytes(p: &Project, meta: &Metadata) -> Result<Vec<u8>, FadeInError> {
    let xml = to_osf(p, meta);
    zip_document(xml.as_bytes())
}

/// Parse a `.fadein` ZIP archive into a project plus title-page metadata.
pub fn from_fadein_bytes(bytes: &[u8]) -> Result<(Project, Metadata), FadeInError> {
    let xml = unzip_document(bytes)?;
    let xml = String::from_utf8(xml)
        .map_err(|_| FadeInError::Zip("document.xml is not valid UTF-8".into()))?;
    from_osf(&xml)
}

#[derive(Default)]
struct Para {
    kind: String,
    number: String,
    note: String,
    bookmark: String,
    align: String,
    dual: bool,
    text: String,
    styles: Vec<StyleSpan>,
}

fn parse_osf_text_style(
    e: &quick_xml::events::BytesStart<'_>,
    reader: &Reader<&[u8]>,
) -> (bool, bool, bool) {
    let mut bold =
        attr(e, reader, "bold").is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
    let mut italic =
        attr(e, reader, "italic").is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
    let mut underline =
        attr(e, reader, "underline").is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
    if let Some(style) = attr(e, reader, "style") {
        for tok in style.split(['+', ',', ' ']) {
            match tok.trim().to_ascii_lowercase().as_str() {
                "bold" => bold = true,
                "italic" => italic = true,
                "underline" => underline = true,
                _ => {}
            }
        }
    }
    (bold, italic, underline)
}

fn attr(
    e: &quick_xml::events::BytesStart<'_>,
    reader: &Reader<&[u8]>,
    key: &str,
) -> Option<String> {
    let a = e.try_get_attribute(key).ok().flatten()?;
    let v = a
        .decoded_and_normalized_value(XmlVersion::Implicit1_0, reader.decoder())
        .ok()?;
    Some(v.into_owned())
}

fn para_from(e: &quick_xml::events::BytesStart<'_>, reader: &Reader<&[u8]>) -> Para {
    Para {
        number: attr(e, reader, "number")
            .or_else(|| attr(e, reader, "scene_number"))
            .unwrap_or_default(),
        note: attr(e, reader, "note").unwrap_or_default(),
        bookmark: attr(e, reader, "bookmark").unwrap_or_default(),
        ..Default::default()
    }
}

fn apply_style(e: &quick_xml::events::BytesStart<'_>, reader: &Reader<&[u8]>, p: &mut Para) {
    p.kind = attr(e, reader, "basestylename")
        .or_else(|| attr(e, reader, "basestyle"))
        .unwrap_or_default();
    p.align = attr(e, reader, "align").unwrap_or_default();
    p.dual =
        attr(e, reader, "dualdialogue").is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
}

/// Parse an OSF `document.xml` into a project plus title-page metadata.
pub fn from_osf(xml: &str) -> Result<(Project, Metadata), FadeInError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);

    let mut saw_root = false;
    let mut in_titlepage = false;
    let mut in_text = false;
    let mut cur_text_style = (false, false, false);
    let mut cur_text_start = 0usize;
    let mut para: Option<Para> = None;

    let mut blocks: Vec<ScriptBlock> = Vec::new();
    let mut scene_numbers: Vec<String> = Vec::new();
    let mut dual_lefts: Vec<usize> = Vec::new();
    let mut title_paras: Vec<(String, String, String)> = Vec::new();

    loop {
        match reader.read_event()? {
            Event::Start(e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if !saw_root {
                    saw_root = true;
                    if name != "document" {
                        return Err(FadeInError::NotFadeIn);
                    }
                    continue;
                }
                match name.as_str() {
                    "titlepage" => in_titlepage = true,
                    "para" => para = Some(para_from(&e, &reader)),
                    "style" => {
                        if let Some(p) = para.as_mut() {
                            apply_style(&e, &reader, p);
                        }
                    }
                    "text" => {
                        in_text = true;
                        cur_text_start = para.as_ref().map(|p| p.text.chars().count()).unwrap_or(0);
                        cur_text_style = parse_osf_text_style(&e, &reader);
                    }
                    _ => {}
                }
            }
            Event::Empty(e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if !saw_root && name == "document" {
                    saw_root = true;
                }
                match name.as_str() {
                    "style" => {
                        if let Some(p) = para.as_mut() {
                            apply_style(&e, &reader, p);
                        }
                    }
                    "para" => dispatch(
                        para_from(&e, &reader),
                        in_titlepage,
                        &mut blocks,
                        &mut scene_numbers,
                        &mut dual_lefts,
                        &mut title_paras,
                    ),
                    _ => {}
                }
            }
            Event::Text(e) => {
                if in_text {
                    if let Some(p) = para.as_mut() {
                        let s = reader.decoder().decode(e.as_ref()).unwrap_or_default();
                        p.text.push_str(&unescape_text(&s));
                    }
                }
            }
            Event::GeneralRef(e) => {
                if !in_text {
                    continue;
                }
                let s = match e.resolve_char_ref().ok().flatten() {
                    Some(c) => c.to_string(),
                    None => match String::from_utf8_lossy(e.as_ref()).as_ref() {
                        "amp" => "&".to_string(),
                        "lt" => "<".to_string(),
                        "gt" => ">".to_string(),
                        "quot" => "\"".to_string(),
                        "apos" => "'".to_string(),
                        _ => String::new(),
                    },
                };
                if let Some(p) = para.as_mut() {
                    p.text.push_str(&s);
                }
            }
            Event::CData(e) => {
                if in_text {
                    if let Some(p) = para.as_mut() {
                        p.text.push_str(&String::from_utf8_lossy(&e.into_inner()));
                    }
                }
            }
            Event::End(e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                match name.as_str() {
                    "titlepage" => in_titlepage = false,
                    "text" => {
                        if in_text {
                            if let Some(p) = para.as_mut() {
                                let end = p.text.chars().count();
                                if end > cur_text_start {
                                    let (bold, italic, underline) = cur_text_style;
                                    if bold {
                                        p.styles.push(StyleSpan {
                                            start: cur_text_start,
                                            end,
                                            style: InlineStyle::Bold,
                                        });
                                    }
                                    if italic {
                                        p.styles.push(StyleSpan {
                                            start: cur_text_start,
                                            end,
                                            style: InlineStyle::Italic,
                                        });
                                    }
                                    if underline {
                                        p.styles.push(StyleSpan {
                                            start: cur_text_start,
                                            end,
                                            style: InlineStyle::Underline,
                                        });
                                    }
                                }
                            }
                            in_text = false;
                            cur_text_style = (false, false, false);
                        }
                    }
                    "para" => {
                        if let Some(p) = para.take() {
                            dispatch(
                                p,
                                in_titlepage,
                                &mut blocks,
                                &mut scene_numbers,
                                &mut dual_lefts,
                                &mut title_paras,
                            );
                        }
                    }
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    if !saw_root {
        return Err(FadeInError::NotFadeIn);
    }

    // Rebuild dual pairs from `dualdialogue="1"` on the left cue: the next
    // character cue after that speech unit is the right-hand side.
    for left in dual_lefts {
        let Some(left_end) = speech_unit_end(&blocks, left) else {
            continue;
        };
        let Some(right) = (left_end < blocks.len()
            && blocks[left_end].element_type == ElementType::Character)
            .then_some(left_end)
        else {
            continue;
        };
        let Some(right_end) = speech_unit_end(&blocks, right) else {
            continue;
        };
        if right_end > right + 1 {
            let group = Uuid::new_v4();
            blocks[left].dual = Some(DualMark {
                group,
                side: DualSide::Left,
            });
            blocks[right].dual = Some(DualMark {
                group,
                side: DualSide::Right,
            });
        }
    }

    let mut meta = Metadata::default();
    for (bookmark, text, align) in &title_paras {
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        match bookmark.trim().to_lowercase().as_str() {
            "title" => meta.title = text.to_string(),
            "credit" | "genre" => meta.genre = text.to_string(),
            "author" | "authors" => meta.author = text.to_string(),
            "source" => meta.additional_info = text.to_string(),
            "draft" | "draft date" | "draftdate" => meta.year = text.to_string(),
            "contact" | "contacts" => {
                if !meta.contacts.is_empty() {
                    meta.contacts.push('\n');
                }
                meta.contacts.push_str(text);
            }
            "" if align.eq_ignore_ascii_case("center") && meta.title.is_empty() => {
                meta.title = text.to_string();
            }
            "" => {
                if !meta.contacts.is_empty() {
                    meta.contacts.push('\n');
                }
                meta.contacts.push_str(text);
            }
            _ => {}
        }
    }

    // Fade In scene numbers are authoritative: freeze them only when every
    // heading carries one (unnumbered scenes keep the app's numbering).
    let scene_count = blocks
        .iter()
        .filter(|b| b.element_type == ElementType::SceneHeading)
        .count();
    if scene_count > 0 && scene_numbers.len() == scene_count {
        meta.locked_scene_numbers = scene_numbers.join(",");
    }

    let title = if meta.title.trim().is_empty() {
        "Untitled".to_string()
    } else {
        meta.title.clone()
    };
    Ok((
        Project {
            title,
            blocks,
            ..Default::default()
        },
        meta,
    ))
}

fn dispatch(
    p: Para,
    in_titlepage: bool,
    blocks: &mut Vec<ScriptBlock>,
    scene_numbers: &mut Vec<String>,
    dual_lefts: &mut Vec<usize>,
    title_paras: &mut Vec<(String, String, String)>,
) {
    let leading_trim = p.text.chars().count() - p.text.trim_start().chars().count();
    let text = p.text.trim().to_string();
    if in_titlepage {
        title_paras.push((p.bookmark, text, p.align));
        return;
    }
    let note = p.note.trim().to_string();
    if text.is_empty() && note.is_empty() {
        return;
    }
    if !text.is_empty() {
        let el = element_type(&p.kind).unwrap_or(ElementType::Action);
        if el == ElementType::SceneHeading {
            scene_numbers.push(p.number.trim().to_string());
        }
        let added_paren =
            el == ElementType::Parenthetical && !text.starts_with('(') && !text.ends_with(')');
        let body = if el == ElementType::Parenthetical {
            wrap_parenthetical(&text)
        } else {
            text
        };
        let bi = blocks.len();
        let mut blk = ScriptBlock::new(el, body);
        if !p.styles.is_empty() {
            let shift = usize::from(added_paren);
            blk.styles = p
                .styles
                .into_iter()
                .filter_map(|s| {
                    let a = s.start.saturating_sub(leading_trim) + shift;
                    let z = s.end.saturating_sub(leading_trim) + shift;
                    (a < z).then_some(StyleSpan {
                        start: a,
                        end: z,
                        style: s.style,
                    })
                })
                .collect();
            blk.normalize_styles();
        }
        blocks.push(blk);
        if el == ElementType::Character && p.dual {
            dual_lefts.push(bi);
        }
    }
    if !note.is_empty() {
        blocks.push(ScriptBlock::new(ElementType::NonPrintable, note));
    }
}

// ---------------------------------------------------------------------------
// Minimal ZIP (one deflated entry)
// ---------------------------------------------------------------------------

const LOCAL_SIG: u32 = 0x0403_4b50;
const CENTRAL_SIG: u32 = 0x0201_4b50;
const EOCD_SIG: u32 = 0x0605_4b50;
/// DOS timestamp for 1980-01-01 00:00, so exports are reproducible.
const DOS_DATE: u16 = 0x0021;
const FLAG_UTF8: u16 = 0x0800;

fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn u16_at(buf: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(buf.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(buf: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(buf.get(at..at + 4)?.try_into().ok()?))
}

fn zip_document(data: &[u8]) -> Result<Vec<u8>, FadeInError> {
    let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
    enc.write_all(data)?;
    let compressed = enc.finish()?;
    let crc = crc32fast::hash(data);
    let name = b"document.xml";

    let mut out = Vec::with_capacity(compressed.len() + 128);
    // Local file header.
    put_u32(&mut out, LOCAL_SIG);
    put_u16(&mut out, 20);
    put_u16(&mut out, FLAG_UTF8);
    put_u16(&mut out, 8);
    put_u16(&mut out, 0);
    put_u16(&mut out, DOS_DATE);
    put_u32(&mut out, crc);
    put_u32(&mut out, compressed.len() as u32);
    put_u32(&mut out, data.len() as u32);
    put_u16(&mut out, name.len() as u16);
    put_u16(&mut out, 0);
    out.extend_from_slice(name);
    out.extend_from_slice(&compressed);

    let cd_start = out.len();
    // Central directory header.
    put_u32(&mut out, CENTRAL_SIG);
    put_u16(&mut out, 20);
    put_u16(&mut out, 20);
    put_u16(&mut out, FLAG_UTF8);
    put_u16(&mut out, 8);
    put_u16(&mut out, 0);
    put_u16(&mut out, DOS_DATE);
    put_u32(&mut out, crc);
    put_u32(&mut out, compressed.len() as u32);
    put_u32(&mut out, data.len() as u32);
    put_u16(&mut out, name.len() as u16);
    put_u16(&mut out, 0);
    put_u16(&mut out, 0);
    put_u16(&mut out, 0);
    put_u16(&mut out, 0);
    put_u32(&mut out, 0);
    put_u32(&mut out, 0);
    out.extend_from_slice(name);
    let cd_size = out.len() - cd_start;

    put_u32(&mut out, EOCD_SIG);
    put_u16(&mut out, 0);
    put_u16(&mut out, 0);
    put_u16(&mut out, 1);
    put_u16(&mut out, 1);
    put_u32(&mut out, cd_size as u32);
    put_u32(&mut out, cd_start as u32);
    put_u16(&mut out, 0);
    Ok(out)
}

fn find_eocd(buf: &[u8]) -> Option<usize> {
    if buf.len() < 22 {
        return None;
    }
    let first = buf.len().saturating_sub(22 + 65_535);
    (first..=buf.len() - 22)
        .rev()
        .find(|&i| u32_at(buf, i) == Some(EOCD_SIG))
}

fn unzip_document(buf: &[u8]) -> Result<Vec<u8>, FadeInError> {
    let eocd = find_eocd(buf).ok_or_else(|| FadeInError::Zip("end of archive not found".into()))?;
    let entries = u16_at(buf, eocd + 10).unwrap_or(0) as usize;
    let cd_offset = u32_at(buf, eocd + 16).unwrap_or(0) as usize;
    if cd_offset == u32::MAX as usize {
        return Err(FadeInError::Zip("ZIP64 archives are not supported".into()));
    }
    let mut pos = cd_offset;
    let mut fallback: Option<(usize, u16, usize, usize, u32)> = None;
    for _ in 0..entries {
        if u32_at(buf, pos) != Some(CENTRAL_SIG) {
            return Err(FadeInError::Zip("central directory is corrupt".into()));
        }
        let method = u16_at(buf, pos + 10).unwrap_or(0);
        let crc = u32_at(buf, pos + 16).unwrap_or(0);
        let comp_size = u32_at(buf, pos + 20).unwrap_or(0) as usize;
        let uncomp_size = u32_at(buf, pos + 24).unwrap_or(0) as usize;
        let name_len = u16_at(buf, pos + 28).unwrap_or(0) as usize;
        let extra_len = u16_at(buf, pos + 30).unwrap_or(0) as usize;
        let comment_len = u16_at(buf, pos + 32).unwrap_or(0) as usize;
        let local = u32_at(buf, pos + 42).unwrap_or(0) as usize;
        let name = buf
            .get(pos + 46..pos + 46 + name_len)
            .ok_or_else(|| FadeInError::Zip("central directory is truncated".into()))?;
        if name == b"document.xml" {
            return read_local(buf, local, method, comp_size, uncomp_size, crc);
        }
        if fallback.is_none() && name.to_ascii_lowercase().ends_with(b".xml") {
            fallback = Some((local, method, comp_size, uncomp_size, crc));
        }
        pos += 46 + name_len + extra_len + comment_len;
    }
    if let Some((local, method, comp, uncomp, crc)) = fallback {
        return read_local(buf, local, method, comp, uncomp, crc);
    }
    Err(FadeInError::Zip(
        "document.xml is missing from the archive".into(),
    ))
}

fn read_local(
    buf: &[u8],
    local: usize,
    method: u16,
    comp_size: usize,
    uncomp_size: usize,
    crc: u32,
) -> Result<Vec<u8>, FadeInError> {
    if u32_at(buf, local) != Some(LOCAL_SIG) {
        return Err(FadeInError::Zip("local header is corrupt".into()));
    }
    let name_len = u16_at(buf, local + 26).unwrap_or(0) as usize;
    let extra_len = u16_at(buf, local + 28).unwrap_or(0) as usize;
    let start = local + 30 + name_len + extra_len;
    let data = buf
        .get(start..start + comp_size)
        .ok_or_else(|| FadeInError::Zip("entry is truncated".into()))?;
    let out = match method {
        0 => data.to_vec(),
        8 => {
            let mut d = DeflateDecoder::new(data);
            let mut out = Vec::with_capacity(uncomp_size);
            d.read_to_end(&mut out)?;
            out
        }
        other => {
            return Err(FadeInError::Zip(format!(
                "unsupported compression method {other}"
            )))
        }
    };
    if crc != 0 && crc32fast::hash(&out) != crc {
        return Err(FadeInError::Zip("document.xml failed its CRC check".into()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ElementType::*;

    fn sample_project() -> Project {
        let mut p = Project {
            title: "The Morning Ritual".into(),
            blocks: vec![],
            ..Default::default()
        };
        for (t, text) in [
            (SceneHeading, "INT. COFFEE SHOP - MORNING"),
            (SceneCharacters, "MIRA, BARISTA"),
            (Action, "The room hums with morning chatter."),
            (Character, "MIRA"),
            (Parenthetical, "(over the noise)"),
            (Dialogue, "One espresso, please."),
            (Shot, "CLOSE ON THE ESPRESSO MACHINE"),
            (Lyrics, "Raindrops keep falling..."),
            (Transition, "CUT TO:"),
            (NonPrintable, "Keep the scene under a page."),
            (SceneHeading, "EXT. STREET - CONTINUOUS"),
            (Action, "Mira steps into the rain & opens her umbrella."),
        ] {
            p.blocks.push(ScriptBlock::new(t, text.into()));
        }
        p
    }

    fn sample_meta() -> Metadata {
        Metadata {
            title: "The Morning Ritual".into(),
            genre: "Drama".into(),
            author: "M. Hayll".into(),
            contacts: "mihail@example.com\n+1 555 0100".into(),
            additional_info: "Based on a short story".into(),
            year: "2026".into(),
            locked_scene_numbers: "*SC3,SC4".into(),
            ..Default::default()
        }
    }

    #[test]
    fn round_trip_preserves_blocks_and_parentheticals() {
        let p = sample_project();
        let meta = sample_meta();
        let bytes = to_fadein_bytes(&p, &meta).unwrap();
        assert_eq!(&bytes[..2], b"PK");
        let (got, got_meta) = from_fadein_bytes(&bytes).unwrap();
        assert_eq!(got.blocks.len(), p.blocks.len());
        for (a, b) in p.blocks.iter().zip(&got.blocks) {
            assert_eq!(a.element_type, b.element_type, "type for {:?}", a.text);
            assert_eq!(a.text, b.text);
        }
        assert_eq!(got_meta.title, "The Morning Ritual");
        assert_eq!(got_meta.genre, "Drama");
        assert_eq!(got_meta.author, "M. Hayll");
        assert_eq!(got_meta.additional_info, "Based on a short story");
        assert_eq!(got_meta.year, "2026");
        assert_eq!(got_meta.contacts, "mihail@example.com\n+1 555 0100");
    }

    #[test]
    fn exports_scene_numbers_and_locks_them_on_import() {
        let p = sample_project();
        let xml = to_osf(&p, &sample_meta());
        assert!(xml.contains("<para number=\"SC3\">"));
        assert!(xml.contains("<para number=\"SC4\">"));
        let (_, meta) = from_osf(&xml).unwrap();
        assert_eq!(meta.locked_scene_numbers, "SC3,SC4");
    }

    #[test]
    fn unnumbered_scenes_are_not_locked() {
        let p = sample_project();
        let meta = Metadata::default();
        let xml = to_osf(&p, &meta);
        // Sequential numbers are still written, so they stay locked.
        assert!(xml.contains("<para number=\"1\">"));
        assert!(xml.contains("<para number=\"2\">"));
        let (_, got) = from_osf(&xml).unwrap();
        assert_eq!(got.locked_scene_numbers, "1,2");
    }

    #[test]
    fn dual_dialogue_round_trips_through_the_left_cue_flag() {
        let mut p = Project {
            title: "Dual".into(),
            ..Default::default()
        };
        for (t, text) in [
            (SceneHeading, "INT. CONTROL ROOM - NIGHT"),
            (Action, "Two operators watch the same screen."),
            (Character, "ADA"),
            (Parenthetical, "(quietly)"),
            (Dialogue, "It is still moving."),
            (Character, "GRACE"),
            (Dialogue, "Then we stop it together."),
        ] {
            p.blocks.push(ScriptBlock::new(t, text.into()));
        }
        let group = Uuid::new_v4();
        p.blocks[2].dual = Some(DualMark {
            group,
            side: DualSide::Left,
        });
        p.blocks[5].dual = Some(DualMark {
            group,
            side: DualSide::Right,
        });

        let xml = to_osf(&p, &sample_meta());
        assert!(xml.contains("<style basestylename=\"Character\" dualdialogue=\"1\"/>"));
        assert_eq!(
            xml.matches("dualdialogue=\"1\"").count(),
            1,
            "only the left cue carries the flag"
        );
        let (got, _) = from_osf(&xml).unwrap();
        let pairs = got.dual_pairs();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].left, 2);
        assert_eq!(pairs[0].right, 5);
    }

    #[test]
    fn notes_stay_notes_and_do_not_print_as_body_text() {
        let mut p = sample_project();
        let xml = to_osf(&p, &sample_meta());
        assert!(xml.contains("<para note=\"Keep the scene under a page.\">"));
        let (got, _) = from_osf(&xml).unwrap();
        let note = got
            .blocks
            .iter()
            .find(|b| b.element_type == NonPrintable)
            .unwrap();
        assert_eq!(note.text, "Keep the scene under a page.");
        p.blocks.retain(|b| b.element_type != NonPrintable);
        assert_eq!(
            got.blocks
                .iter()
                .filter(|b| b.element_type == NonPrintable)
                .count(),
            1
        );
    }

    #[test]
    fn accepts_basestyle_from_early_osf_documents() {
        let xml = "<?xml version=\"1.0\"?><document type=\"Open Screenplay Format document\" version=\"12\">\
                   <paragraphs><para><style basestyle=\"Scene Heading\"/><text>INT. ROOM - DAY</text></para>\
                   <para><style basestyle=\"Action\"/><text>Hello.</text></para></paragraphs></document>";
        let (p, _) = from_osf(xml).unwrap();
        assert_eq!(p.blocks[0].element_type, SceneHeading);
        assert_eq!(p.blocks[0].text, "INT. ROOM - DAY");
        assert_eq!(p.blocks[1].element_type, Action);
    }

    #[test]
    fn rejects_non_fadein_input() {
        assert!(from_osf("<notdocument/>").is_err());
        assert!(from_fadein_bytes(b"not a zip").is_err());
        let xml = b"<document/>";
        let zip = zip_document(xml).unwrap();
        assert!(from_fadein_bytes(&zip).is_ok());
    }

    #[test]
    fn zip_entries_are_deterministic() {
        let a = to_fadein_bytes(&sample_project(), &sample_meta()).unwrap();
        let b = to_fadein_bytes(&sample_project(), &sample_meta()).unwrap();
        // The info uuid differs inside the document; everything from the
        // settings onward must be byte-identical.
        let xa = String::from_utf8(unzip_document(&a).unwrap()).unwrap();
        let xb = String::from_utf8(unzip_document(&b).unwrap()).unwrap();
        assert_eq!(
            &xa[xa.find("<settings").unwrap()..],
            &xb[xb.find("<settings").unwrap()..]
        );
        // The ZIP framing (through the DOS timestamp) is fixed too.
        assert_eq!(&a[..14], &b[..14]);
    }

    #[test]
    fn wrapping_parentheticals_matches_the_storage_convention() {
        assert_eq!(wrap_parenthetical("quietly"), "(quietly)");
        assert_eq!(wrap_parenthetical("(quietly)"), "(quietly)");
        assert_eq!(strip_outer_parentheses("(quietly)"), "quietly");
        assert_eq!(
            strip_outer_parentheses("(angry) (sotto)"),
            "(angry) (sotto)"
        );
    }

    #[test]
    fn inline_styles_round_trip_through_osf() {
        let mut blk = ScriptBlock::new(ElementType::Action, "Bold italic under.".into());
        blk.styles = vec![
            StyleSpan {
                start: 0,
                end: 4,
                style: InlineStyle::Bold,
            },
            StyleSpan {
                start: 5,
                end: 11,
                style: InlineStyle::Italic,
            },
            StyleSpan {
                start: 12,
                end: 17,
                style: InlineStyle::Underline,
            },
        ];
        blk.normalize_styles();
        let p = Project {
            title: "T".into(),
            blocks: vec![blk.clone()],
            ..Default::default()
        };
        let bytes = to_fadein_bytes(&p, &sample_meta()).unwrap();
        let (back, _) = from_fadein_bytes(&bytes).unwrap();
        assert_eq!(back.blocks[0].text, blk.text);
        assert_eq!(back.blocks[0].styles, blk.styles);
    }
}
