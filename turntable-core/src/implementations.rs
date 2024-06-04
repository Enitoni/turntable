use std::{
    path::PathBuf,
    process::{Command as StdCommand, Stdio},
};

use serde::Deserialize;
use tokio::task::spawn_blocking;

#[derive(Debug, Deserialize)]
pub struct FfmpegProbe {
    pub format: FfmpegFormat,
}

#[derive(Debug, Deserialize)]
pub struct FfmpegFormat {
    pub duration: String,
    pub bit_rate: String,
}

/// Probes the given path using ffprobe, returning the duration and bit rate.
pub async fn ffmpeg_probe(path: PathBuf) -> Result<FfmpegProbe, String> {
    spawn_blocking(move || {
        let mut child = StdCommand::new("ffprobe")
            .arg("-v")
            .arg("quiet")
            .arg("-print_format")
            .arg("json")
            .arg("-show_format")
            .arg("--")
            .arg(path)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("ffprobe spawned");

        let stdout = child.stdout.take().unwrap();
        let result: Result<FfmpegProbe, _> =
            serde_json::from_reader(stdout).map_err(|e| format!("{:?}", e));

        child.wait().unwrap();
        result
    })
    .await
    .expect("ffprobe finished")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ffmpeg_probe() {
        let root = env!("CARGO_MANIFEST_DIR");
        let mut path = PathBuf::from(root);

        path.pop();
        path.push("assets");
        path.push("musikk.mp3");

        let probe = ffmpeg_probe(path).await.expect("probes successfully");

        assert_eq!(probe.format.duration, "401.005700");
        assert_eq!(&probe.format.bit_rate[..3], "320");
    }
}
