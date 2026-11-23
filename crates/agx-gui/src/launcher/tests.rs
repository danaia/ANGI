//! Launcher search, ranking and install-state rows.
use super::*;
use crate::wm::Rect;

fn available(_: &apps::Package) -> Availability {
    Availability::Available
}

fn desktop() -> Desktop {
    Desktop::new(
        Rect::new(0., 0., 1280., 720.),
        Rect::new(0., 34., 1280., 660.),
    )
}

#[test]
fn apps_include_running_and_closed_tools_and_open_them_from_any_workspace() {
    let mut desktop = desktop();
    desktop.close(Kind::Services);
    let apps = results(&desktop, "Super", "", SETTINGS, &available);
    assert_eq!(apps.len(), Kind::ALL.len() + 2);
    assert_eq!(
        results(&desktop, "Super", "terminal", SETTINGS, &available)[0].command,
        Cmd::Open(Kind::Terminal)
    );
    assert!(
        !desktop.is_open(Kind::Terminal),
        "shells are started on demand"
    );
    assert!(apps.iter().all(|entry| entry.section == SETTINGS));
    assert_eq!(
        results(&desktop, "Super", "opencode", SETTINGS, &available)[0].command,
        Cmd::OpenCode
    );
    assert_eq!(apps[0].command, Cmd::Open(Kind::Overview));
    assert!(
        apps.iter()
            .any(|entry| entry.command == Cmd::Open(Kind::Services) && !entry.running)
    );
    desktop.goto(3);
    assert_eq!(desktop.focused(), None, "Workspace 4 starts empty");
    desktop.goto(0);
    assert_eq!(
        results(&desktop, "Super", "webkit", SETTINGS, &available)[0].command,
        Cmd::Open(Kind::Web)
    );
    desktop.open(Kind::Web);
    assert_eq!(desktop.focused(), Some(Kind::Web));
    desktop.close(Kind::Web);
    assert!(!desktop.is_open(Kind::Web));
    desktop.goto(4);
    desktop.open(Kind::Overview);
    assert_eq!(desktop.focused(), Some(Kind::Overview));
}

#[test]
fn search_finds_app_aliases_and_secondary_commands() {
    let desktop = desktop();
    for query in ["wifi", "  WIFI  ", "remote access"] {
        assert_eq!(
            results(&desktop, "Super", query, SETTINGS, &available)[0].command,
            Cmd::Open(Kind::Network)
        );
    }
    assert_eq!(
        results(&desktop, "Super", "ram", SETTINGS, &available)[0].command,
        Cmd::Open(Kind::Memory)
    );
    assert_eq!(
        results(&desktop, "Super", "workspace 3", SETTINGS, &available)[0].command,
        Cmd::Workspace(2)
    );
    for query in ["settings", "mouse", "sensitivity", "bluetooth"] {
        assert_eq!(
            results(&desktop, "Super", query, SETTINGS, &available)[0].command,
            Cmd::Open(Kind::Settings)
        );
    }
    assert!(results(&desktop, "Super", "not-an-app", SETTINGS, &available).is_empty());
    assert!(
        results(&desktop, "Super", "", COMMANDS, &available)
            .iter()
            .all(|entry| entry.section == COMMANDS)
    );
}

#[test]
fn install_starts_with_featured_apps_and_apps_only_shows_ready_to_open_entries() {
    let desktop = desktop();
    let listed = results(&desktop, "Super", "", INSTALL, &available);
    assert_eq!(listed.len(), apps::index().len());
    assert_eq!(listed[..5].iter().map(|entry| entry.label.as_str()).collect::<Vec<_>>(),
               ["Blender", "VS Code", "VLC", "GStreamer", "Chromium"]);
    assert_eq!(listed[0].heading, "FEATURED");
    assert_eq!(listed[5].heading, "MORE SOFTWARE");
    assert!(listed.iter().all(|entry| entry.section == INSTALL && entry.status.action == "Install"));
    let apps = results(&desktop, "Super", "", APPS, &available);
    assert_eq!(apps.iter().map(|entry| entry.label.as_str()).collect::<Vec<_>>(), ["Web", "Files", "Terminal"]);
}

