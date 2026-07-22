//! Project Link Graph — Obsidian-style graph visualization of story entities.
//!
//! Shows characters, locations, plot threads, and their relationships as
//! an interactive directed/undirected graph. Built with egui Painter.

use egui::{self, Color32, Pos2, RichText, Stroke, Ui, Vec2};
use std::collections::HashMap;

/// A node in the graph (character, location, plot thread, etc.)
#[derive(Debug, Clone)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub kind: NodeKind,
    pub pos: Pos2,
    pub velocity: Vec2,
    pub radius: f32,
    pub color: Color32,
}

/// A connection between two nodes
#[derive(Debug, Clone)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub label: String,
    pub weight: f32,
    pub color: Color32,
}

/// Kind of graph node
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Character,
    Location,
    PlotThread,
    Event,
    Item,
    Theme,
}

impl NodeKind {
    pub fn label(self) -> &'static str {
        match self {
            NodeKind::Character => "Character",
            NodeKind::Location => "Location",
            NodeKind::PlotThread => "Plot Thread",
            NodeKind::Event => "Event",
            NodeKind::Item => "Item",
            NodeKind::Theme => "Theme",
        }
    }

    pub fn default_color(self) -> Color32 {
        match self {
            NodeKind::Character => Color32::from_rgb(100, 200, 255),
            NodeKind::Location => Color32::from_rgb(100, 255, 150),
            NodeKind::PlotThread => Color32::from_rgb(255, 180, 100),
            NodeKind::Event => Color32::from_rgb(255, 100, 100),
            NodeKind::Item => Color32::from_rgb(200, 150, 255),
            NodeKind::Theme => Color32::from_rgb(255, 200, 255),
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            NodeKind::Character => "👤",
            NodeKind::Location => "📍",
            NodeKind::PlotThread => "📖",
            NodeKind::Event => "⚡",
            NodeKind::Item => "💎",
            NodeKind::Theme => "🎯",
        }
    }
}

/// Actions from the graph
#[derive(Debug, Clone)]
pub enum LinkGraphAction {
    SelectNode(String),
    OpenNode(String),
    DragNode(String, Pos2),
    ZoomIn,
    ZoomOut,
    ResetView,
    AddNode(String, NodeKind),
}

/// Link graph state
#[derive(Debug, Clone)]
pub struct LinkGraphState {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub selected_node: Option<String>,
    pub zoom: f32,
    pub pan: Vec2,
    pub is_dragging: bool,
    pub drag_node_id: Option<String>,
    pub show_labels: bool,
    pub physics_enabled: bool,
}

impl Default for LinkGraphState {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            selected_node: None,
            zoom: 1.0,
            pan: Vec2::ZERO,
            is_dragging: false,
            drag_node_id: None,
            show_labels: true,
            physics_enabled: true,
        }
    }
}

