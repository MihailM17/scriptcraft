//! Title-page editor: a KIT-compatible form next to the exact PDF page.
//!
//! The preview is not a second renderer. It consumes
//! [`scriptcraft_layout::pdf::layout_title_page_with_latest`] — the same line
//! list the exporter paints — so a field can never look right in the form and
//! wrong in the PDF.

use egui::{Align2, Color32, FontId, Rect, Sense, Stroke, StrokeKind, Ui, Vec2};
use scriptcraft_core::kitsp::Metadata;
use scriptcraft_core::revision::RevisionSet;
use scriptcraft_layout::pdf::{
    layout_title_page_with_latest, title_page_line_y_mm, PrintLine, TITLE_PAGE_METRICS,
};

use crate::app::{bold_mono, kit_field, kit_field_multi, kit_section, MONO};
use crate::theme::kit;

/// Lines the title-page preview paints (tests also use this).
pub fn preview_lines(meta: &Metadata, latest: Option<&RevisionSet>) -> Vec<PrintLine> {
    layout_title_page_with_latest(meta, latest)
}

/// Draw the title-page form and its live A4 preview. Returns true when a
/// field changed (the caller marks the metadata dirty and saves it through the
/// existing title-page path).
pub fn title_page_editor(ui: &mut Ui, meta: &mut Metadata, latest: Option<&RevisionSet>) -> bool {
    let wide = ui.available_width() >= 620.0;
    let mut changed = false;
    if wide {
        let height = ui.available_height();
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(360.0, height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_min_width(360.0);
                    ui.set_max_width(360.0);
                    egui::ScrollArea::vertical()
                        .id_salt("title_page_form")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            changed |= form(ui, meta, latest);
                        });
                },
            );
            ui.add_space(16.0);
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    preview(ui, meta, latest, height);
                },
            );
        });
    } else {
        egui::ScrollArea::vertical()
            .id_salt("title_page_form")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                changed |= form(ui, meta, latest);
            });
        ui.add_space(12.0);
        preview(ui, meta, latest, 420.0);
    }
    changed
}

fn form(ui: &mut Ui, meta: &mut Metadata, latest: Option<&RevisionSet>) -> bool {
    let mut dirty = false;
    kit_section(ui, "Title Page");
    ui.label(
        egui::RichText::new(
            "These are KIT's scenario_data fields. The preview is the exact PDF page; \
             untouched rich text (logline/synopsis) is never rewritten.",
        )
        .size(11.0)
        .color(kit().disabled_text),
    );
    ui.add_space(10.0);
    dirty |= kit_field(ui, "Title", &mut meta.title);
    dirty |= kit_field(ui, "Credit / Genre", &mut meta.genre);
    dirty |= kit_field(ui, "Author", &mut meta.author);
    dirty |= kit_field(ui, "Director (ScriptCraft)", &mut meta.director);
    dirty |= kit_field(ui, "Source / Additional", &mut meta.additional_info);
    dirty |= kit_field_multi(ui, "Contact", &mut meta.contacts, 3);
    dirty |= kit_field(ui, "Draft Date", &mut meta.year);

    kit_section(ui, "Revision");
    if let Some(set) = latest {
        let (r, g, b) = set.color.rgb();
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
            ui.painter()
                .rect_filled(rect, 2.0, Color32::from_rgb(r, g, b));
            ui.label(
                egui::RichText::new(format!("{} — {}", set.color.label(), set.name))
                    .size(13.0)
                    .strong(),
            );
        });
        ui.label(
            egui::RichText::new(format!("Started {}", set.started))
                .size(11.0)
                .color(kit().disabled_text),
        );
        ui.label(
            egui::RichText::new(
                "The Revised block (colour, name and date) prints on the title page \
                 and in FDX while this revision is active.",
            )
            .size(11.0)
            .color(kit().disabled_text),
        );
    } else {
        ui.label(
            egui::RichText::new("No active revision. Start one in Research ▸ Versions.").size(12.0),
        );
    }

    ui.add_space(14.0);
    kit_section(ui, "Script Page Header / Footer");
    dirty |= kit_field(ui, "Header", &mut meta.header);
    dirty |= kit_field(ui, "Footer", &mut meta.footer);
    ui.label(
        egui::RichText::new(
            "Tokens: {revision} {date} {page}. These apply to script pages, not the title page.",
        )
        .size(11.0)
        .color(kit().disabled_text),
    );
    dirty
}

