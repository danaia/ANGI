use super::{Action, Binding, Device, Mappings, Trigger};
use crate::{
    App, AppWindow, KeyBindings, ShortcutRow,
    keymap::{Cmd, Mods},
    mods,
};
use slint::{ComponentHandle, SharedString, VecModel};
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
struct Editor {
    edit: Option<Edit>,
    reset: Option<Device>,
}

struct Edit {
    id: String,
    original: Option<Binding>,
    pending: Option<Binding>,
    modifiers: u8,
}

fn publish(ui: &AppWindow, map: &Mappings) {
    let view = ui.global::<KeyBindings>();
    let device = Device::from_index(view.get_device());
    let query = view.get_query().trim().to_lowercase();
    let mut rows = Vec::new();
    for entry in &map.entries {
        if device == Device::Keyboard && !matches!(entry.action, Action::Command(_)) {
            continue;
        }
        let bindings = map.bindings(entry, device);
        let labels: Vec<SharedString> = bindings
            .iter()
            .map(|binding| binding.label().into())
            .collect();
        let searchable = format!(
            "{} {}",
            entry.label,
            labels
                .iter()
                .map(SharedString::as_str)
                .collect::<Vec<_>>()
                .join(" ")
        )
        .to_lowercase();
        if !query
            .split_whitespace()
            .all(|part| searchable.contains(part))
        {
            continue;
        }
        rows.push(ShortcutRow {
            id: entry.id.as_str().into(),
            label: entry.label.as_str().into(),
            bindings: Rc::new(VecModel::from(labels)).into(),
        });
    }
    if device == Device::Mouse {
        use slint::Model as _;
        rows.sort_by_key(|row| row.bindings.row_count() == 0);
    }
    view.set_rows(Rc::new(VecModel::from(rows)).into());
}

fn publish_hints(ui: &AppWindow, map: &Mappings) {
    let view = ui.global::<KeyBindings>();
    view.set_layout_hint(map.hint(Cmd::CycleLayout).into());
    let mut hints = Vec::new();
    for (command, label) in [
        (Cmd::Menu, "Apps"),
        (Cmd::Close, "Close"),
        (Cmd::ToggleFloat, "Float"),
        (Cmd::Fullscreen, "Fullscreen"),
        (Cmd::CycleLayout, "Layout"),
    ] {
        let hint = map.hint(command);
        if !hint.is_empty() {
            hints.push(format!("{hint}  {label}"));
        }
    }
    view.set_desktop_hint(hints.join("     ").into());
}

fn clear_capture(view: &KeyBindings<'_>, edit: &mut Edit) {
    edit.modifiers = 0;
    edit.pending = None;
    view.set_candidate("".into());
    view.set_message("".into());
    view.set_conflict(false);
    view.set_can_accept(false);
}

fn propose(app: &App, view: &KeyBindings<'_>, edit: &mut Edit, binding: Binding) {
    if !binding.allowed() {
        let message = match binding.device() {
            Device::Keyboard => "Press a key to finish the shortcut.",
            Device::Mouse => "Use another button or add a modifier to left click.",
        };
        clear_capture(view, edit);
        view.set_message(message.into());
        return;
    }
    let owner = app.bindings.owner(&binding, app.modk);
    let entry = app
        .bindings
        .entries
        .iter()
        .find(|entry| entry.id == edit.id)
        .unwrap();
    if edit.original.as_ref() != Some(&binding)
        && app
            .bindings
            .bindings(entry, binding.device())
            .contains(&binding)
    {
        view.set_message("This input already runs this action.".into());
        return;
    }
    let conflict = owner.filter(|owner| owner.id != edit.id);
    view.set_candidate(binding.label().into());
    view.set_conflict(conflict.is_some());
    view.set_can_accept(true);
    view.set_message(
        conflict
            .map_or_else(String::new, |owner| {
                format!(
                    "This input runs {}. Replacing it removes that assignment.",
                    owner.label
                )
            })
            .into(),
    );
    edit.pending = Some(binding);
}

fn commit(app: &mut App, edit: &Edit, remove: bool) -> std::io::Result<()> {
    let map = &mut app.bindings;
    let mut next = map.overrides.clone();
    let entry = map
        .entries
        .iter()
        .find(|entry| entry.id == edit.id)
        .unwrap();
    if let Some(original) = &edit.original {
        let device = original.device();
        let mut bindings = map.bindings(entry, device);
        bindings.retain(|binding| binding != original);
        next.set(device, &edit.id, bindings);
    }
    if !remove && let Some(binding) = &edit.pending {
        let device = binding.device();
        if let Some(conflict) = map
            .owner(binding, app.modk)
            .filter(|owner| owner.id != edit.id)
        {
            let mut bindings = map.bindings(conflict, device);
            bindings.retain(|other| other != binding);
            next.set(device, &conflict.id, bindings);
        }
        let mut bindings = next
            .for_device(device)
            .get(&edit.id)
            .cloned()
            .unwrap_or_else(|| map.bindings(entry, device));
        bindings.push(binding.clone());
        next.set(device, &edit.id, bindings);
    }
    map.save(next)
}

