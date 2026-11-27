//! Route macOS Dock/AppleScript quit AND Finder "Open With" / double-click
//! document events through ScriptCraft's normal app lifecycle.
//!
//! - AppKit sends `-[NSApplication terminate:]` for Dock ▸ Quit and
//!   `osascript ... quit`. winit never turns that into `close_requested`, so
//!   without this hook those paths would discard unsaved work.
//! - AppKit sends `application:openURLs:` / `application:openFiles:` /
//!   `application:openFile:` to `NSApplication.delegate` when a user
//!   double-clicks a `.kitsp`, `.fdx`, `.fadein`, or `.fountain` file in
//!   Finder or picks "Open With ▸ ScriptCraft". winit's delegate does not
//!   implement those methods, causing macOS to show *"ScriptCraft cannot open
//!   files in the ... format."* We hook `setDelegate:` early (before
//!   `eframe::run_native`) and attach those delegate methods onto winit's
//!   delegate class, queuing opened paths for the UI loop.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);
static CONTEXT: OnceLock<egui::Context> = OnceLock::new();
static OPEN_FILES: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Install early macOS hooks (before `eframe::run_native`) so cold-launch
/// Finder double-click / "Open With" document events are captured as soon as
/// winit registers its `NSApplicationDelegate`. A no-op outside macOS.
pub fn install_early() {
    #[cfg(target_os = "macos")]
    macos::install_early();
}

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

/// Drain any file paths delivered by macOS Finder ("Open With" / double-click).
pub fn take_open_files() -> Vec<PathBuf> {
    match OPEN_FILES.lock() {
        Ok(mut guard) => std::mem::take(&mut *guard),
        Err(poison) => std::mem::take(&mut *poison.into_inner()),
    }
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
fn enqueue_open_path(path: PathBuf) {
    if let Ok(mut guard) = OPEN_FILES.lock() {
        guard.push(path);
    }
    if let Some(ctx) = CONTEXT.get() {
        ctx.request_repaint();
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{enqueue_open_path, request};
    use objc2::ffi::{class_addMethod, object_getClass};
    use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
    use objc2::{msg_send, sel};
    use std::ffi::{c_char, c_void, CStr};
    use std::mem;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicPtr, Ordering};

    type Terminate = extern "C" fn(&AnyObject, Sel);
    type SetDelegate = extern "C" fn(*mut AnyObject, Sel, *mut AnyObject);
    type OpenUrls = extern "C" fn(*mut AnyObject, Sel, *mut AnyObject, *mut AnyObject);
    type OpenFiles = extern "C" fn(*mut AnyObject, Sel, *mut AnyObject, *mut AnyObject);
    type OpenFile = extern "C" fn(*mut AnyObject, Sel, *mut AnyObject, *mut AnyObject) -> Bool;

    static ORIG_SET_DELEGATE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

    extern "C" fn terminate(_app: &AnyObject, _cmd: Sel) {
        request();
    }

    unsafe fn nsstring_to_path(ns_str: *mut AnyObject) -> Option<PathBuf> {
        if ns_str.is_null() {
            return None;
        }
        let cstr: *const c_char = msg_send![ns_str, UTF8String];
        if cstr.is_null() {
            return None;
        }
        let s = CStr::from_ptr(cstr).to_string_lossy();
        if s.is_empty() {
            None
        } else {
            Some(PathBuf::from(s.as_ref()))
        }
    }

    extern "C" fn application_open_urls(
        _this: *mut AnyObject,
        _cmd: Sel,
        _app: *mut AnyObject,
        urls: *mut AnyObject,
    ) {
        if urls.is_null() {
            return;
        }
        unsafe {
            let count: usize = msg_send![urls, count];
            for i in 0..count {
                let url: *mut AnyObject = msg_send![urls, objectAtIndex: i];
                if url.is_null() {
                    continue;
                }
                let is_file: Bool = msg_send![url, isFileURL];
                if is_file.as_bool() {
                    let path_ns: *mut AnyObject = msg_send![url, path];
                    if let Some(p) = nsstring_to_path(path_ns) {
                        enqueue_open_path(p);
                    }
                }
            }
        }
    }

    extern "C" fn application_open_files(
        _this: *mut AnyObject,
        _cmd: Sel,
        app: *mut AnyObject,
        filenames: *mut AnyObject,
    ) {
        if !filenames.is_null() {
            unsafe {
                let count: usize = msg_send![filenames, count];
                for i in 0..count {
                    let path_ns: *mut AnyObject = msg_send![filenames, objectAtIndex: i];
                    if let Some(p) = nsstring_to_path(path_ns) {
                        enqueue_open_path(p);
                    }
                }
            }
        }
        if !app.is_null() {
            unsafe {
                // NSApplicationDelegateReplySuccess = 0
                let _: () = msg_send![app, replyToOpenOrPrint: 0usize];
            }
        }
    }

    extern "C" fn application_open_file(
        _this: *mut AnyObject,
        _cmd: Sel,
        _app: *mut AnyObject,
        filename: *mut AnyObject,
    ) -> Bool {
        unsafe {
            if let Some(p) = nsstring_to_path(filename) {
                enqueue_open_path(p);
                return Bool::YES;
            }
        }
        Bool::NO
    }

    unsafe fn attach_open_handlers(delegate: *mut AnyObject) {
        if delegate.is_null() {
            return;
        }
        let cls = object_getClass(delegate) as *mut AnyClass;
        if cls.is_null() {
            return;
        }
        let imp_urls = mem::transmute::<OpenUrls, Imp>(application_open_urls);
        let _ = class_addMethod(
            cls,
            sel!(application:openURLs:),
            imp_urls,
            c"v@:@@".as_ptr(),
        );
        let imp_files = mem::transmute::<OpenFiles, Imp>(application_open_files);
        let _ = class_addMethod(
            cls,
            sel!(application:openFiles:),
            imp_files,
            c"v@:@@".as_ptr(),
        );
        let imp_file = mem::transmute::<OpenFile, Imp>(application_open_file);
        let _ = class_addMethod(
            cls,
            sel!(application:openFile:),
            imp_file,
            c"B@:@@".as_ptr(),
        );
    }

    extern "C" fn hooked_set_delegate(app: *mut AnyObject, cmd: Sel, delegate: *mut AnyObject) {
        unsafe {
            attach_open_handlers(delegate);
            let orig_ptr = ORIG_SET_DELEGATE.load(Ordering::Acquire);
            if !orig_ptr.is_null() {
                let orig: SetDelegate = mem::transmute(orig_ptr);
                orig(app, cmd, delegate);
            }
        }
    }

    pub fn install_early() {
        let Some(class) = AnyClass::get(c"NSApplication") else {
            return;
        };
        let Some(method) = class.instance_method(sel!(setDelegate:)) else {
            return;
        };
        let hook = unsafe { mem::transmute::<SetDelegate, Imp>(hooked_set_delegate) };
        let current = method.implementation();
        #[allow(unpredictable_function_pointer_comparisons)]
        if current == hook {
            return;
        }
        ORIG_SET_DELEGATE.store(current as *mut c_void, Ordering::Release);
        unsafe { method.set_implementation(hook) };
    }

    pub fn install() {
        let Some(class) = AnyClass::get(c"NSApplication") else {
            return;
        };
        // Attach open-document handlers on the current delegate too, in case
        // `setDelegate:` ran before `install_early()`.
        unsafe {
            let app: *mut AnyObject = msg_send![class, sharedApplication];
            if !app.is_null() {
                let delegate: *mut AnyObject = msg_send![app, delegate];
                attach_open_handlers(delegate);
            }
        }
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
