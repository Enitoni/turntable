use std::{
    io::{Read, Stdin, Write},
    process::{Child, ChildStdin, ChildStdout},
    sync::Arc,
    thread,
    time::Duration,
};

use colored::Colorize;
use crossbeam::{
    atomic::AtomicCell,
    channel::{unbounded, Receiver, Sender},
};
use dashmap::DashMap;
use log::trace;
use parking_lot::Mutex;

use crate::{
    audio::{raw_samples_from_bytes, Sample, SAMPLES_PER_SEC, SAMPLE_IN_BYTES},
    logging::LogColor,
};

use self::{
    ffmpeg::spawn,
    loading::{LoadResult, Loader},
};

mod ffmpeg;
mod input;
mod loading;
mod sink;

pub use input::*;
pub use sink::*;

#[derive(Debug)]
pub struct Ingestion {
    current_sink_id: AtomicCell<SinkId>,
    child: Mutex<Option<Child>>,

    sinks: DashMap<SinkId, Sink>,
    loaders: DashMap<SinkId, Box<dyn Loader>>,

    loading_sender: Sender<LoadingMessage>,
    loading_receiver: Receiver<LoadingMessage>,

    processing_sender: Sender<ProcessingMessage>,
    processing_receiver: Receiver<ProcessingMessage>,

    check_channel: (Sender<WriteMessage>, Receiver<WriteMessage>),
}

#[derive(Debug)]
enum LoadingMessage {
    Respawn(ChildStdin, SinkId),
    Load(usize),
}

#[derive(Debug)]
enum ProcessingMessage {
    Respawn(ChildStdout),
}

#[derive(Debug)]
struct WriteMessage {
    pub data: Vec<Sample>,
}

impl Ingestion {
    pub fn new() -> Self {
        let (loading_sender, loading_receiver) = unbounded();
        let (processing_sender, processing_receiver) = unbounded();

        Self {
            current_sink_id: Default::default(),
            child: None.into(),

            sinks: Default::default(),
            loaders: Default::default(),

            loading_sender,
            loading_receiver,

            processing_sender,
            processing_receiver,

            check_channel: unbounded(),
        }
    }

    fn respawn(&self, new_sink_id: SinkId) {
        if self.current_sink().filter(|s| !s.is_complete()).is_some() {
            panic!("attempt to respawn ffmpeg before sink is sealed")
        }

        if let Some(mut child) = self.child.lock().take() {
            child.kill().expect("ffmpeg was killed");
            child.wait().expect("ffmpeg exited");
        }

        let mut new_child = ffmpeg::spawn();

        let stdin = new_child.stdin.take().unwrap();
        let stdout = new_child.stdout.take().unwrap();

        self.current_sink_id.store(new_sink_id);

        self.loading_sender
            .send(LoadingMessage::Respawn(stdin, new_sink_id))
            .unwrap();

        self.processing_sender
            .send(ProcessingMessage::Respawn(stdout))
            .unwrap();
    }

    fn ensure_correct_sink(&self, new_sink_id: SinkId) {
        if new_sink_id != self.current_sink_id.load() {
            self.respawn(new_sink_id);
        }
    }

    pub fn add(&self, loader: Box<dyn Loader>) -> Sink {
        let result = loader.probe().unwrap_or_default();

        let sink = Arc::new(InternalSink::new(result.length));
        let sink_id = sink.id();

        self.sinks.insert(sink_id, sink.clone());
        self.loaders.insert(sink_id, loader);

        sink
    }

    pub fn get(&self, id: SinkId) -> Sink {
        self.sinks.get(&id).expect("sink exists").to_owned()
    }

    pub fn request(&self, id: SinkId, amount: usize) {
        self.ensure_correct_sink(id);

        let sink = self.sinks.get(&id).expect("sink exists");

        self.loading_sender
            .send(LoadingMessage::Load(amount))
            .unwrap();

        sink.wait_for_write()
    }

    pub fn current_sink(&self) -> Option<Sink> {
        self.sinks
            .get(&self.current_sink_id.load())
            .map(|s| s.clone())
    }
}

pub fn spawn_loading_thread(ingestion: Arc<Ingestion>) {
    let run = move || {
        let mut stdin: Option<ChildStdin> = None;
        let mut sink_id = 0;

        let receiver = ingestion.loading_receiver.clone();

        loop {
            match receiver.recv() {
                Ok(LoadingMessage::Respawn(new_stdin, new_sink_id)) => {
                    stdin = Some(new_stdin);
                    sink_id = new_sink_id;
                }
                Ok(LoadingMessage::Load(amount)) => {
                    let stdin = stdin
                        .as_mut()
                        .expect("load was not called with empty stdin");

                    let mut loader = ingestion
                        .loaders
                        .get_mut(&sink_id)
                        .expect("loader exists in ingestion");

                    let sink = ingestion
                        .sinks
                        .get(&sink_id)
                        .expect("sink exists in ingestion");

                    trace!(target: "vinyl::audio",
                        "{}: {}",
                        sink_id,
                        format!("Loading {} samples", amount).color(LogColor::White),
                    );

                    match loader.load(amount) {
                        LoadResult::Data(buf) => stdin.write_all(&buf).expect("write all"),
                        LoadResult::Empty => {
                            sink.seal();

                            trace!(target: "vinyl::audio",
                                "{}: {}",
                                sink.id(),
                                format!("Sealed at {} samples", sink.available()).color(LogColor::Orange),
                            );
                        }
                        LoadResult::Error => todo!(),
                    }
                }
                _ => {}
            }

            thread::sleep(Duration::from_millis(500));
        }
    };

    thread::Builder::new()
        .name("audio_loading".to_string())
        .spawn(run)
        .unwrap();
}

pub fn spawn_processing_thread(ingestion: Arc<Ingestion>) {
    let run = move || {
        let mut stdout: Option<ChildStdout> = None;

        let receiver = ingestion.processing_receiver.clone();
        let sender = ingestion.check_channel.0.clone();

        loop {
            if let Ok(ProcessingMessage::Respawn(new_stdout)) = receiver.try_recv() {
                stdout = Some(new_stdout);
            }

            if let Some(stdout) = stdout.as_mut() {
                let mut buf = [0; SAMPLES_PER_SEC * 10];

                let bytes_read = stdout.read(&mut buf).unwrap_or_default();
                let new_samples = raw_samples_from_bytes(&buf[..bytes_read]);

                sender.send(WriteMessage { data: new_samples }).unwrap();
            }
        }
    };

    thread::Builder::new()
        .name("audio_loading".to_string())
        .spawn(run)
        .unwrap();
}

/// This thread waits for ffmpeg to finish processing the samples before writing it to the sink.
pub fn spawn_load_write_thread(ingestion: Arc<Ingestion>) {
    let run = move || {
        let receiver = ingestion.check_channel.1.clone();
        let mut data = vec![];

        loop {
            if let Some(sink) = ingestion.current_sink() {
                // We wait for the ffmpeg thread to block before proceeding
                if let Ok(message) = receiver.recv_timeout(Duration::from_millis(1000)) {
                    data.extend_from_slice(&message.data);

                    // Keep checking before writing
                    continue;
                }

                if data.is_empty() {
                    continue;
                }

                sink.write(&data);

                trace!(target: "vinyl::audio",
                    "{}: {}",
                    sink.id(),
                    format!("Received {}/{} samples", sink.available(), sink.length())
                        .color(LogColor::Success),
                );

                data.clear();
            }
        }
    };

    thread::Builder::new()
        .name("audio_loading".to_string())
        .spawn(run)
        .unwrap();
}
