//! Bounded display projections, not a snapshot/settings or command bridge.
use serde_json::{json, Value};

pub(crate) fn last_picture(source: &Value, directory: &std::path::Path) -> Option<String> {
    use std::io::Read;
    let path = source["gallery"].as_array()?.last()?["path"].as_str()?;
    let path = std::path::Path::new(path).canonicalize().ok()?;
    let directory = directory.canonicalize().ok()?;
    if path.parent()? != directory {
        return None;
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(4_194_305)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 4_194_304 || !bytes.starts_with(b"BM") {
        return None;
    }
    Some(crate::b64_encode(&bytes))
}

pub(crate) const PANELS: &[&str] = &[
    "pota",
    "conditions",
    "needed",
    "spots",
    "bandmapPhone",
    "bandmapCw",
    "memories",
    "dxped",
    "sats",
    "activation",
    "operate",
    "operatemap",
    "waterfall",
    "sstvviewer",
    "fieldday",
    "contestlog",
    "contestscreen",
    "fdclub",
];

pub(crate) fn requested_panel(args: &str) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Request {
        panel: String,
    }
    let request: Request = serde_json::from_str(args).map_err(|_| "Invalid panel request")?;
    if !PANELS.contains(&request.panel.as_str())
        || ["pota", "conditions", "needed"].contains(&request.panel.as_str())
    {
        return Err("Unknown monitor panel".into());
    }
    Ok(request.panel)
}

