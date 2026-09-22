// The work side of late.sh: the tag vocabulary that the work card editor
// normalizes skills into and the job matcher joins on. The job feed itself
// (the press, the shelf, the paper section) lands here next; see JOBS.md.
pub(crate) mod vocab;

#[cfg(test)]
mod vocab_test;
