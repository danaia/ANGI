//! Native and remote pointer events can run reassigned mouse buttons before app widgets.
use crate::AppWindow;
use slint::{ComponentHandle, LogicalPosition, Weak, Window};
use std::cell::RefCell;

thread_local! {
    static DESKTOP: RefCell<Weak<AppWindow>> = RefCell::new(Weak::default());
}

pub(crate) fn attach_pointer(ui: &AppWindow) {
    DESKTOP.with(|desktop| *desktop.borrow_mut() = ui.as_weak());
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
