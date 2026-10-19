use scriptcraft_core::kitsp;
use scriptcraft_layout::pdf;

fn main() {
    let path =
        "/Users/mihailmihaylov/Documents/3D_ Game_Assets/../Writing_Scripts/KITScenarist/A95.kitsp";
    let path = std::path::Path::new(path);
    let path = if path.exists() {
        path.to_path_buf()
    } else {
        std::env::args()
            .nth(1)
            .expect("usage: dump_pdf <file.kitsp>")
            .into()
    };
    let p = kitsp::load_file(&path).expect("load");
    let meta = kitsp::load_metadata(&path).expect("meta");
    let opts = pdf::ExportOptions::default();
    println!("title={:?} author={:?}", meta.title, meta.author);
    let pages = pdf::layout_pages(&p, &meta, &opts);
    println!("script pages: {}", pages.len());
    let tp = pdf::layout_title_page(&meta);
    println!("title page lines: {}", tp.len());
    for (i, l) in tp.iter().enumerate() {
        if !l.text.is_empty() {
            println!("  line {}: {:?}", i + 1, l.text);
        }
    }
    // Show first script page lines for sanity.
    for (i, l) in pages[0].iter().take(8).enumerate() {
        println!("  p1[{}]: {:?}", i, l.text);
    }
}
