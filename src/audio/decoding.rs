use std::{
    fs::File,
    io::{self, Read, Write},
    path::Path,
    process::{Command, Stdio},
    thread,
};

use super::{Sample, SAMPLE_IN_BYTES};

/// Decode any audio to raw 32-bit floating point.
pub fn decode_to_raw<T: 'static + Read + Send + Sync>(mut input: T, name: &str) -> File {
    let path = format!("./processed/temp-{name}",);
    let path = Path::new(path.as_str());

    if path.exists() {
        return File::open(path).unwrap();
    }

    let mut ffmpeg = Command::new("ffmpeg")
        .args(["-i", "pipe:"])
        .args(["-c:a", "pcm_f32le"])
        .args(["-f", "f32le"])
        .args(["-ar", "44100"])
        .args(["-ac", "2"])
        .args(["pipe:"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn ffmpeg. This panic should be a result in the future");

    let mut file = File::create(path).unwrap();
    let mut stdin = ffmpeg.stdin.take().expect("Failed to open stdin");
    let mut stdout = ffmpeg.stdout.take().expect("Failed to open stdout");

    thread::spawn(move || loop {
        let mut buffer = [0; 1024];
        let size = input.read(&mut buffer).unwrap();

        if size == 0 {
            break;
        }

        stdin.write(&mut buffer);
    });

    io::copy(&mut stdout, &mut file).expect("Failed to write to file");
    drop(file);

    ffmpeg.wait().unwrap();
    File::open(path).unwrap()
}

/// Converts a slice of bytes into a vec of [Sample].
pub fn raw_samples_from_bytes(bytes: &[u8]) -> Vec<Sample> {
    bytes
        .chunks_exact(SAMPLE_IN_BYTES)
        .map(|b| {
            let arr: [u8; SAMPLE_IN_BYTES] = [b[0], b[1], b[2], b[3]];
            Sample::from_le_bytes(arr)
        })
        .collect()
}
