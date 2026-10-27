//! Final Draft `.fdx` import/export (XML).
//!
//! Emits the widely supported FDX 1 shape (`FinalDraft` → `Content` →
//! `Paragraph`/`Text`), scene numbers as the `Number` attribute on scene
//! headings, a positional title page and revision colours/marks. Notes ride
//! as `General` paragraphs (FD's unformatted element). Revision metadata is
//! written for other tools; importing it is deferred.

use crate::kitsp::Metadata;
use crate::model::{ElementType, Project, ScriptBlock};
use crate::revision::{RevisionColor, RevisionSet};
use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FdxError {
    #[error("xml: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("not a Final Draft document (root <FinalDraft> missing)")]
    NotFinalDraft,
}

/// FDX paragraph type for an element type.
fn fdx_type(t: ElementType) -> &'static str {
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

/// Inverse of [`fdx_type`]; unknown structural types become notes so they
/// never print as body text. Unknown paragraph types are handled by the
/// caller (falls back to Action).
fn element_type(kind: &str) -> Option<ElementType> {
    match kind {
        "Scene Heading" => Some(ElementType::SceneHeading),
        "Cast List" => Some(ElementType::SceneCharacters),
        "Action" => Some(ElementType::Action),
        "Character" => Some(ElementType::Character),
        "Dialogue" => Some(ElementType::Dialogue),
        "Parenthetical" => Some(ElementType::Parenthetical),
        "Shot" => Some(ElementType::Shot),
        "Transition" => Some(ElementType::Transition),
        "Lyrics" => Some(ElementType::Lyrics),
        "General" | "New Act" | "End of Act" => Some(ElementType::NonPrintable),
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
    let v: Vec<String> = raw
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

/// The number written into a scene heading's `Number` attribute: frozen
/// label when numbering is locked, otherwise prefix + start (same rule as
/// the editor/PDF labels, minus the display dot).
fn scene_number(meta: &Metadata, i: usize) -> String {
    if let Some(l) = locked_labels(meta).and_then(|v| v.get(i).cloned()) {
        return l;
    }
    let start: u32 = meta.scene_number_start.trim().parse().unwrap_or(1);
    format!("{}{}", meta.scene_number_prefix.trim(), start + i as u32)
}

fn esc_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn esc_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        // One FDX paragraph is one line; embedded breaks become spaces.
        .replace(['\r', '\n'], " ")
}

fn push_paragraph(
    out: &mut String,
    kind: &str,
    number: Option<&str>,
    revision: Option<RevisionColor>,
    text: &str,
) {
    out.push_str("<Paragraph Type=\"");
    out.push_str(kind);
    out.push('"');
    if let Some(n) = number {
        out.push_str(" Number=\"");
        out.push_str(&esc_attr(n));
        out.push('"');
    }
    if let Some(color) = revision {
        out.push_str(" Revision=\"");
        out.push_str(&revision_id(color).to_string());
        out.push('"');
    }
    out.push_str(">\n<Text>");
    out.push_str(&esc_text(text));
    out.push_str("</Text>\n</Paragraph>\n");
}

/// Stable FDX revision IDs; White (0) is Final Draft's base revision.
fn revision_id(color: RevisionColor) -> u8 {
    match color {
        RevisionColor::Blue => 1,
        RevisionColor::Pink => 2,
        RevisionColor::Yellow => 3,
        RevisionColor::Green => 4,
        RevisionColor::Goldenrod => 5,
        RevisionColor::Buff => 6,
        RevisionColor::Salmon => 7,
        RevisionColor::Cherry => 8,
    }
}

fn push_revision(out: &mut String, color: RevisionColor) {
    let name = color.label();
    out.push_str("<Revision Color=\"");
    out.push_str(name);
    out.push_str("\" FullRevision=\"No\" ID=\"");
    out.push_str(&revision_id(color).to_string());
    out.push_str("\" Mark=\"*\" Name=\"");
    out.push_str(name);
    out.push_str("\" PageColor=\"");
    out.push_str(name);
    out.push_str("\" TextColor=\"");
    out.push_str(name);
    out.push_str("\" Visible=\"Yes\"/>\n");
}

fn push_title_para(out: &mut String, align: &str, text: &str) {
    out.push_str("<Paragraph Alignment=\"");
    out.push_str(align);
    out.push_str("\">\n<Text>");
    out.push_str(&esc_text(text));
    out.push_str("</Text>\n</Paragraph>\n");
}

/// Serialize a project to FDX XML. Title page layout (positional, so our own
/// importer can round-trip it exactly): centered title, genre, `Written by`,
/// author, then one left paragraph per contact line. The draft year is not
/// carried — FD has no single standard title-page slot for it.
pub fn to_fdx(p: &Project, meta: &Metadata) -> String {
    to_fdx_with_revisions(p, meta, &[])
}

pub fn to_fdx_with_revisions(
    p: &Project,
    meta: &Metadata,
    marks: &[Option<RevisionColor>],
) -> String {
    to_fdx_with_revisions_and_latest(p, meta, marks, None)
}

pub fn to_fdx_with_revisions_and_latest(
    p: &Project,
    meta: &Metadata,
    marks: &[Option<RevisionColor>],
    latest: Option<&RevisionSet>,
) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\" ?>\n\
         <FinalDraft DocumentType=\"Script\" Template=\"No\" Version=\"1\">\n\
         <Content>\n",
    );
    let mut scene_i = 0usize;
    for (i, b) in p.blocks.iter().enumerate() {
        let mut number = None;
        if b.element_type == ElementType::SceneHeading {
            number = Some(scene_number(meta, scene_i));
            scene_i += 1;
        }
        push_paragraph(
            &mut out,
            fdx_type(b.element_type),
            number.as_deref(),
            marks.get(i).copied().flatten(),
            &b.text,
        );
    }
    out.push_str("</Content>\n<TitlePage>\n<Content>\n");
    let title = if meta.title.trim().is_empty() {
        p.title.as_str()
    } else {
        meta.title.as_str()
    };
    push_title_para(&mut out, "Center", title.trim());
    push_title_para(&mut out, "Center", meta.genre.trim());
    if !meta.genre.trim().is_empty() || !meta.author.trim().is_empty() {
        push_title_para(&mut out, "Center", "Written by");
    }
    push_title_para(&mut out, "Center", meta.author.trim());
    for line in meta
        .contacts
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
    {
        push_title_para(&mut out, "Left", line);
    }
    if let Some(set) = latest {
        push_title_para(&mut out, "Center", "Revised");
        if !set.name.trim().is_empty() {
            push_title_para(&mut out, "Center", set.name.trim());
        }
        let date = set.started.split_whitespace().next().unwrap_or("");
        if !date.is_empty() {
            push_title_para(&mut out, "Center", date);
        }
    }
    out.push_str("</Content>\n</TitlePage>\n<Revisions>\n");
    out.push_str(
        "<Revision Color=\"White\" FullRevision=\"No\" ID=\"0\" Mark=\"\" Name=\"White\" PageColor=\"White\" TextColor=\"White\" Visible=\"Yes\"/>\n",
    );
    for color in RevisionColor::ALL {
        if marks.iter().flatten().any(|c| *c == color) {
            push_revision(&mut out, color);
        }
    }
    out.push_str("</Revisions>\n</FinalDraft>\n");
    out
}

