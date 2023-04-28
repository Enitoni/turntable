pub mod sync {
    use std::fmt::Debug;

    use crossbeam::channel::{unbounded, Receiver, Sender};

    /// A helper struct to wait for something to happen
    pub struct Wait {
        sender: Sender<()>,
        receiver: Receiver<()>,
    }

    impl Wait {
        pub fn wait(&self) {
            let _ = self.receiver.recv();
        }

        pub fn notify(&self) {
            self.sender.send(()).unwrap();
        }
    }

    impl Default for Wait {
        fn default() -> Self {
            let (sender, receiver) = unbounded();
            Self { sender, receiver }
        }
    }

    impl Debug for Wait {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "Wait")
        }
    }
}
