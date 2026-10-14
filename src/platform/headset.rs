//! Is the default audio output a Bluetooth headset that is still in call mode? Read through `pw-dump`.

use serde_json::Value;
use std::process::{Command, Stdio};

use super::media::OutputProbe;

pub struct PwDumpProbe;

impl OutputProbe for PwDumpProbe {
    fn in_call_mode(&self) -> bool {
        let Ok(out) = Command::new("pw-dump").stderr(Stdio::null()).output() else { return false };
        out.status.success() && in_call_mode(&String::from_utf8_lossy(&out.stdout))
    }
}

/// True when the default sink belongs to a Bluetooth device whose active profile is not A2DP, which means the
/// headset is in the telephony profile and will switch back once nothing records from it.
/// Anything unreadable counts as settled, so a failure here never delays the music.
pub fn in_call_mode(pw_dump: &str) -> bool {
    let Ok(Value::Array(objects)) = serde_json::from_str::<Value>(pw_dump) else { return false };
    let default_sink = objects
        .iter()
        .filter(|o| o["type"] == "PipeWire:Interface:Metadata")
        .flat_map(|o| o["metadata"].as_array().into_iter().flatten())
        .find(|entry| entry["key"] == "default.audio.sink")
        .and_then(|entry| entry["value"]["name"].as_str());
    let Some(default_sink) = default_sink else { return false };
    let device_id = objects
        .iter()
        .find(|o| o["info"]["props"]["node.name"] == default_sink)
        .and_then(|node| node["info"]["props"]["device.id"].as_u64());
    let Some(device_id) = device_id else { return false };
    let Some(device) = objects.iter().find(|o| o["id"] == device_id && o["type"] == "PipeWire:Interface:Device") else {
        return false;
    };
    if device["info"]["props"]["device.api"] != "bluez5" {
        return false;
    }
    match device["info"]["params"]["Profile"][0]["name"].as_str() {
        Some(profile) => !profile.starts_with("a2dp"),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dump(sink_api: &str, profile: &str, default_sink: &str) -> String {
        format!(
            r#"[
              {{"id": 40, "type": "PipeWire:Interface:Metadata",
                "metadata": [{{"subject": 0, "key": "default.audio.sink", "type": "Spa:String:JSON", "value": {{"name": "{default_sink}"}}}}]}},
              {{"id": 70, "type": "PipeWire:Interface:Device",
                "info": {{"props": {{"device.api": "{sink_api}"}}, "params": {{"Profile": [{{"index": 1, "name": "{profile}"}}]}}}}}},
              {{"id": 63, "type": "PipeWire:Interface:Node",
                "info": {{"props": {{"node.name": "bluez_output.X.1", "device.id": 70, "media.class": "Audio/Sink"}}}}}},
              {{"id": 55, "type": "PipeWire:Interface:Node",
                "info": {{"props": {{"node.name": "alsa_output.hdmi", "device.id": 47, "media.class": "Audio/Sink"}}}}}}
            ]"#
        )
    }

    #[test]
    fn headset_profile_means_call_mode() {
        assert!(in_call_mode(&dump("bluez5", "headset-head-unit", "bluez_output.X.1")));
        assert!(in_call_mode(&dump("bluez5", "headset-head-unit-msbc", "bluez_output.X.1")));
    }

    #[test]
    fn a2dp_is_settled() {
        assert!(!in_call_mode(&dump("bluez5", "a2dp-sink-aac", "bluez_output.X.1")));
        assert!(!in_call_mode(&dump("bluez5", "a2dp-sink", "bluez_output.X.1")));
    }

    #[test]
    fn non_bluetooth_sink_is_settled() {
        assert!(!in_call_mode(&dump("bluez5", "headset-head-unit", "alsa_output.hdmi")));
        assert!(!in_call_mode(&dump("alsa", "headset-head-unit", "bluez_output.X.1")));
    }

    #[test]
    fn unreadable_input_is_settled() {
        assert!(!in_call_mode(""));
        assert!(!in_call_mode("[]"));
        assert!(!in_call_mode("{\"not\": \"a list\"}"));
        assert!(!in_call_mode(&dump("bluez5", "headset-head-unit", "unknown-sink")));
    }
}
