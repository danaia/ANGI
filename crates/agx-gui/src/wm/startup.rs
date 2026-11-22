//! Initial desktop composition, seeded as movable windows at tiled positions.
use super::{Axis, Desktop, INITIAL_WORKSPACES, Kind, Node, Rect, Workspace};
use std::collections::{HashMap, HashSet};

impl Desktop {
    pub fn new(screen: Rect, usable: Rect) -> Self {
        let mut desktop = Self {
            ws: (0..INITIAL_WORKSPACES).map(|_| Workspace::empty()).collect(),
            cur: 0,
            prev: 0,
            next_split: 3,
            usable,
            screen,
            float_rect: HashMap::new(),
            collapsed: HashSet::new(),
        };
        // Settings fills the left half. ANGI sits above CPU and Memory on the
        // right. Explicit axes keep the same composition at every aspect ratio.
        desktop.ws[0].tree = Some(Node::Split {
            id: 0,
            axis: Axis::Horizontal,
            ratio: 0.5,
            a: Box::new(Node::Leaf(Kind::Settings)),
            b: Box::new(Node::Split {
                id: 1,
                axis: Axis::Vertical,
                ratio: 0.5,
                a: Box::new(Node::Leaf(Kind::Overview)),
                b: Box::new(Node::Split {
                    id: 2,
                    axis: Axis::Horizontal,
                    ratio: 0.5,
                    a: Box::new(Node::Leaf(Kind::Cpu)),
                    b: Box::new(Node::Leaf(Kind::Memory)),
                }),
            }),
        });
        desktop.ws[0].z = vec![Kind::Settings, Kind::Overview, Kind::Cpu, Kind::Memory];
        desktop.ws[0].focus = Some(Kind::Memory);
        desktop.cur = 1;
        desktop.open(Kind::AgentAngi);
        // Workspaces 3–5 are available for windows the user opens.
        for i in 0..desktop.workspace_count() {
            desktop.cur = i;
            desktop.cycle_layout();
        }
        desktop.cur = 0;
        desktop
    }
}
