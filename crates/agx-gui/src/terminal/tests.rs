//! Exercise the integration contract, not Alacritty's own parser implementation.
use super::{
    render,
    session::{AgentRun, Proxy, Session, Size, agent_arguments},
};
use alacritty_terminal::{
    Term,
    grid::{Dimensions, Scroll},
    term::Config,
    vte::ansi::Processor,
};
use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
fn terminal() -> Term<Proxy> {
    let size = Size {
        cols: 80,
        lines: 24,
    };
    Term::new(Config::default(), &size, Proxy::new(size))
}
fn feed(term: &mut Term<Proxy>, bytes: &[u8]) {
    let mut parser: Processor = Processor::new();
    parser.advance(term, bytes);
}
#[test]
fn native_rows_preserve_truecolor_styles_and_wide_unicode() {
    let mut term = terminal();
    feed(&mut term, b"\x1b[38;2;12;34;56;4mRGB\x1b[0m");
    let rows = render::rows(&term);
    assert_eq!(rows[0][0].text, "RGB");
    assert!(rows[0][0].underline);
    assert_eq!(rows[0][0].fg, slint::Color::from_rgb_u8(12, 34, 56));
    feed(&mut term, "\r\nA界e\u{301}".as_bytes());
    let rows = render::rows(&term);
    assert!(rows[1][0].text.starts_with("A界e\u{301}"));
    assert_eq!(rows[1].iter().map(|r| r.cells).sum::<usize>(), 80);
}
#[test]
fn alternate_screen_restores_primary_and_scrollback_is_searchable() {
    let mut term = terminal();
    feed(&mut term, b"primary\x1b[?1049hsecondary");
    assert!(
        render::rows(&term)[0]
            .iter()
            .any(|r| r.text.contains("secondary"))
    );
    feed(&mut term, b"\x1b[?1049l");
    assert!(render::rows(&term)[0][0].text.starts_with("primary"));
    for _ in 0..30 {
        feed(&mut term, b"\r\nscrollback");
    }
    assert!(term.history_size() > 0);
    term.scroll_display(Scroll::Top);
    assert!(render::rows(&term)[0][0].text.starts_with("primary"));
}
#[test]
fn shell_pty_resizes_and_closing_it_reaps_the_child() {
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let mut session = Session::new(Size {
        cols: 80,
        lines: 24,
    })
    .unwrap();
    session.resize(Size {
        cols: 100,
        lines: 30,
    });
    session.write(b"printf '__ANGI_PTY_OK__'; stty size\r".to_vec());
    let start = Instant::now();
    loop {
        let rows = render::rows(&session.term.lock());
        if rows.iter().any(|r| {
            r.iter()
                .any(|run| run.text.contains("__ANGI_PTY_OK__30 100"))
        }) {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "PTY output/resize was not received"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    session.write(b"exit\r".to_vec());
    while !session.signals.exited.load(Ordering::Acquire) {
        assert!(start.elapsed() < Duration::from_secs(6));
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn agent_arguments_select_the_operating_system_or_a_project_folder() {
    assert_eq!(
        agent_arguments("make it".into(), AgentRun::Os { single: false }),
        ["angi", "--prompt", "make it"]
    );
    assert_eq!(
        agent_arguments("make it".into(), AgentRun::Project { folder: "/home/dana/shop".into(), single: false }),
        ["angi", "--prompt", "make it", "--project", "/home/dana/shop"]
    );
    // A brief that looks like a flag stays one argument, so it can never choose the folder or the mode.
    assert_eq!(
        agent_arguments("--project /etc".into(), AgentRun::Os { single: false }),
        ["angi", "--prompt", "--project /etc"]
    );
    assert_eq!(
        agent_arguments("--chat".into(), AgentRun::Os { single: false }),
        ["angi", "--prompt", "--chat"]
    );
    assert_eq!(
        agent_arguments("make it".into(), AgentRun::Os { single: true }),
        ["angi", "--prompt", "make it", "--single"]
    );
    assert_eq!(
        agent_arguments("make it".into(), AgentRun::Project { folder: "/home/dana/shop".into(), single: true }),
        ["angi", "--prompt", "make it", "--project", "/home/dana/shop", "--single"]
    );
}
