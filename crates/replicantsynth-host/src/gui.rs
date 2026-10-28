//! A host-owned window for a plugin's editor.
//!
//! On Linux/X11 the editor is created as a *child* of the application window, so
//! it is clipped to, and moves with, the region the UI reserves for it. The
//! plugin creates its own X connection and handles the input events for the
//! child window it puts inside ours, so we never have to run a second event
//! loop.
//!
//! A Wayland session has no X11 parent to give the plugin: a `wl_surface` cannot
//! own an X11 child. There the host creates a *top-level* X11 window of its own
//! instead, which XWayland maps as a separate desktop window. The plugin still
//! gets an X11 window to draw into, so plugins that only implement embedded
//! editors (u-he's CLAP builds, for one) keep their GUI on Wayland too.
//!
//! On platforms we have not implemented embedding for, `open` returns
//! [`PluginError::EmbedUnsupported`] and the caller falls back to a floating
//! plugin window.

use crate::PluginError;

#[cfg(target_os = "linux")]
mod platform {
    use super::PluginError;
    use std::sync::OnceLock;
    use x11_dl::xlib::{self, Xlib};

    fn xlib() -> Result<&'static Xlib, PluginError> {
        static XLIB: OnceLock<Option<Xlib>> = OnceLock::new();
        XLIB.get_or_init(|| Xlib::open().ok())
            .as_ref()
            .ok_or_else(|| PluginError::Embed("libX11 is not available".to_string()))
    }

    /// A child window of the application window, or a host-owned top-level
    /// window when the session has no X11 parent to embed into. Sized in
    /// physical pixels.
    pub struct EditorWindow {
        display: *mut xlib::Display,
        window: xlib::Window,
        parent: xlib::Window,
        /// `(WM_PROTOCOLS, WM_DELETE_WINDOW)` for a detached window, so the
        /// window manager's close button reaches us instead of tearing down the
        /// plugin's parent from under it. `None` for an embedded child.
        close_protocol: Option<(xlib::Atom, xlib::Atom)>,
        /// Set once the window is known to be gone, so `Drop` never destroys an
        /// id the X server has already freed (that is an X error, and Xlib's
        /// default handler exits the process).
        destroyed: std::cell::Cell<bool>,
    }

    // X11 windows are plain integers; the connection is used from the UI thread
    // only, which is where the plugin editor is created and destroyed too.
    unsafe impl Send for EditorWindow {}

    /// Tell the window manager the smallest size that shows the whole editor.
    ///
    /// A tiling compositor sizes a new window by its own rules; without a minimum
    /// it can hand the plugin window less room than its editor needs, and the
    /// editor is then clipped instead of drawn. Only the minimum is pinned, so a
    /// resizable editor can still grow.
    unsafe fn pin_minimum_size(
        library: &Xlib,
        display: *mut xlib::Display,
        window: xlib::Window,
        width: u32,
        height: u32,
    ) {
        let mut hints: xlib::XSizeHints = std::mem::zeroed();
        hints.flags = xlib::PMinSize;
        hints.min_width = width.max(16) as i32;
        hints.min_height = height.max(16) as i32;
        (library.XSetWMNormalHints)(display, window, &mut hints);
    }

    impl EditorWindow {
        pub fn open(
            parent: u64,
            _title: &str,
            width: u32,
            height: u32,
            x: i32,
            y: i32,
        ) -> Result<Self, PluginError> {
            let library = xlib()?;
            unsafe {
                let display = (library.XOpenDisplay)(std::ptr::null());
                if display.is_null() {
                    return Err(PluginError::Embed("cannot open the X display".to_string()));
                }
                let parent = parent as xlib::Window;
                let window = (library.XCreateSimpleWindow)(
                    display,
                    parent,
                    x.max(0),
                    y.max(0),
                    width.max(16),
                    height.max(16),
                    0,
                    0,
                    0x101018,
                );
                if window == 0 {
                    (library.XCloseDisplay)(display);
                    return Err(PluginError::Embed("XCreateSimpleWindow failed".to_string()));
                }
                (library.XMapWindow)(display, window);
                (library.XFlush)(display);
                Ok(Self {
                    display,
                    window,
                    parent,
                    close_protocol: None,
                    destroyed: std::cell::Cell::new(false),
                })
            }
        }

        /// A top-level window the window manager owns, for sessions where the
        /// application window cannot parent an X11 child (Wayland).
        ///
        /// The plugin's editor is parented to it exactly as it would be to an
        /// embedded frame, so the plugin never learns the difference.
        pub fn open_detached(title: &str, width: u32, height: u32) -> Result<Self, PluginError> {
            let library = xlib()?;
            unsafe {
                let display = (library.XOpenDisplay)(std::ptr::null());
                if display.is_null() {
                    return Err(PluginError::Embed(
                        "cannot open the X display (no XWayland?)".to_string(),
                    ));
                }
                let root = (library.XDefaultRootWindow)(display);
                let window = (library.XCreateSimpleWindow)(
                    display,
                    root,
                    0,
                    0,
                    width.clamp(160, 4096),
                    height.clamp(120, 2400),
                    0,
                    0,
                    0x101018,
                );
                if window == 0 {
                    (library.XCloseDisplay)(display);
                    return Err(PluginError::Embed("XCreateSimpleWindow failed".to_string()));
                }
                if let Ok(title) = std::ffi::CString::new(title) {
                    (library.XStoreName)(display, window, title.as_ptr());
                }
                let protocols = (library.XInternAtom)(display, c"WM_PROTOCOLS".as_ptr(), 0);
                let mut delete = (library.XInternAtom)(display, c"WM_DELETE_WINDOW".as_ptr(), 0);
                (library.XSetWMProtocols)(display, window, &mut delete, 1);
                // A class of its own, so desktops can group the editor windows and
                // a tiling compositor can float them with a single rule.
                if let (Ok(name), Ok(class)) = (
                    std::ffi::CString::new("plugin-editor"),
                    std::ffi::CString::new("ReplicantSynth"),
                ) {
                    // X copies both strings into a property; it does not write
                    // through the pointers.
                    let mut hint = xlib::XClassHint {
                        res_name: name.as_ptr() as *mut std::os::raw::c_char,
                        res_class: class.as_ptr() as *mut std::os::raw::c_char,
                    };
                    (library.XSetClassHint)(display, window, &mut hint);
                }
                // A destroy we did not ask for still has to be noticed before
                // `Drop` tries to free the window itself.
                (library.XSelectInput)(display, window, xlib::StructureNotifyMask);
                pin_minimum_size(library, display, window, width, height);
                (library.XMapWindow)(display, window);
                (library.XFlush)(display);
                Ok(Self {
                    display,
                    window,
                    parent: root,
                    close_protocol: Some((protocols, delete)),
                    destroyed: std::cell::Cell::new(false),
                })
            }
        }

        /// True when this window is the editor's own top-level window rather
        /// than a child of the application window.
        pub fn is_detached(&self) -> bool {
            self.close_protocol.is_some()
        }

        /// Has the window manager asked to close this detached window, or
        /// destroyed it outright?
        ///
        /// Only detached windows can answer yes. An embedded child has no title
        /// bar and no protocol of its own.
        pub fn close_requested(&self) -> bool {
            let Some((protocols, delete)) = self.close_protocol else {
                return false;
            };
            if self.destroyed.get() {
                return true;
            }
            let Ok(library) = xlib() else {
                return false;
            };
            unsafe {
                loop {
                    let mut event: xlib::XEvent = std::mem::zeroed();
                    let found = (library.XCheckTypedWindowEvent)(
                        self.display,
                        self.window,
                        xlib::ClientMessage,
                        &mut event,
                    );
                    if found == 0 {
                        break;
                    }
                    let message = event.client_message;
                    if message.message_type == protocols
                        && message.format == 32
                        && message.data.as_longs()[0] as xlib::Atom == delete
                    {
                        // A close *request*: the window is still ours to free, so
                        // `destroyed` stays false and `Drop` destroys it.
                        return true;
                    }
                }
                let mut event: xlib::XEvent = std::mem::zeroed();
                let destroyed = (library.XCheckTypedWindowEvent)(
                    self.display,
                    self.window,
                    xlib::DestroyNotify,
                    &mut event,
                );
                if destroyed != 0 {
                    self.destroyed.set(true);
                }
                self.destroyed.get()
            }
        }

        /// The X11 window id.
        ///
        /// XIDs are 32-bit even on a 64-bit server, and `c_ulong` is 32 bits on
        /// Windows and 64 on Unix, so returning a fixed `u32` keeps the callers
        /// identical on every platform.
        pub fn handle(&self) -> u32 {
            self.window as u32
        }

        pub fn resize(&self, width: u32, height: u32) {
            if let Ok(library) = xlib() {
                unsafe {
                    (library.XResizeWindow)(
                        self.display,
                        self.window,
                        width.max(16),
                        height.max(16),
                    );
                    if self.is_detached() {
                        pin_minimum_size(library, self.display, self.window, width, height);
                    }
                    (library.XFlush)(self.display);
                }
            }
        }

        pub fn move_to(&self, x: i32, y: i32) {
            if let Ok(library) = xlib() {
                unsafe {
                    (library.XMoveWindow)(self.display, self.window, x.max(0), y.max(0));
                    (library.XFlush)(self.display);
                }
            }
        }

        pub fn show(&self) {
            if let Ok(library) = xlib() {
                unsafe {
                    (library.XMapWindow)(self.display, self.window);
                    (library.XFlush)(self.display);
                }
            }
        }

        pub fn hide(&self) {
            if let Ok(library) = xlib() {
                unsafe {
                    (library.XUnmapWindow)(self.display, self.window);
                    (library.XFlush)(self.display);
                }
            }
        }

        pub fn parent(&self) -> u64 {
            self.parent
        }
    }

    impl Drop for EditorWindow {
        fn drop(&mut self) {
            // Drain anything the server already queued for this window, so a
            // window the window manager destroyed is not freed a second time.
            if self.close_protocol.is_some() {
                self.close_requested();
            }
            if let Ok(library) = xlib() {
                unsafe {
                    if !self.destroyed.get() {
                        (library.XDestroyWindow)(self.display, self.window);
                        (library.XFlush)(self.display);
                    }
                    (library.XCloseDisplay)(self.display);
                }
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod platform {
    use super::PluginError;

    /// Placeholder for platforms where embedding is not implemented yet.
    pub struct EditorWindow {
        parent: u64,
    }

    impl EditorWindow {
        pub fn open(
            _parent: u64,
            _title: &str,
            _width: u32,
            _height: u32,
            _x: i32,
            _y: i32,
        ) -> Result<Self, PluginError> {
            Err(PluginError::EmbedUnsupported)
        }

        pub fn open_detached(_title: &str, _width: u32, _height: u32) -> Result<Self, PluginError> {
            Err(PluginError::EmbedUnsupported)
        }

        pub fn is_detached(&self) -> bool {
            false
        }

        pub fn close_requested(&self) -> bool {
            false
        }

        pub fn handle(&self) -> u32 {
            0
        }

        pub fn resize(&self, _width: u32, _height: u32) {}

        pub fn move_to(&self, _x: i32, _y: i32) {}

        pub fn show(&self) {}

        pub fn hide(&self) {}

        pub fn parent(&self) -> u64 {
            self.parent
        }
    }
}

pub use platform::EditorWindow;

/// The rectangular region, in physical pixels relative to the application
/// window's client area, that an embedded editor occupies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EditorRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