pub(crate) fn install(ui: &AppWindow, app: &Rc<RefCell<App>>) {
    app.borrow_mut().bindings.load();
    let editor = Rc::new(RefCell::new(Editor::default()));
    let view = ui.global::<KeyBindings>();
    publish(ui, &app.borrow().bindings);
    publish_hints(ui, &app.borrow().bindings);
    macro_rules! hook {
        ($on:ident, |$ui:ident, $app:ident, $state:ident $(, $arg:ident)*| $body:block) => {{
            let (weak, app, editor) = (ui.as_weak(), app.clone(), editor.clone());
            view.$on(move |$($arg),*| {
                if let (Some($ui), Ok(mut app), Ok(mut editor)) =
                    (weak.upgrade(), app.try_borrow_mut(), editor.try_borrow_mut())
                {
                    let $app = &mut *app;
                    let $state = &mut *editor;
                    $body
                }
            });
        }};
    }
    hook!(on_filter, |ui, app, _editor| {
        publish(&ui, &app.bindings);
    });
    hook!(on_edit, |ui, app, editor, id, slot| {
        let view = ui.global::<KeyBindings>();
        let Some(entry) = app
            .bindings
            .entries
            .iter()
            .find(|entry| entry.id == id.as_str())
        else {
            return;
        };
        let device = Device::from_index(view.get_device());
        let original = usize::try_from(slot)
            .ok()
            .and_then(|slot| app.bindings.bindings(entry, device).get(slot).cloned());
        let mut edit = Edit {
            id: entry.id.clone(),
            original,
            pending: None,
            modifiers: 0,
        };
        clear_capture(&view, &mut edit);
        view.set_action(entry.label.as_str().into());
        view.set_original(
            edit.original
                .as_ref()
                .map(Binding::label)
                .unwrap_or_default()
                .into(),
        );
        view.set_record_device(view.get_device());
        view.set_mouse_only(!matches!(entry.action, Action::Command(_)));
        view.set_resetting(false);
        editor.edit = Some(edit);
        view.set_open(true);
    });
    hook!(on_retry, |ui, _app, editor| {
        if let Some(edit) = &mut editor.edit {
            clear_capture(&ui.global::<KeyBindings>(), edit);
        }
    });
    hook!(on_record_key, |ui,
                          app,
                          editor,
                          key,
                          ctrl,
                          alt,
                          shift,
                          meta| {
        let view = ui.global::<KeyBindings>();
        let Some(edit) = &mut editor.edit else { return };
        if let Some(bit) = modifier(&key) {
            edit.modifiers |= bit;
        } else if view.get_record_device() == 0 && edit.pending.is_none() {
            let bits = edit.modifiers | mods(ctrl, alt, shift, meta).bits();
            propose(app, &view, edit, Binding::key(&key, bits));
        }
    });
    hook!(on_record_mouse, |ui,
                            app,
                            editor,
                            button,
                            ctrl,
                            alt,
                            shift,
                            meta| {
        let view = ui.global::<KeyBindings>();
        let Some(edit) = &mut editor.edit else { return };
        if view.get_record_device() == 1 && edit.pending.is_none() {
            let bits = edit.modifiers | mods(ctrl, alt, shift, meta).bits();
            propose(
                app,
                &view,
                edit,
                Binding {
                    trigger: Trigger::Mouse(button),
                    bits,
                },
            );
        }
    });
    hook!(on_reset, |ui, _app, editor| {
        let view = ui.global::<KeyBindings>();
        editor.reset = Some(Device::from_index(view.get_device()));
        view.set_action("Restore defaults?".into());
        view.set_message("".into());
        view.set_resetting(true);
        view.set_open(true);
    });
    hook!(on_cancel, |ui, _app, editor| {
        *editor = Editor::default();
        ui.global::<KeyBindings>().set_open(false);
        ui.invoke_focus_settings();
    });
    hook!(on_commit, |ui, app, editor, remove| {
        let view = ui.global::<KeyBindings>();
        let result = if let Some(device) = editor.reset {
            let mut next = app.bindings.overrides.clone();
            match device {
                Device::Keyboard => next.keyboard.clear(),
                Device::Mouse => next.mouse.clear(),
            }
            app.bindings.save(next)
        } else if let Some(edit) = &editor.edit {
            if (remove && edit.original.is_none()) || (!remove && edit.pending.is_none()) {
                return;
            }
            commit(app, edit, remove)
        } else {
            return;
        };
        match result {
            Ok(()) => {
                *editor = Editor::default();
                publish(&ui, &app.bindings);
                publish_hints(&ui, &app.bindings);
                view.set_open(false);
                ui.invoke_focus_settings();
            }
            Err(error) => {
                eprintln!("agx-gui: save key bindings: {error}");
                view.set_message("Could not save key bindings. Try again.".into());
            }
        }
    });
}