/// Undo XML text entities (quick_xml hands raw bytes back; CDATA is raw).
fn unescape_text(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// Title-page lines that are credit markers, not content.
fn is_written_by(s: &str) -> bool {
    matches!(
        s.trim().trim_end_matches(':').to_lowercase().as_str(),
        "written by" | "by" | "a screenplay by" | "a play by" | "screenplay by" | "story by"
    )
}

#[derive(Default)]
struct Para {
    kind: String,
    number: String,
    alignment: String,
    text: String,
    notes: Vec<String>,
}

/// Parse an FDX document into a project plus title-page metadata.
pub fn from_fdx(xml: &str) -> Result<(Project, Metadata), FdxError> {
    let mut reader = Reader::from_str(xml);
    // Keep raw text runs (entities are separate `GeneralRef` events) and
    // trim whole paragraph bodies at dispatch instead.
    reader.config_mut().trim_text(false);

    let mut saw_root = false;
    let mut in_title_page = false;
    let mut in_text = false;
    let mut in_note = false;

    let mut para: Option<Para> = None;
    let mut note_buf = String::new();

    let mut blocks: Vec<ScriptBlock> = Vec::new();
    let mut title_paras: Vec<(String, String)> = Vec::new();
    let mut scene_numbers: Vec<String> = Vec::new();
    let mut meta = Metadata::default();

    loop {
        match reader.read_event()? {
            Event::Start(e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if !saw_root {
                    saw_root = true;
                    if name != "FinalDraft" {
                        return Err(FdxError::NotFinalDraft);
                    }
                    continue;
                }
                match name.as_str() {
                    "TitlePage" => in_title_page = true,
                    "Paragraph" => {
                        let mut p = Para::default();
                        for key in ["Type", "Number", "Alignment"] {
                            if let Ok(Some(a)) = e.try_get_attribute(key) {
                                if let Ok(v) = a.decoded_and_normalized_value(
                                    XmlVersion::Implicit1_0,
                                    reader.decoder(),
                                ) {
                                    match key {
                                        "Type" => p.kind = v.into_owned(),
                                        "Number" => p.number = v.into_owned(),
                                        _ => p.alignment = v.into_owned(),
                                    }
                                }
                            }
                        }
                        para = Some(p);
                    }
                    "Text" if para.is_some() && !in_note => in_text = true,
                    "ScriptNote" => {
                        in_note = true;
                        note_buf.clear();
                    }
                    _ => {}
                }
            }
            Event::Empty(e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if name == "Paragraph" && in_title_page {
                    // Self-closing spacer paragraph.
                    let mut p = Para::default();
                    if let Ok(Some(a)) = e.try_get_attribute("Alignment") {
                        if let Ok(v) = a
                            .decoded_and_normalized_value(XmlVersion::Implicit1_0, reader.decoder())
                        {
                            p.alignment = v.into_owned();
                        }
                    }
                    title_paras.push((p.alignment, String::new()));
                }
            }
            Event::GeneralRef(e) => {
                if !(in_note || in_text) {
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
                if in_note {
                    note_buf.push_str(&s);
                } else if let Some(p) = para.as_mut() {
                    p.text.push_str(&s);
                }
            }
            Event::Text(e) => {
                if in_note || in_text {
                    let s = reader.decoder().decode(e.as_ref()).unwrap_or_default();
                    let s = unescape_text(&s);
                    if in_note {
                        note_buf.push_str(&s);
                    } else if let Some(p) = para.as_mut() {
                        p.text.push_str(&s);
                    }
                }
            }
            Event::CData(e) => {
                if in_note || in_text {
                    let s = String::from_utf8_lossy(&e.into_inner()).into_owned();
                    if in_note {
                        note_buf.push_str(&s);
                    } else if let Some(p) = para.as_mut() {
                        p.text.push_str(&s);
                    }
                }
            }
            Event::End(e) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                match name.as_str() {
                    "TitlePage" => in_title_page = false,
                    "Text" => in_text = false,
                    "ScriptNote" => {
                        in_note = false;
                        let note = note_buf.trim().to_string();
                        if let Some(p) = para.as_mut() {
                            if !note.is_empty() {
                                p.notes.push(note);
                            }
                        }
                        note_buf.clear();
                    }
                    "Paragraph" => {
                        let Some(p) = para.take() else { continue };
                        if in_title_page {
                            title_paras.push((p.alignment, p.text));
                            continue;
                        }
                        let text = p.text.trim().to_string();
                        if text.is_empty() && p.notes.is_empty() {
                            continue;
                        }
                        let el = element_type(&p.kind).unwrap_or(ElementType::Action);
                        if el == ElementType::SceneHeading && !p.number.trim().is_empty() {
                            scene_numbers.push(p.number.trim().to_string());
                        }
                        if !text.is_empty() {
                            blocks.push(ScriptBlock::new(el, text));
                        }
                        for note in p.notes {
                            blocks.push(ScriptBlock::new(ElementType::NonPrintable, note));
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
        return Err(FdxError::NotFinalDraft);
    }

    // Title page: first centered line is the title; the marker "Written by"
    // separates the genre/credit slot from the author; left lines are the
    // contacts block. This matches both our export and common FD files.
    let centered: Vec<String> = title_paras
        .iter()
        .filter(|(a, _)| a.eq_ignore_ascii_case("center"))
        .map(|(_, t)| t.trim().to_string())
        .collect();
    let title_idx = centered.iter().position(|t| !t.is_empty());
    if let Some(ti) = title_idx {
        meta.title = centered[ti].clone();
        let after: Vec<&String> = centered[ti + 1..].iter().collect();
        let by_idx = after.iter().position(|t| is_written_by(t));
        match by_idx {
            Some(k) => {
                meta.genre = after[..k]
                    .iter()
                    .find(|t| !t.is_empty())
                    .map(|t| t.to_string())
                    .unwrap_or_default();
                meta.author = after[k + 1..]
                    .iter()
                    .find(|t| !t.is_empty())
                    .map(|t| t.to_string())
                    .unwrap_or_default();
            }
            None => {
                meta.author = after
                    .iter()
                    .find(|t| !t.is_empty())
                    .map(|t| t.to_string())
                    .unwrap_or_default();
            }
        }
    }
    let contacts: Vec<String> = title_paras
        .iter()
        .filter(|(a, _)| !a.eq_ignore_ascii_case("center"))
        .map(|(_, t)| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    meta.contacts = contacts.join("\n");

    // Final Draft numbers are authoritative: freeze them if every scene has
    // one (unnumbered scenes keep the app's computed numbering).
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
    Ok((Project { title, blocks }, meta))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_project() -> Project {
        use ElementType::*;
        let mut p = Project {
            title: "The Morning Ritual".into(),
            blocks: vec![],
        };
        for (t, text) in [
            (SceneHeading, "INT. COFFEE SHOP - MORNING"),
            (SceneCharacters, "MIRA, BARISTA"),
            (Action, "The room hums with morning chatter."),
            (Character, "MIRA"),
            (Parenthetical, "(over the noise)"),
            (Dialogue, "One espresso, please."),
            (Shot, "CLOSE ON THE ESPRESSO MACHINE"),
            (Transition, "CUT TO:"),
            (Lyrics, "Raindrops keep falling..."),
            (NonPrintable, "Keep the scene under a page."),
            (SceneHeading, "EXT. STREET - CONTINUOUS"),
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
            locked_scene_numbers: "*SC3,SC4".into(),
            ..Default::default()
        }
    }

    #[test]
    fn exports_shape_scene_numbers_and_revisions() {
        let p = sample_project();
        let xml = to_fdx(&p, &sample_meta());
        assert!(
            xml.starts_with("<?xml version=\"1.0\"") || xml.starts_with("<?xml version=\"1.0\"")
        );
        assert!(xml.contains("<FinalDraft DocumentType=\"Script\""));
        assert!(xml.contains("<Paragraph Type=\"Scene Heading\" Number=\"SC3\">"));
        assert!(xml.contains("<Paragraph Type=\"Scene Heading\" Number=\"SC4\">"));
        assert!(xml.contains("<Paragraph Type=\"Cast List\">"));
        assert!(xml.contains("<Paragraph Type=\"General\">"));
        assert!(xml.contains("<Text>Keep the scene under a page.</Text>"));
        assert!(xml.contains("<TitlePage>"));
        assert!(xml.contains("<Text>Written by</Text>"));
        assert!(xml.contains("<Paragraph Alignment=\"Left\">\n<Text>mihail@example.com</Text>"));
        assert!(xml.contains("<Revisions>"));
        assert!(xml.ends_with("</FinalDraft>\n"));
    }

    #[test]
    fn exports_revision_colors_and_star_marks() {
        let p = sample_project();
        let mut marks = vec![None; p.blocks.len()];
        marks[2] = Some(RevisionColor::Blue);
        marks[5] = Some(RevisionColor::Pink);
        let xml = to_fdx_with_revisions(&p, &sample_meta(), &marks);

        assert!(xml.contains(
            "<Revision Color=\"Blue\" FullRevision=\"No\" ID=\"1\" Mark=\"*\" Name=\"Blue\""
        ));
        assert!(xml.contains(
            "<Revision Color=\"Pink\" FullRevision=\"No\" ID=\"2\" Mark=\"*\" Name=\"Pink\""
        ));
        assert!(xml.contains("<Paragraph Type=\"Action\" Revision=\"1\">"));
        assert!(xml.contains("<Paragraph Type=\"Dialogue\" Revision=\"2\">"));
        assert!(!xml.contains("Type=\"Scene Heading\" Revision="));
        assert!(!xml.contains("<Paragraph Type=\"Shot\" Revision="));

        let (back, _) = from_fdx(&xml).expect("revisioned FDX still imports");
        let bodies: Vec<_> = back
            .blocks
            .iter()
            .map(|b| (b.element_type, b.text.as_str()))
            .collect();
        let want: Vec<_> = p
            .blocks
            .iter()
            .map(|b| (b.element_type, b.text.as_str()))
            .collect();
        assert_eq!(bodies, want);
    }

    #[test]
    fn title_page_records_latest_revision() {
        let p = sample_project();
        let set = RevisionSet {
            color: RevisionColor::Blue,
            name: "Blue Revision".into(),
            started: "21.10.2026 16:03".into(),
            pages: 1,
            baseline: vec![],
        };
        let xml = to_fdx_with_revisions_and_latest(&p, &sample_meta(), &[], Some(&set));
        assert!(xml.contains("<Text>Revised</Text>"));
        assert!(xml.contains("<Text>Blue Revision</Text>"));
        assert!(xml.contains("<Text>21.10.2026</Text>"));
    }

    #[test]
    fn round_trips_blocks_and_title_page() {
        let p = sample_project();
        let meta = sample_meta();
        let xml = to_fdx(&p, &meta);
        let (q, m) = from_fdx(&xml).unwrap();
        assert_eq!(q.title, "The Morning Ritual");
        assert_eq!(m.title, "The Morning Ritual");
        assert_eq!(m.genre, "Drama");
        assert_eq!(m.author, "M. Hayll");
        assert_eq!(m.contacts, "mihail@example.com\n+1 555 0100");
        assert_eq!(m.locked_scene_numbers, "SC3,SC4");
        assert_eq!(q.blocks.len(), p.blocks.len());
        for (a, b) in p.blocks.iter().zip(q.blocks.iter()) {
            assert_eq!(a.element_type, b.element_type, "type for {:?}", a.text);
            assert_eq!(a.text, b.text);
        }
    }

    #[test]
    fn number_rule_follows_meta() {
        let mut m = Metadata::default();
        assert_eq!(scene_number(&m, 0), "1");
        assert_eq!(scene_number(&m, 4), "5");
        m.scene_number_prefix = "SC".into();
        m.scene_number_start = "7".into();
        assert_eq!(scene_number(&m, 0), "SC7");
        m.locked_scene_numbers = "A*,B".into();
        assert_eq!(scene_number(&m, 0), "A*");
        assert_eq!(scene_number(&m, 1), "B");
        // Beyond the frozen list numbering resumes from the configured start.
        assert_eq!(scene_number(&m, 2), "SC9");
    }

    #[test]
    fn imports_foreign_title_page_and_script_notes() {
        let xml = r##"<?xml version="1.0" encoding="UTF-8" standalone="no" ?>
<FinalDraft DocumentType="Script" Template="No" Version="1">
<Content>
<Paragraph Type="Scene Heading" Number="12"><Text>INT. HALL - NIGHT</Text></Paragraph>
<Paragraph Type="Action"><Text>Rain &amp; thunder.</Text>
<ScriptNote Type="General" Color="#000000" ID="1"><Text>Check continuity.</Text></ScriptNote>
</Paragraph>
<Paragraph Type="Scene Heading" Number="12A"><Text>EXT. STREET - NIGHT</Text></Paragraph>
</Content>
<TitlePage>
<Content>
<Paragraph Alignment="Center"><Text>FOREIGN SCRIPT</Text></Paragraph>
<Paragraph Alignment="Center"><Text></Text></Paragraph>
<Paragraph Alignment="Center"><Text>Written by</Text></Paragraph>
<Paragraph Alignment="Center"><Text>Jane Doe</Text></Paragraph>
<Paragraph Alignment="Right"><Text>jane@example.com</Text></Paragraph>
</Content>
</TitlePage>
</FinalDraft>"##;
        let (p, m) = from_fdx(xml).unwrap();
        assert_eq!(m.title, "FOREIGN SCRIPT");
        assert_eq!(m.author, "Jane Doe");
        assert_eq!(m.genre, "");
        assert_eq!(m.contacts, "jane@example.com");
        assert_eq!(m.locked_scene_numbers, "12,12A");
        use ElementType::*;
        let kinds: Vec<ElementType> = p.blocks.iter().map(|b| b.element_type).collect();
        assert_eq!(
            kinds,
            vec![SceneHeading, Action, NonPrintable, SceneHeading]
        );
        assert_eq!(p.blocks[1].text, "Rain & thunder.");
        assert_eq!(p.blocks[2].text, "Check continuity.");
    }

    #[test]
    fn rejects_non_fdx_roots() {
        assert!(matches!(
            from_fdx("<scenario><action/></scenario>").unwrap_err(),
            FdxError::NotFinalDraft
        ));
        assert!(matches!(
            from_fdx("not xml at all").unwrap_err(),
            FdxError::NotFinalDraft
        ));
        assert!(matches!(from_fdx("").unwrap_err(), FdxError::NotFinalDraft));
    }

    #[test]
    fn imports_sample_fixture() {
        let xml = include_str!("../../fixtures/example.fdx");
        let (p, m) = from_fdx(xml).unwrap();
        assert_eq!(m.title, "THE MORNING RITUAL");
        assert_eq!(m.author, "M. HAYLL");
        assert_eq!(m.genre, "");
        assert_eq!(m.contacts, "mihail@example.com");
        assert_eq!(m.locked_scene_numbers, "1,2");
        assert_eq!(p.scenes().len(), 2);
        use ElementType::*;
        let kinds: Vec<ElementType> = p.blocks.iter().map(|b| b.element_type).collect();
        assert_eq!(
            kinds,
            vec![
                SceneHeading,
                Action,
                NonPrintable,
                Character,
                Parenthetical,
                Dialogue,
                SceneCharacters,
                Transition,
                Shot,
                SceneHeading,
                Action,
                NonPrintable,
                Lyrics,
                NonPrintable,
            ]
        );
        assert_eq!(
            p.blocks[10].text,
            "Mira steps into the rain & opens her umbrella."
        );
        assert_eq!(p.blocks[2].text, "Check the shop name with production.");
        // Exports again cleanly (our notes ride as FD "General" paragraphs).
        let xml2 = to_fdx(&p, &m);
        assert!(xml2.contains("<Paragraph Type=\"General\">"));
        let (q, _) = from_fdx(&xml2).unwrap();
        assert_eq!(q.blocks.len(), p.blocks.len());
    }
}
