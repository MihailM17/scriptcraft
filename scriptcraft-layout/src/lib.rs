//! Screenplay layout engine shared by the editor (and, eventually, PDF export).
//!
//! Document units: horizontal positions are **millimetres** from the left edge
//! of the template text column; vertical positions are **rows** of the fixed
//! line grid ([`place::ROW_MM`] millimetres tall). Pixels and zoom enter only
//! when painting — layout itself is zoom-independent, so zooming and window
//! resizing can never reflow text.
//!
//! M1 scope: `wrap` and `paginate` are 1:1 ports of the proven
//! `scriptcraft-core::editing` algorithms, extended with source char spans and
//! a `place` step that resolves pages to concrete `PlacedLine`s for the
//! upcoming `ScriptView` widget. `tests/parity.rs` pins the port to the old
//! engine on the A95 fixture and a synthetic corpus, so no line break or page
//! break can drift while the editor is being rebuilt.

pub mod paginate;
pub mod pdf;
pub mod place;
pub mod wrap;

pub use paginate::{paginate, paginate_lines, Chunk, Page};
pub use place::{
    layout, layout_document_view, layout_with_page_lock, relayout_from, DocLayout, LineRole,
    PageLayout, PlacedLine, SourceSpan, TemplateGeometry, MM_PER_CELL, ROW_MM, RUSSIAN_GEOMETRY,
    TEXT_W_MM,
};
pub use wrap::{wrap_lines, wrap_paragraph, wrap_rows, WrappedLine};
