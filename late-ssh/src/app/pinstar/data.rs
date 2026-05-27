use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub enum DiagramOrientation {
    #[default]
    TopDown,
    LeftRight,
    RightLeft,
    DownTop,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum DiagramLockMode {
    #[default]
    Unlocked,
    All,
    EditorOnly,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct CanvasData {
    pub nodes: Vec<CanvasNode>,
    pub edges: Vec<CanvasEdge>,
    #[serde(default)]
    pub orientation: DiagramOrientation,
    #[serde(default)]
    pub lock_mode: DiagramLockMode,
    // Legacy lock flag kept for backward compatibility with old persisted data.
    #[serde(default)]
    pub locked: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CanvasNode {
    Text(TextNode),
    File(FileNode),
    Link(LinkNode),
    Group(GroupNode),
}

// NodeShape removed: Canvas nodes always use Rectangle

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TextNode {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub text: String,
    pub color: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FileNode {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub file: String,
    pub subpath: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LinkNode {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub url: String,
    pub color: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GroupNode {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub label: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub enum EdgeStyle {
    #[default]
    Solid,
    Dashed,
    Thick,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CanvasEdge {
    pub id: String,
    pub from_node: String,
    pub from_side: Option<String>,
    pub to_node: String,
    pub to_side: Option<String>,
    pub label: Option<String>,
    pub color: Option<String>,
    #[serde(default)]
    pub style: EdgeStyle,
}

impl CanvasNode {
    pub fn id(&self) -> &str {
        match self {
            CanvasNode::Text(n) => &n.id,
            CanvasNode::File(n) => &n.id,
            CanvasNode::Link(n) => &n.id,
            CanvasNode::Group(n) => &n.id,
        }
    }

    pub fn pos(&self) -> (f64, f64) {
        match self {
            CanvasNode::Text(n) => (n.x, n.y),
            CanvasNode::File(n) => (n.x, n.y),
            CanvasNode::Link(n) => (n.x, n.y),
            CanvasNode::Group(n) => (n.x, n.y),
        }
    }

    pub fn size(&self) -> (f64, f64) {
        match self {
            CanvasNode::Text(n) => (n.width, n.height),
            CanvasNode::File(n) => (n.width, n.height),
            CanvasNode::Link(n) => (n.width, n.height),
            CanvasNode::Group(n) => (n.width, n.height),
        }
    }

    pub fn text(&self) -> &str {
        match self {
            CanvasNode::Text(n) => &n.text,
            CanvasNode::File(n) => &n.file,
            CanvasNode::Link(n) => &n.url,
            CanvasNode::Group(n) => n.label.as_deref().unwrap_or(""),
        }
    }

    pub fn set_text(&mut self, text: String) {
        match self {
            CanvasNode::Text(n) => n.text = text,
            CanvasNode::File(n) => n.file = text,
            CanvasNode::Link(n) => n.url = text,
            CanvasNode::Group(n) => n.label = Some(text),
        }
    }

    pub fn is_generated(&self) -> bool {
        is_generated_id(self.id())
    }

    // shape() removed: Canvas nodes have no shape enum tier
}

pub fn is_generated_id(id: &str) -> bool {
    if let Some(rest) = id.strip_prefix("node_") {
        if rest.len() <= 11 && rest.chars().all(|c| c.is_ascii_hexdigit()) {
            return true;
        }
        if rest.len() == 36 && rest.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
            return true;
        }
    }
    if let Some(rest) = id.strip_prefix("group_")
        && rest.len() == 36
        && rest.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
    {
        return true;
    }
    if id.len() == 16 && id.chars().all(|c| c.is_ascii_hexdigit()) {
        return true;
    }
    if id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return true;
    }
    false
}

// --- Collaborative editing protocol ---

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PinstarOp {
    AddNode(CanvasNode),
    UpdateNode { id: String, node: CanvasNode },
    RenameNode { old_id: String, new_id: String },
    RemoveNode { id: String },
    AddEdge(CanvasEdge),
    UpdateEdge { id: String, edge: CanvasEdge },
    RemoveEdge { id: String },
    SetOrientation(DiagramOrientation),
    SetLockMode(DiagramLockMode),
    ReplaceAll(CanvasData),
}

impl PinstarOp {
    pub fn apply(&self, data: &mut CanvasData) {
        match self {
            PinstarOp::AddNode(node) => {
                if !data.nodes.iter().any(|n| n.id() == node.id()) {
                    data.nodes.push(node.clone());
                }
            }
            PinstarOp::UpdateNode { id, node } => {
                if let Some(idx) = data.nodes.iter().position(|n| n.id() == id) {
                    data.nodes[idx] = node.clone();
                }
            }
            PinstarOp::RenameNode { old_id, new_id } => {
                if old_id == new_id || data.nodes.iter().any(|n| n.id() == new_id) {
                    return;
                }
                for node in &mut data.nodes {
                    match node {
                        CanvasNode::Text(n) if n.id == *old_id => n.id = new_id.clone(),
                        CanvasNode::File(n) if n.id == *old_id => n.id = new_id.clone(),
                        CanvasNode::Link(n) if n.id == *old_id => n.id = new_id.clone(),
                        CanvasNode::Group(n) if n.id == *old_id => n.id = new_id.clone(),
                        _ => {}
                    }
                }
                for edge in &mut data.edges {
                    if edge.from_node == *old_id {
                        edge.from_node = new_id.clone();
                    }
                    if edge.to_node == *old_id {
                        edge.to_node = new_id.clone();
                    }
                }
            }
            PinstarOp::RemoveNode { id } => {
                data.nodes.retain(|n| n.id() != id);
                data.edges
                    .retain(|e| e.from_node != *id && e.to_node != *id);
            }
            PinstarOp::AddEdge(edge) => {
                if !data.edges.iter().any(|e| e.id == edge.id) {
                    data.edges.push(edge.clone());
                }
            }
            PinstarOp::UpdateEdge { id, edge } => {
                if let Some(idx) = data.edges.iter().position(|e| e.id == *id) {
                    data.edges[idx] = edge.clone();
                }
            }
            PinstarOp::RemoveEdge { id } => {
                data.edges.retain(|e| e.id != *id);
            }
            PinstarOp::SetOrientation(orientation) => {
                data.orientation = *orientation;
            }
            PinstarOp::SetLockMode(lock_mode) => {
                data.lock_mode = *lock_mode;
                data.locked = matches!(lock_mode, DiagramLockMode::All);
            }
            PinstarOp::ReplaceAll(new_data) => {
                data.nodes = new_data.nodes.clone();
                data.edges = new_data.edges.clone();
                data.orientation = new_data.orientation;
                data.lock_mode = new_data.lock_mode;
                data.locked = new_data.locked;
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinstarPeer {
    pub user_id: uuid::Uuid,
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ClientMsg {
    Hello {
        user_id: uuid::Uuid,
        username: String,
    },
    SubmitOp {
        client_seq: u64,
        op: PinstarOp,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ServerMsg {
    Welcome {
        peers: Vec<PinstarPeer>,
        snapshot: CanvasData,
        your_role: String,
    },
    Ack {
        client_seq: u64,
        server_seq: u64,
    },
    OpBroadcast {
        from: uuid::Uuid,
        op: PinstarOp,
        server_seq: u64,
    },
    PeerJoined {
        peer: PinstarPeer,
    },
    PeerLeft {
        user_id: uuid::Uuid,
    },
    Rejected {
        reason: String,
    },
}
