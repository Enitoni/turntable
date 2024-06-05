use std::{
    error::Error,
    ffi::{OsStr, OsString},
    io::SeekFrom,
    path::PathBuf,
    process::{Command as StdCommand, Stdio},
    sync::Arc,
};

use async_trait::async_trait;
use crossbeam::atomic::AtomicCell;
use serde::Deserialize;
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    process::{Child, ChildStdin, ChildStdout, Command},
    spawn,
    sync::Mutex,
    task::spawn_blocking,
};

use crate::{
    raw_samples_from_bytes, BoxedLoadable, Config, LoadResult, Loadable, Loader, ProbeResult, Sink,
    SinkState,
};

#[derive(Debug, Deserialize)]
pub struct FfmpegProbe {
    pub format: FfmpegFormat,
    pub streams: Vec<FfmpegStream>,
}

#[derive(Debug, Deserialize)]
pub struct FfmpegFormat {
    pub duration: String,
    pub format_name: String,
    pub size: String,
}

#[derive(Debug, Deserialize)]
pub struct FfmpegStream {
    pub sample_rate: String,
    pub bit_rate: String,
    pub channels: u32,
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
            // Show json output
            .arg("-print_format")
            .arg("json")
            // Show stream entries
            .arg("-show_entries")
            .arg("stream=sample_rate,channels,bit_rate,size")
            // Only select audio stream
            .arg("-select_streams")
            .arg("a:0")
            // Show format information
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
        let mut bytes_read = 0;

        while bytes_read < amount {
            let amount_read = file.read(&mut buf[bytes_read..]).await?;
            bytes_read += amount_read;

            if amount_read == 0 {
                break;
            }
        }

        Ok(LoadResult {
            end_reached: bytes_read < amount,
            at_offset: at_offset as usize,
            bytes: buf[0..bytes_read].to_vec(),
        })
    }

    async fn probe(&self) -> Result<ProbeResult, Box<dyn Error>> {
        let probe = ffmpeg_probe(self.path.clone()).await?;
        let stream = probe.streams.first().ok_or("audio stream not found")?;

        let format = probe.format.format_name;
        let length = probe.format.size.parse::<usize>().unwrap_or_default();
        let bit_rate = stream.bit_rate.parse::<usize>().unwrap_or_default();
        let sample_rate = stream.sample_rate.parse::<usize>().unwrap_or_default();

        let is_lossy = format == "mp3" || format == "aac" || format == "opus" || format == "vorbis";

        let probe_result = if is_lossy {
            ProbeResult::Compressed {
                length,
                bit_rate,
                frame_size: None,
                sample_rate,
            }
        } else {
            ProbeResult::Raw {
                length,
                sample_rate,
            }
        };

        Ok(probe_result)
    }
}

/// A loader which uses ffmpeg to process data from [Loadable]s into [Sink]s.
/// This is the recommended loader to use.
pub struct FfmpegLoader {
    config: Config,
    probe: ProbeResult,
    sink: Arc<Sink>,
    loadable: BoxedLoadable,
    // The ffmpeg child process
    child: Arc<Mutex<Option<Child>>>,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    stdout: Arc<Mutex<Option<ChildStdout>>>,
    /// Where the last read operation left off at
    last_offset: AtomicCell<usize>,
}

impl FfmpegLoader {
    async fn respawn(&self) {
        let existing_child = {
            let mut child = self.child.lock().await;
            child.take()
        };

        if let Some(mut child) = existing_child {
            child.kill().await.expect("child was killed");
            child.wait().await.expect("child exited");
        }

        let mut child = Command::new("ffmpeg")
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .args(["-i", "pipe:"])
            .args(["-c:a", "pcm_f32le"])
            .args(["-f", "f32le"])
            .args(["-fflags", "+discardcorrupt"])
            .args(["-ar", &self.config.sample_rate.to_string()])
            .args(["-ac", &self.config.channel_count.to_string()])
            .args(["pipe:"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("ffmpeg spawned");

        self.stdin.lock().await.replace(child.stdin.take().unwrap());
        self.stdout
            .lock()
            .await
            .replace(child.stdout.take().unwrap());

        self.child.lock().await.replace(child);
    }

    async fn ensure_spawned(&self) {
        let child_is_none = self.child.lock().await.is_none();

        if child_is_none {
            self.respawn().await;
        }
    }
}

#[async_trait]
impl Loader for FfmpegLoader {
    fn new<L: Loadable>(config: Config, probe: ProbeResult, loadable: L, sink: Arc<Sink>) -> Self {
        Self {
            sink,
            probe,
            config,
            loadable: loadable.boxed(),
            child: Default::default(),
            stdin: Default::default(),
            stdout: Default::default(),
            last_offset: Default::default(),
        }
    }

    async fn load(&self, offset: usize, amount: usize) {
        self.ensure_spawned().await;

        let byte_offset = self.probe.byte_offset(offset);
        let byte_amount = self.probe.byte_offset(offset + amount);
        let load = self.loadable.load(byte_offset, byte_amount).await;

        // Set the loading state on the sink
        self.sink.set_state(SinkState::Loading);

        // If an error occurs, set the sink to an error state and abort
        if let Err(e) = load {
            self.sink.set_state(SinkState::Error(format!("{:?}", e)));
            return;
        }

        let load = load.expect("error is handled above");
        let stdin = self.stdin.clone();

        // Write the data to the stdin so ffmpeg can process it
        // This needs to be done in a spawned task otherwise we get a deadlock
        spawn(async move {
            stdin
                .lock()
                .await
                .as_mut()
                .expect("stdin exists")
                .write_all(&load.bytes)
                .await
        });

        let mut bytes = vec![0; amount];
        let amount_read = self
            .stdout
            .lock()
            .await
            .as_mut()
            .expect("stdout exists")
            .read(&mut bytes)
            .await
            .expect("read without issue");

        let samples = raw_samples_from_bytes(&bytes[..amount_read]);

        self.sink.write(offset, &samples);
        self.sink.set_state(SinkState::Idle);

        if load.end_reached {
            self.sink.set_state(SinkState::Sealed);
        }
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

        dbg!(&probe);

        assert_eq!(probe.format.duration, "401.005700");
        assert_eq!(&probe.streams[0].bit_rate[..3], "320");
    }
}
