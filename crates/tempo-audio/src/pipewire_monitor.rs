//! Explicit native PipeWire receive-monitor outputs. Neither capture nor TX uses this module.
//! A persisted node name resolves to a live ALSA card, server cookie and object serial before
//! the existing TX-card guard runs. The output opens only that instance, never a default sink.

use serde_json::{json, Value};

#[cfg(feature = "device")]
const PREFIX: &str = "pipewire:";
const INSTANCE_PREFIX: &str = "pipewire-instance:";

pub(crate) fn supported_server(version: &str) -> bool {
    version
        .split_once('.')
        .and_then(|(major, _)| major.parse::<u32>().ok())
        .is_some_and(|major| major >= 1)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Target {
    pub name: String,
    pub label: String,
    pub serial: u64,
    pub cookie: u32,
    pub card: String,
}

impl Target {
    fn instance_name(&self) -> String {
        format!(
            "{INSTANCE_PREFIX}{}",
            json!({ "name": self.name, "label": self.label,
            "serial": self.serial, "cookie": self.cookie, "card": self.card })
        )
    }
}

fn number(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}

fn targets(graph: &Value, card_id: &dyn Fn(u32) -> Option<String>) -> Vec<Target> {
    let Some(objects) = graph.as_array() else {
        return Vec::new();
    };
    if !objects.iter().any(|o| {
        o["type"] == "PipeWire:Interface:Core"
            && o["info"]["version"].as_str().is_some_and(supported_server)
    }) {
        return Vec::new();
    }
    let Some(cookie) = objects
        .iter()
        .find(|o| o["type"] == "PipeWire:Interface:Core")
        .and_then(|o| number(&o["info"]["cookie"]))
        .and_then(|n| u32::try_from(n).ok())
    else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for object in objects
        .iter()
        .filter(|o| o["type"] == "PipeWire:Interface:Node")
    {
        let props = &object["info"]["props"];
        if props["media.class"] != "Audio/Sink" {
            continue;
        }
        let Some(name) = props["node.name"].as_str().filter(|s| !s.is_empty()) else {
            continue;
        };
        let Some(serial) = number(&props["object.serial"]) else {
            continue;
        };
        let Some(device_id) = number(&props["device.id"]) else {
            continue;
        };
        let Some(device) = objects.iter().find(|o| {
            o["type"] == "PipeWire:Interface:Device" && number(&o["id"]) == Some(device_id)
        }) else {
            continue;
        };
        let device_props = &device["info"]["props"];
        if device_props["device.api"] != "alsa" {
            continue;
        }
        let Some(card_number) =
            number(&device_props["api.alsa.card"]).and_then(|n| u32::try_from(n).ok())
        else {
            continue;
        };
        if number(&props["api.alsa.pcm.card"]).is_some_and(|n| n != u64::from(card_number)) {
            continue;
        }
        let Some(card) = card_id(card_number)
            .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
        else {
            continue;
        };
        result.push(Target {
            name: name.into(),
            label: props["node.description"].as_str().unwrap_or(name).into(),
            serial,
            cookie,
            card,
        });
    }
    result.retain(|t| {
        objects
            .iter()
            .filter(|o| {
                o["type"] == "PipeWire:Interface:Node"
                    && o["info"]["props"]["node.name"].as_str() == Some(t.name.as_str())
            })
            .count()
            == 1
    });
    result
}

pub(crate) fn instance(name: &str) -> Result<Option<Target>, String> {
    let Some(body) = name.strip_prefix(INSTANCE_PREFIX) else {
        return Ok(None);
    };
    let value: Value =
        serde_json::from_str(body).map_err(|_| "Invalid PipeWire monitor instance")?;
    let text = |key: &str| {
        value[key]
            .as_str()
            .map(str::to_owned)
            .ok_or("Invalid PipeWire monitor instance")
    };
    Ok(Some(Target {
        name: text("name")?,
        label: text("label")?,
        card: text("card")?,
        serial: number(&value["serial"]).ok_or("Invalid PipeWire monitor serial")?,
        cookie: number(&value["cookie"])
            .and_then(|n| u32::try_from(n).ok())
            .ok_or("Invalid PipeWire server identity")?,
    }))
}

#[cfg(feature = "device")]
fn query() -> Result<Vec<Target>, String> {
    use std::io::Read;
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    const LIMIT: u64 = 4 * 1024 * 1024;
    let mut child = tempo_core::process::command("pw-dump")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "PipeWire device discovery needs pw-dump from the PipeWire tools")?;
    let output = child
        .stdout
        .take()
        .ok_or("PipeWire discovery has no output")?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        output
            .take(LIMIT + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    let success = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    let bytes = reader
        .join()
        .map_err(|_| "PipeWire discovery reader stopped")?
        .map_err(|_| "PipeWire discovery could not be read")?;
    if !success || bytes.len() as u64 > LIMIT {
        return Err("PipeWire device discovery is unavailable".into());
    }
    let graph: Value =
        serde_json::from_slice(&bytes).map_err(|_| "PipeWire returned invalid device data")?;
    Ok(targets(&graph, &crate::notify_sound::system_card_id))
}

#[cfg(feature = "device")]
pub fn devices() -> Vec<crate::audiodev::AudioDevice> {
    query()
        .unwrap_or_default()
        .into_iter()
        .map(|t| crate::audiodev::AudioDevice {
            name: format!("{PREFIX}{}", t.name),
            label: format!("PipeWire: {}", t.label),
        })
        .collect()
}

#[cfg(feature = "device")]
pub(crate) fn resolve(name: &str) -> Result<Option<(String, String)>, String> {
    if name.starts_with(INSTANCE_PREFIX) {
        return Err("A PipeWire instance is runtime-only; select a named output".into());
    }
    let Some(node) = name.strip_prefix(PREFIX) else {
        return Ok(None);
    };
    let target = query()?.into_iter().find(|t| t.name == node)
        .ok_or("The selected PipeWire output is unavailable or cannot be identified as a physical ALSA card; no default output was substituted")?;
    Ok(Some((
        format!("hw:CARD={}", target.card),
        target.instance_name(),
    )))
}

#[cfg(feature = "device")]
mod native;
#[cfg(feature = "device")]
pub(crate) use native::Output;

#[cfg(test)]
mod tests {
    use super::*;
    fn graph() -> Value {
        json!([
        {"type":"PipeWire:Interface:Core","id":0,"info":{"cookie":1234,"version":"1.6.10"}},
            {"type":"PipeWire:Interface:Device","id":40,"info":{"props":{"device.api":"alsa","api.alsa.card":"0"}}},
            {"type":"PipeWire:Interface:Node","id":50,"info":{"props":{"media.class":"Audio/Sink","node.name":"alsa_output.test","node.description":"Built-in Audio Analog Stereo","object.serial":900,"device.id":40,"api.alsa.pcm.card":0}}}
        ])
    }
    #[test]
    fn a_named_physical_sink_has_a_guard_card_and_a_server_bound_instance() {
        let selected = targets(&graph(), &|n| (n == 0).then(|| "PCH".into()));
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].card, "PCH");
        assert_eq!(
            instance(&selected[0].instance_name()).unwrap(),
            Some(selected[0].clone())
        );
        assert!(!crate::monitor::monitor_would_transmit(
            "hw:CARD=PCH",
            &["plughw:CARD=CODEC,DEV=0".into()],
            None
        ));
        assert!(crate::monitor::monitor_would_transmit(
            "hw:CARD=PCH",
            &["plughw:CARD=PCH,DEV=0".into()],
            None
        ));
    }
    #[test]
    fn unproven_virtual_renumbered_or_ambiguous_routes_are_not_accepted() {
        let mut old = graph();
        old[0]["info"]["version"] = json!("0.3.48");
        assert!(targets(&old, &|_| Some("PCH".into())).is_empty());
        assert!(targets(&graph(), &|_| None).is_empty());
        for field in ["device.id", "object.serial", "node.name"] {
            let mut g = graph();
            g[2]["info"]["props"].as_object_mut().unwrap().remove(field);
            assert!(targets(&g, &|_| Some("PCH".into())).is_empty());
        }
        let mut g = graph();
        g[2]["info"]["props"]["api.alsa.pcm.card"] = json!(1);
        assert!(targets(&g, &|_| Some("PCH".into())).is_empty());
        let mut g = graph();
        g[1]["info"]["props"]["device.api"] = json!("virtual");
        assert!(targets(&g, &|_| Some("PCH".into())).is_empty());
        let mut g = graph();
        let duplicate = g[2].clone();
        g.as_array_mut().unwrap().push(duplicate);
        assert!(targets(&g, &|_| Some("PCH".into())).is_empty());
        assert!(instance("pipewire:default").unwrap().is_none());
    }

    #[cfg(feature = "device")]
    #[test]
    fn persisted_settings_cannot_supply_their_own_runtime_proof() {
        let target = targets(&graph(), &|_| Some("PCH".into())).remove(0);
        assert!(resolve(&target.instance_name()).is_err());
    }
}
