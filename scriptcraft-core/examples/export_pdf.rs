use scriptcraft_core::{kitsp, pdf};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: export_pdf <file.kitsp> <out.pdf>");
        std::process::exit(1);
    }
    let path = std::path::Path::new(&args[1]);
    let p = kitsp::load_file(path).expect("load");
    let meta = kitsp::load_metadata(path).expect("meta");
    let regular = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../scriptcraft-app/assets/fonts/cour.ttf"
    ))
    .expect("regular font");
    let bold = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../scriptcraft-app/assets/fonts/courb.ttf"
    ))
    .expect("bold font");
    let bytes = pdf::to_pdf_bytes(&p, &meta, &pdf::ExportOptions::default(), &regular, &bold)
        .expect("render");
    std::fs::write(&args[2], bytes).expect("write");
    println!("wrote {}", args[2]);
}
