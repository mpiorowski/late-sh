use askama::Template;

use super::{CONTACT_EMAIL, Careers, CareersCard, POSTINGS, job_posting_ld};

fn rendered() -> String {
    let cards = POSTINGS
        .iter()
        .map(|posting| CareersCard {
            live: posting.live,
            title: posting.title,
            location: posting.location,
            employment_type: posting.employment_type,
            summary: posting.summary,
            requirements: posting.requirements,
            json_ld: posting.live.then(|| job_posting_ld(posting)),
        })
        .collect();
    Careers {
        contact_email: CONTACT_EMAIL,
        cards,
    }
    .render()
    .expect("careers template renders")
}

#[test]
fn illustrative_posting_is_labelled_and_carries_no_structured_data() {
    let page = rendered();

    assert!(
        page.contains("example") && page.contains("not an open role"),
        "a non-live posting must be visibly labelled as illustrative"
    );
    assert!(
        !page.contains("application/ld+json"),
        "structured JobPosting data must never be emitted for a posting \
         nobody can actually apply to — that would mislead job crawlers"
    );
}

#[test]
fn page_names_the_hobby_project_and_a_reachable_contact() {
    let page = rendered();

    assert!(
        page.contains("hobby project"),
        "the page must not imply late.sh is a company"
    );
    assert!(
        page.contains(&format!("mailto:{}", CONTACT_EMAIL)),
        "there must be a working contact address"
    );
}

#[test]
fn live_posting_would_carry_a_job_posting_ld_block() {
    let live = super::Posting {
        live: true,
        title: "Test Role",
        location: "Remote",
        employment_type: "CONTRACTOR",
        date_posted: "2026-01-01",
        valid_through: "2026-02-01",
        summary: "A test role.",
        requirements: &[],
    };
    let ld = job_posting_ld(&live);

    assert!(ld.contains("\"@type\":\"JobPosting\""));
    assert!(ld.contains("\"title\":\"Test Role\""));
    assert!(ld.contains("\"hiringOrganization\""));
}
