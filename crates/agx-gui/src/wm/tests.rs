use super::*;

#[test]
fn collapsed_window_moves_as_a_header_and_restores_its_full_size() {
    let mut desktop = desk();
    desktop.cycle_layout();
    let original = desktop.rect_of(Kind::Overview).unwrap();
    desktop.toggle_collapsed(Kind::Overview);
    let collapsed = desktop
        .scene()
        .windows
        .into_iter()
        .find(|w| w.kind == Kind::Overview)
        .unwrap();
    assert!(collapsed.collapsed);
    assert_eq!(collapsed.rect.h, COLLAPSED_HEIGHT);
    assert!(matches!(
        desktop.hit(original.x + 40., original.y + 2.),
        Hit::Header(Kind::Overview)
    ));
    assert!(!matches!(
        desktop.hit(original.x + 40., original.y + 80.),
        Hit::Body(Kind::Overview)
    ));
    desktop.move_floating(Kind::Overview, 40., 60.);
    desktop.resize_floating_edges(
        Kind::Overview,
        Edges {
            bottom: true,
            ..Edges::default()
        },
        0.,
        100.,
    );
    desktop.resize_window(Kind::Overview, 100., 100.);
    desktop.toggle_collapsed(Kind::Overview);
    assert_eq!(
        desktop.rect_of(Kind::Overview),
        Some(Rect::new(
            original.x + 40.,
            original.y + 60.,
            original.w,
            original.h
        ))
    );
}

#[test]
fn layout_round_trip_restores_collapsed_windows_and_floating_positions() {
    let mut desktop = desk();
    let tiled = desktop.rect_of(Kind::Overview).unwrap();
    desktop.toggle_collapsed(Kind::Overview);
    assert!(
        !desktop.is_collapsed(Kind::Overview),
        "tiled headers do not collapse"
    );
    desktop.cycle_layout();
    desktop.move_floating(Kind::Overview, 50., 40.);
    let floating = desktop.rect_of(Kind::Overview).unwrap();
    desktop.toggle_collapsed(Kind::Overview);
    desktop.toggle_collapsed(Kind::Memory);
    desktop.cycle_layout();
    assert_eq!(desktop.mode(), Mode::Tiled);
    assert_eq!(desktop.rect_of(Kind::Overview), Some(tiled));
    assert!(
        desktop
            .scene()
            .windows
            .iter()
            .all(|window| !window.collapsed)
    );
    desktop.cycle_layout();
    assert_eq!(desktop.mode(), Mode::Floating);
    assert!(desktop.is_collapsed(Kind::Overview));
    assert!(desktop.is_collapsed(Kind::Memory));
    assert!(!desktop.is_collapsed(Kind::Cpu));
    assert_eq!(
        desktop.rect_of(Kind::Overview),
        Some(Rect {
            h: COLLAPSED_HEIGHT,
            ..floating
        })
    );
    desktop.toggle_collapsed(Kind::Overview);
    assert_eq!(desktop.rect_of(Kind::Overview), Some(floating));
}

#[test]
fn launcher_reopen_and_fullscreen_expand_collapsed_windows() {
    let mut desktop = desk();
    desktop.cycle_layout();
    desktop.toggle_collapsed(Kind::Overview);
    desktop.open(Kind::Overview);
    assert!(!desktop.is_collapsed(Kind::Overview));
    desktop.toggle_collapsed(Kind::Overview);
    desktop.toggle_fullscreen(Full::Screen);
    assert!(!desktop.scene().windows[0].collapsed);
    desktop.toggle_fullscreen(Full::Screen);
    assert!(!desktop.is_collapsed(Kind::Overview));
    desktop.toggle_collapsed(Kind::Overview);
    desktop.close(Kind::Overview);
    desktop.open(Kind::Overview);
    assert!(!desktop.is_collapsed(Kind::Overview));
}

#[test]
fn collapsed_headers_remain_reachable_after_a_display_resize() {
    let mut desktop = desk();
    desktop.cycle_layout();
    desktop.move_floating(Kind::Overview, 900., 600.);
    desktop.toggle_collapsed(Kind::Overview);
    desktop.set_area(
        Rect::new(0., 0., 640., 400.),
        Rect::new(0., 34., 640., 340.),
    );
    let rect = desktop.rect_of(Kind::Overview).unwrap();
    assert!(rect.x <= 560. && rect.right() >= 80.);
    assert!(rect.y >= 34. && rect.bottom() <= 400.);
}