fn preview(ui: &mut Ui, meta: &Metadata, latest: Option<&RevisionSet>, height: f32) {
    let lines = preview_lines(meta, latest);
    let avail = Vec2::new(ui.available_width(), height);
    let (outer, _) = ui.allocate_exact_size(avail, Sense::hover());
    let m = TITLE_PAGE_METRICS;
    let scale = ((avail.x - 24.0) / m.page_w_mm)
        .min((avail.y - 24.0) / m.page_h_mm)
        .max(0.05);
    let page = Rect::from_center_size(
        outer.center(),
        Vec2::new(m.page_w_mm * scale, m.page_h_mm * scale),
    );
    let painter = ui.painter();
    painter.rect_filled(
        page.translate(Vec2::new(4.0, 4.0)),
        2.0,
        Color32::from_black_alpha(60),
    );
    painter.rect_filled(page, 0.0, Color32::from_rgb(0xFA, 0xFA, 0xF7));
    painter.rect_stroke(
        page,
        0.0,
        Stroke::new(1.0_f32, Color32::from_black_alpha(50)),
        StrokeKind::Inside,
    );

    let font_px = m.font_pt * (25.4 / 72.0) * scale;
    for (i, line) in lines.iter().enumerate() {
        if line.text.is_empty() {
            continue;
        }
        let x = page.left() + line.x_mm * scale;
        let y_mm = title_page_line_y_mm(i, line);
        let y = page.top() + (m.page_h_mm - y_mm) * scale;
        let font = if line.bold {
            FontId::new(font_px, bold_mono())
        } else {
            FontId::new(font_px, MONO)
        };
        painter.text(
            egui::pos2(x, y + font_px * 0.22),
            Align2::LEFT_BOTTOM,
            &line.text,
            font,
            Color32::BLACK,
        );
        if line.underline {
            let w = line.text.chars().count() as f32 * m.char_mm * scale;
            painter.line_segment(
                [
                    egui::pos2(x, y + 1.2 * scale),
                    egui::pos2(x + w, y + 1.2 * scale),
                ],
                Stroke::new(0.8_f32, Color32::BLACK),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Metadata {
        Metadata {
            title: "A95".into(),
            genre: "Thriller/Horror".into(),
            author: "Mihail Mihaylov".into(),
            additional_info: "Concept by Martin Videv".into(),
            contacts: "mohailmihaylov2004@gmail.com".into(),
            year: "2026".into(),
            ..Default::default()
        }
    }

    #[test]
    fn preview_is_the_pdf_layout() {
        let meta = sample();
        assert_eq!(
            preview_lines(&meta, None),
            scriptcraft_layout::pdf::layout_title_page(&meta)
        );
    }

    #[test]
    fn empty_and_cyrillic_fields_stay_on_the_page() {
        let mut meta = Metadata {
            title: "ЗАГЛАВИЕ НА ФИЛМ".into(),
            author: "ИВАН ПЕТРОВ".into(),
            contacts: "ул. Примерна 1\nСофия".into(),
            ..Default::default()
        };
        let lines = preview_lines(&meta, None);
        assert_eq!(lines[16].text, "ЗАГЛАВИЕ НА ФИЛМ");
        assert!(lines.iter().any(|l| l.text == "ул. Примерна 1"));
        assert!(lines.iter().any(|l| l.text == "София"));
        meta.title.clear();
        let lines = preview_lines(&meta, None);
        assert_eq!(lines[16].text, "");
        assert!(!lines.is_empty());
    }

    #[test]
    fn long_title_wraps_in_the_preview_too() {
        let meta = Metadata {
            title: "THE EXTRAORDINARILY LONG TITLE OF A FILM THAT KEEPS GOING AND GOING".into(),
            ..Default::default()
        };
        let lines = preview_lines(&meta, None);
        let underlined = lines[16..].iter().take_while(|l| l.underline).count();
        assert_eq!(underlined, 2);
    }
}
