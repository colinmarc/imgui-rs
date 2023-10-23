//! This crate provides a winit-based backend platform for imgui-rs.
//!
//! A backend platform handles window/input device events and manages their
//! state.
//!
//! # Using the library
//!
//! There are five things you need to do to use this library correctly:
//!
//! 1. Initialize a `WinitPlatform` instance
//! 2. Attach it to a winit `Window`
//! 3. Pass events to the platform (every frame)
//! 4. Call frame preparation callback (every frame)
//! 5. Call render preparation callback (every frame)
//!
//! ## Complete example (without a renderer)
//!
//! ```no_run
//! use imgui::Context;
//! use imgui_winit_support::{HiDpiMode, WinitPlatform};
//! use std::time::Instant;
//! use winit::event::{Event, WindowEvent};
//! use winit::event_loop::{ControlFlow, EventLoop};
//! use winit::window::Window;
//!
//! let mut event_loop = EventLoop::new();
//! let mut window = Window::new(&event_loop).unwrap();
//!
//! let mut imgui = Context::create();
//! // configure imgui-rs Context if necessary
//!
//! let mut platform = WinitPlatform::init(&mut imgui); // step 1
//! platform.attach_window(imgui.io_mut(), &window, HiDpiMode::Default); // step 2
//!
//! let mut last_frame = Instant::now();
//! let mut run = true;
//! event_loop.run(move |event, _, control_flow| {
//!     match event {
//!         Event::NewEvents(_) => {
//!             // other application-specific logic
//!             let now = Instant::now();
//!             imgui.io_mut().update_delta_time(now - last_frame);
//!             last_frame = now;
//!         },
//!         Event::MainEventsCleared => {
//!             // other application-specific logic
//!             platform.prepare_frame(imgui.io_mut(), &window) // step 4
//!                 .expect("Failed to prepare frame");
//!             window.request_redraw();
//!         }
//!         Event::RedrawRequested(_) => {
//!             let ui = imgui.frame();
//!             // application-specific rendering *under the UI*
//!
//!             // construct the UI
//!
//!             platform.prepare_render(&ui, &window); // step 5
//!             // render the UI with a renderer
//!             let draw_data = imgui.render();
//!             // renderer.render(..., draw_data).expect("UI rendering failed");
//!
//!             // application-specific rendering *over the UI*
//!         },
//!         Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
//!             *control_flow = ControlFlow::Exit;
//!         }
//!         // other application-specific event handling
//!         event => {
//!             platform.handle_event(imgui.io_mut(), &window, &event); // step 3
//!             // other application-specific event handling
//!         }
//!     }
//! })
//! ```

use imgui::{self, BackendFlags, ConfigFlags, Context, Io, Ui};
use std::cmp::Ordering;

// Re-export winit to make it easier for users to use the correct version.
pub use winit;
use winit::dpi::{LogicalPosition, LogicalSize};

use winit::{
    error::ExternalError,
    event::{ElementState, Event, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent},
    keyboard::Key as WinitKey,
    keyboard::KeyLocation,
    keyboard::NamedKey,
    platform::modifier_supplement::KeyEventExtModifierSupplement,
    window::{CursorIcon as MouseCursor, Window},
};