impl LinkGraphState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, id: &str, label: &str, kind: NodeKind) {
        // Place near center with slight random offset
        let angle = self.nodes.len() as f32 * 1.256;
        let dist = 80.0 + self.nodes.len() as f32 * 20.0;
        let pos = Pos2::new(400.0 + angle.cos() * dist, 300.0 + angle.sin() * dist);
        self.nodes.push(GraphNode {
            id: id.to_string(),
            label: label.to_string(),
            kind,
            pos,
            velocity: Vec2::ZERO,
            radius: 24.0,
            color: kind.default_color(),
        });
    }

    pub fn add_edge(&mut self, source: &str, target: &str, label: &str) {
        self.edges.push(GraphEdge {
            source: source.to_string(),
            target: target.to_string(),
            label: label.to_string(),
            weight: 1.0,
            color: Color32::from_rgba_premultiplied(150, 150, 150, 100),
        });
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
        self.edges.clear();
        self.selected_node = None;
    }

    /// Simple force-directed layout tick
    pub fn tick_physics(&mut self) {
        if !self.physics_enabled || self.nodes.is_empty() {
            return;
        }

        let repulsion = 5000.0;
        let attraction = 0.005;
        let damping = 0.85;
        let center_attraction = 0.01;

        let mut forces: HashMap<usize, Vec2> = HashMap::new();

        // Repulsion between all pairs
        for i in 0..self.nodes.len() {
            for j in (i + 1)..self.nodes.len() {
                let delta = self.nodes[i].pos - self.nodes[j].pos;
                let dist = delta.length().max(1.0);
                let force = repulsion / (dist * dist);
                let dir = delta / dist;
                *forces.entry(i).or_insert(Vec2::ZERO) += dir * force;
                *forces.entry(j).or_insert(Vec2::ZERO) -= dir * force;
            }
        }

        // Attraction along edges
        for edge in &self.edges {
            let src_idx = self.nodes.iter().position(|n| n.id == edge.source);
            let tgt_idx = self.nodes.iter().position(|n| n.id == edge.target);
            if let (Some(si), Some(ti)) = (src_idx, tgt_idx) {
                let delta = self.nodes[ti].pos - self.nodes[si].pos;
                let dist = delta.length().max(1.0);
                let force = delta * attraction * dist;
                *forces.entry(si).or_insert(Vec2::ZERO) += force;
                *forces.entry(ti).or_insert(Vec2::ZERO) -= force;
            }
        }

        // Apply forces
        for (i, force) in forces {
            let vel = self.nodes[i].velocity + force;
            let damped = vel * damping;
            self.nodes[i].velocity = damped;
            self.nodes[i].pos += damped;

            // Center attraction
            let center = Pos2::new(400.0, 300.0);
            let to_center = center - self.nodes[i].pos;
            self.nodes[i].pos += to_center * center_attraction;
        }
    }

    pub fn selected_node(&self) -> Option<&GraphNode> {
        self.selected_node
            .as_ref()
            .and_then(|id| self.nodes.iter().find(|n| n.id == *id))
    }
}

/// Link graph widget
pub struct LinkGraph;

