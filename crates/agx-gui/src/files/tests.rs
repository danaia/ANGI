//! Behavior of the file engine: sorting, selection, operations, undo, and the browser's commands.
use super::{
    entry::Entry,
    hub::{Effect, Hub, PICKER, WINDOW},
    listing::{self, Sort, SortKey, natural},
    model::{FileModel, View},
    ops::{self, Action, Op},
    trash,
    worker::Reply,
};
use slint::Model;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{atomic::AtomicU64, mpsc},
    time::{Duration, Instant},
};

/// A directory under the system temp folder, removed on drop.
struct Scratch(PathBuf);
impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("agx-files-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn file(&self, rel: &str, text: &str) -> PathBuf {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        path
    }
    fn dir(&self, rel: &str) -> PathBuf {
        let path = self.0.join(rel);
        fs::create_dir_all(&path).unwrap();
        path
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn names(model: &FileModel) -> Vec<String> {
    (0..model.len()).map(|i| model.entry(i).unwrap().name).collect()
}

fn quiet() -> impl FnMut(usize, usize, &str) {
    |_, _, _| {}
}

#[test]
fn natural_order_compares_digit_runs_by_value_and_ignores_case() {
    let mut list = ["file10", "File2", "file1", "file02", "Zed", "alpha"];
    list.sort_by(|a, b| natural(a, b));
    assert_eq!(list, ["alpha", "file1", "File2", "file02", "file10", "Zed"]);
}

#[test]
fn listing_puts_folders_first_and_sorts_by_column_in_both_directions() {
    let tmp = Scratch::new("sort");
    tmp.file("b.txt", "12345");
    tmp.file("a.txt", "1");
    tmp.file("c.log", "123");
    tmp.dir("zdir");
    tmp.dir("adir");
    let model = FileModel::default();
    let mut view = View::default();
    model.replace(listing::list(&tmp.0).unwrap(), &view, &[]);
    assert_eq!(names(&model), ["adir", "zdir", "a.txt", "b.txt", "c.log"]);
    view.sort = Sort { key: SortKey::Size, ascending: false };
    model.restyle(&view);
    assert_eq!(names(&model), ["adir", "zdir", "b.txt", "c.log", "a.txt"]);
    view.sort = Sort { key: SortKey::Type, ascending: true };
    model.restyle(&view);
    assert_eq!(names(&model)[2], "c.log");
}

#[test]
fn hidden_files_filter_and_folder_only_views_restrict_rows() {
    let tmp = Scratch::new("hidden");
    tmp.file(".secret", "");
    tmp.file("notes.md", "");
    tmp.dir("Docs");
    let entries = listing::list(&tmp.0).unwrap();
    let model = FileModel::default();
    let mut view = View::default();
    model.replace(entries, &view, &[]);
    assert_eq!(names(&model), ["Docs", "notes.md"]);
    view.show_hidden = true;
    model.restyle(&view);
    assert_eq!(model.len(), 3);
    view.filter = "do".into();
    model.restyle(&view);
    assert_eq!(names(&model), ["Docs"]);
    view = View { folders_only: true, show_hidden: true, ..View::default() };
    model.restyle(&view);
    assert_eq!(names(&model), ["Docs"]);
}

#[test]
fn selection_supports_click_ctrl_shift_keyboard_and_survives_filtering() {
    let entries: Vec<_> = ["a", "b", "c", "d", "e"].iter().map(|n| Entry { name: (*n).into(), kind: super::entry::Kind::File, link: false, size: 10, modified: 0, executable: false }).collect();
    let model = FileModel::default();
    let view = View::default();
    model.replace(entries, &view, &[]);
    model.click(1, false, false);
    model.click(3, true, false);
    let picked = |m: &FileModel| m.selected().into_iter().map(|e| e.name).collect::<Vec<_>>();
    assert_eq!(picked(&model), ["b", "d"]);
    model.click(3, false, true);
    assert_eq!(picked(&model), ["d"], "shift extends from the anchor, which the ctrl-click moved to d");
    model.click(0, false, false);
    model.click(2, false, true);
    assert_eq!(picked(&model), ["a", "b", "c"]);
    model.move_cursor(4, true);
    assert_eq!(picked(&model), ["a", "b", "c", "d", "e"]);
    assert_eq!(model.stats().selected_bytes, 50);
    model.move_cursor(0, false);
    assert_eq!(picked(&model), ["a"]);
    model.restyle(&View { filter: "b".into(), ..View::default() });
    assert!(picked(&model).is_empty(), "rows hidden by a filter are never selected");
    assert_eq!(model.find_prefix("D", 0), None);
}

#[test]
fn rows_report_selection_and_cursor_as_they_move() {
    let entries: Vec<_> = (0..1000)
        .map(|i| Entry { name: format!("f{i:04}"), kind: super::entry::Kind::File, link: false, size: 1, modified: 0, executable: false })
        .collect();
    let model = FileModel::default();
    model.replace(entries, &View::default(), &[]);
    model.click(10, false, false);
    assert!(model.row_data(10).unwrap().selected && model.row_data(10).unwrap().cursor);
    model.click(20, false, false);
    assert!(!model.row_data(10).unwrap().selected && !model.row_data(10).unwrap().cursor);
    assert!(model.row_data(20).unwrap().selected);
    assert_eq!(model.row_data(20).unwrap().size, "1 B");
}

#[test]
fn search_finds_names_in_subfolders_and_reports_their_relative_path() {
    let tmp = Scratch::new("search");
    tmp.file("src/main.rs", "");
    tmp.file("src/deep/Main.md", "");
    tmp.file("other.txt", "");
    let latest = AtomicU64::new(7);
    let mut found: Vec<_> = listing::search(&tmp.0, "MAIN", 7, &latest).into_iter().map(|e| e.name).collect();
    found.sort();
    assert_eq!(found, ["src/deep/Main.md", "src/main.rs"]);
    assert!(listing::search(&tmp.0, "main", 6, &latest).is_empty(), "a superseded request stops at once");
}

#[test]
fn unique_names_never_collide_and_keep_extensions() {
    let tmp = Scratch::new("unique");
    tmp.file("report.txt", "");
    tmp.file("report (copy).txt", "");
    tmp.file("a.tar.gz", "");
    tmp.dir("docs");
    assert_eq!(ops::unique_name(&tmp.0, "fresh.txt"), "fresh.txt");
    assert_eq!(ops::unique_name(&tmp.0, "report.txt"), "report (copy 2).txt");
    assert_eq!(ops::unique_name(&tmp.0, "a.tar.gz"), "a (copy).tar.gz");
    assert_eq!(ops::unique_name(&tmp.0, "docs"), "docs (copy)");
}

#[test]
fn copy_is_recursive_keeps_symlinks_and_refuses_to_enter_itself() {
    let tmp = Scratch::new("copy");
    tmp.file("proj/src/lib.rs", "code");
    std::os::unix::fs::symlink("src/lib.rs", tmp.0.join("proj/link")).unwrap();
    let into = tmp.dir("dest");
    let out = ops::run(Op::Copy { items: vec![tmp.0.join("proj")], into: into.clone() }, &mut quiet());
    assert!(out.errors.is_empty(), "{:?}", out.errors);
    assert_eq!(fs::read_to_string(into.join("proj/src/lib.rs")).unwrap(), "code");
    assert_eq!(fs::read_link(into.join("proj/link")).unwrap(), Path::new("src/lib.rs"));
    let again = ops::run(Op::Copy { items: vec![tmp.0.join("proj")], into: tmp.0.join("proj/src") }, &mut quiet());
    assert!(again.action.is_none() && again.errors[0].contains("inside itself"), "{:?}", again.errors);
    let twice = ops::run(Op::Copy { items: vec![tmp.0.join("proj")], into }, &mut quiet());
    assert!(twice.results[0].ends_with("proj (copy)"), "a taken name gets a suffix, never an overwrite");
}

#[test]
fn rename_refuses_bad_and_taken_names_and_undo_redo_round_trips() {
    let tmp = Scratch::new("rename");
    let a = tmp.file("a.txt", "A");
    tmp.file("b.txt", "B");
    assert!(ops::run(Op::Rename { path: a.clone(), name: "b.txt".into() }, &mut quiet()).action.is_none());
    assert!(ops::run(Op::Rename { path: a.clone(), name: "x/y".into() }, &mut quiet()).action.is_none());
    assert_eq!(fs::read_to_string(tmp.0.join("b.txt")).unwrap(), "B", "nothing was overwritten");
    let done = ops::run(Op::Rename { path: a.clone(), name: "c.txt".into() }, &mut quiet());
    let action = done.action.unwrap();
    assert!(tmp.0.join("c.txt").exists() && !a.exists());
    let undone = ops::run(Op::Undo(action), &mut quiet());
    assert!(a.exists() && !tmp.0.join("c.txt").exists());
    let redone = ops::run(Op::Redo(undone.action.unwrap()), &mut quiet());
    assert!(tmp.0.join("c.txt").exists() && redone.action.is_some());
}

#[test]
fn trash_is_recoverable_and_undo_never_overwrites_a_new_file() {
    let tmp = Scratch::new("trash");
    let bin = Scratch::new("trash-bin");
    let file = tmp.file("keep.txt", "precious");
    let stored = trash::trash_with(&file, Some(bin.0.join("Trash"))).unwrap();
    assert!(!file.exists());
    let info = fs::read_to_string(&stored.info).unwrap();
    assert!(info.contains(&format!("Path={}", file.display())) && info.contains("DeletionDate="));
    fs::write(&file, "new").unwrap();
    assert!(trash::restore(&stored).is_err(), "restoring over an existing file must fail");
    assert_eq!(fs::read_to_string(&file).unwrap(), "new");
    fs::remove_file(&file).unwrap();
    trash::restore(&stored).unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), "precious");
    assert!(!stored.info.exists());
}