/// winit backend platform state
#[derive(Debug)]
pub struct WinitPlatform {
    hidpi_mode: ActiveHiDpiMode,
    hidpi_factor: f64,
    cursor_cache: Option<CursorSettings>,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
struct CursorSettings {
    cursor: Option<imgui::MouseCursor>,
    draw_cursor: bool,
}

fn to_winit_cursor(cursor: imgui::MouseCursor) -> MouseCursor {
    match cursor {
        imgui::MouseCursor::Arrow => MouseCursor::Default,
        imgui::MouseCursor::TextInput => MouseCursor::Text,
        imgui::MouseCursor::ResizeAll => MouseCursor::Move,
        imgui::MouseCursor::ResizeNS => MouseCursor::NsResize,
        imgui::MouseCursor::ResizeEW => MouseCursor::EwResize,
        imgui::MouseCursor::ResizeNESW => MouseCursor::NeswResize,
        imgui::MouseCursor::ResizeNWSE => MouseCursor::NwseResize,
        imgui::MouseCursor::Hand => MouseCursor::Pointer,
        imgui::MouseCursor::NotAllowed => MouseCursor::NotAllowed,
    }
}

impl CursorSettings {
    fn apply(&self, window: &Window) {
        match self.cursor {
            Some(mouse_cursor) if !self.draw_cursor => {
                window.set_cursor_visible(true);
                window.set_cursor_icon(to_winit_cursor(mouse_cursor));
            }
            _ => window.set_cursor_visible(false),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum ActiveHiDpiMode {
    Default,
    Rounded,
    Locked,
}

/// DPI factor handling mode.
///
/// Applications that use imgui-rs might want to customize the used DPI factor and not use
/// directly the value coming from winit.
///
/// **Note: if you use a mode other than default and the DPI factor is adjusted, winit and imgui-rs
/// will use different logical coordinates, so be careful if you pass around logical size or
/// position values.**
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum HiDpiMode {
    /// The DPI factor from winit is used directly without adjustment
    Default,
    /// The DPI factor from winit is rounded to an integer value.
    ///
    /// This prevents the user interface from becoming blurry with non-integer scaling.
    Rounded,
    /// The DPI factor from winit is ignored, and the included value is used instead.
    ///
    /// This is useful if you want to force some DPI factor (e.g. 1.0) and not care about the value
    /// coming from winit.
    Locked(f64),
}

impl HiDpiMode {
    fn apply(&self, hidpi_factor: f64) -> (ActiveHiDpiMode, f64) {
        match *self {
            HiDpiMode::Default => (ActiveHiDpiMode::Default, hidpi_factor),
            HiDpiMode::Rounded => (ActiveHiDpiMode::Rounded, hidpi_factor.round()),
            HiDpiMode::Locked(value) => (ActiveHiDpiMode::Locked, value),
        }
    }
}

fn to_imgui_mouse_button(button: MouseButton) -> Option<imgui::MouseButton> {
    match button {
        MouseButton::Left | MouseButton::Other(0) => Some(imgui::MouseButton::Left),
        MouseButton::Right | MouseButton::Other(1) => Some(imgui::MouseButton::Right),
        MouseButton::Middle | MouseButton::Other(2) => Some(imgui::MouseButton::Middle),
        MouseButton::Other(3) => Some(imgui::MouseButton::Extra1),
        MouseButton::Other(4) => Some(imgui::MouseButton::Extra2),
        _ => None,
    }
}

fn to_imgui_key(logical_key: WinitKey, location: KeyLocation) -> Option<imgui::Key> {
    use KeyLocation::*;
    use NamedKey::*;

    match logical_key {
        WinitKey::Named(named_key) => match (named_key, location) {
            (Tab, _) => Some(imgui::Key::Tab),
            (ArrowLeft, _) => Some(imgui::Key::LeftArrow),
            (ArrowRight, _) => Some(imgui::Key::RightArrow),
            (ArrowUp, _) => Some(imgui::Key::UpArrow),
            (ArrowDown, _) => Some(imgui::Key::DownArrow),
            (PageUp, _) => Some(imgui::Key::PageUp),
            (PageDown, _) => Some(imgui::Key::PageDown),
            (Home, _) => Some(imgui::Key::Home),
            (End, _) => Some(imgui::Key::End),
            (Insert, _) => Some(imgui::Key::Insert),
            (Delete, _) => Some(imgui::Key::Delete),
            (Backspace, _) => Some(imgui::Key::Backspace),
            (Space, _) => Some(imgui::Key::Space),
            (Enter, Numpad) => Some(imgui::Key::KeypadEnter),
            (Enter, _) => Some(imgui::Key::Enter),
            (Escape, _) => Some(imgui::Key::Escape),
            (Control, Left) => Some(imgui::Key::LeftCtrl),
            (Control, Right) => Some(imgui::Key::RightCtrl),
            (Shift, Left) => Some(imgui::Key::LeftShift),
            (Shift, Right) => Some(imgui::Key::RightShift),
            (Alt, Left) => Some(imgui::Key::LeftAlt),
            (Alt, Right) => Some(imgui::Key::RightAlt),
            (Super, Left) => Some(imgui::Key::LeftSuper),
            (Super, Right) => Some(imgui::Key::RightSuper),
            (ContextMenu, _) => Some(imgui::Key::Menu),
            (F1, _) => Some(imgui::Key::F1),
            (F2, _) => Some(imgui::Key::F2),
            (F3, _) => Some(imgui::Key::F3),
            (F4, _) => Some(imgui::Key::F4),
            (F5, _) => Some(imgui::Key::F5),
            (F6, _) => Some(imgui::Key::F6),
            (F7, _) => Some(imgui::Key::F7),
            (F8, _) => Some(imgui::Key::F8),
            (F9, _) => Some(imgui::Key::F9),
            (F10, _) => Some(imgui::Key::F10),
            (F11, _) => Some(imgui::Key::F11),
            (F12, _) => Some(imgui::Key::F12),
            (CapsLock, _) => Some(imgui::Key::CapsLock),
            (ScrollLock, _) => Some(imgui::Key::ScrollLock),
            (NumLock, _) => Some(imgui::Key::NumLock),
            (PrintScreen, _) => Some(imgui::Key::PrintScreen),
            (Pause, _) => Some(imgui::Key::Pause),
            _ => None,
        },
        WinitKey::Dead(ch) => match ch {
            Some('\u{0300}') => Some(imgui::Key::GraveAccent),
            _ => None,
        },
        WinitKey::Character(ch) => match (ch.as_ref(), location) {
            ("a", _) => Some(imgui::Key::A),
            ("b", _) => Some(imgui::Key::B),
            ("c", _) => Some(imgui::Key::C),
            ("d", _) => Some(imgui::Key::D),
            ("e", _) => Some(imgui::Key::E),
            ("f", _) => Some(imgui::Key::F),
            ("g", _) => Some(imgui::Key::G),
            ("h", _) => Some(imgui::Key::H),
            ("i", _) => Some(imgui::Key::I),
            ("j", _) => Some(imgui::Key::J),
            ("k", _) => Some(imgui::Key::K),
            ("l", _) => Some(imgui::Key::L),
            ("m", _) => Some(imgui::Key::M),
            ("n", _) => Some(imgui::Key::N),
            ("o", _) => Some(imgui::Key::O),
            ("p", _) => Some(imgui::Key::P),
            ("q", _) => Some(imgui::Key::Q),
            ("r", _) => Some(imgui::Key::R),
            ("s", _) => Some(imgui::Key::S),
            ("t", _) => Some(imgui::Key::T),
            ("u", _) => Some(imgui::Key::U),
            ("v", _) => Some(imgui::Key::V),
            ("w", _) => Some(imgui::Key::W),
            ("x", _) => Some(imgui::Key::X),
            ("y", _) => Some(imgui::Key::Y),
            ("z", _) => Some(imgui::Key::Z),
            ("0", Numpad) => Some(imgui::Key::Keypad0),
            ("1", Numpad) => Some(imgui::Key::Keypad1),
            ("2", Numpad) => Some(imgui::Key::Keypad2),
            ("3", Numpad) => Some(imgui::Key::Keypad3),
            ("4", Numpad) => Some(imgui::Key::Keypad4),
            ("5", Numpad) => Some(imgui::Key::Keypad5),
            ("6", Numpad) => Some(imgui::Key::Keypad6),
            ("7", Numpad) => Some(imgui::Key::Keypad7),
            ("8", Numpad) => Some(imgui::Key::Keypad8),
            ("9", Numpad) => Some(imgui::Key::Keypad9),
            (".", Numpad) => Some(imgui::Key::KeypadDecimal),
            ("/", Numpad) => Some(imgui::Key::KeypadDivide),
            ("*", Numpad) => Some(imgui::Key::KeypadMultiply),
            ("-", Numpad) => Some(imgui::Key::KeypadSubtract),
            ("+", Numpad) => Some(imgui::Key::KeypadAdd),
            ("=", Numpad) => Some(imgui::Key::KeypadEqual),
            ("0", _) => Some(imgui::Key::Alpha0),
            ("1", _) => Some(imgui::Key::Alpha1),
            ("2", _) => Some(imgui::Key::Alpha2),
            ("3", _) => Some(imgui::Key::Alpha3),
            ("4", _) => Some(imgui::Key::Alpha4),
            ("5", _) => Some(imgui::Key::Alpha5),
            ("6", _) => Some(imgui::Key::Alpha6),
            ("7", _) => Some(imgui::Key::Alpha7),
            ("8", _) => Some(imgui::Key::Alpha8),
            ("9", _) => Some(imgui::Key::Alpha9),
            ("'", _) => Some(imgui::Key::Apostrophe),
            (",", _) => Some(imgui::Key::Comma),
            ("-", _) => Some(imgui::Key::Minus),
            (".", _) => Some(imgui::Key::Period),
            ("/", _) => Some(imgui::Key::Slash),
            (";", _) => Some(imgui::Key::Semicolon),
            ("=", _) => Some(imgui::Key::Equal),
            ("[", _) => Some(imgui::Key::LeftBracket),
            ("\\", _) => Some(imgui::Key::Backslash),
            ("]", _) => Some(imgui::Key::RightBracket),
            _ => None,
        },
        WinitKey::Unidentified(_) => None,
    }
}

impl WinitPlatform {
    /// Initializes a winit platform instance and configures imgui.
    ///
    /// This function configures imgui-rs in the following ways:
    ///
    /// * backend flags are updated
    /// * keys are configured
    /// * platform name is set
    pub fn init(imgui: &mut Context) -> WinitPlatform {
        let io = imgui.io_mut();
        io.backend_flags.insert(BackendFlags::HAS_MOUSE_CURSORS);
        io.backend_flags.insert(BackendFlags::HAS_SET_MOUSE_POS);
        imgui.set_platform_name(Some(format!(
            "imgui-winit-support {}",
            env!("CARGO_PKG_VERSION")
        )));
        WinitPlatform {
            hidpi_mode: ActiveHiDpiMode::Default,
            hidpi_factor: 1.0,
            cursor_cache: None,
        }
    }
    /// Attaches the platform instance to a winit window.
    ///
    /// This function configures imgui-rs in the following ways:
    ///
    /// * framebuffer scale (= DPI factor) is set
    /// * display size is set
    pub fn attach_window(&mut self, io: &mut Io, window: &Window, hidpi_mode: HiDpiMode) {
        let (hidpi_mode, hidpi_factor) = hidpi_mode.apply(window.scale_factor());
        self.hidpi_mode = hidpi_mode;
        self.hidpi_factor = hidpi_factor;
        io.display_framebuffer_scale = [hidpi_factor as f32, hidpi_factor as f32];
        let logical_size = window.inner_size().to_logical(hidpi_factor);
        let logical_size = self.scale_size_from_winit(window, logical_size);
        io.display_size = [logical_size.width as f32, logical_size.height as f32];
    }
    /// Returns the current DPI factor.
    ///
    /// The value might not be the same as the winit DPI factor (depends on the used DPI mode)
    pub fn hidpi_factor(&self) -> f64 {
        self.hidpi_factor
    }
    /// Scales a logical size coming from winit using the current DPI mode.
    ///
    /// This utility function is useful if you are using a DPI mode other than default, and want
    /// your application to use the same logical coordinates as imgui-rs.
    pub fn scale_size_from_winit(
        &self,
        window: &Window,
        logical_size: LogicalSize<f64>,
    ) -> LogicalSize<f64> {
        match self.hidpi_mode {
            ActiveHiDpiMode::Default => logical_size,
            _ => logical_size
                .to_physical::<f64>(window.scale_factor())
                .to_logical(self.hidpi_factor),
        }
    }
    /// Scales a logical position coming from winit using the current DPI mode.
    ///
    /// This utility function is useful if you are using a DPI mode other than default, and want
    /// your application to use the same logical coordinates as imgui-rs.
    pub fn scale_pos_from_winit(
        &self,
        window: &Window,
        logical_pos: LogicalPosition<f64>,
    ) -> LogicalPosition<f64> {
        match self.hidpi_mode {
            ActiveHiDpiMode::Default => logical_pos,
            _ => logical_pos
                .to_physical::<f64>(window.scale_factor())
                .to_logical(self.hidpi_factor),
        }
    }
    /// Scales a logical position for winit using the current DPI mode.
    ///
    /// This utility function is useful if you are using a DPI mode other than default, and want
    /// your application to use the same logical coordinates as imgui-rs.
    pub fn scale_pos_for_winit(
        &self,
        window: &Window,
        logical_pos: LogicalPosition<f64>,
    ) -> LogicalPosition<f64> {
        match self.hidpi_mode {
            ActiveHiDpiMode::Default => logical_pos,
            _ => logical_pos
                .to_physical::<f64>(self.hidpi_factor)
                .to_logical(window.scale_factor()),
        }
    }
    /// Handles a winit event.
    ///
    /// This function performs the following actions (depends on the event):
    ///
    /// * window size / dpi factor changes are applied
    /// * keyboard state is updated
    /// * mouse state is updated
    pub fn handle_event<T>(&mut self, io: &mut Io, window: &Window, event: &Event<T>) {
        match *event {
            Event::WindowEvent {
                window_id,
                ref event,
            } if window_id == window.id() => {
                self.handle_window_event(io, window, event);
            }
            _ => (),
        }
    }
    fn handle_window_event(&mut self, io: &mut Io, window: &Window, event: &WindowEvent) {
        match *event {
            WindowEvent::Resized(physical_size) => {
                let logical_size = physical_size.to_logical(window.scale_factor());
                let logical_size = self.scale_size_from_winit(window, logical_size);
                io.display_size = [logical_size.width as f32, logical_size.height as f32];
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let hidpi_factor = match self.hidpi_mode {
                    ActiveHiDpiMode::Default => scale_factor,
                    ActiveHiDpiMode::Rounded => scale_factor.round(),
                    _ => return,
                };
                // Mouse position needs to be changed while we still have both the old and the new
                // values
                if io.mouse_pos[0].is_finite() && io.mouse_pos[1].is_finite() {
                    io.mouse_pos = [
                        io.mouse_pos[0] * (hidpi_factor / self.hidpi_factor) as f32,
                        io.mouse_pos[1] * (hidpi_factor / self.hidpi_factor) as f32,
                    ];
                }
                self.hidpi_factor = hidpi_factor;
                io.display_framebuffer_scale = [hidpi_factor as f32, hidpi_factor as f32];
                // Window size might change too if we are using DPI rounding
                let logical_size = window.inner_size().to_logical(scale_factor);
                let logical_size = self.scale_size_from_winit(window, logical_size);
                io.display_size = [logical_size.width as f32, logical_size.height as f32];
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                // We need to track modifiers separately because some system like macOS, will
                // not reliably send modifier states during certain events like ScreenCapture.
                // Gotta let the people show off their pretty imgui widgets!
                let state = modifiers.state();
                io.add_key_event(imgui::Key::ModShift, state.shift_key());
                io.add_key_event(imgui::Key::ModCtrl, state.control_key());
                io.add_key_event(imgui::Key::ModAlt, state.alt_key());
                io.add_key_event(imgui::Key::ModSuper, state.super_key());
            }
            WindowEvent::KeyboardInput { ref event, .. } => {
                let logical_key = event.key_without_modifiers();
                let pressed = event.state == ElementState::Pressed;

                // We map both left and right ctrl to `ModCtrl`, etc.
                // imgui is told both "left control is pressed" and
                // "consider the control key is pressed". Allows
                // applications to use either general "ctrl" or a
                // specific key. Same applies to other modifiers.
                // https://github.com/ocornut/imgui/issues/5047
                if let WinitKey::Named(k) = logical_key {
                    match k {
                        NamedKey::Shift => io.add_key_event(imgui::Key::ModShift, pressed),
                        NamedKey::Control => io.add_key_event(imgui::Key::ModCtrl, pressed),
                        NamedKey::Alt => io.add_key_event(imgui::Key::ModAlt, pressed),
                        NamedKey::Super => io.add_key_event(imgui::Key::ModSuper, pressed),
                        _ => (),
                    }
                }

                // Add main key event
                if let Some(key) = to_imgui_key(logical_key, event.location) {
                    io.add_key_event(key, pressed);
                }

                if let Some(ref s) = event.text {
                    for ch in s.chars() {
                        // Exclude the backspace key ('\u{7f}'). Otherwise we will insert this char and then
                        // delete it.
                        if ch != '\u{7f}' {
                            io.add_input_character(ch)
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let position = position.to_logical(window.scale_factor());
                let position = self.scale_pos_from_winit(window, position);
                io.add_mouse_pos_event([position.x as f32, position.y as f32]);
            }
            WindowEvent::MouseWheel {
                delta,
                phase: TouchPhase::Moved,
                ..
            } => {
                let (h, v) = match delta {
                    MouseScrollDelta::LineDelta(h, v) => (h, v),
                    MouseScrollDelta::PixelDelta(pos) => {
                        let pos = pos.to_logical::<f64>(self.hidpi_factor);
                        let h = match pos.x.partial_cmp(&0.0) {
                            Some(Ordering::Greater) => 1.0,
                            Some(Ordering::Less) => -1.0,
                            _ => 0.0,
                        };
                        let v = match pos.y.partial_cmp(&0.0) {
                            Some(Ordering::Greater) => 1.0,
                            Some(Ordering::Less) => -1.0,
                            _ => 0.0,
                        };
                        (h, v)
                    }
                };
                io.add_mouse_wheel_event([h, v]);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(mb) = to_imgui_mouse_button(button) {
                    let pressed = state == ElementState::Pressed;
                    io.add_mouse_button_event(mb, pressed);
                }
            }
            WindowEvent::Focused(newly_focused) => {
                if !newly_focused {
                    // Set focus-lost to avoid stuck keys (like 'alt'
                    // when alt-tabbing)
                    io.app_focus_lost = true;
                }
            }
            _ => (),
        }
    }
    /// Frame preparation callback.
    ///
    /// Call this before calling the imgui-rs context `frame` function.
    /// This function performs the following actions:
    ///
    /// * mouse cursor is repositioned (if requested by imgui-rs)
    pub fn prepare_frame(&self, io: &mut Io, window: &Window) -> Result<(), ExternalError> {
        if io.want_set_mouse_pos {
            let logical_pos = self.scale_pos_for_winit(
                window,
                LogicalPosition::new(f64::from(io.mouse_pos[0]), f64::from(io.mouse_pos[1])),
            );
            window.set_cursor_position(logical_pos)
        } else {
            Ok(())
        }
    }

    /// Render preparation callback.
    ///
    /// Call this before calling the imgui-rs UI `render_with`/`render` function.
    /// This function performs the following actions:
    ///
    /// * mouse cursor is changed and/or hidden (if requested by imgui-rs)
    pub fn prepare_render(&mut self, ui: &Ui, window: &Window) {
        let io = ui.io();
        if !io
            .config_flags
            .contains(ConfigFlags::NO_MOUSE_CURSOR_CHANGE)
        {
            let cursor = CursorSettings {
                cursor: ui.mouse_cursor(),
                draw_cursor: io.mouse_draw_cursor,
            };
            if self.cursor_cache != Some(cursor) {
                cursor.apply(window);
                self.cursor_cache = Some(cursor);
            }
        }
    }
}
