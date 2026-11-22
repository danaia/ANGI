//! Remote keyboard and pointer events: RFB state translated into Slint window events.
use slint::{
    LogicalPosition, SharedString,
    platform::{Key, PointerEventButton, WindowEvent},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerAction {
    Move,
    Press(Button),
    Release(Button),
    /// One wheel notch; positive `dy` scrolls up and positive `dx` scrolls right.
    Scroll {
        dx: i8,
        dy: i8,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    Pointer {
        x: u16,
        y: u16,
        action: PointerAction,
    },
    Key {
        down: bool,
        keysym: u32,
    },
}

/// Events for a pointer message: a move, then one event per button edge.
/// RFB button bits: 0 left, 1 middle, 2 right, 3/4 wheel up/down, 5/6 wheel left/right.
pub fn pointer_inputs(previous: u8, buttons: u8, x: u16, y: u16) -> Vec<Input> {
    let mut inputs = vec![Input::Pointer {
        x,
        y,
        action: PointerAction::Move,
    }];
    let mut push = |action| inputs.push(Input::Pointer { x, y, action });
    for (bit, button) in [(0, Button::Left), (1, Button::Middle), (2, Button::Right), (7, Button::Back)] {
        let (was, is) = (previous >> bit & 1 != 0, buttons >> bit & 1 != 0);
        if !was && is {
            push(PointerAction::Press(button));
        } else if was && !is {
            push(PointerAction::Release(button));
        }
    }
    for (bit, dx, dy) in [(3, 0, 1), (4, 0, -1), (5, -1, 0), (6, 1, 0)] {
        if previous >> bit & 1 == 0 && buttons >> bit & 1 != 0 {
            push(PointerAction::Scroll { dx, dy });
        }
    }
    inputs
}

/// The Slint key text for an X11 keysym, or `None` for keys with no equivalent.
pub fn key_text(keysym: u32) -> Option<SharedString> {
    let special = match keysym {
        0xff08 => Key::Backspace,
        0xff09 => Key::Tab,
        0xfe20 => Key::Backtab,
        0xff0d | 0xff8d => Key::Return,
        0xff1b => Key::Escape,
        0xffff => Key::Delete,
        0xff63 => Key::Insert,
        0xff50 => Key::Home,
        0xff57 => Key::End,
        0xff55 => Key::PageUp,
        0xff56 => Key::PageDown,
        0xff51 => Key::LeftArrow,
        0xff52 => Key::UpArrow,
        0xff53 => Key::RightArrow,
        0xff54 => Key::DownArrow,
        0xff67 => Key::Menu,
        0xffe1 => Key::Shift,
        0xffe2 => Key::ShiftR,
        0xffe3 => Key::Control,
        0xffe4 => Key::ControlR,
        0xffe5 => Key::CapsLock,
        0xffe7 | 0xffeb => Key::Meta,
        0xffe8 | 0xffec => Key::MetaR,
        0xffe9 => Key::Alt,
        0xffea | 0xfe03 => Key::AltGr,
        0xffbe => Key::F1,
        0xffbf => Key::F2,
        0xffc0 => Key::F3,
        0xffc1 => Key::F4,
        0xffc2 => Key::F5,
        0xffc3 => Key::F6,
        0xffc4 => Key::F7,
        0xffc5 => Key::F8,
        0xffc6 => Key::F9,
        0xffc7 => Key::F10,
        0xffc8 => Key::F11,
        0xffc9 => Key::F12,
        0xffca..=0xffd5 => return char::from_u32(0xf710 + keysym - 0xffca).map(|c| c.to_string().into()),
        _ => return printable(keysym).map(|c| c.to_string().into()),
    };
    Some(special.into())
}

fn printable(keysym: u32) -> Option<char> {
    match keysym {
        0x20..=0x7e | 0xa0..=0xff => char::from_u32(keysym),
        0xffb0..=0xffb9 => char::from_u32(u32::from(b'0') + keysym - 0xffb0), // keypad digits
        0xffaa => Some('*'),
        0xffab => Some('+'),
        0xffad => Some('-'),
        0xffae => Some('.'),
        0xffaf => Some('/'),
        0x0100_0100..=0x0110_ffff => char::from_u32(keysym - 0x0100_0000), // Unicode keysyms
        _ => None,
    }
}

/// Deliver `input` to the window. Must run on the UI thread. Positions arrive in
/// physical pixels (the captured frame) and Slint expects logical ones.
pub fn dispatch(window: &slint::Window, input: Input) {
    match input {
        Input::Key { down, keysym } => {
            use crate::sound::VolumeAction;
            let volume = match keysym {
                0x1008_ff11 => Some(VolumeAction::Step(-5)),
                0x1008_ff12 => Some(VolumeAction::Mute),
                0x1008_ff13 => Some(VolumeAction::Step(5)),
                _ => None,
            };
            if let Some(action) = volume {
                crate::sound::media_key(action, down);
                return;
            }
            let Some(text) = key_text(keysym) else { return };
            window.dispatch_event(if down {
                WindowEvent::KeyPressed { text }
            } else {
                WindowEvent::KeyReleased { text }
            });
        }
        Input::Pointer { x, y, action } => {
            let scale = window.scale_factor();
            let position = LogicalPosition::new(f32::from(x) / scale, f32::from(y) / scale);
            let binding_button = |button| match button {
                Button::Left => 0, Button::Right => 1, Button::Middle => 2, Button::Back => 4,
            };
            let event = match action {
                PointerAction::Press(b) => Some((0, binding_button(b))),
                PointerAction::Release(b) => Some((1, binding_button(b))),
                PointerAction::Move => Some((2, 3)),
                PointerAction::Scroll { .. } => None,
            };
            if event.is_some_and(|(kind, button)| crate::key_bindings::dispatch_pointer(window, kind, button, position)) {
                return;
            }
            let button = |button| match button {
                Button::Left => PointerEventButton::Left,
                Button::Middle => PointerEventButton::Middle,
                Button::Right => PointerEventButton::Right,
                Button::Back => PointerEventButton::Back,
            };
            window.dispatch_event(match action {
                PointerAction::Move => WindowEvent::PointerMoved { position },
                PointerAction::Press(b) => WindowEvent::PointerPressed {
                    position,
                    button: button(b),
                },
                PointerAction::Release(b) => WindowEvent::PointerReleased {
                    position,
                    button: button(b),
                },
                // Slint's own backends report 60 logical pixels per wheel notch.
                PointerAction::Scroll { dx, dy } => WindowEvent::PointerScrolled {
                    position,
                    delta_x: -f32::from(dx) * 60.0,
                    delta_y: f32::from(dy) * 60.0,
                },
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actions(inputs: Vec<Input>) -> Vec<PointerAction> {
        inputs
            .into_iter()
            .map(|input| match input {
                Input::Pointer { action, .. } => action,
                Input::Key { .. } => unreachable!(),
            })
            .collect()
    }

    #[test]
    fn button_edges_become_press_and_release_after_a_move() {
        assert_eq!(
            actions(pointer_inputs(0, 0b001, 5, 6)),
            [PointerAction::Move, PointerAction::Press(Button::Left)]
        );
        assert_eq!(
            actions(pointer_inputs(0b001, 0b001, 7, 8)),
            [PointerAction::Move],
            "holding a button while dragging is only a move"
        );
        assert_eq!(
            actions(pointer_inputs(0b101, 0b010, 7, 8)),
            [
                PointerAction::Move,
                PointerAction::Release(Button::Left),
                PointerAction::Press(Button::Middle),
                PointerAction::Release(Button::Right),
            ]
        );
    }

    #[test]
    fn each_wheel_press_is_one_scroll_notch() {
        assert_eq!(
            actions(pointer_inputs(0, 0b1_0000, 0, 0)),
            [PointerAction::Move, PointerAction::Scroll { dx: 0, dy: -1 }]
        );
        // The client releases the wheel "button" straight afterwards; that is not a scroll.
        assert_eq!(
            actions(pointer_inputs(0b1_0000, 0, 0, 0)),
            [PointerAction::Move]
        );
        assert_eq!(
            actions(pointer_inputs(0, 0b100_0000, 0, 0)),
            [PointerAction::Move, PointerAction::Scroll { dx: 1, dy: 0 }]
        );
    }

    #[test]
    fn keysyms_map_to_slint_key_text() {
        assert_eq!(key_text(u32::from(b'a')).unwrap(), "a");
        assert_eq!(key_text(u32::from(b'A')).unwrap(), "A");
        assert_eq!(key_text(0x20).unwrap(), " ");
        assert_eq!(key_text(0xe9).unwrap(), "é");
        assert_eq!(key_text(0x0100_20ac).unwrap(), "€");
        assert_eq!(key_text(0xffb7).unwrap(), "7");
        assert_eq!(key_text(0xff0d).unwrap(), SharedString::from(Key::Return));
        assert_eq!(key_text(0xff8d).unwrap(), SharedString::from(Key::Return));
        assert_eq!(key_text(0xff1b).unwrap(), SharedString::from(Key::Escape));
        assert_eq!(key_text(0xff52).unwrap(), SharedString::from(Key::UpArrow));
        assert_eq!(key_text(0xffeb).unwrap(), SharedString::from(Key::Meta));
        assert_eq!(key_text(0xffc5).unwrap(), SharedString::from(Key::F8));
        assert_eq!(key_text(0xfe20).unwrap(), SharedString::from(Key::Backtab));
    }

    #[test]
    fn unknown_keysyms_are_ignored() {
        assert!(key_text(0).is_none());
        assert!(key_text(0xfd01).is_none());
        assert!(key_text(0x0111_0000).is_none());
    }
}
