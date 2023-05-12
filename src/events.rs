use crossbeam::channel::{unbounded, Receiver, Sender};
use serde::Serialize;
use tokio::task::spawn_blocking;

use crate::{
    audio::Track,
    auth::{User, UserId},
    rooms::RoomId,
    server::ws::Recipients,
    VinylContext,
};

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "kebab-case")]
pub enum Event {
    /// A new user connected the room stream
    UserEnteredRoom {
        user: User,
        room: RoomId,
    },
    /// A user disconnected from the room stream
    UserLeftRoom {
        user: UserId,
        room: RoomId,
    },
    /// The current track in a room changed
    TrackUpdate {
        room: RoomId,
        track: Track,
    },
    QueueAdd {
        user: UserId,
        track: Track,
    },
    /// Scheduler read a sink and set a new offset
    PlayerTime {
        room: RoomId,
        seconds: f32,
    },
}

type Message = (Event, Recipients);

#[derive(Debug, Clone)]
pub struct Events {
    sender: Sender<Message>,
    receiver: Receiver<Message>,
}

impl Events {
    pub fn emit(&self, event: Event, recipients: Recipients) {
        self.sender.send((event, recipients)).unwrap();
    }
}

impl Default for Events {
    fn default() -> Self {
        let (sender, receiver) = unbounded();
        Self { sender, receiver }
    }
}

pub async fn check_events(context: VinylContext) {
    while let Ok((event, recipients)) = {
        let receiver = context.events.receiver.clone();
        spawn_blocking(move || receiver.recv()).await.unwrap()
    } {
        let message = serde_json::to_string(&event).expect("serialize event");
        context.websockets.broadcast(message, recipients).await
    }
}

mod new {
    use std::sync::{Arc, Weak};

    use crossbeam::channel::{unbounded, Receiver, Sender};
    use parking_lot::Mutex;

    /// A type that implements emitting and polling for events
    pub trait Gateway<E> {
        fn emit(&self, event: E);
        fn poll(&self) -> E;
    }

    /// A bus manages the registration of handlers and emitters.
    pub struct Bus<G, E> {
        me: Weak<Self>,
        handlers: Mutex<Vec<Box<dyn RawHandler<E> + Send + Sync>>>,
        gateway: G,
    }

    impl<G, E> Bus<G, E>
    where
        G: Gateway<E>,
    {
        pub fn new(gateway: G) -> Arc<Self> {
            Arc::new_cyclic(|me| Self {
                me: me.clone(),
                handlers: Default::default(),
                gateway,
            })
        }

        fn dispatch<T>(&self, event: T)
        where
            T: IntoEvent<E>,
        {
            self.gateway.emit(event.into_event());
        }

        /// Create an emitter that can dispatch to this bus
        pub fn emitter(&self) -> Emitter<G, E> {
            Emitter(self.me.clone())
        }

        /// Register a handler so that it can listen for events
        pub fn register<H: RawHandler<E> + 'static + Send + Sync>(&self, handler: H) {
            let mut handlers = self.handlers.lock();
            let boxed = Box::new(handler);

            handlers.push(boxed);
        }