#[test]
fn typing_finds_tools_by_name_or_the_command_they_replace() {
    let desktop = desktop();
    for (query, name) in [
        ("ripgrep", "ripgrep"),
        ("btop", "btop"),
        ("neovim", "Neovim"),
        ("pihole", "AdGuard Home"),
        ("tmux", "Zellij"),
        ("neofetch", "fastfetch"),
    ] {
        let first = &results(&desktop, "Super", query, INSTALL, &available)[0];
        assert_eq!(first.label, name, "searching {query:?}");
        assert!(matches!(first.command, Cmd::Package(_)), "{name} must install from the launcher");
        assert_eq!(first.status.action, "Install");
    }
    let editors = results(&desktop, "Super", "editor", INSTALL, &available);
    assert!(editors.len() >= 3, "several editors: {}", editors.len());
    assert!(editors.iter().all(|entry| entry.section == INSTALL));
}

#[test]
fn every_install_state_shows_what_is_happening_and_what_enter_does() {
    let desktop = desktop();
    let row = |availability: Availability| {
        let state = availability.clone();
        let found = results(&desktop, "Super", "ripgrep", APPS, &move |_| state.clone());
        let entry = found.into_iter().next().unwrap();
        (entry.status, entry.hint, entry.description, entry.running)
    };
    let (status, hint, _, running) = row(Availability::Available);
    assert_eq!((status.state, status.action.as_str(), status.keeps_open, status.removable, running), (1, "Install", true, false, false));
    assert_eq!(hint, "Terminal");
    let (status, hint, description, _) = row(Availability::Working {
        fraction: 0.456,
        message: "Downloading ripgrep 15.2.0: 0.9 of 1.9 MB".into(),
        removing: false,
    });
    assert_eq!((status.state, status.keeps_open, hint.as_str()), (3, true, "46%"));
    assert!((status.progress - 0.456).abs() < 1e-6);
    assert!(description.starts_with("Downloading ripgrep"), "the row narrates the real stage");
    let (status, hint, _, _) = row(Availability::Queued { removing: false });
    assert_eq!((status.state, status.action.as_str(), hint.as_str()), (2, "", "Waiting…"));
    let (status, hint, description, running) = row(Availability::Installed { tag: "15.2.0".into(), update: None });
    assert_eq!((status.state, status.action.as_str(), status.keeps_open, status.removable, running), (4, "Open", false, true, true));
    assert_eq!(hint, "Installed · 15.2.0");
    assert!(description.contains("grep"));
    let (status, hint, description, _) = row(Availability::Installed { tag: "15.1.0".into(), update: Some("15.2.0".into()) });
    assert_eq!((status.state, status.action.as_str(), status.keeps_open), (5, "Update", true));
    assert_eq!(hint, "Update available");
    assert!(description.starts_with("15.1.0 → 15.2.0"));
    let (status, hint, description, _) = row(Availability::Failed("Cannot reach GitHub. Check the network connection and try again.".into()));
    assert_eq!((status.state, status.action.as_str(), hint.as_str()), (6, "Retry", "Retry"));
    assert!(description.starts_with("Cannot reach GitHub"), "the reason is shown, not hidden in a log");
}

#[test]
fn unsupported_desktop_entries_have_a_reason_and_incompatible_artifacts_are_filtered() {
    let state = |package: &apps::Package| {
        if package.id == "blender" || !package.native {
            Availability::Unavailable("This device cannot run the package.".into())
        } else {
            Availability::Available
        }
    };
    let entries = results(&desktop(), "Super", "", INSTALL, &state);
    assert_eq!(entries.len(), 5);
    assert_eq!(entries[0].label, "Blender");
    assert_eq!(entries[0].status.state, 7);
    assert!(entries[0].status.action.is_empty());
    assert!(entries[0].description.contains("cannot run"));
    assert!(results(&desktop(), "Super", "btop", INSTALL, &state).is_empty());
}

