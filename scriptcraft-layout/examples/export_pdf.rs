use scriptcraft_core::kitsp;
use scriptcraft_layout::pdf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: export_pdf <file.kitsp> <out.pdf> [--no-title]");
        std::process::exit(1);
    }
    let path = std::path::Path::new(&args[1]);
    let p = kitsp::load_file(path).expect("load");
    let meta = kitsp::load_metadata(path).expect("meta");
    let read = |name: &str| {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../scriptcraft-app/assets/fonts")
                .join(name),
        )
        .expect("font")
    };
    let fonts = pdf::ExportFonts {
        regular: &read("CourierPrime-Regular.ttf"),
        bold: &read("CourierPrime-Bold.ttf"),
        fallback_regular: &read("IBMPlexMono-Regular.ttf"),
        fallback_bold: &read("IBMPlexMono-Bold.ttf"),
    };
    let mut opts = pdf::ExportOptions::default();
    if args.iter().any(|a| a == "--no-title") {
        opts.title_page = false;
    }
    let bytes = pdf::to_pdf_bytes(&p, &meta, &opts, &fonts).expect("render");
    std::fs::write(&args[2], bytes).expect("write");
    println!("wrote {}", args[2]);
}
