//! Window creation, callback wiring, timers and background system sampling.
use crate::system_ui::{apply, apply_network_safety};
use crate::{
    App, AppWindow, Sys, browser, graphics, keymap::Cmd, mods, remote, session_action, terminal,
};
use agx_core::Sampler;
use slint::{ComponentHandle, ModelRc, Timer, TimerMode};
use std::{cell::RefCell, rc::Rc, time::Duration};
pub(super) fn run() -> Result<(), Box<dyn std::error::Error>> {
    let wayland_shell = cfg!(feature = "wayland") && graphics::using_winit();
    #[cfg(feature = "wayland")]
    if wayland_shell {
        graphics::wayland_workspaces::identify_shell()?;
        graphics::wayland_workspaces::reserve_desktop_chrome();
        graphics::wayland_workspaces::select(0);
    }
    let ui = AppWindow::new()?;
    if wayland_shell {
        ui.set_wayland_shell(true);
        // Maximized keeps the shell on the normal desktop layer, alongside native app windows.
        ui.window().set_maximized(true);
        #[cfg(feature = "wayland")]
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            graphics::wayland_workspaces::mark_desktop_opaque(&ui);
        }
    }
    graphics::attach_input(&ui);
    let _settings_save = crate::settings::install(&ui);
    let _sound_watch = crate::sound::install(&ui);
    let _process_watch = crate::memory_processes::install(&ui);
    let _storage_watch = crate::storage::install(&ui);
    #[cfg(feature = "wayland")]
    let fullscreen_controls = if wayland_shell {
        graphics::wayland_workspaces::fullscreen_controls()
    } else {
        None
    };
    #[cfg(feature = "wayland")]
    let controls = fullscreen_controls.as_ref().map(ComponentHandle::as_weak);
    crate::appearance::install(&ui, move |_dark| {
        #[cfg(feature = "wayland")]
        if let Some(controls) = controls.as_ref().and_then(slint::Weak::upgrade) {
            controls.global::<crate::Theme>().set_dark(_dark);
        }
    });
    ui.global::<Sys>().set_mode_note(
        if wayland_shell {
            "Native Wayland session"
        } else if cfg!(feature = "kms") {
            "DRM/KMS appliance, no display server"
        } else {
            "desktop preview window; power actions disabled"
        }
        .into(),
    );

    let app = Rc::new(RefCell::new(App::new()));
    #[cfg(feature = "wayland")]
    let _native_tiles = graphics::native_tiles::install(&ui, &app);
    crate::key_bindings::install(&ui, &app);
    crate::key_bindings::attach_pointer(&ui);
    {
        let (weak, state) = (ui.as_weak(), app.clone());
        ui.global::<crate::SoundSettings>().on_radio(move || {
            if let (Some(ui), Ok(mut app)) = (weak.upgrade(), state.try_borrow_mut()) {
                app.terminal_open_program(&ui, "/usr/bin/cliamp".into(), "CLIamp".into());
            }
        });
    }
    let _services_watch = crate::services::install(&ui);
    let _build_notice_watch = crate::build_notice::install(&ui);
    ui.set_wins(ModelRc::from(app.borrow().view.windows.clone()));
    ui.set_ws_occupied(ModelRc::from(app.borrow().occupied.clone()));
    ui.set_mod_name(app.borrow().modk.name().into());
    ui.set_draw_cursor(graphics::draw_cursor());
    ui.set_resize_cursor_overlay(cfg!(feature = "kms") && !graphics::using_winit());
    let _remote = remote::start(&ui);
    // Callbacks never nest, but a stray re-entrant event is dropped rather than panicking.
    macro_rules! hook {
        ($on:ident, |$app:ident, $ui:ident $(, $arg:ident)*| $body:expr) => {{
            let (weak, state) = (ui.as_weak(), app.clone());
            ui.$on(move |$($arg),*| {
                let (Some($ui), Ok(mut $app)) = (weak.upgrade(), state.try_borrow_mut()) else {
                    return Default::default();
                };
                $body
            });
        }};
    }
    hook!(on_key, |a, ui, name, ctrl, alt, shift, meta| {
        if std::env::var_os("AGX_GUI_DEBUG").is_some() {
            eprintln!("key {name:?} ctrl={ctrl} alt={alt} shift={shift} meta={meta}");
        }
        a.key(&ui, &name, mods(ctrl, alt, shift, meta))
    });
    hook!(on_shortcut_key, |a, ui, name, ctrl, alt, shift, meta, allowed, repeat| {
        a.shortcut_mods = mods(ctrl, alt, shift, meta);
        // Keep bare typing keys in text fields and published application shortcuts.
        let global = ctrl || alt || meta || name.chars().next().is_some_and(|key| ('\u{f704}'..='\u{f71b}').contains(&key));
        if allowed && global && let Some(cmd) = a.bindings.custom_key(&name, a.shortcut_mods) {
            if !repeat { a.run(&ui, cmd); }
            return true;
        }
        false
    });
    hook!(on_mapped_pointer, |a, ui, kind, button, x, y| {
        a.native_mouse_binding(&ui, kind, button, x, y)
    });
    hook!(on_fullscreen_action, |a, ui, action| a.fullscreen_action(&ui, action));
    hook!(on_pointer, |a,
                       ui,
                       kind,
                       button,
                       x,
                       y,
                       ctrl,
                       alt,
                       shift,
                       meta| {
        a.pointer(&ui, kind, button, x, y, mods(ctrl, alt, shift, meta))
    });
    hook!(on_scroll, |a, ui, dx, delta, ctrl, alt, shift, meta| a
        .scroll(&ui, dx, delta, mods(ctrl, alt, shift, meta)));
    hook!(on_resized, |a, ui, w, h| a.resized(&ui, w, h));
    hook!(on_menu_show, |a, ui| a.run(&ui, Cmd::Menu));
    hook!(on_header_menu_dismiss, |a, ui| a.dismiss_header_menu(&ui));
    hook!(on_header_menu_action, |a, ui, index| a.choose_header_action(&ui, index));
    hook!(on_workspace_menu, |a, ui, index, x, y| a.open_workspace_menu(&ui, index, x, y));
    hook!(on_workspace_select, |a, ui, index| {
        if let Ok(index) = usize::try_from(index) {
            a.run(&ui, Cmd::Workspace(index));
        }
    });
    hook!(on_menu_query, |a, ui, query| a.set_menu_rows(&ui, &query));
    hook!(on_menu_accept, |a, ui, index| a.menu_accept(&ui, index));
    hook!(on_menu_remove, |a, ui, index| a.menu_remove(&ui, index));
    hook!(on_confirm, |a, ui, yes| a.confirm(&ui, yes));
    hook!(on_session_action, |a, ui, index| {
        if let Some(cmd) = session_action(index) {
            a.run(&ui, cmd);
        }
    });
    {
        let (weak, state) = (ui.as_weak(), app.clone());
        ui.global::<crate::AgentAngi>().on_activate(move || {
            if let (Some(ui), Ok(mut app)) = (weak.upgrade(), state.try_borrow_mut()) {
                app.desk.focus(crate::wm::Kind::AgentAngi);
                app.refresh(&ui);
            }
        });
    }
    let attachments = std::rc::Rc::new(crate::agent_attachments::install(&ui));
    {
        let (weak, state, staged) = (ui.as_weak(), app.clone(), attachments.clone());
        ui.global::<crate::AgentAngi>().on_submit(move |prompt| {
            if let (Some(ui), Ok(mut app)) = (weak.upgrade(), state.try_borrow_mut()) {
                // OS mode builds the operating system; Project mode builds in the chosen folder.
                let scope = ui.global::<crate::AgentScope>();
                let single = scope.get_single_agent();
                let run = match scope.get_mode().as_str() {
                    "project" if scope.get_project().is_empty() => return,
                    "project" => terminal::AgentRun::Project { folder: scope.get_project().to_string(), single },
                    // Chat is the bubble view's own send; it never opens a terminal.
                    "chat" => return,
                    _ => terminal::AgentRun::Os { single },
                };
                // Files attached in the composer go to the agent as paths in the brief.
                let (files, _) = staged.take_for_send(&ui);
                let brief = crate::agent_attachments::brief_with_files(&prompt, &files);
                app.terminal_open_agent(&ui, brief, run);
            }
        });
    }
    hook!(on_settings_open, |a, ui| a
        .run(&ui, Cmd::Open(crate::wm::Kind::Settings)));
    {
        let (weak, state) = (ui.as_weak(), app.clone());
        ui.global::<crate::MouseSettings>().on_activate(move || {
            if let (Some(ui), Ok(mut app)) = (weak.upgrade(), state.try_borrow_mut()) {
                if app.desk.focused() != Some(crate::wm::Kind::Settings) {
                    app.desk.focus(crate::wm::Kind::Settings);
                    app.refresh(&ui);
                }
            }
        });
    }
    {
        let (weak, state) = (ui.as_weak(), app.clone());
        ui.global::<crate::MouseSettings>().on_refresh(move || {
            if let (Some(ui), Ok(mut app)) = (weak.upgrade(), state.try_borrow_mut()) {
                app.run(&ui, Cmd::RefreshDesktop);
            }
        });
    }
    hook!(on_layout_toggle, |a, ui| a.run(&ui, Cmd::CycleLayout));
    hook!(on_browser_key, |a,
                           ui,
                           text,
                           ctrl,
                           alt,
                           shift,
                           meta,
                           down| {
        a.terminal_key(&ui, &text, mods(ctrl, alt, shift, meta), down)
            || a.browser_key(&ui, &text, mods(ctrl, alt, shift, meta), down)
    });
    let _terminal_watch = terminal::install(&ui, &app);
    let _browser_watch = browser::install(&ui, &app);
    let _provider_watch = crate::agent_providers::install(&ui, &app);
    let _chat_watch = crate::agent_chat::install(&ui, &attachments);
    let _voice_watch = crate::agent_voice::install(&ui);
    let _history_watch = crate::agent_history::install(&ui, &app);
    let _knowledge_watch = crate::agent_knowledge::install(&ui);
    let _files_watch = crate::files::install(&ui, &app);
    let _project_watch = crate::agent_project::install(&ui);
    let _bluetooth_watch = crate::bluetooth::install(&ui, &app);
    let _library_watch = crate::library::install(&ui, &app);
    crate::agent_swarm::install(&ui, &app);
    let _open_watch = crate::open_request::install(&ui, &app);
    app.borrow().refresh(&ui);

    // The compositor-less backend sizes the window after the UI is built, and a change
    // notification alone left the layout at its 1280x720 default on a 2560x1080 display.
    // Compare against the real window size and re-lay out whenever it differs.
    let size_watch = Timer::default();
    {
        let (weak, state) = (ui.as_weak(), app.clone());
        size_watch.start(TimerMode::Repeated, Duration::from_millis(250), move || {
            let (Some(ui), Ok(mut app)) = (weak.upgrade(), state.try_borrow_mut()) else {
                return;
            };
            let (size, scale) = (ui.window().size(), ui.window().scale_factor());
            let (w, h) = (size.width as f32 / scale, size.height as f32 / scale);
            if w > 0.0 && h > 0.0 && (w, h) != app.size {
                app.resized(&ui, w, h);
            }
        });
    }

    let _clock = crate::calendar::install(&ui);

    // Sampling stays off the UI thread; results are applied via the event loop.
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let mut sampler = Sampler::default();
        loop {
            let result = sampler.collect();
            let temperature = crate::cpu_temperature::temperature();
            let exposure = crate::network_safety::collect();
            let weak = weak.clone();
            if slint::invoke_from_event_loop(move || {
                if let Some(ui) = weak.upgrade() {
                    apply_network_safety(&ui.global::<Sys>(), exposure);
                    match result {
                        Ok(snapshot) => apply(&ui, &snapshot, temperature),
                        Err(error) => ui
                            .global::<Sys>()
                            .set_warnings(format!("sampling failed: {error}").into()),
                    }
                }
            })
            .is_err()
            {
                break;
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    });

    ui.run()?;
    Ok(())
}