fn modifier(key: &str) -> Option<u8> {
    Some(match key {
        "\u{0010}" | "\u{0015}" => Mods::SHIFT,
        "\u{0011}" | "\u{0016}" => Mods::CTRL,
        "\u{0012}" => Mods::ALT,
        "\u{0017}" | "\u{0018}" => Mods::META,
        _ => return None,
    })
}

#[cfg(test)]
pub(crate) fn test_editor(ui: &AppWindow, app: &Rc<RefCell<App>>, render: &impl Fn()) {
    use crate::wm::Kind;
    use slint::platform::{Key, WindowEvent};

    let folder = std::env::temp_dir().join(format!(
        "agx-key-bindings-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let path = folder.join("key-bindings.json");
    app.borrow_mut().bindings.path = Some(path.clone());
    install(ui, app);
    ui.global::<crate::MouseSettings>().set_page(4);
    app.borrow_mut().run(ui, Cmd::Open(Kind::Settings));
    render();
    let bindings = ui.global::<KeyBindings>();
    let tap = |text: SharedString| {
        ui.window()
            .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        ui.window()
            .dispatch_event(WindowEvent::KeyReleased { text });
    };

    // A tapped and released modifier remains in the draft. Escape discards the draft.
    bindings.invoke_edit("settings".into(), -1);
    render();
    tap(Key::Control.into());
    tap("k".into());
    assert_eq!(bindings.get_candidate(), "Ctrl + K");
    assert!(!path.exists());
    tap(Key::Escape.into());
    assert!(!bindings.get_open());
    assert!(
        app.borrow()
            .bindings
            .custom_key("k", Mods::from_bits(Mods::CTRL))
            .is_none()
    );

    // A conflicting default requires explicit acceptance and is persisted as one update.
    bindings.invoke_edit("settings".into(), -1);
    render();
    let primary = match app.borrow().modk {
        crate::keymap::ModKey::Super => Key::Meta,
        crate::keymap::ModKey::Alt => Key::Alt,
    };
    tap(primary.into());
    tap("q".into());
    assert!(bindings.get_conflict());
    assert!(!path.exists());
    tap(Key::Return.into());
    assert!(!bindings.get_open());
    {
        let model = app.borrow();
        let modifier = if model.modk == crate::keymap::ModKey::Super {
            Mods::META
        } else {
            Mods::ALT
        };
        let mods = Mods::from_bits(modifier);
        assert_eq!(
            model.bindings.custom_key("q", mods),
            Some(Cmd::Open(Kind::Settings))
        );
        assert_eq!(model.bindings.custom_key("w", mods), Some(Cmd::Close));
        assert_eq!(model.bindings.default_key("q", mods, model.modk), None);
    }

    // Change the device of that binding. Its old keyboard slot is released.
    bindings.invoke_edit("settings".into(), 0);
    render();
    bindings.set_record_device(1);
    bindings.invoke_retry();
    bindings.invoke_record_mouse(2, false, false, false, false);
    assert_eq!(bindings.get_candidate(), "Middle click");
    bindings.invoke_commit(false);
    {
        let model = app.borrow();
        let mut loaded = Mappings::new(model.modk);
        loaded.path = Some(path);
        loaded.load();
        assert_eq!(
            loaded.mouse(2, Mods::default(), model.modk),
            Some(Action::Command(Cmd::Open(Kind::Settings)))
        );
        assert!(
            loaded
                .hint(Cmd::Open(Kind::Settings))
                .contains("Middle click")
        );
    }
    bindings.set_device(1);
    bindings.invoke_reset();
    bindings.invoke_commit(false);
    assert_eq!(app.borrow().bindings.hint(Cmd::Open(Kind::Settings)), "");
    std::fs::remove_dir_all(folder).unwrap();
    ui.global::<crate::MouseSettings>().set_page(0);
}
