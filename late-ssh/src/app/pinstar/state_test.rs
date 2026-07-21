use super::*;
use crate::app::pinstar::data::{CanvasEdge, GroupNode, TextNode};

static PINSTAR_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct EnvVarRestore {
    key: &'static str,
    value: Option<std::ffi::OsString>,
}

impl EnvVarRestore {
    fn set_path(key: &'static str, value: &std::path::Path) -> Self {
        let restore = Self {
            key,
            value: std::env::var_os(key),
        };
        // SAFETY: tests that mutate this process-wide variable hold
        // PINSTAR_ENV_LOCK until the restore guard is dropped.
        unsafe {
            std::env::set_var(key, value);
        }
        restore
    }
}

impl Drop for EnvVarRestore {
    fn drop(&mut self) {
        // SAFETY: tests that mutate this process-wide variable hold
        // PINSTAR_ENV_LOCK until the restore guard is dropped.
        unsafe {
            match &self.value {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }
}

#[test]
fn new_node_id_returns_unique_values() {
    let mut nodes = Vec::new();
    for _ in 0..100 {
        let id = new_node_id("node", &nodes);
        assert!(!nodes.iter().any(|n: &CanvasNode| n.id() == id));
        nodes.push(CanvasNode::Text(TextNode {
            id,
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
            text: String::new(),
            color: None,
        }));
    }
}

#[test]
fn rewrite_text_shape_metadata_roundtrip() {
    let text = "hello\nworld";
    let with_shape = rewrite_text_shape_metadata(text, Some("diamond"));
    assert!(with_shape.starts_with("// pinstar:shape=diamond\n"));
    assert!(with_shape.contains("hello\nworld"));

    let replaced = rewrite_text_shape_metadata(&with_shape, Some("circle"));
    assert!(replaced.starts_with("// pinstar:shape=circle\n"));
    assert!(!replaced.contains("shape=diamond"));

    let cleared = rewrite_text_shape_metadata(&replaced, None);
    assert_eq!(cleared, text);
}

#[test]
fn rename_selected_updates_group_label_not_id() {
    let _env_guard = PINSTAR_ENV_LOCK.lock().unwrap();
    let root = std::env::temp_dir().join(format!("late-sh-pinstar-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let _root_env = EnvVarRestore::set_path("LATE_PINSTAR_LOCAL_ROOT", &root);

    let path = root.join(format!("rename-group-{}.json", uuid::Uuid::new_v4()));
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&CanvasData::default()).unwrap(),
    )
    .unwrap();
    let mut state = PinstarState::load(&path).unwrap();

    state.data.nodes.push(CanvasNode::Group(GroupNode {
        id: "group_1".to_string(),
        x: 0.0,
        y: 0.0,
        width: 50.0,
        height: 30.0,
        label: Some("Old".to_string()),
        color: None,
    }));
    state.selected_node_id = Some("group_1".to_string());

    state.rename_selected("New Title".to_string());

    let g = state
        .data
        .nodes
        .iter()
        .find_map(|n| match n {
            CanvasNode::Group(g) if g.id == "group_1" => Some(g),
            _ => None,
        })
        .unwrap();
    assert_eq!(g.label.as_deref(), Some("New Title"));
    assert_eq!(g.id, "group_1");
}

#[test]
fn normalize_duplicate_node_ids_keeps_edges_on_retained_ids() {
    let mut data = CanvasData {
        nodes: vec![
            CanvasNode::Text(TextNode {
                id: "node_dup".to_string(),
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
                text: "A".to_string(),
                color: None,
            }),
            CanvasNode::Text(TextNode {
                id: "node_dup".to_string(),
                x: 20.0,
                y: 20.0,
                width: 10.0,
                height: 10.0,
                text: "B".to_string(),
                color: None,
            }),
            CanvasNode::Group(GroupNode {
                id: "group_dup".to_string(),
                x: 100.0,
                y: 100.0,
                width: 40.0,
                height: 30.0,
                label: Some("G1".to_string()),
                color: None,
            }),
            CanvasNode::Group(GroupNode {
                id: "group_dup".to_string(),
                x: 200.0,
                y: 100.0,
                width: 40.0,
                height: 30.0,
                label: Some("G2".to_string()),
                color: None,
            }),
        ],
        edges: vec![
            CanvasEdge {
                id: "edge1".to_string(),
                from_node: "node_dup".to_string(),
                from_side: None,
                to_node: "group_dup".to_string(),
                to_side: None,
                label: None,
                color: None,
                style: Default::default(),
            },
            CanvasEdge {
                id: "edge2".to_string(),
                from_node: "group_dup".to_string(),
                from_side: None,
                to_node: "node_dup".to_string(),
                to_side: None,
                label: None,
                color: None,
                style: Default::default(),
            },
        ],
        ..CanvasData::default()
    };

    let changed = normalize_duplicate_node_ids(&mut data);
    assert!(changed);

    let node_ids: std::collections::HashSet<String> =
        data.nodes.iter().map(|n| n.id().to_string()).collect();
    assert_eq!(node_ids.len(), data.nodes.len());

    for edge in &data.edges {
        assert!(node_ids.contains(&edge.from_node));
        assert!(node_ids.contains(&edge.to_node));
    }

    assert_eq!(data.edges[0].from_node, "node_dup");
    assert_eq!(data.edges[0].to_node, "group_dup");
    assert_eq!(data.edges[1].from_node, "group_dup");
    assert_eq!(data.edges[1].to_node, "node_dup");
}

#[test]
fn undo_restores_previous_state() {
    let _lock = PINSTAR_ENV_LOCK.lock().unwrap();
    let dir = std::env::temp_dir().join("pinstar-undo-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.canvas.json");
    let _env = EnvVarRestore::set_path("LATE_PINSTAR_LOCAL_ROOT", &dir);

    let mut state = PinstarState::load(&path).unwrap();
    state.add_text_node(0.0, 0.0);
    let first_node_id = state.data.nodes[0].id().to_string();
    state.add_text_node(100.0, 100.0);
    assert_eq!(state.data.nodes.len(), 2);
    assert_eq!(state.undo_stack.len(), 2);

    state.undo().unwrap();
    assert_eq!(state.data.nodes.len(), 1);
    assert_eq!(state.data.nodes[0].id(), first_node_id);

    state.redo().unwrap();
    assert_eq!(state.data.nodes.len(), 2);
}

#[test]
fn undo_last_node_move_restores_previous_coordinates() {
    let _lock = PINSTAR_ENV_LOCK.lock().unwrap();
    let dir = std::env::temp_dir().join("pinstar-move-undo-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.canvas.json");
    let _env = EnvVarRestore::set_path("LATE_PINSTAR_LOCAL_ROOT", &dir);

    let mut state = PinstarState::load(&path).unwrap();
    state.add_text_node(100.0, 100.0);
    let node_id = state.data.nodes[0].id().to_string();
    state.selected_node_id = Some(node_id.clone());

    state.begin_move_tracking();
    state.move_selected_node(40.0, 25.0);
    state.finalize_move_tracking();

    let moved_node = state
        .data
        .nodes
        .iter()
        .find(|n| n.id() == node_id)
        .expect("node after move");
    assert_eq!(moved_node.pos(), (140.0, 125.0));

    state.undo_last_node_move();

    let restored_node = state
        .data
        .nodes
        .iter()
        .find(|n| n.id() == node_id)
        .expect("node after undo move");
    assert_eq!(restored_node.pos(), (100.0, 100.0));
}

#[test]
fn delete_selected_edge_removes_edge_and_saves() {
    let _lock = PINSTAR_ENV_LOCK.lock().unwrap();
    let dir = std::env::temp_dir().join("pinstar-edge-delete-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.canvas.json");
    let _env = EnvVarRestore::set_path("LATE_PINSTAR_LOCAL_ROOT", &dir);

    let mut state = PinstarState::load(&path).unwrap();
    state.add_text_node(0.0, 0.0);
    let n1 = state.data.nodes[0].id().to_string();
    state.add_text_node(100.0, 100.0);
    let n2 = state.data.nodes[1].id().to_string();

    state.selected_node_id = Some(n1.clone());
    state.start_connection();
    state.finish_connection(&n2);

    assert_eq!(state.data.edges.len(), 1);
    let edge_id = state.data.edges[0].id.clone();

    state.selected_edge_id = Some(edge_id.clone());
    state.delete_selected_edge();

    assert_eq!(state.data.edges.len(), 0);
    assert!(state.selected_edge_id.is_none());
}
