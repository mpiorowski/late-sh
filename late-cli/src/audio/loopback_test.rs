use super::*;
use serde_json::json;

const OWNER: u32 = 4242;

fn stream_node(serial: u64, state: &str, props: Value) -> Value {
    let mut all = json!({
        "media.class": "Stream/Output/Audio",
        "object.serial": serial,
    });
    for (key, value) in props.as_object().expect("props object") {
        all[key] = value.clone();
    }
    json!({
        "id": serial,
        "type": "PipeWire:Interface:Node",
        "info": { "state": state, "props": all },
    })
}

fn helper_props(owner: Value) -> Value {
    json!({ "application.id": HELPER_APP_ID, OWNER_PROP: owner })
}

fn find(objects: Vec<Value>) -> Option<u64> {
    let dump = serde_json::to_vec(&objects).expect("dump json");
    find_helper_stream(&dump, OWNER).expect("valid dump")
}

#[test]
fn picks_this_owners_helper_stream_and_ignores_other_players() {
    let objects = vec![
        json!({ "id": 1, "type": "PipeWire:Interface:Core", "info": null }),
        stream_node(
            81,
            "running",
            json!({ "application.name": "PipeWire ALSA [late]" }),
        ),
        stream_node(
            223,
            "running",
            json!({ "application.name": "Google Chrome" }),
        ),
        // Another late session's helper on the same desktop.
        stream_node(300, "running", helper_props(json!("9999"))),
        stream_node(329, "running", helper_props(json!("4242"))),
    ];
    assert_eq!(find(objects), Some(329));
}

#[test]
fn prefers_a_running_stream_then_the_newest() {
    let objects = vec![
        stream_node(400, "idle", helper_props(json!("4242"))),
        stream_node(340, "running", helper_props(json!(4242))),
        stream_node(350, "running", helper_props(json!("4242"))),
    ];
    assert_eq!(find(objects), Some(350));
}

#[test]
fn no_helper_stream_means_nothing_to_record() {
    let objects = vec![
        stream_node(
            81,
            "running",
            json!({ "application.name": "PipeWire ALSA [late]" }),
        ),
        stream_node(329, "running", json!({ "application.id": HELPER_APP_ID })),
    ];
    assert_eq!(find(objects), None);
}

#[test]
fn silent_chunks_are_not_analyzed_and_sound_is_decoded() {
    let mut chunk = [1.0f32; 4];
    assert!(!decode_audible_chunk(&[0u8; 16], &mut chunk));

    let samples = [0.0f32, 0.25, -0.5, 0.0];
    let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    assert!(decode_audible_chunk(&bytes, &mut chunk));
    assert_eq!(chunk, samples);
}

#[test]
fn helper_env_tags_both_pulse_and_pipewire_clients_with_the_owner() {
    let env = helper_stream_env(OWNER);
    assert_eq!(
        env,
        [
            (
                "PULSE_PROP",
                "application.id=sh.late.youtube late.webview.owner=4242".to_string()
            ),
            (
                "PIPEWIRE_PROPS",
                "{ application.id = sh.late.youtube late.webview.owner = 4242 }".to_string()
            ),
        ]
    );
}
