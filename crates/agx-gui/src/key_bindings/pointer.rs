//! Native and remote pointer events can run reassigned mouse buttons before app widgets.
use crate::AppWindow;
use slint::{ComponentHandle, LogicalPosition, Weak, Window};
use std::cell::RefCell;

thread_local! {
    static DESKTOP: RefCell<Weak<AppWindow>> = RefCell::new(Weak::default());
}

pub(crate) fn attach_pointer(ui: &AppWindow) {
    DESKTOP.with(|desktop| *desktop.borrow_mut() = ui.as_weak());
    #[cfg(feature = "wayland")]
    attach_wayland(ui);
}

#[cfg(feature = "wayland")]
fn attach_wayland(ui: &AppWindow) {
    use i_slint_backend_winit::{
        EventResult, WinitWindowAccessor,
        winit::event::{ElementState, MouseButton, WindowEvent},
    };

    let desktop = ui.as_weak();
    let mut position = LogicalPosition::default();
    let debug = std::env::var_os("AGX_GUI_DEBUG").is_some();
    ui.window().on_winit_window_event(move |window, event| {
        let Some(ui) = desktop.upgrade() else {
            return EventResult::Propagate;
        };
        let handled = match event {
            WindowEvent::CursorMoved {
                position: physical, ..
            } => {
                let logical = physical.to_logical(window.scale_factor().into());
                position = LogicalPosition::new(logical.x, logical.y);
                ui.invoke_mouse_binding(2, 3, position.x, position.y)
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let button = match button {
                    MouseButton::Left => 0,
                    MouseButton::Right => 1,
                    MouseButton::Middle => 2,
                    MouseButton::Back => 4,
                    MouseButton::Forward => 5,
                    MouseButton::Other(_) => return EventResult::Propagate,
                };
                let handled = ui.invoke_mouse_binding(
                    i32::from(*state == ElementState::Released),
                    button,
                    position.x,
                    position.y,
                );
                if debug {
                    eprintln!("wayland pointer {button} {state:?} bound={handled}");
                }
                handled
            }
            _ => false,
        };
        if handled {
            EventResult::PreventDefault
        } else {
            EventResult::Propagate
        }
    });
}

pub(crate) fn dispatch_pointer(
    window: &Window,
    kind: i32,
    button: i32,
    p: LogicalPosition,
) -> bool {
    let desktop = DESKTOP.with(|desktop| desktop.borrow().upgrade());
    desktop.is_some_and(|ui| {
        std::ptr::eq(ui.window(), window) && ui.invoke_mouse_binding(kind, button, p.x, p.y)
    })
}
