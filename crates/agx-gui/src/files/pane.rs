//! One browser's state: where it is, how it looks, and what it is asking the user. Two exist: the
//! Files window and the folder picker overlay.
use super::{
    listing::SortKey,
    model::{FileModel, View},
    places::Place,
};
use crate::{Crumb, PaneState, PlaceRow};
use slint::{ModelRc, VecModel};
use std::{
    path::{Path, PathBuf},
    rc::Rc,
    time::Instant,
};

/// How a successful listing changes the back/forward history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    /// A new location: the old one becomes "back" and "forward" is cleared.
    Visit,
    Back,
    Forward,
    /// Same place, newer contents.
    Reload,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Prompt {
    NewFolder,
    NewFile,
    Rename(PathBuf),
    /// Permanent deletion, asked for explicitly.
    Delete(Vec<PathBuf>),
}

pub struct Pane {
    pub picker: bool,
    /// The picker is choosing files to attach, not a folder.
    pub attach: bool,
    pub model: Rc<FileModel>,
    pub dir: PathBuf,
    pub target: Option<(PathBuf, Move)>,
    pub back: Vec<PathBuf>,
    pub forward: Vec<PathBuf>,
    pub view: View,
    /// The text of the filter box, as typed.
    pub filter: String,
    /// The recursive search being shown, if any.
    pub search: Option<String>,
    pub request: u64,
    pub loading: bool,
    pub error: String,
    pub status: String,
    pub busy: String,
    pub prompt: Option<Prompt>,
    pub prompt_serial: i32,
    pub cursor_serial: i32,
    pub typed: (String, Option<Instant>),
    /// Names to select once the next listing arrives.
    pub reselect: Vec<String>,
    pub page: usize,
    pub rows: ModelRc<crate::FileRow>,
}

impl Pane {
    pub fn new(picker: bool, dir: PathBuf) -> Self {
        let model = Rc::new(FileModel::default());
        Self {
            picker,
            attach: false,
            rows: ModelRc::from(model.clone()),
            model,
            dir,
            target: None,
            back: vec![],
            forward: vec![],
            view: View { folders_only: picker, ..View::default() },
            filter: String::new(),
            search: None,
            request: 0,
            loading: false,
            error: String::new(),
            status: String::new(),
            busy: String::new(),
            prompt: None,
            prompt_serial: 0,
            cursor_serial: 0,
            typed: (String::new(), None),
            reselect: vec![],
            page: 10,
        }
    }

    /// Commits a finished listing of `dir` and updates the history.
    pub fn arrived(&mut self, dir: PathBuf) {
        let how = self.target.take().map_or(Move::Reload, |(_, how)| how);
        if dir != self.dir {
            match how {
                Move::Visit => {
                    self.back.push(std::mem::replace(&mut self.dir, dir));
                    self.forward.clear();
                }
                Move::Back => {
                    self.back.pop();
                    self.forward.push(std::mem::replace(&mut self.dir, dir));
                }
                Move::Forward => {
                    self.forward.pop();
                    self.back.push(std::mem::replace(&mut self.dir, dir));
                }
                Move::Reload => self.dir = dir,
            }
        }
        // Keep each history stack bounded.
        for stack in [&mut self.back, &mut self.forward] {
            if stack.len() > 100 {
                stack.drain(..stack.len() - 100);
            }
        }
    }

    pub fn can_up(&self) -> bool {
        self.dir.parent().is_some()
    }

    pub fn clear_prompt(&mut self) {
        self.prompt = None;
        self.prompt_serial += 1;
    }

