use super::*;
use crate::app::pinstar::data::{CanvasNode, TextNode};
use std::time::Instant;

fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

fn text_node(id: &str) -> CanvasNode {
    CanvasNode::Text(TextNode {
        id: id.to_string(),
        x: 10.0,
        y: 20.0,
        width: 120.0,
        height: 60.0,
        text: "node".to_string(),
        color: None,
    })
}

#[test]
fn broadcasts_ops_to_every_connected_client() {
    let registry = PinstarServerRegistry::new(None);
    let server = registry.create_server(
        Uuid::now_v7(),
        "test".to_string(),
        CanvasData::default(),
        None,
    );

    let alice_id = Uuid::now_v7();
    let bob_id = Uuid::now_v7();
    let cara_id = Uuid::now_v7();

    let alice = PinstarService::new(&server, alice_id, "alice", "editor".to_string());
    let bob = PinstarService::new(&server, bob_id, "bob", "editor".to_string());
    let cara = PinstarService::new(&server, cara_id, "cara", "editor".to_string());

    assert!(wait_until(|| {
        alice.snapshot().your_user_id == Some(alice_id)
            && bob.snapshot().your_user_id == Some(bob_id)
            && cara.snapshot().your_user_id == Some(cara_id)
    }));

    alice.submit_op(PinstarOp::AddNode(text_node("shared-node")));

    assert!(wait_until(|| {
        bob.snapshot()
            .data
            .nodes
            .iter()
            .any(|node| node.id() == "shared-node")
            && cara
                .snapshot()
                .data
                .nodes
                .iter()
                .any(|node| node.id() == "shared-node")
    }));
}