#[test]
fn a_built_in_tool_still_wins_over_an_installable_app_with_the_same_word() {
    let desktop = desktop();
    assert_eq!(
        results(&desktop, "Super", "terminal", APPS, &available)[0].command,
        Cmd::Open(Kind::Terminal),
        "an exact built-in name outranks other tabs even when Apps is open"
    );
}

#[test]
fn tabs_are_ordered_apps_then_commands_then_settings_and_the_launcher_lands_on_apps() {
    assert_eq!((APPS, COMMANDS, SETTINGS, INSTALL), (0, 1, 2, 3));
}

fn with_installed(installed: &'static [&'static str]) -> impl Fn(&apps::Package) -> Availability {
    move |package| {
        if installed.contains(&package.id.as_str()) {
            Availability::Installed { tag: "1.0".into(), update: None }
        } else {
            Availability::Available
        }
    }
}

#[test]
fn installed_apps_come_first_under_their_own_heading_and_the_rest_follow() {
    let desktop = desktop();
    let state = with_installed(&["jq", "btop"]);
    let listed = results(&desktop, "Super", "", APPS, &state);
    assert_eq!(listed.len(), 5, "built-in and installed apps are listed");
    assert_eq!(listed[..3].iter().map(|entry| entry.label.as_str()).collect::<Vec<_>>(), ["Web", "Files", "Terminal"]);
    let listed = &listed[3..];
    let labels: Vec<_> = listed.iter().map(|entry| entry.label.as_str()).collect();
    assert_eq!(&labels[..2], ["jq", "btop"], "installed first, in catalogue order");
    assert!(listed[..2].iter().all(packages::is_installed));
    assert!(listed[2..].iter().all(|entry| !packages::is_installed(entry)));
    assert_eq!(listed[0].heading, "INSTALLED");
    assert_eq!(listed[1].heading, "");
}

#[test]
fn with_nothing_installed_there_is_one_heading_and_no_empty_installed_section() {
    let desktop = desktop();
    let listed = results(&desktop, "Super", "", APPS, &available);
    assert_eq!(listed.len(), 3);
    assert!(listed.iter().all(|entry| entry.heading.is_empty()));
}

#[test]
fn installing_apps_stay_put_and_headings_only_appear_without_a_search() {
    let desktop = desktop();
    let state = |package: &apps::Package| {
        if package.id == "fd" {
            Availability::Working { fraction: 0.4, message: "Downloading".into(), removing: false }
        } else {
            Availability::Available
        }
    };
    let listed = results(&desktop, "Super", "", INSTALL, &state);
    let idle = results(&desktop, "Super", "", INSTALL, &available);
    assert!(listed.iter().map(|entry| &entry.label).eq(idle.iter().map(|entry| &entry.label)),
            "an app mid-install does not change the catalogue order");
    let searched = results(&desktop, "Super", "find", APPS, &with_installed(&["fd"]));
    assert!(searched.iter().all(|entry| entry.heading.is_empty()), "search results are one flat list");
    assert_eq!(searched[0].label, "fd", "a match you already have is listed first");
}

#[test]
fn only_a_few_apps_and_the_built_in_browser_carry_the_recommended_badge() {
    let desktop = desktop();
    let listed = results(&desktop, "Super", "", INSTALL, &available);
    let badged: Vec<_> = listed.iter().filter(|entry| !entry.badge.is_empty()).map(|entry| entry.label.as_str()).collect();
    assert!(badged.contains(&"ripgrep") && badged.contains(&"btop"), "{badged:?}");
    assert!(badged.len() < listed.len() / 3, "a badge on everything means nothing");
    let tools = results(&desktop, "Super", "", SETTINGS, &available);
    let web = tools.iter().find(|entry| entry.command == Cmd::Open(Kind::Web)).unwrap();
    assert_eq!(web.badge, "Recommended", "ANGI Web is the tuned WebKit browser");
    assert_eq!(tools.iter().filter(|entry| !entry.badge.is_empty()).count(), 1);
}