    pub fn state(&self, places: &[Place], can_paste: bool, can_undo: bool, can_redo: bool) -> PaneState {
        let stats = self.model.stats();
        let (kind, title, text) = match &self.prompt {
            None => (0, String::new(), String::new()),
            Some(Prompt::NewFolder) => (1, "New folder".into(), super::ops::unique_name(&self.dir, "New folder")),
            Some(Prompt::NewFile) => (2, "New file".into(), super::ops::unique_name(&self.dir, "New file")),
            Some(Prompt::Rename(path)) => {
                (3, "Rename".into(), path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
            }
            Some(Prompt::Delete(paths)) => (
                4,
                format!(
                    "Permanently delete {}? This cannot be undone. Enter confirms, Esc cancels.",
                    if paths.len() == 1 { "this item".to_owned() } else { format!("{} items", paths.len()) }
                ),
                String::new(),
            ),
        };
        let plural = |n: usize, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
        let summary = match (stats.selected, &stats.single) {
            (1, Some(e)) if e.is_dir() => format!("“{}” · {}", e.name, e.type_label()),
            (1, Some(e)) => format!("“{}” · {} · {}", e.name, super::entry::size_text(e.size), super::entry::time_text(e.modified)),
            (0, _) => {
                let mut parts = Vec::new();
                if stats.folders > 0 {
                    parts.push(plural(stats.folders, "folder"));
                }
                if stats.visible > stats.folders && (!self.picker || self.attach) {
                    parts.push(plural(stats.visible - stats.folders, "file"));
                }
                let found = if parts.is_empty() { None } else { Some(parts.join(", ")) };
                match (&self.search, found) {
                    (Some(q), found) => format!("{} found for “{q}”", found.unwrap_or_else(|| "Nothing".into())),
                    (None, found) => found.unwrap_or_else(|| "Empty".into()),
                }
            }
            (n, _) => format!("{n} selected · {}", super::entry::size_text(stats.selected_bytes)),
        };
        PaneState {
            rows: self.rows.clone(),
            path: self.dir.to_string_lossy().into_owned().into(),
            crumbs: ModelRc::new(VecModel::from(crumbs(&self.dir))),
            places: ModelRc::new(VecModel::from(
                places
                    .iter()
                    .map(|p| PlaceRow {
                        label: p.label.as_str().into(),
                        glyph: p.glyph.into(),
                        path: p.path.to_string_lossy().into_owned().into(),
                        section: p.section,
                        active: p.path == self.dir,
                    })
                    .collect::<Vec<_>>(),
            )),
            can_back: !self.back.is_empty(),
            can_forward: !self.forward.is_empty(),
            can_up: self.can_up() && self.search.is_none(),
            sort_key: match self.view.sort.key {
                SortKey::Name => 0,
                SortKey::Size => 1,
                SortKey::Modified => 2,
                SortKey::Type => 3,
            },
            sort_ascending: self.view.sort.ascending,
            show_hidden: self.view.show_hidden,
            searching: self.search.clone().unwrap_or_default().into(),
            loading: self.loading,
            error: self.error.as_str().into(),
            status: self.status.as_str().into(),
            busy: self.busy.as_str().into(),
            prompt_kind: kind,
            prompt_title: title.into(),
            prompt_text: text.into(),
            prompt_serial: self.prompt_serial,
            summary: summary.into(),
            selected: stats.selected as i32,
            single_folder: stats.single.as_ref().is_some_and(|e| e.is_dir()),
            can_paste: can_paste && self.search.is_none(),
            can_undo,
            can_redo,
            cursor: self.model.cursor().map_or(-1, |c| c as i32),
            cursor_serial: self.cursor_serial,
            empty_text: if self.loading {
                "Loading…".into()
            } else if !self.error.is_empty() {
                "".into()
            } else if self.search.is_some() {
                "No matches in this folder or its subfolders".into()
            } else if !self.filter.is_empty() {
                "No items match. Press Enter to search subfolders.".into()
            } else if self.picker && self.attach {
                "Nothing here".into()
            } else if self.picker {
                "No subfolders".into()
            } else {
                "This folder is empty".into()
            },
        }
    }
}

/// "/home/dana/dev" → "/", "home", "dana", "dev", each with the path it leads to.
pub fn crumbs(dir: &Path) -> Vec<Crumb> {
    let mut path = PathBuf::new();
    dir.components()
        .map(|part| {
            path.push(part);
            let label = match part {
                std::path::Component::RootDir => "/".to_owned(),
                other => other.as_os_str().to_string_lossy().into_owned(),
            };
            Crumb { label: label.into(), path: path.to_string_lossy().into_owned().into() }
        })
        .collect()
}
