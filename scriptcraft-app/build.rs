//! Windows only: embed the application icon and version strings in the .exe.
//!
//! Explorer reads the file icon from the executable's resources, not from the
//! window icon the app sets at runtime, so without this the built
//! `ScriptCraft.exe` shows the generic blank executable icon.

fn main() {
    println!("cargo:rerun-if-changed=assets/ScriptCraft.ico");

    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/ScriptCraft.ico");
        res.set("FileDescription", "ScriptCraft");
        res.set("ProductName", "ScriptCraft");
        res.set("LegalCopyright", "Apache-2.0");
        if let Err(err) = res.compile() {
            panic!("failed to embed the Windows icon resource: {err}");
        }
    }
}
