//! Native PipeWire playback of the existing receive-monitor rings. Output is pinned to a
//! validated server/serial pair and may neither fall back nor move to another sink.

use super::Target;
use crate::monitor::{MonitorFeed, Playout, SpscRing};
use pipewire as pw;
use pw::prelude::*;
use pw::{properties, spa};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::Duration;

pub(crate) struct Output {
    quit: pw::channel::Sender<()>,
    worker: Option<JoinHandle<()>>,
}

impl Drop for Output {
    fn drop(&mut self) {
        let _ = self.quit.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Output {
    pub(crate) fn open(
        target: Target,
        rate: u32,
        ring: Arc<SpscRing>,
        feed: MonitorFeed,
        level: Arc<AtomicU32>,
    ) -> Result<Self, String> {
        let (quit, receiver) = pw::channel::channel();
        let (ready, result) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("rx-pipewire".into())
            .spawn(move || {
                let report = ready.clone();
                if let Err(error) = run(target, rate, ring, feed, level, receiver, ready) {
                    let _ = report.send(Err(error.clone()));
                    eprintln!("tempo-audio: PipeWire receive monitor stopped: {error}");
                }
            })
            .map_err(|_| "PipeWire monitor worker could not start")?;
        let output = Self {
            quit,
            worker: Some(worker),
        };
        match result.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(output),
            Ok(Err(error)) => Err(error),
            Err(_) => Err("PipeWire monitor did not connect to the selected output; no default was substituted".into()),
        }
    }
}

struct Playback {
    playout: Playout,
    ring: Arc<SpscRing>,
    feed: MonitorFeed,
    level: Arc<AtomicU32>,
}

fn run(
    target: Target,
    rate: u32,
    ring: Arc<SpscRing>,
    feed: MonitorFeed,
    level: Arc<AtomicU32>,
    quit: pw::channel::Receiver<()>,
    ready: mpsc::Sender<Result<(), String>>,
) -> Result<(), String> {
    pw::init();
    let error = |e: pw::Error| e.to_string();
    let mainloop = pw::MainLoop::new().map_err(error)?;
    let context = pw::Context::new(&mainloop).map_err(error)?;
    let core = context.connect(None).map_err(error)?;
    let cookie = Rc::new(Cell::new(None));
    let fault = Rc::new(RefCell::new(None));
    let observed = cookie.clone();
    let version_fault = fault.clone();
    let stop = mainloop.clone();
    let failed = fault.clone();
    let fail_stop = mainloop.clone();
    let _core_listener = core
        .add_listener_local()
        .info(move |info| {
            if observed.get().is_none() {
                if !super::supported_server(info.version()) {
                    *version_fault.borrow_mut() =
                        Some("Named receive monitoring requires PipeWire 1.0 or newer".into());
                }
                observed.set(Some(info.cookie()));
                stop.quit();
            }
        })
        .error(move |id, _, _, message| {
            // Object-local errors are delivered to their own stream listener. Only a
            // core connection failure tears down the entire playback connection here.
            if id == 0 {
                *failed.borrow_mut() = Some(message.to_string());
                fail_stop.quit();
            }
        })
        .register();
    let stop = mainloop.clone();
    let cancelled = Rc::new(Cell::new(false));
    let cancel = cancelled.clone();
    let _quit = quit.attach(mainloop.as_ref(), move |_| {
        cancel.set(true);
        stop.quit();
    });
    let started = Rc::new(Cell::new(false));
    let timed_started = started.clone();
    let stop = mainloop.clone();
    let timer = mainloop.add_timer(move |_| {
        if !timed_started.get() {
            stop.quit();
        }
    });
    timer
        .update_timer(Some(Duration::from_secs(2)), None)
        .into_result()
        .map_err(|e| e.to_string())?;
    if cookie.get().is_none() {
        mainloop.run();
    }
    if cancelled.get() {
        return Err("PipeWire monitor opening cancelled".into());
    }
    if cookie.get() != Some(target.cookie) {
        return Err(
            "PipeWire server changed or could not be identified; select the output again".into(),
        );
    }
    if let Some(error) = fault.borrow_mut().take() {
        return Err(error);
    }

    let mut stream = pw::stream::Stream::new(
        &core,
        "Nexus receive monitor",
        properties! {
            "application.name" => "Nexus",
            "media.type" => "Audio",
            "media.role" => "Music",
            "media.category" => "Playback",
            "media.name" => "Nexus receive monitor",
            "target.object" => target.serial.to_string(),
            "node.dont-fallback" => "true",
            "node.dont-move" => "true",
            "node.dont-reconnect" => "true",
        },
    )
    .map_err(error)?;
    let stop = mainloop.clone();
    let playback = Playback {
        playout: Playout::new(rate, rate),
        ring,
        feed,
        level,
    };
    let readiness = RefCell::new(Some(ready));
    let streaming = started.clone();
    let _listener = stream
        .add_local_listener_with_user_data(Some(playback))
        .state_changed(move |_, state| match state {
            pw::stream::StreamState::Streaming => {
                streaming.set(true);
                if let Some(ready) = readiness.borrow_mut().take() {
                    let _ = ready.send(Ok(()));
                }
            }
            pw::stream::StreamState::Error(message) => {
                if let Some(ready) = readiness.borrow_mut().take() {
                    let _ = ready.send(Err(message));
                }
                stop.quit();
            }
            _ => {}
        })
        .process(|stream, data| {
            let Some(data) = data.as_mut() else { return };
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let Some(plane) = buffer.datas_mut().first_mut() else {
                return;
            };
            let frames = if let Some(bytes) = plane.data() {
                let frames = bytes.len() / 4;
                let mut chunks = bytes.chunks_exact_mut(4);
                let level = f32::from_bits(data.level.load(Ordering::Relaxed));
                let audible = data.feed.output_open();
                data.playout.fill(&data.ring, &data.feed, frames, |sample| {
                    if let Some(chunk) = chunks.next() {
                        let value = if audible {
                            (sample * level).clamp(-1.0, 1.0)
                        } else {
                            0.0
                        };
                        chunk.copy_from_slice(&value.to_le_bytes());
                    }
                });
                frames
            } else {
                0
            };
            let chunk = plane.chunk_mut();
            *chunk.offset_mut() = 0;
            *chunk.stride_mut() = 4;
            *chunk.size_mut() = (frames * 4) as u32;
        })
        .register()
        .map_err(error)?;
    let id = |value| spa::pod::Value::Id(spa::utils::Id(value));
    let property = |key, value| spa::pod::Property {
        key,
        value,
        flags: spa::pod::PropertyFlags::empty(),
    };
    let bytes = spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(spa::pod::Object {
            type_: spa_sys::SPA_TYPE_OBJECT_Format,
            id: spa_sys::SPA_PARAM_EnumFormat,
            properties: vec![
                property(
                    spa_sys::SPA_FORMAT_mediaType,
                    id(spa_sys::SPA_MEDIA_TYPE_audio),
                ),
                property(
                    spa_sys::SPA_FORMAT_mediaSubtype,
                    id(spa_sys::SPA_MEDIA_SUBTYPE_raw),
                ),
                property(
                    spa_sys::SPA_FORMAT_AUDIO_format,
                    id(spa_sys::SPA_AUDIO_FORMAT_F32_LE),
                ),
                property(
                    spa_sys::SPA_FORMAT_AUDIO_rate,
                    spa::pod::Value::Int(
                        i32::try_from(rate).map_err(|_| "Invalid PipeWire sample rate")?,
                    ),
                ),
                property(spa_sys::SPA_FORMAT_AUDIO_channels, spa::pod::Value::Int(1)),
            ],
        }),
    )
    .map_err(|_| "PipeWire audio format could not be encoded")?
    .0
    .into_inner();
    // The legacy binding accepts a raw POD pointer; preserve its required 8-byte alignment.
    let mut aligned = vec![0_u64; bytes.len().div_ceil(8)];
    for (word, chunk) in aligned.iter_mut().zip(bytes.chunks(8)) {
        let mut octets = [0; 8];
        octets[..chunk.len()].copy_from_slice(chunk);
        *word = u64::from_ne_bytes(octets);
    }
    stream
        .connect(
            spa::Direction::Output,
            None,
            pw::stream::StreamFlags::AUTOCONNECT
                | pw::stream::StreamFlags::MAP_BUFFERS
                | pw::stream::StreamFlags::DONT_RECONNECT,
            &mut [aligned.as_ptr().cast()],
        )
        .map_err(error)?;
    timer
        .update_timer(Some(Duration::from_secs(2)), None)
        .into_result()
        .map_err(|e| e.to_string())?;
    mainloop.run();
    if let Some(error) = fault.borrow_mut().take() {
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::sync::atomic::AtomicBool;
    use std::time::Instant;

    fn dump() -> Value {
        let output = tempo_core::process::command("pw-dump").output().unwrap();
        assert!(output.status.success());
        serde_json::from_slice(&output.stdout).unwrap()
    }

    fn command(program: &str, args: &[&str]) {
        assert!(tempo_core::process::command(program)
            .args(args)
            .status()
            .unwrap()
            .success());
    }

    #[test]
    #[ignore = "requires an isolated PipeWire server with nexus-test-speakers and nexus-test-radio null sinks"]
    fn pinned_output_never_follows_a_new_default_or_a_removed_target() {
        assert_eq!(
            std::env::var("NEXUS_ISOLATED_PIPEWIRE_TEST").as_deref(),
            Ok("1")
        );
        let graph = dump();
        let objects = graph.as_array().unwrap();
        let node = |name: &str| {
            objects
                .iter()
                .find(|o| o["info"]["props"]["node.name"] == name)
                .unwrap()
        };
        let speaker = node("nexus-test-speakers");
        let radio = node("nexus-test-radio");
        assert_eq!(
            speaker["info"]["props"]["factory.name"],
            "support.null-audio-sink"
        );
        assert_eq!(
            radio["info"]["props"]["factory.name"],
            "support.null-audio-sink"
        );
        let speaker_id = speaker["id"].as_u64().unwrap();
        let radio_id = radio["id"].as_u64().unwrap();
        let cookie = objects
            .iter()
            .find(|o| o["type"] == "PipeWire:Interface:Core")
            .unwrap()["info"]["cookie"]
            .as_u64()
            .unwrap() as u32;
        let target = Target {
            name: "nexus-test-speakers".into(),
            label: "Test speakers".into(),
            card: "TEST".into(),
            serial: speaker["info"]["props"]["object.serial"].as_u64().unwrap(),
            cookie,
        };
        let feed = MonitorFeed::new(
            48_000,
            Arc::new(AtomicBool::new(true)),
            Arc::new(AtomicBool::new(false)),
        );
        let ring = Arc::new(SpscRing::new(131_072));
        let samples: Vec<f32> = (0..96_000)
            .map(|n| (n as f32 * std::f32::consts::TAU * 1_000.0 / 48_000.0).sin() * 0.1)
            .collect();
        ring.push_slice(&samples);
        assert_eq!(ring.len(), samples.len());
        let open = |target| {
            Output::open(
                target,
                48_000,
                ring.clone(),
                feed.clone(),
                Arc::new(AtomicU32::new(0.5_f32.to_bits())),
            )
        };
        let mut wrong_server = target.clone();
        wrong_server.cookie ^= 1;
        assert!(open(wrong_server).is_err());
        let output = open(target.clone()).unwrap();
        std::thread::sleep(Duration::from_millis(250));
        let queued = ring.len();
        assert!((40_000..95_000).contains(&queued), "a 48 kHz output consumed an implausible number of samples in 250 ms: {queued} remain from 96000");
        let assert_links = || {
            let graph = dump();
            let objects = graph.as_array().unwrap();
            let stream = objects
                .iter()
                .find(|o| {
                    o["type"] == "PipeWire:Interface:Node"
                        && o["info"]["props"]["application.name"] == "Nexus"
                })
                .expect("monitor must remain on its selected sink");
            let props = &stream["info"]["props"];
            for key in [
                "node.dont-fallback",
                "node.dont-move",
                "node.dont-reconnect",
            ] {
                assert!(props[key] == true || props[key] == "true", "{key}: {props}");
            }
            let links: Vec<_> = objects
                .iter()
                .filter(|o| {
                    o["type"] == "PipeWire:Interface:Link"
                        && o["info"]["output-node-id"] == stream["id"]
                })
                .collect();
            assert!(
                !links.is_empty(),
                "positive control: playback must actually link"
            );
            assert!(links
                .iter()
                .all(|o| o["info"]["input-node-id"] == speaker_id));
        };
        assert_links();
        command("wpctl", &["set-volume", &speaker_id.to_string(), "0.25"]);
        let volume = tempo_core::process::command("wpctl")
            .args(["get-volume", &speaker_id.to_string()])
            .output()
            .unwrap();
        assert!(volume.status.success());
        assert!(String::from_utf8(volume.stdout).unwrap().contains("0.25"));
        assert_links();
        command("wpctl", &["set-default", &radio_id.to_string()]);
        std::thread::sleep(Duration::from_millis(250));
        assert_links();
        command("pw-cli", &["destroy", &speaker_id.to_string()]);
        let deadline = Instant::now() + Duration::from_secs(3);
        while !output.worker.as_ref().unwrap().is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            output.worker.as_ref().unwrap().is_finished(),
            "removed output must stop rather than reconnect"
        );
        drop(output);
        assert!(
            open(target).is_err(),
            "stale target must never fall back to the test radio"
        );
        let graph = dump();
        assert!(!graph
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["type"] == "PipeWire:Interface:Link"
                && o["info"]["input-node-id"] == radio_id));
    }
}
