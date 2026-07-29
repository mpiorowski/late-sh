use askama::Template;
use axum::{
    Router,
    response::{Html, IntoResponse},
    routing::get,
};

use crate::{AppState, error::AppError, metrics};

#[cfg(test)]
mod legal_test;

/// Operator identity shown in the terms. The service is run by a private
/// individual, so this is a name and a contact address, not a company record:
/// the GDPR controller identity and the DSA Article 16 contact point both
/// resolve here. Deliberately constants rather than env config, because they
/// are document content that must not vary per deployment.
const OPERATOR_NAME: &str = "Mateusz Piorowski";
const CONTACT_EMAIL: &str = "admin@dwarfforge.io";

/// Date this version of the terms started applying. Bump it whenever the
/// document changes in a way users would care about.
const EFFECTIVE_DATE: &str = "2026-07-29";

pub(crate) fn router() -> Router<AppState> {
    Router::new().route("/terms", get(terms_handler))
}

#[derive(Template)]
#[template(path = "pages/legal/terms.html")]
struct Terms {
    operator_name: &'static str,
    contact_email: &'static str,
    effective_date: &'static str,
}

async fn terms_handler() -> Result<impl IntoResponse, AppError> {
    metrics::record_page_view("terms", false);
    Ok(Html(
        Terms {
            operator_name: OPERATOR_NAME,
            contact_email: CONTACT_EMAIL,
            effective_date: EFFECTIVE_DATE,
        }
        .render()?,
    ))
}
