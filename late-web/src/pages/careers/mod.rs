use askama::Template;
use axum::{
    Router,
    http::header,
    response::{Html, IntoResponse},
    routing::get,
};
use serde::Serialize;

use crate::{AppState, error::AppError, metrics};

#[cfg(test)]
mod careers_test;

/// Same inbox as the terms page (legal::CONTACT_EMAIL) — the one address the
/// operator actually reads. Kept as a separate constant so this module has
/// no compile-time dependency on `legal`'s private items.
const CONTACT_EMAIL: &str = "admin@dwarfforge.io";

/// A job posting. `live` gates whether it is a real, applyable-to role.
/// Only `live` postings are serialized as schema.org `JobPosting` JSON-LD or
/// included in the XML feed — publishing structured job data for a posting
/// nobody can actually apply to would mislead job-search crawlers (Google,
/// LinkedIn) and risks late.sh's structured data being flagged as spam.
/// Illustrative postings still render on the page, clearly labelled.
struct Posting {
    live: bool,
    title: &'static str,
    location: &'static str,
    employment_type: &'static str,
    date_posted: &'static str,
    valid_through: &'static str,
    summary: &'static str,
    requirements: &'static [&'static str],
}

const POSTINGS: &[Posting] = &[Posting {
    live: false,
    title: "Contributor-in-Residence",
    location: "Remote",
    employment_type: "CONTRACTOR",
    date_posted: "2026-09-13",
    valid_through: "2026-12-31",
    summary: "There is no open role right now. This card exists so a real \
              posting has somewhere to go, and to show what one will look \
              like when there is one.",
    requirements: &[
        "Comfortable living in a terminal",
        "Cares about small, cozy software over scale",
        "Rust experience is a plus, not a requirement",
    ],
}];

#[derive(Serialize)]
struct JobPostingLd {
    #[serde(rename = "@context")]
    context: &'static str,
    #[serde(rename = "@type")]
    type_: &'static str,
    title: &'static str,
    description: &'static str,
    #[serde(rename = "datePosted")]
    date_posted: &'static str,
    #[serde(rename = "validThrough")]
    valid_through: &'static str,
    #[serde(rename = "employmentType")]
    employment_type: &'static str,
    #[serde(rename = "hiringOrganization")]
    hiring_organization: LdOrganization,
    #[serde(rename = "jobLocation")]
    job_location: LdPlace,
}

#[derive(Serialize)]
struct LdOrganization {
    #[serde(rename = "@type")]
    type_: &'static str,
    name: &'static str,
    #[serde(rename = "sameAs")]
    same_as: &'static str,
}

#[derive(Serialize)]
struct LdPlace {
    #[serde(rename = "@type")]
    type_: &'static str,
    address: LdAddress,
}

#[derive(Serialize)]
struct LdAddress {
    #[serde(rename = "@type")]
    type_: &'static str,
    #[serde(rename = "addressCountry")]
    address_country: &'static str,
}

fn job_posting_ld(posting: &Posting) -> String {
    let ld = JobPostingLd {
        context: "https://schema.org/",
        type_: "JobPosting",
        title: posting.title,
        description: posting.summary,
        date_posted: posting.date_posted,
        valid_through: posting.valid_through,
        employment_type: posting.employment_type,
        hiring_organization: LdOrganization {
            type_: "Organization",
            name: "late.sh",
            same_as: "https://late.sh/",
        },
        job_location: LdPlace {
            type_: "Place",
            address: LdAddress {
                type_: "PostalAddress",
                address_country: "Remote",
            },
        },
    };
    serde_json::to_string(&ld).unwrap_or_default()
}

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/careers", get(careers_handler))
        .route("/careers.xml", get(careers_feed_handler))
}

struct CareersCard {
    live: bool,
    title: &'static str,
    location: &'static str,
    employment_type: &'static str,
    summary: &'static str,
    requirements: &'static [&'static str],
    json_ld: Option<String>,
}

#[derive(Template)]
#[template(path = "pages/careers/page.html")]
struct Careers {
    contact_email: &'static str,
    cards: Vec<CareersCard>,
}

async fn careers_handler() -> Result<impl IntoResponse, AppError> {
    metrics::record_page_view("careers", false);
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
    Ok(Html(
        Careers {
            contact_email: CONTACT_EMAIL,
            cards,
        }
        .render()?,
    ))
}

async fn careers_feed_handler() -> impl IntoResponse {
    let items: String = POSTINGS
        .iter()
        .filter(|posting| posting.live)
        .map(|posting| {
            format!(
                "  <job>\n    <title>{}</title>\n    <location>{}</location>\n    <employmentType>{}</employmentType>\n    <datePosted>{}</datePosted>\n    <validThrough>{}</validThrough>\n    <description>{}</description>\n    <applyEmail>{}</applyEmail>\n  </job>\n",
                xml_escape(posting.title),
                xml_escape(posting.location),
                xml_escape(posting.employment_type),
                xml_escape(posting.date_posted),
                xml_escape(posting.valid_through),
                xml_escape(posting.summary),
                xml_escape(CONTACT_EMAIL),
            )
        })
        .collect();
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<jobs source=\"https://late.sh/careers\">\n{}</jobs>\n",
        items
    );
    ([(header::CONTENT_TYPE, "application/xml; charset=utf-8")], body)
}

fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