impl LinkGraph {
    /// Show the graph panel
    pub fn show(ui: &mut Ui, state: &mut LinkGraphState) -> Option<LinkGraphAction> {
        let mut action = None;

        ui.vertical(|ui| {
            // Header + controls
            ui.horizontal(|ui| {
                ui.label(RichText::new("Link Graph").strong().size(14.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("🔍+").on_hover_text("Zoom in").clicked() {
                        state.zoom = (state.zoom * 1.2).min(5.0);
                        action = Some(LinkGraphAction::ZoomIn);
                    }
                    if ui.button("🔍-").on_hover_text("Zoom out").clicked() {
                        state.zoom = (state.zoom / 1.2).max(0.2);
                        action = Some(LinkGraphAction::ZoomOut);
                    }
                    if ui.button("↺").on_hover_text("Reset view").clicked() {
                        state.zoom = 1.0;
                        state.pan = Vec2::ZERO;
                        action = Some(LinkGraphAction::ResetView);
                    }
                    ui.checkbox(&mut state.show_labels, "Labels");
                    ui.checkbox(&mut state.physics_enabled, "Physics");
                });
            });

            if state.nodes.is_empty() {
                ui.label(RichText::new("No nodes to display. Add characters, locations, and plot threads to see the graph.").size(12.0).color(ui.visuals().weak_text_color()));
                return;
            }

            ui.separator();

            // Graph canvas
            let (response, painter) = ui.allocate_painter(
                Vec2::new(ui.available_width(), ui.available_height().max(300.0)),
                egui::Sense::click_and_drag(),
            );

            let canvas_rect = response.rect;
            let canvas_center = canvas_rect.center().to_vec2();
            let to_screen = |pos: Pos2| -> Pos2 {
                let zoomed = (pos.to_vec2() - Vec2::new(400.0, 300.0)) * state.zoom + canvas_center + state.pan;
                Pos2::new(zoomed.x, zoomed.y)
            };

            // Draw edges
            for edge in &state.edges {
                let src = state.nodes.iter().find(|n| n.id == edge.source);
                let tgt = state.nodes.iter().find(|n| n.id == edge.target);
                if let (Some(src_node), Some(tgt_node)) = (src, tgt) {
                    let src_screen = to_screen(src_node.pos);
                    let tgt_screen = to_screen(tgt_node.pos);
                    painter.line_segment(
                        [src_screen, tgt_screen],
                        Stroke::new(edge.weight * 2.0, edge.color),
                    );

                    // Edge label
                    if state.show_labels && !edge.label.is_empty() {
                        let mid = (src_screen + tgt_screen.to_vec2()) / 2.0;
                        painter.text(
                            mid,
                            egui::Align2::CENTER_CENTER,
                            &edge.label,
                            egui::FontId::proportional(9.0),
                            Color32::from_rgba_premultiplied(200, 200, 200, 150),
                        );
                    }
                }
            }

            // Draw nodes
            for node in &state.nodes {
                let screen_pos = to_screen(node.pos);
                let is_selected = state.selected_node.as_ref() == Some(&node.id);
                let radius = node.radius * state.zoom;

                // Glow for selected
                if is_selected {
                    painter.circle_filled(screen_pos, radius + 8.0, Color32::from_rgba_premultiplied(255, 255, 0, 60));
                }

                // Node circle
                painter.circle_filled(screen_pos, radius, node.color);
                painter.circle_stroke(screen_pos, radius, Stroke::new(
                    if is_selected { 3.0 } else { 1.5 },
                    if is_selected { Color32::WHITE } else { Color32::from_rgba_premultiplied(255, 255, 255, 100) },
                ));

                // Node icon
                if radius > 10.0 {
                    painter.text(
                        screen_pos,
                        egui::Align2::CENTER_CENTER,
                        node.kind.icon(),
                        egui::FontId::proportional(radius * 0.7),
                        Color32::WHITE,
                    );
                }

                // Label
                if state.show_labels && radius > 8.0 {
                    painter.text(
                        screen_pos + Vec2::new(0.0, radius + 14.0),
                        egui::Align2::CENTER_TOP,
                        &node.label,
                        egui::FontId::proportional(11.0 * state.zoom),
                        node.color.gamma_multiply(0.9),
                    );
                }
            }

            // Handle clicks
            if response.clicked() {
                let click_pos = response.interact_pointer_pos().unwrap_or(Pos2::ZERO);
                // Find closest node
                let mut closest: Option<(usize, f32)> = None;
                for (i, node) in state.nodes.iter().enumerate() {
                    let screen_pos = to_screen(node.pos);
                    let dist = (click_pos - screen_pos).length();
                    let threshold = node.radius * state.zoom + 4.0;
                    if dist < threshold {
                        let better = closest.is_none_or(|(_, d)| dist < d);
                        if better {
                            closest = Some((i, dist));
                        }
                    }
                }
                if let Some((i, _)) = closest {
                    let node_id = state.nodes[i].id.clone();
                    state.selected_node = Some(node_id.clone());
                    action = Some(LinkGraphAction::SelectNode(node_id));
                } else {
                    state.selected_node = None;
                }
            }

            // Handle double-click
            if response.double_clicked() {
                if let Some(ref selected) = state.selected_node {
                    action = Some(LinkGraphAction::OpenNode(selected.clone()));
                }
            }

            // Physics tick
            state.tick_physics();

            // Legend
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                for kind in &[
                    NodeKind::Character,
                    NodeKind::Location,
                    NodeKind::PlotThread,
                    NodeKind::Event,
                    NodeKind::Item,
                    NodeKind::Theme,
                ] {
                    let color = kind.default_color();
                    egui::Frame::none().fill(color).show(ui, |ui| { ui.allocate_space(Vec2::splat(8.0)); });
                    ui.label(RichText::new(kind.label()).size(10.0));
                    ui.add_space(8.0);
                }
            });
        });

        action
    }

    /// Compact version
    pub fn show_compact(ui: &mut Ui, state: &mut LinkGraphState) -> Option<LinkGraphAction> {
        // Simple node list for sidebar
        let mut action = None;
        ui.vertical(|ui| {
            ui.label(RichText::new("Links").strong().size(12.0));
            ui.label(
                RichText::new(format!(
                    "{} nodes, {} edges",
                    state.nodes.len(),
                    state.edges.len()
                ))
                .size(10.0)
                .color(ui.visuals().weak_text_color()),
            );
            for node in &state.nodes {
                let selected = state.selected_node.as_ref() == Some(&node.id);
                let label = format!("{} {}", node.kind.icon(), node.label);
                if ui.selectable_label(selected, &label).clicked() {
                    state.selected_node = Some(node.id.clone());
                    action = Some(LinkGraphAction::SelectNode(node.id.clone()));
                }
            }
        });
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_kind_variants() {
        for kind in &[
            NodeKind::Character,
            NodeKind::Location,
            NodeKind::PlotThread,
            NodeKind::Event,
            NodeKind::Item,
            NodeKind::Theme,
        ] {
            assert!(!kind.label().is_empty());
            assert!(!kind.icon().is_empty());
        }
    }

    #[test]
    fn test_link_graph_state_new() {
        let state = LinkGraphState::new();
        assert!(state.nodes.is_empty());
        assert!(state.edges.is_empty());
        assert_eq!(state.zoom, 1.0);
    }

    #[test]
    fn test_link_graph_state_add_node() {
        let mut state = LinkGraphState::new();
        state.add_node("hero", "Hero", NodeKind::Character);
        assert_eq!(state.nodes.len(), 1);
        assert_eq!(state.nodes[0].id, "hero");
        assert_eq!(state.nodes[0].label, "Hero");
        assert_eq!(state.nodes[0].kind, NodeKind::Character);
        assert!(state.nodes[0].radius > 0.0);
    }

    #[test]
    fn test_link_graph_state_add_edge() {
        let mut state = LinkGraphState::new();
        state.add_node("a", "Node A", NodeKind::Character);
        state.add_node("b", "Node B", NodeKind::Location);
        state.add_edge("a", "b", "lives in");
        assert_eq!(state.edges.len(), 1);
        assert_eq!(state.edges[0].source, "a");
        assert_eq!(state.edges[0].target, "b");
        assert_eq!(state.edges[0].label, "lives in");
    }

    #[test]
    fn test_link_graph_state_clear() {
        let mut state = LinkGraphState::new();
        state.add_node("a", "A", NodeKind::Character);
        state.add_node("b", "B", NodeKind::Location);
        state.add_edge("a", "b", "at");
        state.selected_node = Some("a".into());
        state.clear();
        assert!(state.nodes.is_empty());
        assert!(state.edges.is_empty());
        assert!(state.selected_node.is_none());
    }

    #[test]
    fn test_link_graph_state_selected_node() {
        let mut state = LinkGraphState::new();
        state.add_node("hero", "Hero", NodeKind::Character);
        state.add_node("villain", "Villain", NodeKind::Character);
        assert!(state.selected_node().is_none());

        state.selected_node = Some("hero".into());
        assert!(state.selected_node().is_some());
        assert_eq!(state.selected_node().unwrap().label, "Hero");

        state.selected_node = Some("nonexistent".into());
        assert!(state.selected_node().is_none());
    }

    #[test]
    fn test_link_graph_action_variants() {
        let pos = Pos2::new(100.0, 200.0);
        let actions = [
            LinkGraphAction::SelectNode("n1".into()),
            LinkGraphAction::OpenNode("n1".into()),
            LinkGraphAction::DragNode("n1".into(), pos),
            LinkGraphAction::ZoomIn,
            LinkGraphAction::ZoomOut,
            LinkGraphAction::ResetView,
            LinkGraphAction::AddNode("n1".into(), NodeKind::Character),
        ];
        assert_eq!(actions.len(), 7);
    }

    #[test]
    fn test_physics_tick_does_not_panic() {
        let mut state = LinkGraphState::new();
        state.add_node("a", "A", NodeKind::Character);
        state.add_node("b", "B", NodeKind::Location);
        state.add_edge("a", "b", "connects");
        state.physics_enabled = true;

        // Multiple ticks should not panic
        for _ in 0..10 {
            state.tick_physics();
        }
    }

    #[test]
    fn test_physics_empty() {
        let mut state = LinkGraphState::new();
        state.tick_physics(); // Should not panic
    }

    #[test]
    fn test_physics_disabled() {
        let mut state = LinkGraphState::new();
        state.add_node("a", "A", NodeKind::Character);
        state.physics_enabled = false;
        let pos_before = state.nodes[0].pos;
        state.tick_physics();
        assert_eq!(state.nodes[0].pos, pos_before);
    }

    #[test]
    fn test_node_kind_default_colors() {
        for kind in &[
            NodeKind::Character,
            NodeKind::Location,
            NodeKind::PlotThread,
            NodeKind::Event,
            NodeKind::Item,
            NodeKind::Theme,
        ] {
            let c = kind.default_color();
            assert!(c.r() > 0 || c.g() > 0 || c.b() > 0);
        }
    }
}