fn desk() -> Desktop {
    let screen = Rect::new(0.0, 0.0, 1200.0, 800.0);
    // Existing tiling/interaction checks use a three-panel tiled fixture.
    let mut desktop = Desktop::new(screen, Rect::new(0.0, 34.0, 1200.0, 740.0));
    for i in 0..INITIAL_WORKSPACES {
        desktop.goto(i);
        if desktop.mode() != Mode::Tiled {
            desktop.cycle_layout();
        }
    }
    desktop.goto(0);
    for kind in [Kind::Settings, Kind::Overview, Kind::Cpu, Kind::Memory] {
        desktop.close(kind);
    }
    desktop.open(Kind::Overview);
    desktop.open(Kind::Cpu);
    desktop.open(Kind::Memory);
    desktop.focus(Kind::Overview);
    desktop.set_ratio_for(Kind::Overview, 0.55);
    for workspace in &mut desktop.ws {
        workspace.has_windowed_layout = false;
    }
    desktop
}
fn rect(d: &Desktop, k: Kind) -> Rect {
    d.rect_of(k).unwrap()
}

#[test]
fn default_desktop_opens_the_reference_layout_in_windowed_mode() {
    for (width, height) in [(1280., 720.), (1920., 1080.), (2560., 1080.)] {
        let mut d = Desktop::new(
            Rect::new(0., 0., width, height),
            Rect::new(0., 34., width, height - 60.),
        );
        let scene = d.scene();
        assert_eq!(scene.windows.len(), 4);
        assert!(scene.windows.iter().all(|w| w.floating && !w.collapsed));
        assert!(scene.handles.is_empty());
        assert_eq!(d.focused(), Some(Kind::Memory));
        let (s, o, c, m) = (
            rect(&d, Kind::Settings),
            rect(&d, Kind::Overview),
            rect(&d, Kind::Cpu),
            rect(&d, Kind::Memory),
        );
        assert_eq!(s.x, GAP_OUT);
        assert!((s.h - (height - 60. - 2. * GAP_OUT)).abs() < 0.01);
        assert!(s.right() < o.x && o.bottom() < c.y);
        assert_eq!(o.x, c.x);
        assert!(c.right() < m.x);
        assert_eq!(c.y, m.y);
        assert!((m.right() - (width - GAP_OUT)).abs() < 0.01);
        for i in 0..INITIAL_WORKSPACES {
            d.goto(i);
            assert_eq!(d.mode(), Mode::Floating);
        }
        d.goto(0);
        d.cycle_layout();
        assert_eq!(d.mode(), Mode::Tiled);
        assert_eq!(rect(&d, Kind::Settings), s);
    }
}

#[test]
fn initial_windows_follow_display_size_but_custom_positions_survive() {
    let mut d = Desktop::new(
        Rect::new(0., 0., 1280., 720.),
        Rect::new(0., 34., 1280., 660.),
    );
    d.set_area(
        Rect::new(0., 0., 1920., 1080.),
        Rect::new(0., 34., 1920., 1020.),
    );
    assert!((rect(&d, Kind::Memory).right() - (1920. - GAP_OUT)).abs() < 0.01);
    assert!((rect(&d, Kind::Settings).bottom() - (1054. - GAP_OUT)).abs() < 0.01);
    d.move_floating(Kind::Overview, 30., 40.);
    let custom = rect(&d, Kind::Overview);
    d.set_area(
        Rect::new(0., 0., 2560., 1440.),
        Rect::new(0., 34., 2560., 1380.),
    );
    assert_eq!(rect(&d, Kind::Overview), custom);
    assert!((rect(&d, Kind::Memory).right() - (2560. - GAP_OUT)).abs() < 0.01);
}

#[test]
fn opening_splits_the_focused_tile_along_its_long_side() {
    let mut d = desk();
    d.cur = 1; // Agent Angi
    let before = rect(&d, Kind::AgentAngi);
    assert!(before.w > before.h);
    d.open(Kind::Hardware);
    let (n, h) = (rect(&d, Kind::AgentAngi), rect(&d, Kind::Hardware));
    assert!(n.right() <= h.x, "wide tile splits side by side");
    assert_eq!(d.focused(), Some(Kind::Hardware));
}

