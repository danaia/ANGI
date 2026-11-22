//! Pin Web beside Agent Angi in the workspace that owns the conversation.
use super::{Axis, Desktop, GAP_IN, GAP_OUT, Kind, Mode, Node, Rect};

impl Desktop {
    pub(crate) fn open_agent_preview(&mut self) {
        let (agent, web) = (Kind::AgentAngi, Kind::Web);
        let target = self
            .ws
            .iter()
            .position(|workspace| workspace.z.contains(&agent));
        let Some(target) = target else {
            self.open(web);
            return;
        };
        let return_to = self.cur;
        self.cur = target;
        self.ws[target].full = None;
        self.collapsed.remove(&agent);
        self.collapsed.remove(&web);
        match self.ws[target].mode {
            Mode::Floating => {
                self.open(web);
                let area = self.usable.inset(GAP_OUT);
                let width = (area.w - GAP_IN) / 2.0;
                self.float_rect
                    .insert(agent, Rect::new(area.x, area.y, width, area.h));
                self.float_rect.insert(
                    web,
                    Rect::new(area.x + width + GAP_IN, area.y, width, area.h),
                );
            }
            Mode::Tiled => {
                // Make the anchor a leaf even if the user had floated it on this workspace.
                if self.ws[target].floats.contains(&agent) {
                    self.detach(agent);
                    self.attach(target, agent);
                }
                let already_right = !self.ws[target].floats.contains(&web)
                    && self.neighbor(agent, super::Dir::Right) == Some(web);
                if !already_right {
                    self.detach(web);
                    let workspace = &mut self.ws[target];
                    if let Some(tree) = &mut workspace.tree {
                        tree.insert_at(agent, web, Axis::Horizontal, &mut self.next_split, false);
                    } else {
                        workspace.tree = Some(Node::Leaf(web));
                    }
                    workspace.z.push(web);
                }
            }
        }
        self.focus(agent);
        self.focus(web);
        self.cur = return_to;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_previews_open_on_the_right_in_both_layout_modes() {
        let screen = Rect::new(0.0, 0.0, 1200.0, 800.0);
        for mode in [Mode::Floating, Mode::Tiled] {
            let mut desktop = Desktop::new(screen, screen);
            desktop.open(Kind::Web);
            desktop.goto(1);
            if desktop.mode() != mode {
                desktop.cycle_layout();
            }
            desktop.goto(0);
            desktop.open_agent_preview();
            assert_eq!(desktop.current(), 0);
            assert!(desktop.scene().rect_of(Kind::Web).is_none());
            desktop.goto(1);
            let agent = desktop.rect_of(Kind::AgentAngi).unwrap();
            let web = desktop.rect_of(Kind::Web).unwrap();
            assert_eq!(agent.y, web.y);
            assert!(web.x >= agent.right());
            desktop.open_agent_preview();
            assert_eq!(desktop.rect_of(Kind::Web), Some(web));
        }
    }
}
