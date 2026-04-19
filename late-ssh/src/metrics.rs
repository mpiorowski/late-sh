#[cfg(feature = "otel")]
mod inner {
    use std::sync::OnceLock;

    use opentelemetry::{
        KeyValue, global,
        metrics::{Counter, UpDownCounter},
    };

    fn meter() -> opentelemetry::metrics::Meter {
        global::meter("late-ssh")
    }

    fn ssh_connections_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_connections_total")
                .with_description("Total inbound SSH connections accepted by the server")
                .build()
        })
    }

    fn ssh_sessions_active() -> &'static UpDownCounter<i64> {
        static METRIC: OnceLock<UpDownCounter<i64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .i64_up_down_counter("late_ssh_sessions_active")
                .with_description("Current number of authenticated active SSH sessions")
                .build()
        })
    }

    fn ws_pair_success_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_ws_pair_success_total")
                .with_description("Successful browser websocket pair connections")
                .build()
        })
    }

    fn ws_pair_rejected_unknown_token_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_ws_pair_rejected_unknown_token_total")
                .with_description(
                    "Websocket pair attempts rejected because no live session owned the token",
                )
                .build()
        })
    }

    fn render_frame_drops_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_render_frame_drops_total")
                .with_description("Frames dropped because the SSH channel was busy")
                .build()
        })
    }

    fn chat_messages_sent_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_chat_messages_sent_total")
                .with_description("Chat messages successfully sent")
                .build()
        })
    }

    fn chat_messages_edited_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_chat_messages_edited_total")
                .with_description("Chat messages successfully edited")
                .build()
        })
    }

    fn votes_cast_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_votes_cast_total")
                .with_description("Votes successfully cast")
                .build()
        })
    }

    pub fn record_ssh_connection() {
        ssh_connections_total().add(1, &[]);
    }

    pub fn add_ssh_session(delta: i64) {
        ssh_sessions_active().add(delta, &[]);
    }

    pub fn record_ws_pair_success() {
        ws_pair_success_total().add(1, &[]);
    }

    pub fn record_ws_pair_rejected_unknown_token() {
        ws_pair_rejected_unknown_token_total().add(1, &[]);
    }

    pub fn record_render_frame_drop() {
        render_frame_drops_total().add(1, &[]);
    }

    pub fn record_chat_message_sent() {
        chat_messages_sent_total().add(1, &[]);
    }

    pub fn record_chat_message_edited() {
        chat_messages_edited_total().add(1, &[]);
    }

    pub fn record_vote_cast(genre: &str) {
        votes_cast_total().add(1, &[KeyValue::new("genre", genre.to_string())]);
    }
}

#[cfg(not(feature = "otel"))]
mod inner {
    pub fn record_ssh_connection() {}
    pub fn add_ssh_session(_delta: i64) {}
    pub fn record_ws_pair_success() {}
    pub fn record_ws_pair_rejected_unknown_token() {}
    pub fn record_render_frame_drop() {}
    pub fn record_chat_message_sent() {}
    pub fn record_chat_message_edited() {}
    pub fn record_vote_cast(_genre: &str) {}
}

pub use inner::*;