fn text(value: &Value) -> String {
    match value {
        Value::String(s) => s.chars().take(300).collect(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

fn cell(row: &Value, key: &str) -> String {
    if ["aosUnix", "losUnix", "whenUnix"].contains(&key) {
        return row[key]
            .as_i64()
            .filter(|&n| n > 0)
            .and_then(|n| chrono::DateTime::from_timestamp(n, 0))
            .map(|date| date.format("%Y-%m-%d %H:%MZ").to_string())
            .unwrap_or_default();
    }
    text(&row[key])
}

fn table(title: &str, rows: &Value, columns: &[(&str, &str)], newest: bool) -> Value {
    let rows = rows.as_array().map(Vec::as_slice).unwrap_or_default();
    let selected: Vec<&Value> = if newest {
        rows.iter().rev().take(200).collect()
    } else {
        rows.iter().take(200).collect()
    };
    json!({ "title": title, "columns": columns.iter().map(|(label, _)| *label).collect::<Vec<_>>(),
        "total": rows.len(), "rows": selected.iter().map(|r| {
            columns.iter().map(|(_, key)| cell(r, key)).collect::<Vec<_>>()
        }).collect::<Vec<_>>() })
}

pub(crate) fn project(panel: &str, source: &Value) -> Result<Value, String> {
    let mut groups = Vec::new();
    let mut metrics = Vec::new();
    let mut extra = json!({});
    match panel {
        "spots" | "bandmapPhone" | "bandmapCw" => {
            let mut rows: Vec<Value> = source
                .as_array()
                .into_iter()
                .flatten()
                .filter(|r| match panel {
                    "bandmapPhone" => r["mode"] == "Phone",
                    "bandmapCw" => r["mode"] == "CW",
                    _ => true,
                })
                .take(2000)
                .cloned()
                .collect();
            if panel != "spots" {
                rows.sort_by(|a, b| {
                    a["freqMhz"]
                        .as_f64()
                        .unwrap_or(f64::INFINITY)
                        .total_cmp(&b["freqMhz"].as_f64().unwrap_or(f64::INFINITY))
                });
            }
            groups.push(table(
                "Spots",
                &json!(rows),
                &[
                    ("Call", "call"),
                    ("MHz", "freqMhz"),
                    ("Band", "band"),
                    ("Mode", "exactMode"),
                    ("Class", "mode"),
                    ("Entity", "entity"),
                ],
                false,
            ));
        }
        "memories" => groups.push(table(
            "Receive memories",
            &source["memories"],
            &[
                ("Name", "name"),
                ("MHz", "rxMhz"),
                ("Mode", "mode"),
                ("Kind", "kind"),
            ],
            false,
        )),
        "dxped" => groups.push(table(
            "DXpedition windows",
            source,
            &[
                ("Call", "call"),
                ("Best window", "best"),
                ("Model", "engine"),
            ],
            false,
        )),
        "sats" => {
            groups.push(table(
                "Satellites",
                &source["birds"],
                &[
                    ("Name", "name"),
                    ("Latitude", "lat"),
                    ("Longitude", "lon"),
                    ("Altitude km", "altKm"),
                ],
                false,
            ));
            groups.push(table(
                "Upcoming passes",
                &source["passes"],
                &[
                    ("Satellite", "name"),
                    ("AOS UTC", "aosUnix"),
                    ("LOS UTC", "losUnix"),
                    ("Max elevation", "maxElDeg"),
                ],
                false,
            ));
            metrics.push(json!(["TLE age (days)", text(&source["tleAgeDays"])]));
        }
        "waterfall" => {
            let bins: Vec<f64> = source["row"]
                .as_array()
                .into_iter()
                .flatten()
                .take(4096)
                .map(|v| {
                    v.as_f64()
                        .filter(|v| v.is_finite())
                        .unwrap_or(0.0)
                        .clamp(0.0, 1.0)
                })
                .collect();
            extra = json!({"spectrum": bins, "loHz": source["loHz"], "hiHz": source["hiHz"]});
        }
        "sstvviewer" => {
            // Raw RGB thumbnail only. Never expose host gallery paths or a file-reading route.
            let w = source["previewWidth"].as_u64().unwrap_or(0);
            let h = source["previewHeight"].as_u64().unwrap_or(0);
            if w > 0 && w <= 160 && h > 0 && h <= 512 {
                if let Some(rgb) = source["previewRgbBase64"]
                    .as_str()
                    .filter(|s| s.len() <= 350_000)
                {
                    extra = json!({"rgb": rgb, "width": w, "height": h});
                }
            }
            metrics.push(json!(["Mode", text(&source["mode"])]));
            metrics.push(json!(["Lines received", text(&source["linesDone"])]));
            groups.push(table(
                "Received images",
                &source["gallery"],
                &[
                    ("Mode", "mode"),
                    ("Received UTC", "finishedUtc"),
                    ("MHz", "freqMhz"),
                    ("Call", "fskId"),
                ],
                true,
            ));
            if extra.get("rgb").is_none() {
                if let Some(bmp) = source["observerBmp"]
                    .as_str()
                    .filter(|b| b.len() <= 5_600_000)
                {
                    extra = json!({"bmp": bmp});
                }
            }
        }
        "activation" => {
            let a = &source["activation"];
            for (label, key) in [
                ("Program", "program"),
                ("Reference", "reference"),
                ("Contacts", "qsoCount"),
                ("Grid", "grid"),
            ] {
                metrics.push(json!([label, text(&a[key])]));
            }
            groups.push(table(
                "Parks",
                &a["parks"],
                &[
                    ("Park", "reference"),
                    ("Contacts", "count"),
                    ("Park to park", "p2p"),
                ],
                false,
            ));
        }
        "operate" | "operatemap" => {
            if panel == "operatemap" {
                let stations: Vec<Value> = source["stations"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .take(200)
                    .map(|s| {
                        json!({"call": text(&s["call"]), "grid": text(&s["grid"]), "snr": s["snr"],
                        "lastHeardSlot": s["lastHeardSlot"], "heardCount": s["heardCount"],
                        "presence": s["presence"], "worked": s["worked"]})
                    })
                    .collect();
                extra = json!({"stations": stations, "myGrid": text(&source["mygrid"])});
            }
            groups.push(table(
                "Heard stations",
                &source["stations"],
                &[
                    ("Call", "call"),
                    ("Grid", "grid"),
                    ("SNR", "snr"),
                    ("Country", "country"),
                    ("Mode", "tier"),
                ],
                false,
            ));
            groups.push(table(
                "Recent decodes",
                &source["recentDecodes"],
                &[
                    ("From", "from"),
                    ("SNR", "snr"),
                    ("Audio Hz", "freqHz"),
                    ("Message", "message"),
                ],
                false,
            ));
        }
        "fieldday" | "contestlog" | "contestscreen" | "fdclub" => {
            let fd = &source["fieldDay"];
            let club = &fd["club"];
            let stats = if panel == "fdclub" { club } else { fd };
            for (label, key) in if panel == "fdclub" {
                vec![
                    ("Sync", "syncState"),
                    ("Score", "score"),
                    ("Contacts", "qsos"),
                    ("Host", "hostCall"),
                ]
            } else {
                vec![
                    ("Contest", "event"),
                    ("Score", "totalScore"),
                    ("Contacts", "qsoCount"),
                    ("Sections", "sections"),
                    ("Operator", "operatorNow"),
                ]
            } {
                metrics.push(json!([label, text(&stats[key])]));
            }
            groups.push(if panel == "fdclub" {
                table(
                    "Club positions",
                    &club["board"],
                    &[
                        ("Position", "posName"),
                        ("Operator", "operator"),
                        ("Band", "band"),
                        ("Mode", "mode"),
                        ("Contacts", "qsos"),
                        ("Rate/hr", "rate"),
                        ("Last seen (s)", "lastSeenSecs"),
                    ],
                    false,
                )
            } else {
                table(
                    "Contest contacts",
                    &fd["log"],
                    &[
                        ("Call", "call"),
                        ("Band", "band"),
                        ("Mode", "mode"),
                        ("Class", "class"),
                        ("Section", "section"),
                        ("Exchange sent", "mex"),
                        ("Logged UTC", "whenUnix"),
                    ],
                    true,
                )
            });
        }
        _ => return Err("Unknown monitor panel".into()),
    }
    Ok(json!({"panel": panel, "groups": groups, "metrics": metrics, "media": extra}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projections_are_bounded_and_never_leak_unselected_fields() {
        let source = json!({"memories": (0..250).map(|_| json!({"name": "x".repeat(400),
            "rxMhz": 14.074, "mode": "FT8", "txMhz": 99, "apiKey": "SECRET"})).collect::<Vec<_>>(),
            "credentials": "SECRET"});
        let view = project("memories", &source).unwrap();
        assert_eq!(view["groups"][0]["rows"].as_array().unwrap().len(), 200);
        assert_eq!(view["groups"][0]["rows"][0][0].as_str().unwrap().len(), 300);
        assert!(!view.to_string().contains("SECRET"));
        assert!(!view.to_string().contains("txMhz"));
        assert!(project("set_tx_enabled", &source).is_err());
    }
    #[test]
    fn every_additional_panel_has_a_projection() {
        for panel in PANELS
            .iter()
            .filter(|p| !["pota", "conditions", "needed"].contains(p))
        {
            assert!(project(panel, &json!({})).is_ok(), "{panel}");
        }
    }

    #[test]
    fn panel_arguments_cannot_add_actions_or_arbitrary_commands() {
        assert_eq!(
            requested_panel(r#"{"panel":"memories"}"#).unwrap(),
            "memories"
        );
        for args in [
            r#"{"panel":"set_ptt"}"#,
            r#"{"panel":"operate","tx":true}"#,
            r#"{"panel":"operate","panel":"memories"}"#,
            r#"{"panel":"../settings"}"#,
        ] {
            assert!(requested_panel(args).is_err());
        }
    }

    #[test]
    fn contest_contacts_are_recent_first_and_hidden_data_is_not_forwarded() {
        let view = project(
            "contestlog",
            &json!({"fieldDay":{"log":[{"call":"OLD"},{"call":"NEW"}],
            "upload":{"password":"SECRET"}}, "conversations":[{"text":"SECRET"}]}),
        )
        .unwrap();
        assert_eq!(view["groups"][0]["rows"][0][0], "NEW");
        assert!(!view.to_string().contains("SECRET"));
    }

    #[test]
    fn band_maps_filter_the_class_and_sort_by_frequency() {
        let source = json!([
            {"call":"HIGH", "mode":"CW", "freqMhz":14.06},
            {"call":"VOICE", "mode":"Phone", "freqMhz":14.25},
            {"call":"LOW", "mode":"CW", "freqMhz":7.03}
        ]);
        let view = project("bandmapCw", &source).unwrap();
        assert_eq!(view["groups"][0]["total"], 2);
        assert_eq!(view["groups"][0]["rows"][0][0], "LOW");
        assert_eq!(
            project("bandmapPhone", &source).unwrap()["groups"][0]["rows"][0][0],
            "VOICE"
        );
        assert_eq!(
            cell(&json!({"whenUnix": 1_700_000_000}), "whenUnix"),
            "2023-11-14 22:13Z"
        );
    }

    #[test]
    fn latest_picture_is_bounded_and_confined_to_the_gallery() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("nexus-observer-{}-{stamp}", std::process::id()));
        let gallery = root.join("gallery");
        std::fs::create_dir_all(&gallery).unwrap();
        let picture = gallery.join("picture.bmp");
        let other = root.join("outside.bmp");
        std::fs::write(&picture, b"BMtest").unwrap();
        std::fs::write(&other, b"BMsecret").unwrap();
        let source = |path: &std::path::Path| json!({"gallery":[{"path":path}]});
        assert_eq!(
            last_picture(&source(&picture), &gallery).unwrap(),
            crate::b64_encode(b"BMtest")
        );
        assert!(last_picture(&source(&other), &gallery).is_none());
        std::fs::write(&picture, vec![b'B'; 4_194_306]).unwrap();
        assert!(last_picture(&source(&picture), &gallery).is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
}