        /// Delegate events to handlers when they arrive.
        ///
        /// **This should run in a loop in some thread.**
        pub fn tick(&self)
        where
            E: Clone,
        {
            let event = self.gateway.poll();
            let handlers = self.handlers.lock();

            for handler in &*handlers {
                handler.handle(event.clone())
            }
        }
    }

    /// An emitter for a [Bus]
    #[derive(Debug, Clone)]
    pub struct Emitter<G, E>(Weak<Bus<G, E>>);

    impl<G, E> Emitter<G, E>
    where
        G: Gateway<E>,
    {
        pub fn dispatch<T>(&self, event: T)
        where
            T: IntoEvent<E>,
        {
            self.0
                .upgrade()
                .expect("upgrade event bus in emitter")
                .dispatch(event)
        }
    }

    /// A raw event handler that simply handles an incoming event.
    /// Avoid implementing this, and implement [Handler] instead, which has a blanket implementation for this.
    pub trait RawHandler<E> {
        fn handle(&self, incoming: E);
    }

    /// An event handler that can handle incoming events.
    ///
    /// If you don't want to filter any events, set `Incoming` to [Event].
    /// In some cases, you want to avoid boilerplate by matching events, and would like to simply filter events that are relevant to the handler.
    /// To do this, set `Incoming` to a type that implements [Filter].
    pub trait Handler<E> {
        type Incoming: Filter<E>;

        fn handle(&self, incoming: Self::Incoming);
    }

    impl<E, T> RawHandler<E> for T
    where
        T: Handler<E>,
    {
        fn handle(&self, incoming: E) {
            let result = T::Incoming::filter(incoming);

            if let Some(incoming) = result {
                T::handle(self, incoming)
            }
        }
    }

    /// Extracts the sub-event of an event, or returns None if it does not match.
    pub trait Filter<E>
    where
        Self: Sized,
    {
        fn filter(event: E) -> Option<Self>;
    }

    /// Any sub-event that can be converted into an actual event.
    /// This makes ergonomics for module-specific events better.
    ///
    /// For example, one might have an `AudioEvent` and a `QueueEvent`, but these need to be converted into [Event].
    /// This trait helps with that.
    pub trait IntoEvent<E> {
        fn into_event(self) -> E;
    }

    /// A gateway that uses [crossbeam::channel] to send and receive events.
    #[derive(Debug)]
    pub struct Channel<E> {
        sender: Sender<E>,
        receiver: Receiver<E>,
    }

    impl<E> Channel<E> {
        pub fn new() -> Self {
            let (sender, receiver) = unbounded();
            Self { sender, receiver }
        }
    }

    impl<E> Default for Channel<E> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<E> Gateway<E> for Channel<E> {
        fn emit(&self, event: E) {
            self.sender
                .send(event)
                .expect("channel gateway sends event")
        }

        fn poll(&self) -> E {
            self.receiver
                .recv()
                .expect("channel gateway receives event")
        }
    }

    #[cfg(test)]
    mod test {
        use super::{Bus, Channel, Filter, Handler, IntoEvent};
        use parking_lot::Mutex;
        use std::{sync::Arc, thread, time::Duration};

        #[derive(Debug, Clone)]
        enum WeatherEvent {
            Sunny,
            Rain,
        }

        struct WeatherHandler {
            message: Arc<Mutex<String>>,
        }

        impl IntoEvent<Event> for WeatherEvent {
            fn into_event(self) -> Event {
                Event::Weather(self)
            }
        }

        impl Filter<Event> for WeatherEvent {
            fn filter(event: Event) -> Option<Self> {
                match event {
                    Event::Weather(x) => Some(x),
                    _ => None,
                }
            }
        }

        impl Handler<Event> for WeatherHandler {
            type Incoming = WeatherEvent;

            fn handle(&self, incoming: Self::Incoming) {
                let message = match incoming {
                    WeatherEvent::Sunny => "It's sunny and sweaty...",
                    WeatherEvent::Rain => "It's raining! Yay!",
                };

                *self.message.lock() = message.to_string();
            }
        }

        #[derive(Debug, Clone)]
        enum TimeEvent {
            Day,
            Night,
        }

        struct TimeHandler {
            message: Arc<Mutex<String>>,
        }

        impl Filter<Event> for TimeEvent {
            fn filter(event: Event) -> Option<Self> {
                match event {
                    Event::Time(x) => Some(x),
                    _ => None,
                }
            }
        }

        impl IntoEvent<Event> for TimeEvent {
            fn into_event(self) -> Event {
                Event::Time(self)
            }
        }

        impl Handler<Event> for TimeHandler {
            type Incoming = TimeEvent;

            fn handle(&self, incoming: Self::Incoming) {
                let message = match incoming {
                    TimeEvent::Day => "It seems to be noon. I feel so sleepy.",
                    TimeEvent::Night => "Sun is going down? Time to get shit done!",
                };

                *self.message.lock() = message.to_string();
            }
        }

        #[derive(Debug, Clone)]
        enum Event {
            Weather(WeatherEvent),
            Time(TimeEvent),
        }

        fn wait() {
            thread::sleep(Duration::from_millis(10));
        }

        #[test]
        fn test_event_system() {
            let channel: Channel<Event> = Channel::new();
            let bus = Bus::new(channel);
            let emitter = bus.emitter();

            let weather_message =
                Arc::new(Mutex::new("Who knows what the weather is.".to_string()));
            let weather_handler = WeatherHandler {
                message: weather_message.clone(),
            };

            let time_message = Arc::new(Mutex::new("My clock is broken!".to_string()));
            let time_handler = TimeHandler {
                message: time_message.clone(),
            };

            bus.register(weather_handler);
            bus.register(time_handler);

            thread::spawn(move || loop {
                bus.tick()
            });

            emitter.dispatch(TimeEvent::Day);
            wait();

            assert_eq!(
                *time_message.lock(),
                "It seems to be noon. I feel so sleepy."
            );

            emitter.dispatch(TimeEvent::Day);
            emitter.dispatch(TimeEvent::Night);
            wait();

            assert_eq!(
                *time_message.lock(),
                "Sun is going down? Time to get shit done!"
            );

            emitter.dispatch(WeatherEvent::Rain);
            emitter.dispatch(TimeEvent::Day);
            wait();

            assert_eq!(*weather_message.lock(), "It's raining! Yay!");
            assert_eq!(
                *time_message.lock(),
                "It seems to be noon. I feel so sleepy."
            );

            emitter.dispatch(WeatherEvent::Sunny);
            emitter.dispatch(TimeEvent::Night);
            wait();

            assert_eq!(*weather_message.lock(), "It's sunny and sweaty...");
            assert_eq!(
                *time_message.lock(),
                "Sun is going down? Time to get shit done!"
            );
        }
    }
}

pub use new::*;
