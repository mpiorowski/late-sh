// The work side of late.sh's Profiles page: the job feed. `svc.rs` is the
// nightly press and the shelf snapshot, `sources.rs` the feed parsers,
// `vocab.rs` the tag vocabulary the work card editor and the matcher
// share, `state.rs` the shelf's session state and the match score,
// `ui.rs` and `input.rs` the shelf itself. See JOBS.md.
pub(crate) mod input;
pub(crate) mod sources;
pub(crate) mod state;
pub mod svc;
pub(crate) mod ui;
pub(crate) mod vocab;

#[cfg(test)]
mod sources_test;
#[cfg(test)]
pub(crate) mod state_test;
#[cfg(test)]
mod svc_test;
#[cfg(test)]
mod vocab_test;