#[test]
fn close_collapses_the_split() {
    let mut d = desk();
    d.close(Kind::Cpu);
    assert!(!d.is_open(Kind::Cpu));
    let m = rect(&d, Kind::Memory);
    assert!(m.h > 600.0, "memory takes the freed column");
}

#[test]
fn directional_focus_and_swap() {
    let mut d = desk();
    d.focus(Kind::Overview);
    d.focus_dir(Dir::Right);
    assert!(matches!(d.focused(), Some(Kind::Cpu | Kind::Memory)));
    d.focus(Kind::Cpu);
    d.focus_dir(Dir::Down);
    assert_eq!(d.focused(), Some(Kind::Memory));
    d.focus_dir(Dir::Left);
    assert_eq!(d.focused(), Some(Kind::Overview));
    let (o, c) = (rect(&d, Kind::Overview), rect(&d, Kind::Cpu));
    d.swap(Kind::Overview, Kind::Cpu);
    assert_eq!(rect(&d, Kind::Overview), c);
    assert_eq!(rect(&d, Kind::Cpu), o);
}

#[test]
fn toggle_split_flips_orientation() {
    let mut d = desk();
    d.focus(Kind::Cpu);
    let (c0, m0) = (rect(&d, Kind::Cpu), rect(&d, Kind::Memory));
    assert!(c0.bottom() <= m0.y);
    d.toggle_split();
    let (c1, m1) = (rect(&d, Kind::Cpu), rect(&d, Kind::Memory));
    assert!(c1.right() <= m1.x, "now side by side");
}

#[test]
fn keyboard_resize_moves_the_split_from_either_side() {
    let mut d = desk();
    let w0 = rect(&d, Kind::Overview).w;
    d.focus(Kind::Overview);
    d.resize(100.0, 0.0);
    assert!((rect(&d, Kind::Overview).w - w0 - 100.0).abs() < 1.0);
    // The right-hand column grows by moving the same split left.
    d.focus(Kind::Cpu);
    let c0 = rect(&d, Kind::Cpu).w;
    d.resize(100.0, 0.0);
    assert!((rect(&d, Kind::Cpu).w - c0 - 100.0).abs() < 1.0);
}

#[test]
fn float_and_tile_round_trip() {
    let mut d = desk();
    d.focus(Kind::Cpu);
    assert!(d.toggle_float().is_none());
    assert!(d.is_floating(Kind::Cpu));
    assert!(rect(&d, Kind::Memory).h > 600.0, "memory reflows");
    let s = d.scene();
    assert_eq!(
        s.windows.last().unwrap().kind,
        Kind::Cpu,
        "floating draws on top"
    );
    d.toggle_float();
    assert!(!d.is_floating(Kind::Cpu));
}

#[test]
fn untiling_a_float_returns_beside_the_tile_it_covers() {
    let mut d = desk();
    d.focus(Kind::Memory);
    d.toggle_float();
    // Memory floats where it was, over the lower right; CPU grows underneath.
    assert!(rect(&d, Kind::Cpu).h > 600.0);
    d.toggle_float();
    let (c, m) = (rect(&d, Kind::Cpu), rect(&d, Kind::Memory));
    assert!(
        c.bottom() <= m.y,
        "memory is back under cpu, not beside the overview"
    );
    assert!(m.x > rect(&d, Kind::Overview).right());
}

#[test]
fn ultrawide_screen_tiles_edge_to_edge_and_windows_can_go_right() {
    let mut d = desk();
    d.set_area(
        Rect::new(0.0, 0.0, 2560.0, 1080.0),
        Rect::new(0.0, 34.0, 2560.0, 1020.0),
    );
    let right_edge = [Kind::Overview, Kind::Cpu, Kind::Memory]
        .iter()
        .map(|k| rect(&d, *k).right())
        .fold(0.0, f32::max);
    assert!(
        (right_edge - (2560.0 - GAP_OUT)).abs() < 0.01,
        "tiles reach the right edge"
    );
    d.cycle_layout();
    d.move_floating(Kind::Cpu, 1800.0, 0.0);
    assert!(rect(&d, Kind::Cpu).x > 1800.0, "no stale 1280px limit");
}

