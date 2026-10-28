//! A host-owned window for a plugin's editor.
//!
//! On Linux/X11 the editor is created as a *child* of the application window, so
//! it is clipped to, and moves with, the region the UI reserves for it. The
//! plugin creates its own X connection and handles the input events for the
//! child window it puts inside ours, so we never have to run a second event
//! loop.
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

    /// A child window of the application window, sized in physical pixels.
    pub struct EditorWindow {
        display: *mut xlib::Display,
        window: xlib::Window,
        parent: xlib::Window,
    }

    // X11 windows are plain integers; the connection is used from the UI thread
    // only, which is where the plugin editor is created and destroyed too.
    unsafe impl Send for EditorWindow {}

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
                })
            }
        }

        pub fn handle(&self) -> u64 {
            self.window
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
            if let Ok(library) = xlib() {
                unsafe {
                    (library.XDestroyWindow)(self.display, self.window);
                    (library.XFlush)(self.display);
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

        pub fn handle(&self) -> u64 {
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
