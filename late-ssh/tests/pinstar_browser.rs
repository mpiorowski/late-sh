use late_core::{
    models::{
        pinstar_diagram::{PinstarDiagram, PinstarDiagramParams},
        pinstar_diagram_member::PinstarDiagramMember,
    },
    test_utils::{create_test_user, test_db},
};
use late_ssh::app::pinstar::browser::load_diagram_list_with_client;

async fn create_diagram(
    client: &tokio_postgres::Client,
    owner_id: uuid::Uuid,
    title: &str,
) -> PinstarDiagram {
    PinstarDiagram::create(
        client,
        PinstarDiagramParams {
            owner_id,
            title: title.to_string(),
            diagram_data: serde_json::json!({}),
            format: "canvas".to_string(),
        },
    )
    .await
    .expect("create pinstar diagram")
}

#[tokio::test]
async fn browser_lists_all_diagrams_with_effective_access() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let owner = create_test_user(&test_db.db, "pin-owner").await;
    let viewer = create_test_user(&test_db.db, "pin-viewer").await;
    let member = create_test_user(&test_db.db, "pin-member").await;

    let owned = create_diagram(&client, viewer.id, "Viewer owned").await;
    let shared = create_diagram(&client, owner.id, "Shared editable").await;
    let public = create_diagram(&client, owner.id, "Public read-only").await;

    PinstarDiagramMember::upsert_member(&client, shared.id, viewer.id, "editor")
        .await
        .expect("add editor member");
    PinstarDiagramMember::upsert_member(&client, shared.id, member.id, "viewer")
        .await
        .expect("add viewer member");

    let entries = load_diagram_list_with_client(&client, viewer.id)
        .await
        .expect("load pinstar browser list");

    assert_eq!(entries.len(), 3);

    let owned_entry = entries
        .iter()
        .find(|entry| entry.id == owned.id)
        .expect("owned diagram listed");
    assert!(owned_entry.is_owner);
    assert!(owned_entry.is_member);
    assert_eq!(owned_entry.role, "owner");
    assert_eq!(owned_entry.owner, viewer.username);

    let shared_entry = entries
        .iter()
        .find(|entry| entry.id == shared.id)
        .expect("shared diagram listed");
    assert!(!shared_entry.is_owner);
    assert!(shared_entry.is_member);
    assert_eq!(shared_entry.role, "editor");
    assert!(shared_entry.members.contains(&viewer.username));
    assert!(shared_entry.members.contains("editor"));
    assert!(shared_entry.members.contains(&member.username));
    assert!(shared_entry.members.contains("viewer"));

    let public_entry = entries
        .iter()
        .find(|entry| entry.id == public.id)
        .expect("public diagram listed");
    assert!(!public_entry.is_owner);
    assert!(!public_entry.is_member);
    assert_eq!(public_entry.role, "viewer");
    assert_eq!(public_entry.members, "");
}

#[tokio::test]
async fn non_members_open_existing_diagrams_as_viewers() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let owner = create_test_user(&test_db.db, "pin-open-owner").await;
    let stranger = create_test_user(&test_db.db, "pin-open-stranger").await;
    let diagram = create_diagram(&client, owner.id, "Readable").await;

    let (_, role) = PinstarDiagram::get_with_member_role(&client, diagram.id, stranger.id)
        .await
        .expect("access check")
        .expect("public viewer access");

    assert_eq!(role, "viewer");
}
