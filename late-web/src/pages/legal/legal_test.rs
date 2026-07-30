use askama::Template;

use super::{CONTACT_EMAIL, EFFECTIVE_DATE, OPERATOR_NAME, Terms};

fn rendered() -> String {
    Terms {
        operator_name: OPERATOR_NAME,
        contact_email: CONTACT_EMAIL,
        effective_date: EFFECTIVE_DATE,
    }
    .render()
    .expect("terms template renders")
}

#[test]
fn terms_name_the_operator_and_a_reachable_contact() {
    let page = rendered();

    assert!(page.contains(OPERATOR_NAME), "operator must be named");
    assert!(
        page.contains(&format!("mailto:{}", CONTACT_EMAIL)),
        "contact address must be a working mailto link"
    );
    assert!(
        page.contains(EFFECTIVE_DATE),
        "readers must be able to tell which version applies"
    );
}

#[test]
fn terms_state_the_age_limit_and_the_consent_rules() {
    let page = rendered();

    assert!(
        page.contains("18 or older to use late.sh"),
        "the service is adults only and must say so"
    );
    assert!(
        page.contains("Consent can be withdrawn"),
        "withdrawal of consent is the rule takedowns rest on"
    );
    assert!(
        page.contains("Adult content may appear in any room, without warning"),
        "there are no safe rooms, and the page must say so before someone connects"
    );
}

#[test]
fn terms_disclose_retention_and_the_moderation_log() {
    let page = rendered();

    assert!(
        page.contains("until you ask for deletion"),
        "gdpr requires stating how long data is kept"
    );
    assert!(
        page.contains("kept in the moderation log"),
        "deleted messages are retained in the audit log and that must be disclosed"
    );
}

#[test]
fn terms_prohibit_minors_and_non_consensual_intimate_images() {
    let page = rendered();

    assert!(
        page.contains("Sexual content involving anyone under 18"),
        "the absolute prohibition must be stated"
    );
    assert!(
        page.contains("has not consented to their publication"),
        "non-consensual intimate imagery must be prohibited"
    );
    assert!(
        page.contains("Article 16"),
        "the report path is the DSA notice mechanism and should say so"
    );
}
