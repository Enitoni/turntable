use crossbeam::channel::unbounded;
use dashmap::DashMap;
use implementors::SymphoniaIngestion;
use std::{error::Error, sync::Arc, thread};

mod config;
mod events;
mod ingestion;
mod output;
mod playback;
mod queuing;
mod util;

pub mod implementors;
pub use config::*;
pub use events::*;
pub use ingestion::*;
pub use output::*;
pub use playback::*;
pub use queuing::*;
pub use util::*;

// Reduces verbosity
type Store<Id, T> = Arc<DashMap<Id, T>>;
type ArcedStore<Id, T> = Store<Id, Arc<T>>;

/// The turntable pipeline, facilitating ingestion, playback, and output.
pub struct Pipeline<I> {
    ingestion: Arc<I>,
    playback: Playback,
    output: Arc<Output>,
    queuing: Arc<Queuing>,

    event_receiver: EventReceiver,
}

/// A type passed to various components of the pipeline, to access state, emit events, and dispatch actions.
#[derive(Clone)]
pub struct PipelineContext {
    pub config: Config,

    action_sender: ActionSender,
    event_sender: EventSender,

    pub sinks: ArcedStore<SinkId, Sink>,
    pub players: ArcedStore<PlayerId, Player>,
    pub queues: Store<PlayerId, BoxedQueue>,
}

impl<I> Pipeline<I>
where
    I: Ingestion + 'static,
{
    pub fn new(config: Config) -> Pipeline<I> {
        let (action_sender, action_receiver) = unbounded();
        let (event_sender, event_receiver) = unbounded();

        let context = PipelineContext {
            config: config.clone(),

            action_sender,
            event_sender,

            sinks: Default::default(),
            players: Default::default(),
            queues: Default::default(),
        };

        let ingestion = Arc::new(I::new(&context));
        let output = Arc::new(Output::new(&context));
        let queuing = Arc::new(Queuing::new(&context, ingestion.clone()));
        let playback = Playback::new(&context, ingestion.clone(), output.clone());

        spawn_action_handler_thread(&context, queuing.clone(), action_receiver);

        Pipeline {
            output,
            queuing,
            playback,
            ingestion,
            event_receiver,
        }
    }

    /// Creates a new player and returns its id.
    pub fn create_player(&self) -> PlayerContext {
        self.playback.create_player()
    }

    /// Creates a new queue for a player and returns it.
    pub fn create_queue<T>(&self, player_id: PlayerId) -> Arc<T>
    where
        T: Queue,
    {
        self.queuing.create_queue(player_id)
    }

    /// Ingests a loader and returns the sink.
    pub async fn ingest<L>(&self, loader: L) -> Result<Arc<Sink>, Box<dyn Error>>
    where
        L: IntoLoadable,
    {
        self.ingestion.ingest(loader).await
    }

    /// Creates a consumer for a player.
    pub fn consume_player<E>(&self, player_id: PlayerId) -> Arc<Consumer>
    where
        E: Encoder,
    {
        self.output.consume_player::<E>(player_id)
    }

    /// Receive events from the pipeline.
    pub fn wait_for_event(&self) -> PipelineEvent {
        self.event_receiver
            .recv()
            .expect("event is received without error")
    }
}

impl Default for Pipeline<SymphoniaIngestion> {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

impl PipelineContext {
    pub fn dispatch(&self, action: PipelineAction) {
        self.action_sender.send(action).expect("action is sent");
    }

    pub fn emit(&self, event: PipelineEvent) {
        self.event_sender.send(event).expect("event is sent");
    }

    /// Creates a new context with the given config.
    /// Only used in tests.
    #[cfg(test)]
    pub fn with_config(config: &Config) -> Self {
        let (action_sender, _) = unbounded();
        let (event_sender, _) = unbounded();

        Self {
            config: config.clone(),
            action_sender,
            event_sender,
            ..Default::default()
        }
    }
}

fn spawn_action_handler_thread(
    context: &PipelineContext,
    queueing: Arc<Queuing>,
    action_receiver: ActionReceiver,
) {
    let players = context.players.clone();
    let config = context.config.clone();

    let run = move || loop {
        let action = action_receiver.recv().unwrap();

        match action {
            PipelineAction::NotifyQueueUpdate { player_id } => {
                queueing.notify_queue_update(player_id);
            }
            PipelineAction::PlayPlayer { player_id } => {
                let player = players.get(&player_id).expect("player exists");
                player.play();
            }
            PipelineAction::PausePlayer { player_id } => {
                let player = players.get(&player_id).expect("player exists");
                player.pause();
            }
            PipelineAction::SeekPlayer {
                player_id,
                position,
            } => {
                let player = players.get(&player_id).expect("player exists");
                let position_in_samples = config.seconds_to_samples(position);

                player.seek(position_in_samples);
            }
        }
    };

    thread::spawn(run);
}

// Realistically, the context should always be created by the pipeline.
// However, in a test, this may not be possible.
#[cfg(test)]
impl Default for PipelineContext {
    fn default() -> Self {
        let (action_sender, _) = unbounded();
        let (event_sender, _) = unbounded();

        Self {
            config: Config::default(),
            action_sender,
            event_sender,

            sinks: Default::default(),
            players: Default::default(),
            queues: Default::default(),
        }
    }
}
