//! Route macOS Dock/AppleScript quit through the normal unsaved-changes
//! prompt.
//!
//! AppKit sends `-[NSApplication terminate:]` for Dock ▸ Quit and
//! `osascript ... quit`. winit never turns that into `close_requested`, so
//! without this hook those paths would discard unsaved work. We swizzle
//! `terminate:` on the shared `NSApplication` to record a request and wake
//! egui; the app's next update asks the same Save / Don't Save / Cancel
//! prompt used by the red close button and Cmd+Q.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);
static CONTEXT: OnceLock<egui::Context> = OnceLock::new();

/// Install the quit hook and remember the egui context used to wake the app.
/// Call once, after the eframe context exists. A no-op outside macOS.
pub fn install(ctx: &egui::Context) {
    let _ = CONTEXT.set(ctx.clone());
    #[cfg(target_os = "macos")]
    macos::install();
}

/// Consume a pending native quit request. Called once per frame.
pub fn take_request() -> bool {
    QUIT_REQUESTED.swap(false, Ordering::AcqRel)
}

/// Test hook for the app-level plumbing: behaves like a Dock quit request.
#[doc(hidden)]
pub fn request_for_test() {
    request();
}

fn request() {
    QUIT_REQUESTED.store(true, Ordering::Release);
    if let Some(ctx) = CONTEXT.get() {
        ctx.request_repaint();
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::request;
    use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
    use objc2::sel;
    use std::mem;

    type Terminate = extern "C" fn(&AnyObject, Sel);

    extern "C" fn terminate(_app: &AnyObject, _cmd: Sel) {
        request();
    }

    pub fn install() {
        let Some(class) = AnyClass::get(c"NSApplication") else {
            return;
        };
        let Some(method) = class.instance_method(sel!(terminate:)) else {
            return;
        };
        let hook = unsafe { mem::transmute::<Terminate, Imp>(terminate) };
        #[allow(unpredictable_function_pointer_comparisons)]
        if method.implementation() == hook {
            return;
        }
        // SAFETY: `terminate:` has the signature `void (id, SEL)`; the
        // replacement only sets a flag and wakes the UI loop.
        unsafe { method.set_implementation(hook) };
    }
}