#[test]
fn undoing_a_trash_action_restores_and_redo_trashes_again() {
    let tmp = Scratch::new("undo-trash");
    let bin = Scratch::new("undo-trash-bin");
    let file = tmp.file("doc.txt", "text");
    let stored = trash::trash_with(&file, Some(bin.0.join("Trash"))).unwrap();
    let undone = ops::run(Op::Undo(Action::Trashed { items: vec![stored] }), &mut quiet());
    assert_eq!(fs::read_to_string(&file).unwrap(), "text");
    assert_eq!(undone.results, [file.clone()]);
    // Undoing again has nothing to restore: it must report failure, not push a bogus record.
    let missing = ops::run(Op::Undo(undone.action.unwrap()), &mut quiet());
    assert!(missing.action.is_none(), "{}", missing.summary);
}

#[test]
fn permanent_delete_removes_trees_without_following_links() {
    let tmp = Scratch::new("delete");
    let outside = tmp.file("outside/secret.txt", "x");
    tmp.file("victim/inner/f.txt", "y");
    std::os::unix::fs::symlink(tmp.0.join("outside"), tmp.0.join("victim/link")).unwrap();
    let out = ops::run(Op::Delete { items: vec![tmp.0.join("victim")] }, &mut quiet());
    assert!(out.errors.is_empty(), "{:?}", out.errors);
    assert!(!tmp.0.join("victim").exists());
    assert!(outside.exists(), "a symlink inside a deleted folder must not delete its target");
}

