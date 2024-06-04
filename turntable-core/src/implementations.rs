use std::{
    error::Error,
    ffi::{OsStr, OsString},
    io::SeekFrom,
    path::PathBuf,
    process::{Command as StdCommand, Stdio},
};

use async_trait::async_trait;
use serde::Deserialize;
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt},
    task::spawn_blocking,
};

use crate::{LoadResult, Loadable, ProbeResult};

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
pub async fn ffmpeg_probe<S>(path: S) -> Result<FfmpegProbe, String>
where
    S: Into<PathBuf>,
{
    let path = path.into();

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

/// Represents [Loadable] file path.
pub struct LoadableFile {
    path: OsString,
}

impl LoadableFile {
    pub fn new<S>(path: S) -> Self
    where
        S: AsRef<OsStr>,
    {
        Self {
            path: path.as_ref().into(),
        }
    }
}

#[async_trait]
impl Loadable for LoadableFile {
    async fn load(&self, offset: usize, amount: usize) -> Result<LoadResult, Box<dyn Error>> {
        let mut file = File::open(&self.path).await?;
        let mut buf = vec![0; amount];

        let at_offset = file.seek(SeekFrom::Start(offset as u64)).await?;
        let bytes_read = file.read_exact(&mut buf).await?;
        let bytes = buf[0..bytes_read].to_vec();

        Ok(LoadResult {
            end_reached: bytes_read < amount,
            at_offset: at_offset as usize,
            bytes,
        })
    }

    async fn probe(&self) -> Result<ProbeResult, Box<dyn Error>> {
        let probe = ffmpeg_probe(self.path.clone()).await?;

        Ok(ProbeResult {
            length: probe.format.duration.parse::<f32>().ok(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_file_path() -> PathBuf {
        let root = env!("CARGO_MANIFEST_DIR");
        let mut path = PathBuf::from(root);

        path.pop();
        path.push("assets");
        path.push("musikk.mp3");

        path
    }

    #[tokio::test]
    async fn test_ffmpeg_probe() {
        let probe = ffmpeg_probe(test_file_path())
            .await
            .expect("probes successfully");

        assert_eq!(probe.format.duration, "401.005700");
        assert_eq!(&probe.format.bit_rate[..3], "320");
    }
}