#[test]
fn window_mode_floats_in_place_and_returns() {
    let mut d = desk();
    let before: Vec<_> = [Kind::Overview, Kind::Cpu, Kind::Memory]
        .iter()
        .map(|k| rect(&d, *k))
        .collect();
    d.cycle_layout();
    assert_eq!(d.mode(), Mode::Floating);
    let during: Vec<_> = [Kind::Overview, Kind::Cpu, Kind::Memory]
        .iter()
        .map(|k| rect(&d, *k))
        .collect();
    assert_eq!(before, during, "windows keep their place when floated");
    d.move_floating(Kind::Cpu, 50.0, 40.0);
    assert!(rect(&d, Kind::Cpu).x > before[1].x);
    assert!(d.scene().handles.is_empty());
    d.cycle_layout();
    assert_eq!(rect(&d, Kind::Cpu), before[1], "tiling restores the layout");
}

#[test]
fn fullscreen_modes() {
    let mut d = desk();
    d.focus(Kind::Memory);
    d.toggle_fullscreen(Full::Screen);
    let s = d.scene();
    assert!(s.screen_full);
    assert_eq!(s.windows.len(), 1);
    assert_eq!(s.windows[0].rect, Rect::new(0.0, 0.0, 1200.0, 800.0));
    d.toggle_fullscreen(Full::Screen);
    d.toggle_fullscreen(Full::Maximized);
    assert_eq!(
        d.scene().windows[0].rect,
        Rect::new(
            GAP_OUT,
            34.0 + GAP_OUT,
            1200.0 - 2.0 * GAP_OUT,
            740.0 - 2.0 * GAP_OUT
        )
    );
}

#[test]
fn move_window_between_workspaces() {
    let mut d = desk();
    d.focus(Kind::Cpu);
    d.move_to_workspace(2, false);
    assert_eq!(d.current(), 0);
    d.goto(2);
    assert!(d.rect_of(Kind::Cpu).is_some());
    assert_eq!(d.scene().windows.len(), 1);
    d.goto(0);
    assert!(d.rect_of(Kind::Cpu).is_none());
    d.focus(Kind::Overview);
    d.move_to_workspace(3, true);
    assert_eq!(d.current(), 3);
    d.cycle_layout();
    let full_rect = rect(&d, Kind::Overview);
    d.toggle_collapsed(Kind::Overview);
    d.cycle_layout();
    d.move_to_workspace(4, false);
    d.goto(4);
    d.cycle_layout();
    assert!(d.is_collapsed(Kind::Overview));
    d.toggle_collapsed(Kind::Overview);
    assert_eq!(rect(&d, Kind::Overview), full_rect);
}

#[test]
fn creating_numbered_workspaces_preserves_existing_windows_and_wraps_navigation() {
    let mut desktop = desk();
    let original = desktop.scene().windows.len();
    desktop.goto(4);
    desktop.move_to_workspace(7, true);
    assert_eq!(desktop.current(), 7);
    assert_eq!(desktop.workspace_count(), 8);
    desktop.next_workspace(false);
    assert_eq!(desktop.current(), 0);
    assert_eq!(desktop.scene().windows.len(), original);
    assert!(!desktop.ensure_workspace(MAX_WORKSPACES));
    assert_eq!(desktop.workspace_count(), 8);
}

#[test]
fn removing_the_active_workspace_keeps_windows_and_the_last_workspace() {
    let mut desktop = desk();
    desktop.goto(1);
    desktop.cycle_layout();
    desktop.goto(0);
    desktop.cycle_layout();
    let full_rect = rect(&desktop, Kind::Overview);
    desktop.toggle_collapsed(Kind::Overview);
    desktop.remove_workspace(0);
    assert_eq!(desktop.current(), 0);
    assert_eq!(desktop.focused(), Some(Kind::Overview));
    assert!(desktop.scene().rect_of(Kind::AgentAngi).is_some());
    assert!(desktop.is_collapsed(Kind::Overview));
    desktop.toggle_collapsed(Kind::Overview);
    assert_eq!(rect(&desktop, Kind::Overview), full_rect);
    for _ in 0..INITIAL_WORKSPACES {
        desktop.remove_workspace(0);
    }
    assert_eq!(desktop.workspace_count(), 1);
    assert!(desktop.scene().rect_of(Kind::Overview).is_some());
}

