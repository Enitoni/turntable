use serde_json::Value;
use std::process::{Child, Command, Stdio};

use crate::audio::{CHANNEL_COUNT, SAMPLE_RATE};

#[derive(Debug)]
pub struct Probe {
    pub duration: f32,
    pub bit_rate: u32,
}

pub fn spawn() -> Child {
    let child = Command::new("ffmpeg")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .args(["-i", "pipe:"])
        .args(["-c:a", "pcm_f32le"])
        .args(["-f", "f32le"])
        .args(["-fflags", "+discardcorrupt"])
        .args(["-ar", &SAMPLE_RATE.to_string()])
        .args(["-ac", &CHANNEL_COUNT.to_string()])
        .args(["pipe:"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("ffmpeg spawned");

    child
}

pub fn probe(path: &str) -> Option<Probe> {
    let mut child = Command::new("ffprobe")
        .arg("-v")
        .arg("quiet")
        .arg("-print_format")
        .arg("json")
        .arg("-show_format")
        .arg("--")
        .arg(path)
        .stdout(Stdio::piped())
        .spawn()
        .expect("ffprobe spawned");

    let stdout = child.stdout.take().unwrap();

    let value: Option<Value> = serde_json::from_reader(stdout).ok();
    let format = value
        .and_then(|v| v.get("format").cloned())
        .and_then(|f| f.as_object().cloned());

    match format {
        Some(format) => {
            let duration: f32 = format
                .get("duration")
                .and_then(|d| d.as_str())
                .and_then(|d| d.parse().ok())
                .unwrap_or_default();

            let bit_rate: u32 = format
                .get("bit_rate")
                .and_then(|d| d.as_str())
                .and_then(|d| d.parse().ok())
                .unwrap_or_default();

            Some(Probe { duration, bit_rate })
        }
        None => None,
    }
}

// ffprobe -v quiet -print_format json -show_format -- Deep_Blue.wav
