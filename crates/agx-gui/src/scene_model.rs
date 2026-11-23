//! Persistent Slint rows plus the last laid-out scene for allocation-free hit testing.
use crate::{
    Win,
    wm::{Kind, Scene},
};
use slint::{Model, VecModel};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

#[derive(Default)]
pub struct SceneModel {
    pub windows: Rc<VecModel<Win>>,
    pub scene: RefCell<Scene>,
    pub native_titles: RefCell<HashMap<u32, String>>,
}

impl SceneModel {
    /// Reset the repeater only when its membership or stacking order changes.
    /// Ordinary moves/resizes notify exactly the rows whose values changed.
    pub fn sync(&self, scene: Scene, target: Option<Kind>) -> usize {
        let titles = self.native_titles.borrow();
        let rows: Vec<_> = scene
            .windows
            .iter()
            .map(|p| Win {
                kind: p.kind.index() as i32,
                title: match p.kind {
                    Kind::Native(id) => titles.get(&id).map(String::as_str).unwrap_or(p.kind.title()),
                    kind => kind.title(),
                }.into(),
                x: p.rect.x,
                y: p.rect.y,
                w: p.rect.w,
                h: p.rect.h,
                focused: p.focused,
                floating: p.floating,
                collapsed: p.collapsed,
                target: target == Some(p.kind),
            })
            .collect();
        let structural = self.windows.row_count() != rows.len()
            || rows.iter().enumerate().any(|(i, row)| {
                self.windows
                    .row_data(i)
                    .is_none_or(|old| old.kind != row.kind)
            });
        let changed = if structural {
            let count = rows.len();
            self.windows.set_vec(rows);
            count
        } else {
            let mut count = 0;
            for (i, row) in rows.into_iter().enumerate() {
                if self.windows.row_data(i).as_ref() != Some(&row) {
                    self.windows.set_row_data(i, row);
                    count += 1;
                }
            }
            count
        };
        *self.scene.borrow_mut() = scene;
        changed
    }

    /// Tiled dragging changes the overlay, never the layout or panel contents.
    pub fn target(&self, target: Option<Kind>) -> usize {
        let mut changed = 0;
        for i in 0..self.windows.row_count() {
            let Some(mut row) = self.windows.row_data(i) else {
                continue;
            };
            let active = target.is_some_and(|k| k.index() as i32 == row.kind);
            if row.target != active {
                row.target = active;
                self.windows.set_row_data(i, row);
                changed += 1;
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wm::{Desktop, Edges, Rect};

    #[test]
    fn floating_motion_changes_one_row_and_keeps_other_panels_intact() {
        let area = Rect::new(0.0, 0.0, 1920.0, 1080.0);
        let mut desk = Desktop::new(area, area);
        desk.toggle_float();
        let focused = desk.focused().unwrap();
        let view = SceneModel::default();
        view.sync(desk.scene(), None);
        let before: Vec<_> = view.windows.iter().collect();
        desk.move_floating(focused, 25.0, 20.0);
        assert_eq!(view.sync(desk.scene(), None), 1);
        for (i, old) in before.iter().enumerate() {
            let new = view.windows.row_data(i).unwrap();
            if old.kind == focused.index() as i32 {
                assert_eq!((new.x, new.y), (old.x + 25.0, old.y + 20.0));
            } else {
                assert_eq!(&new, old);
            }
        }
        desk.resize_floating_edges(
            focused,
            Edges {
                right: true,
                ..Default::default()
            },
            30.0,
            0.0,
        );
        assert_eq!(view.sync(desk.scene(), None), 1);
        assert_eq!(
            view.sync(desk.scene(), None),
            0,
            "unchanged scenes must not dirty rows"
        );
    }

    #[test]
    fn tiled_target_changes_only_highlights_then_workspace_removes_old_rows() {
        let area = Rect::new(0.0, 0.0, 1920.0, 1080.0);
        let mut desk = Desktop::new(area, area);
        let view = SceneModel::default();
        view.sync(desk.scene(), None);
        let before: Vec<_> = view.windows.iter().collect();
        assert_eq!(view.target(Some(Kind::Overview)), 1);
        assert_eq!(view.target(Some(Kind::Overview)), 0);
        assert_eq!(view.target(Some(Kind::Cpu)), 2);
        assert_eq!(view.target(None), 1);
        assert_eq!(view.windows.iter().collect::<Vec<_>>(), before);
        desk.goto(1);
        view.sync(desk.scene(), None);
        assert_eq!(view.windows.row_count(), 1);
        assert_eq!(
            view.windows.row_data(0).unwrap().kind,
            Kind::AgentAngi.index() as i32
        );
        desk.close(Kind::AgentAngi);
        view.sync(desk.scene(), None);
        assert_eq!(view.windows.row_count(), 0);
        desk.goto(0);
        view.sync(desk.scene(), None);
        assert_eq!(view.windows.iter().collect::<Vec<_>>(), before);
    }
}