#[test]
fn closed_panels_reopen_on_the_current_workspace() {
    let mut d = desk();
    d.close(Kind::AgentAngi);
    assert!(!d.occupied(1));
    d.open(Kind::Network);
    assert!(d.rect_of(Kind::Network).is_some());
    assert!(d.occupied(0));
}

#[test]
fn floating_edges_resize_and_clamp() {
    let mut d = desk();
    d.cycle_layout();
    let r0 = rect(&d, Kind::Cpu);
    d.resize_floating_edges(
        Kind::Cpu,
        Edges {
            right: true,
            bottom: true,
            ..Default::default()
        },
        30.0,
        20.0,
    );
    let r1 = rect(&d, Kind::Cpu);
    assert_eq!((r1.w, r1.h), (r0.w + 30.0, r0.h + 20.0));
    d.resize_floating_edges(
        Kind::Cpu,
        Edges {
            left: true,
            ..Default::default()
        },
        10_000.0,
        0.0,
    );
    assert_eq!(rect(&d, Kind::Cpu).w, MIN_W);
}

#[test]
fn hit_testing_finds_buttons_headers_and_bodies() {
    let d = desk();
    let o = rect(&d, Kind::Overview);
    assert!(matches!(
        d.hit(o.x + 40.0, o.y + 10.0),
        Hit::Header(Kind::Overview)
    ));
    assert!(matches!(
        d.hit(o.x + 40.0, o.y + 200.0),
        Hit::Body(Kind::Overview)
    ));
    let close_x = o.right() - BTN_INSET - 5.0;
    assert!(matches!(
        d.hit(close_x, o.y + 10.0),
        Hit::Button(Kind::Overview, Button::Close)
    ));
    assert!(matches!(
        d.hit(close_x - BTN_W, o.y + 10.0),
        Hit::Button(Kind::Overview, Button::Full)
    ));
}

#[test]
fn dragging_a_gap_changes_the_split() {
    let mut d = desk();
    let o = rect(&d, Kind::Overview);
    let gap_x = o.right() + GAP_IN;
    let hit = d.hit(gap_x, 300.0);
    let Hit::Handle(h) = hit else {
        panic!("expected handle, got {hit:?}")
    };
    let ratio = d.handle_ratio(h.split, gap_x + 100.0, 300.0).unwrap();
    d.set_split(h.split, ratio);
    assert!((rect(&d, Kind::Overview).w - o.w - 100.0).abs() < 1.0);
}

#[test]
fn settings_tiles_floats_moves_workspaces_and_reopens_once() {
    let mut d = desk();
    assert!(!d.is_open(Kind::Settings));
    d.open(Kind::Settings);
    assert_eq!(d.focused(), Some(Kind::Settings));
    assert!(!d.is_floating(Kind::Settings));
    let count = d.scene().windows.len();
    d.open(Kind::Settings);
    assert_eq!(d.scene().windows.len(), count);
    d.toggle_float();
    assert!(d.is_floating(Kind::Settings));
    d.move_to_workspace(2, true);
    assert_eq!(d.current(), 2);
    assert_eq!(d.focused(), Some(Kind::Settings));
    d.close(Kind::Settings);
    assert!(!d.is_open(Kind::Settings));
    d.open(Kind::Settings);
    assert_eq!(d.focused(), Some(Kind::Settings));
}

#[test]
fn fresh_desktop_keeps_three_workspaces_empty() {
    let screen = Rect::new(0.0, 0.0, 1200.0, 800.0);
    let mut desktop = Desktop::new(screen, Rect::new(0.0, 34.0, 1200.0, 740.0));
    assert!(desktop.occupied(0));
    desktop.goto(1);
    assert_eq!(desktop.focused(), Some(Kind::AgentAngi));
    for workspace in 2..INITIAL_WORKSPACES {
        desktop.goto(workspace);
        assert!(!desktop.occupied(workspace));
        assert!(desktop.scene().windows.is_empty());
        assert_eq!(desktop.focused(), None);
    }
}
