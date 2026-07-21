pub(crate) const ACTION_MESSAGE_PREFIX: &str = "\x01ACTION ";
const ACTION_MESSAGE_SUFFIX: &str = "\x01";

pub(crate) fn encode_action_body(action: &str) -> Option<String> {
    let action = action.trim();
    if action.is_empty() {
        return None;
    }
    Some(format!(
        "{ACTION_MESSAGE_PREFIX}{action}{ACTION_MESSAGE_SUFFIX}"
    ))
}

pub(crate) fn parse_action_body(body: &str) -> Option<&str> {
    body.strip_prefix(ACTION_MESSAGE_PREFIX)
        .map(|rest| rest.trim_end_matches(ACTION_MESSAGE_SUFFIX).trim())
        .filter(|action| !action.is_empty())
}
