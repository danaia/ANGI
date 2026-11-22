//! Commands, worker replies, clipboard and undo history shared by both browsers.
use super::{
    listing::SortKey,
    ops::{self, Action, Op},
    pane::{Move, Pane, Prompt},
    places::{self, Place},
    worker::{Reply, Worker},
};
use crate::PaneState;
use std::{
    collections::HashMap,
    fs,
    path::{Component, Path, PathBuf},
    sync::mpsc::Sender,
};

pub const WINDOW: usize = 0;
pub const PICKER: usize = 1;
const HISTORY: usize = 100;

/// Work only the desktop can do, returned from a command for the caller to perform.
#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    None,
    Terminal(PathBuf),
    PythonServer(PathBuf),
    Choose(PathBuf),
    /// Files picked in the picker's attach mode, for the Chat composer.
    Attach(Vec<PathBuf>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Replay {
    Fresh,
    Undo,
    Redo,
}

struct Job {
    replay: Replay,
    /// The action an undo or redo took off its stack, put back if the attempt fails.
    taken: Option<Action>,
    cut: bool,
    verb: &'static str,
}

pub struct Hub {
    pub panes: [Pane; 2],
    pub picker_open: bool,
    worker: Worker,
    clipboard: Option<(Vec<PathBuf>, bool)>,
    undo: Vec<Action>,
    redo: Vec<Action>,
    next: u64,
    jobs: HashMap<u64, Job>,
    places: Vec<Place>,
}

impl Hub {
    pub fn new(replies: Sender<Reply>) -> Self {
        let home = places::home();
        let mut hub = Self {
            panes: [Pane::new(false, home.clone()), Pane::new(true, home.clone())],
            picker_open: false,
            worker: Worker::start(replies),
            clipboard: None,
            undo: vec![],
            redo: vec![],
            next: 0,
            jobs: HashMap::new(),
            places: places::places(),
        };
        hub.visit(WINDOW, home, Move::Reload);
        hub
    }

    /// True while a listing or operation is outstanding, so the UI polls quickly.
    pub fn busy(&self) -> bool {
        !self.jobs.is_empty() || self.panes.iter().any(|p| p.loading)
    }

    pub fn state(&self, pane: usize) -> PaneState {
        let can_paste = self.clipboard.is_some();
        self.panes[pane].state(&self.places, can_paste, !self.undo.is_empty(), !self.redo.is_empty())
    }

    /// Open the picker for folders, or (`attach`) for choosing files to attach to a Chat message.
    pub fn open_picker(&mut self, start: Option<PathBuf>, attach: bool) {
        let start = start.filter(|p| p.is_dir()).unwrap_or_else(places::home);
        let pane = &mut self.panes[PICKER];
        pane.attach = attach;
        pane.view.folders_only = !attach;
        pane.back.clear();
        pane.forward.clear();
        pane.filter.clear();
        pane.view.filter.clear();
        pane.search = None;
        pane.prompt = None;
        pane.status.clear();
        pane.error.clear();
        pane.dir = start.clone();
        self.picker_open = true;
        self.places = places::places();
        self.visit(PICKER, start, Move::Reload);
    }

    fn id(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    // --- navigation ----------------------------------------------------------------------

    pub fn visit(&mut self, pane: usize, dir: PathBuf, how: Move) {
        let request = self.id();
        let p = &mut self.panes[pane];
        p.request = request;
        p.loading = true;
        p.search = None;
        p.target = Some((dir.clone(), how));
        if how != Move::Reload {
            p.filter.clear();
            p.view.filter.clear();
        }
        self.worker.list(pane, request, dir);
    }

    /// Re-reads the current folder, keeping the selection.
    pub fn reload(&mut self, pane: usize) {
        let keep = self.panes[pane].model.selected().into_iter().map(|e| e.name).collect();
        self.reload_selecting(pane, keep);
    }

    fn reload_selecting(&mut self, pane: usize, keep: Vec<String>) {
        let dir = self.panes[pane].dir.clone();
        self.visit(pane, dir, Move::Reload);
        self.panes[pane].reselect = keep;
    }

    /// A path typed or clicked: folders are entered, files reveal themselves in their folder.
    pub fn go(&mut self, pane: usize, text: &str) {
        let path = self.resolve(pane, text);
        match fs::metadata(&path) {
            Ok(m) if m.is_dir() => self.visit(pane, path, Move::Visit),
            Ok(_) => {
                if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
                    self.visit(pane, parent.to_path_buf(), Move::Visit);
                    self.panes[pane].reselect = vec![name.to_string_lossy().into_owned()];
                }
            }
            Err(_) => self.panes[pane].error = format!("“{}” does not exist.", text.trim()),
        }
    }

    fn resolve(&self, pane: usize, text: &str) -> PathBuf {
        let text = text.trim();
        let joined = if text == "~" {
            places::home()
        } else if let Some(rest) = text.strip_prefix("~/") {
            places::home().join(rest)
        } else {
            self.panes[pane].dir.join(text)
        };
        normalize(&joined)
    }

    pub fn back(&mut self, pane: usize) {
        if let Some(dir) = self.panes[pane].back.last().cloned() {
            self.visit(pane, dir, Move::Back);
        }
    }

    pub fn forward(&mut self, pane: usize) {
        if let Some(dir) = self.panes[pane].forward.last().cloned() {
            self.visit(pane, dir, Move::Forward);
        }
    }

    pub fn up(&mut self, pane: usize) {
        let p = &self.panes[pane];
        if let Some(parent) = p.dir.parent().map(Path::to_path_buf) {
            let came_from = p.dir.file_name().map(|n| n.to_string_lossy().into_owned());
            self.visit(pane, parent, Move::Visit);
            self.panes[pane].reselect = came_from.into_iter().collect();
        }
    }

    // --- entry points for the UI ---------------------------------------------------------

    pub fn activate(&mut self, pane: usize, row: usize) -> Effect {
        let Some(entry) = self.panes[pane].model.entry(row) else { return Effect::None };
        let path = self.panes[pane].dir.join(&entry.name);
        if entry.is_dir() {
            self.visit(pane, normalize(&path), Move::Visit);
        } else if pane == PICKER && self.panes[PICKER].attach {
            // Double-clicking a file in the attach picker chooses it.
            return Effect::Attach(vec![path]);
        } else {
            self.panes[pane].status = format!("“{}” cannot be opened yet: no viewer is installed.", entry.name);
        }
        Effect::None
    }

    pub fn command(&mut self, pane: usize, name: &str, arg: &str) -> Effect {
        let selected = match name {
            "trash" | "copy" | "cut" | "new-folder" | "new-file" | "rename" | "delete" => self.panes[pane].model.paths(&self.panes[pane].dir),
            _ => vec![],
        };
        self.panes[pane].status.clear();
        self.panes[pane].error.clear();
        match name {
            "navigate" => self.go(pane, arg),
            "back" => self.back(pane),
            "forward" => self.forward(pane),
            "up" => self.up(pane),
            "home" => self.visit(pane, places::home(), Move::Visit),
            "refresh" => self.reload(pane),
            "sort" => {
                let key = match arg {
                    "size" => SortKey::Size,
                    "modified" => SortKey::Modified,
                    "type" => SortKey::Type,
                    _ => SortKey::Name,
                };
                let p = &mut self.panes[pane];
                p.view.sort.choose(key);
                p.model.restyle(&p.view);
            }
            "hidden" => {
                let p = &mut self.panes[pane];
                p.view.show_hidden ^= true;
                p.model.restyle(&p.view);
            }
            "filter" => self.filter(pane, arg),
            "search" if !arg.trim().is_empty() => self.search(pane, arg.trim()),
            "clear-search" => self.leave_search(pane),
            "new-folder" | "new-file" | "rename" | "delete" => self.ask(pane, name, selected),
            "prompt-accept" => self.accept(pane, arg),
            "prompt-cancel" => self.panes[pane].clear_prompt(),
            "trash" => self.act(pane, Op::Trash { items: selected }, "Moving to Trash"),
            "copy" | "cut" if !selected.is_empty() => {
                let n = selected.len();
                if name == "copy" {
                    crate::agent_attachments::offer_copied(&selected);
                }
                self.clipboard = Some((selected, name == "cut"));
                self.panes[pane].status = format!("{} {n} item{} to the clipboard", if name == "cut" { "Cut" } else { "Copied" }, if n == 1 { "" } else { "s" });
            }
            "paste" => self.paste(pane),
            "select-all" => self.panes[pane].model.select_all(),
            "deselect" => self.panes[pane].model.clear(),
            "undo" => self.replay(pane, true),
            "redo" => self.replay(pane, false),
            "terminal" => return Effect::Terminal(self.terminal_dir(pane)),
            "python-server-here" if pane == WINDOW => {
                self.panes[pane].status = "Starting Python server…".into();
                return Effect::PythonServer(self.panes[pane].dir.clone());
            }
            "python-server" if pane == WINDOW => {
                let p = &mut self.panes[pane];
                if let [one] = p.model.selected().as_slice() {
                    if one.is_dir() {
                        p.status = "Starting Python server…".into();
                        return Effect::PythonServer(p.dir.join(&one.name));
                    }
                }
            }
            "choose" if self.panes[PICKER].attach && pane == PICKER => return Effect::Attach(self.chosen_files()),
            "choose" => return Effect::Choose(self.choice()),
            _ => {}
        }
        Effect::None
    }

    pub fn select(&mut self, pane: usize, row: usize, ctrl: bool, shift: bool) {
        self.panes[pane].model.click(row, ctrl, shift);
    }

    fn terminal_dir(&self, pane: usize) -> PathBuf {
        let p = &self.panes[pane];
        match p.model.selected().as_slice() {
            [one] if one.is_dir() => p.dir.join(&one.name),
            _ => p.dir.clone(),
        }
    }

    /// What "Choose" picks: the one selected folder, else the folder being viewed.
    /// What the attach picker hands over: the selected files and folders, or, with nothing
    /// selected, the folder being viewed.
    fn chosen_files(&self) -> Vec<PathBuf> {
        let p = &self.panes[PICKER];
        let picked = p.model.paths(&p.dir);
        if picked.is_empty() { vec![p.dir.clone()] } else { picked }
    }

    fn choice(&self) -> PathBuf {
        let p = &self.panes[PICKER];
        match p.model.selected().as_slice() {
            [one] if one.is_dir() => normalize(&p.dir.join(&one.name)),
            _ => p.dir.clone(),
        }
    }

    fn filter(&mut self, pane: usize, text: &str) {
        let p = &mut self.panes[pane];
        p.filter = text.to_owned();
        p.view.filter = text.trim().to_lowercase();
        p.model.restyle(&p.view);
        if p.search.is_some() && text.is_empty() {
            self.leave_search(pane);
        }
    }

    fn search(&mut self, pane: usize, query: &str) {
        let request = self.id();
        let p = &mut self.panes[pane];
        p.request = request;
        p.loading = true;
        p.search = Some(query.to_owned());
        p.view.filter.clear();
        p.filter.clear();
        self.worker.search(pane, request, p.dir.clone(), query.to_owned());
    }

    fn leave_search(&mut self, pane: usize) {
        if self.panes[pane].search.is_some() {
            self.reload_selecting(pane, vec![]);
        }
    }

    // --- prompts -------------------------------------------------------------------------

    fn ask(&mut self, pane: usize, name: &str, selected: Vec<PathBuf>) {
        let p = &mut self.panes[pane];
        let searching = p.search.is_some();
        let prompt = match name {
            "new-folder" if !searching => Some(Prompt::NewFolder),
            "new-file" if !searching => Some(Prompt::NewFile),
            "rename" if selected.len() == 1 => Some(Prompt::Rename(selected[0].clone())),
            "delete" if !selected.is_empty() => Some(Prompt::Delete(selected)),
            _ => None,
        };
        if prompt.is_some() {
            p.prompt = prompt;
            p.prompt_serial += 1;
        } else if searching && name.starts_with("new") {
            p.status = "Leave the search to create items.".into();
        }
    }

    fn accept(&mut self, pane: usize, text: &str) {
        let dir = self.panes[pane].dir.clone();
        let Some(prompt) = self.panes[pane].prompt.clone() else { return };
        let name = text.trim();
        if !matches!(prompt, Prompt::Delete(_)) {
            if let Err(message) = ops::valid_name(name) {
                self.panes[pane].status = message;
                return;
            }
        }
        self.panes[pane].clear_prompt();
        match prompt {
            Prompt::NewFolder => self.act(pane, Op::Create { dir, name: name.into(), folder: true }, "Creating"),
            Prompt::NewFile => self.act(pane, Op::Create { dir, name: name.into(), folder: false }, "Creating"),
            Prompt::Rename(path) => self.act(pane, Op::Rename { path, name: name.into() }, "Renaming"),
            Prompt::Delete(items) => self.act(pane, Op::Delete { items }, "Deleting"),
        }
    }

    // --- operations ----------------------------------------------------------------------

    fn act(&mut self, pane: usize, op: Op, verb: &'static str) {
        let empty = match &op {
            Op::Trash { items } | Op::Delete { items } | Op::Copy { items, .. } | Op::Move { items, .. } => items.is_empty(),
            _ => false,
        };
        if !empty {
            self.launch(pane, op, Job { replay: Replay::Fresh, taken: None, cut: false, verb });
        }
    }

    fn launch(&mut self, pane: usize, op: Op, job: Job) {
        let id = self.id();
        self.panes[pane].busy = format!("{}…", job.verb);
        self.jobs.insert(id, job);
        self.worker.run(pane, id, op);
    }

    fn paste(&mut self, pane: usize) {
        let Some((items, cut)) = self.clipboard.clone() else { return };
        if self.panes[pane].search.is_some() {
            return;
        }
        let into = self.panes[pane].dir.clone();
        let (op, verb) = if cut {
            (Op::Move { items, into }, "Moving")
        } else {
            (Op::Copy { items, into }, "Copying")
        };
        self.launch(pane, op, Job { replay: Replay::Fresh, taken: None, cut, verb });
    }

    fn replay(&mut self, pane: usize, undo: bool) {
        let Some(action) = (if undo { self.undo.pop() } else { self.redo.pop() }) else {
            self.panes[pane].status = if undo { "Nothing to undo." } else { "Nothing to redo." }.into();
            return;
        };
        let (op, replay, verb) = if undo {
            (Op::Undo(action.clone()), Replay::Undo, "Undoing")
        } else {
            (Op::Redo(action.clone()), Replay::Redo, "Redoing")
        };
        self.launch(pane, op, Job { replay, taken: Some(action), cut: false, verb });
    }

    // --- replies -------------------------------------------------------------------------

    pub fn handle(&mut self, reply: Reply) {
        match reply {
            Reply::Listed { pane, request, dir, result } => {
                let p = &mut self.panes[pane];
                if p.request != request {
                    return;
                }
                p.loading = false;
                match result {
                    Ok(entries) => {
                        let keep = std::mem::take(&mut p.reselect);
                        p.error.clear();
                        p.arrived(dir);
                        p.model.replace(entries, &p.view, &keep);
                        p.cursor_serial += 1;
                    }
                    Err(message) => {
                        p.target = None;
                        p.reselect.clear();
                        p.error = message;
                    }
                }
            }
            Reply::Found { pane, request, entries } => {
                let p = &mut self.panes[pane];
                if p.request == request {
                    p.loading = false;
                    p.model.replace(entries, &p.view, &[]);
                    p.cursor_serial += 1;
                }
            }
            Reply::Changed { pane } => {
                let p = &self.panes[pane];
                let live = pane == WINDOW || self.picker_open;
                if live && !p.loading && p.search.is_none() {
                    self.reload(pane);
                }
            }
            Reply::Progress { pane, done, total, current } => {
                let verb = self.jobs.values().next().map_or("Working", |j| j.verb);
                self.panes[pane].busy = if total > 1 {
                    format!("{verb} {} of {total}: {current}", done + 1)
                } else {
                    format!("{verb} “{current}”…")
                };
            }
            Reply::Done { pane, request, outcome } => self.finish(pane, request, outcome),
        }
    }

    fn finish(&mut self, pane: usize, request: u64, outcome: ops::Outcome) {
        let job = self.jobs.remove(&request);
        let ok = outcome.action.is_some();
        match (job.as_ref().map(|j| j.replay), outcome.action.clone()) {
            (Some(Replay::Undo), Some(action)) => self.redo.push(action),
            (Some(Replay::Redo), Some(action)) => self.undo.push(action),
            (Some(Replay::Undo), None) => self.undo.extend(job.as_ref().and_then(|j| j.taken.clone())),
            (Some(Replay::Redo), None) => self.redo.extend(job.as_ref().and_then(|j| j.taken.clone())),
            (_, Some(action)) => {
                self.undo.push(action);
                self.redo.clear();
            }
            _ => {}
        }
        for stack in [&mut self.undo, &mut self.redo] {
            if stack.len() > HISTORY {
                stack.drain(..stack.len() - HISTORY);
            }
        }
        if ok && job.as_ref().is_some_and(|j| j.cut) {
            self.clipboard = None;
        }
        self.panes[pane].busy.clear();
        self.panes[pane].status = if outcome.errors.is_empty() || !outcome.summary.is_empty() {
            outcome.summary
        } else {
            outcome.errors.join("; ")
        };
        for index in [WINDOW, PICKER] {
            if index == PICKER && !self.picker_open {
                continue;
            }
            let dir = self.panes[index].dir.clone();
            let keep = outcome
                .results
                .iter()
                .filter(|p| p.parent() == Some(dir.as_path()))
                .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                .collect::<Vec<_>>();
            let keep = if keep.is_empty() && index != pane { self.panes[index].model.selected().into_iter().map(|e| e.name).collect() } else { keep };
            self.reload_selecting(index, keep);
        }
    }
}

/// Resolves "." and ".." lexically, so a breadcrumb never shows a path with dots in it.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    if out.as_os_str().is_empty() { PathBuf::from("/") } else { out }
}
