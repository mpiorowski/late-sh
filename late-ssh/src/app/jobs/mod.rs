// The work side of late.sh's Profiles page: the job feed. `svc.rs` is the
// nightly press and the shelf snapshot, `sources.rs` the feed parsers,
// `state.rs` the shelf's session state and the match score, `ui.rs` and
// `input.rs` the shelf itself. The tag vocabulary the read, the matcher,
// and the tag picker share is `late_core::vocab`. See JOBS.md.
pub(crate) mod input;
pub(crate) mod post;
pub(crate) mod sources;
pub(crate) mod state;
pub mod svc;
pub(crate) mod ui;

#[cfg(test)]
mod post_test;
#[cfg(test)]
mod sources_test;
#[cfg(test)]
pub(crate) mod state_test;
#[cfg(test)]
mod svc_test;