#[test]
fn create_validates_names() {
    let tmp = Scratch::new("create");
    assert!(ops::valid_name("").is_err() && ops::valid_name("..").is_err() && ops::valid_name("a/b").is_err());
    assert!(ops::valid_name("ok name.txt").is_ok());
    let made = ops::run(Op::Create { dir: tmp.0.clone(), name: "New folder".into(), folder: true }, &mut quiet());
    assert!(tmp.0.join("New folder").is_dir() && made.action.is_some());
    let dup = ops::run(Op::Create { dir: tmp.0.clone(), name: "New folder".into(), folder: true }, &mut quiet());
    assert!(dup.action.is_none() && dup.summary.contains("already exists"));
}

// --- the browser as a whole ----------------------------------------------------------------

struct Harness {
    hub: Hub,
    inbox: mpsc::Receiver<Reply>,
}
impl Harness {
    fn new() -> Self {
        let (tx, inbox) = mpsc::channel();
        let mut h = Self { hub: Hub::new(tx), inbox };
        h.settle();
        h
    }
    /// Applies worker replies until nothing is outstanding.
    fn settle(&mut self) {
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            while let Ok(reply) = self.inbox.try_recv() {
                self.hub.handle(reply);
            }
            if !self.hub.busy() {
                // One more pass: a finished operation queues a reload.
                std::thread::sleep(Duration::from_millis(30));
                if self.inbox.try_recv().map(|r| self.hub.handle(r)).is_err() && !self.hub.busy() {
                    return;
                }
            }
            assert!(Instant::now() < end, "worker did not settle");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn cmd(&mut self, pane: usize, name: &str, arg: &str) -> Effect {
        let effect = self.hub.command(pane, name, arg);
        self.settle();
        effect
    }
    fn rows(&self, pane: usize) -> Vec<String> {
        names(&self.hub.panes[pane].model)
    }
    fn select(&mut self, pane: usize, name: &str) {
        let row = self.rows(pane).iter().position(|n| n == name).expect("row exists");
        self.hub.select(pane, row, false, false);
    }
}

#[test]
fn python_server_targets_one_folder_or_explicit_current_directory() {
    let tmp = Scratch::new("server-command");
    let folder = tmp.dir("site with spaces");
    tmp.file("file.txt", "");
    let mut h = Harness::new();
    h.cmd(WINDOW, "navigate", tmp.0.to_str().unwrap());
    assert_eq!(h.cmd(WINDOW, "python-server", ""), Effect::None);
    h.select(WINDOW, "file.txt");
    assert_eq!(h.cmd(WINDOW, "python-server", ""), Effect::None);
    h.select(WINDOW, "site with spaces");
    assert_eq!(h.cmd(WINDOW, "python-server", ""), Effect::PythonServer(folder));
    assert_eq!(h.cmd(WINDOW, "python-server-here", ""), Effect::PythonServer(tmp.0.clone()));
    let row = h.rows(WINDOW).iter().position(|name| name == "file.txt").unwrap();
    h.hub.select(WINDOW, row, true, false);
    assert_eq!(h.cmd(WINDOW, "python-server", ""), Effect::None);
    assert_eq!(h.cmd(PICKER, "python-server-here", ""), Effect::None);
}

#[test]
fn browsing_history_up_and_typed_paths() {
    let tmp = Scratch::new("browse");
    tmp.file("one/two/f.txt", "");
    let mut h = Harness::new();
    h.cmd(WINDOW, "navigate", tmp.0.to_str().unwrap());
    assert_eq!(h.rows(WINDOW), ["one"]);
    h.cmd(WINDOW, "navigate", "one/two");
    assert_eq!(h.hub.panes[WINDOW].dir, tmp.0.join("one/two"));
    assert_eq!(h.rows(WINDOW), ["f.txt"]);
    h.cmd(WINDOW, "up", "");
    assert_eq!(h.rows(WINDOW), ["two"]);
    assert_eq!(h.hub.panes[WINDOW].model.selected()[0].name, "two", "going up selects where you came from");
    h.cmd(WINDOW, "back", "");
    assert_eq!(h.hub.panes[WINDOW].dir, tmp.0.join("one/two"));
    h.cmd(WINDOW, "forward", "");
    assert_eq!(h.hub.panes[WINDOW].dir, tmp.0.join("one"));
    h.cmd(WINDOW, "navigate", &format!("{}/one/../one/two/f.txt", tmp.0.display()));
    assert_eq!(h.hub.panes[WINDOW].dir, tmp.0.join("one/two"), "a file path opens its folder");
    assert_eq!(h.hub.panes[WINDOW].model.selected()[0].name, "f.txt");
    h.cmd(WINDOW, "navigate", "/definitely/not/here");
    assert!(h.hub.panes[WINDOW].error.contains("does not exist"));
    assert_eq!(h.hub.panes[WINDOW].dir, tmp.0.join("one/two"), "a failed jump leaves the view where it was");
}

#[test]
fn create_rename_copy_paste_cut_and_undo_through_the_browser() {
    let tmp = Scratch::new("flow");
    tmp.file("notes.txt", "hello");
    tmp.dir("archive");
    let mut h = Harness::new();
    h.cmd(WINDOW, "navigate", tmp.0.to_str().unwrap());

    h.cmd(WINDOW, "new-folder", "");
    h.cmd(WINDOW, "prompt-accept", "Projects");
    assert!(h.rows(WINDOW).contains(&"Projects".to_string()));
    assert_eq!(h.hub.panes[WINDOW].model.selected()[0].name, "Projects", "the new item is selected");

    h.select(WINDOW, "notes.txt");
    h.cmd(WINDOW, "rename", "");
    h.cmd(WINDOW, "prompt-accept", "todo.txt");
    assert!(h.rows(WINDOW).contains(&"todo.txt".to_string()) && !h.rows(WINDOW).contains(&"notes.txt".to_string()));

    h.select(WINDOW, "todo.txt");
    h.cmd(WINDOW, "copy", "");
    h.cmd(WINDOW, "paste", "");
    assert!(h.rows(WINDOW).contains(&"todo (copy).txt".to_string()), "{:?}", h.rows(WINDOW));

    h.select(WINDOW, "todo (copy).txt");
    h.cmd(WINDOW, "cut", "");
    h.cmd(WINDOW, "navigate", "archive");
    h.cmd(WINDOW, "paste", "");
    assert_eq!(h.rows(WINDOW), ["todo (copy).txt"]);
    h.cmd(WINDOW, "paste", "");
    assert_eq!(h.rows(WINDOW).len(), 1, "a cut clipboard is spent after one paste");

    h.cmd(WINDOW, "undo", "");
    assert!(h.rows(WINDOW).is_empty(), "undo moved the file back");
    h.cmd(WINDOW, "navigate", tmp.0.to_str().unwrap());
    assert!(h.rows(WINDOW).contains(&"todo (copy).txt".to_string()));
    h.cmd(WINDOW, "redo", "");
    h.cmd(WINDOW, "navigate", "archive");
    assert_eq!(h.rows(WINDOW), ["todo (copy).txt"]);
}

#[test]
fn filter_search_and_hidden_toggle() {
    let tmp = Scratch::new("find");
    tmp.file("src/alpha.rs", "");
    tmp.file("beta.rs", "");
    tmp.file(".hidden", "");
    let mut h = Harness::new();
    h.cmd(WINDOW, "navigate", tmp.0.to_str().unwrap());
    assert_eq!(h.rows(WINDOW), ["src", "beta.rs"]);
    h.cmd(WINDOW, "filter", "BE");
    assert_eq!(h.rows(WINDOW), ["beta.rs"]);
    h.cmd(WINDOW, "filter", "");
    h.cmd(WINDOW, "search", "alpha");
    assert_eq!(h.rows(WINDOW), ["src/alpha.rs"]);
    h.cmd(WINDOW, "new-file", "");
    assert!(h.hub.panes[WINDOW].prompt.is_none(), "nothing can be created inside search results");
    h.cmd(WINDOW, "clear-search", "");
    assert_eq!(h.rows(WINDOW), ["src", "beta.rs"]);
    h.cmd(WINDOW, "hidden", "");
    assert_eq!(h.rows(WINDOW), ["src", ".hidden", "beta.rs"]);
}

#[test]
fn picker_shows_only_folders_and_chooses_the_selection_or_current_folder() {
    let tmp = Scratch::new("picker");
    tmp.file("plain.txt", "");
    tmp.dir("app");
    tmp.dir("lib");
    let mut h = Harness::new();
    h.hub.open_picker(Some(tmp.0.clone()), false);
    h.settle();
    assert_eq!(h.rows(PICKER), ["app", "lib"]);
    assert_eq!(h.cmd(PICKER, "choose", ""), Effect::Choose(tmp.0.clone()), "no selection chooses the folder being viewed");
    h.select(PICKER, "lib");
    assert_eq!(h.cmd(PICKER, "choose", ""), Effect::Choose(tmp.0.join("lib")));
    assert_ne!(h.hub.panes[WINDOW].dir, tmp.0, "the two browsers are independent");
}

#[test]
fn attach_picker_lists_files_and_folders_and_chooses_the_selection() {
    let tmp = Scratch::new("attach");
    tmp.file("a.png", "");
    tmp.file("b.txt", "");
    tmp.dir("sub");
    let mut h = Harness::new();
    h.hub.open_picker(Some(tmp.0.clone()), true);
    h.settle();
    assert_eq!(h.rows(PICKER), ["sub", "a.png", "b.txt"]);
    assert_eq!(h.cmd(PICKER, "choose", ""), Effect::Attach(vec![tmp.0.clone()]), "nothing selected attaches the folder being viewed");
    h.select(PICKER, "a.png");
    assert_eq!(h.cmd(PICKER, "choose", ""), Effect::Attach(vec![tmp.0.join("a.png")]));
    h.select(PICKER, "sub");
    assert_eq!(h.cmd(PICKER, "choose", ""), Effect::Attach(vec![tmp.0.join("sub")]), "a selected folder can be attached");
}

#[test]
fn keyboard_moves_selects_and_jumps_by_typing() {
    let tmp = Scratch::new("keys");
    for n in ["apple", "avocado", "banana", "cherry"] {
        tmp.file(n, "");
    }
    let mut h = Harness::new();
    h.cmd(WINDOW, "navigate", tmp.0.to_str().unwrap());
    let key = |h: &mut Harness, k: &str, ctrl: bool, shift: bool| h.hub.key(WINDOW, k, ctrl, false, shift, 10);
    let current = |h: &Harness| h.hub.panes[WINDOW].model.selected().into_iter().map(|e| e.name).collect::<Vec<_>>();
    key(&mut h, "down", false, false);
    assert_eq!(current(&h), ["apple"]);
    key(&mut h, "down", false, true);
    key(&mut h, "down", false, true);
    assert_eq!(current(&h), ["apple", "avocado", "banana"]);
    key(&mut h, "end", false, false);
    assert_eq!(current(&h), ["cherry"]);
    key(&mut h, "b", false, false);
    assert_eq!(current(&h), ["banana"], "typing jumps to the matching name");
    key(&mut h, "a", true, false);
    assert_eq!(current(&h).len(), 4, "Ctrl+A selects everything");
    assert!(key(&mut h, "q", false, false).is_some() && key(&mut h, "tab", false, false).is_none(), "unknown keys bubble to the desktop");
    key(&mut h, "escape", false, false);
    assert!(current(&h).is_empty());
}

#[test]
fn permanent_delete_needs_confirmation_and_the_keyboard_can_answer_it() {
    let tmp = Scratch::new("confirm");
    let doomed = tmp.file("doomed.txt", "bye");
    let mut h = Harness::new();
    h.cmd(WINDOW, "navigate", tmp.0.to_str().unwrap());
    h.select(WINDOW, "doomed.txt");
    h.cmd(WINDOW, "delete", "");
    assert!(doomed.exists(), "nothing is deleted before the question is answered");
    h.hub.key(WINDOW, "escape", false, false, false, 10);
    assert!(h.hub.panes[WINDOW].prompt.is_none() && doomed.exists());
    h.cmd(WINDOW, "delete", "");
    h.hub.key(WINDOW, "enter", false, false, false, 10);
    h.settle();
    assert!(!doomed.exists());
}

#[test]
fn a_large_folder_lists_and_sorts_quickly() {
    let tmp = Scratch::new("big");
    for i in 0..20_000 {
        fs::write(tmp.0.join(format!("file{i}.dat")), "").unwrap();
    }
    let start = Instant::now();
    let entries = listing::list(&tmp.0).unwrap();
    let model = FileModel::default();
    model.replace(entries, &View::default(), &[]);
    assert_eq!(model.len(), 20_000);
    assert_eq!(model.entry(0).unwrap().name, "file0.dat");
    assert_eq!(model.entry(19_999).unwrap().name, "file19999.dat");
    assert!(start.elapsed() < Duration::from_secs(3), "took {:?}", start.elapsed());
}
