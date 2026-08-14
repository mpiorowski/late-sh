use super::GraduatePayload;
use late_core::models::bashquest_graduate::BashquestGraduate;

fn sample_graduate() -> BashquestGraduate {
    BashquestGraduate {
        id: uuid::Uuid::nil(),
        created: chrono::DateTime::parse_from_rfc3339("2026-08-15T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
        updated: chrono::Utc::now(),
        user_id: Some(uuid::Uuid::nil()),
        handle: "grad_one".to_string(),
        certificate: "BASHQUEST - CERTIFICATE OF COMPLETION".to_string(),
        certificate_digest: "deadbeef".to_string(),
    }
}

#[test]
fn payload_carries_public_fields_only() {
    let payload: GraduatePayload = sample_graduate().into();
    assert_eq!(payload.handle, "grad_one");
    assert_eq!(
        payload.certificate,
        "BASHQUEST - CERTIFICATE OF COMPLETION"
    );
    assert_eq!(payload.certificate_digest, "deadbeef");
    assert_eq!(payload.graduated_at, "2026-08-15T12:00:00+00:00");
}
