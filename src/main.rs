use std::time::Duration;

use audio::run_audio_backend;
use tokio::sync::mpsc;

// #[tokio::main]
// async fn main() {
fn main() {
    // let (sender, mut receiver) = mpsc::channel(100);

    // tokio::task::spawn(audio::run_subscription(sender));

    // while let Some(message) = receiver.recv().await {
    //     dbg!(message);
    // }
    let (status_sender, mut status_receiver) = mpsc::channel(100);

    std::thread::spawn(|| run_audio_backend(status_sender));

    std::thread::spawn(move || {
        loop {
            if let Ok(event) = status_receiver.try_recv() {
                dbg!(event);
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        // while let Ok(event) = status_receiver.try_recv() {
        //     dbg!(event);
        // }
    })
    .join()
    .unwrap()
}

mod audio {
    use std::{sync::Arc, time::Duration};

    use tokio::sync::{
        mpsc::{self},
        oneshot,
    };

    #[derive(Debug, Clone)]
    pub enum Message {
        Ready,
        Error(Error),
        RetryIn(Duration),
    }

    pub async fn run_subscription(output: mpsc::Sender<Message>) {
        let (status_sender, mut status_receiver) = mpsc::channel(100);

        std::thread::spawn(|| run_audio_backend(status_sender));

        while let Some(event) = status_receiver.recv().await {
            let _ = output.send(event).await;
        }

        println!("Subscription ended.")
    }

    pub fn run_audio_backend(status: mpsc::Sender<Message>) {
        let mut state = State::NotConnected(0);

        loop {
            match state {
                State::NotConnected(retry_count) => {
                    let close_channel = Arc::new(signal_hook::low_level::channel::Channel::new());

                    let (result_tx, result_rx) = oneshot::channel();
                    let (shutdown, mut shutdown_rx) = oneshot::channel();

                    let close = close_channel.clone();
                    std::thread::spawn(move || {
                        let _client = match start_jack_client(close) {
                            Ok(client) => {
                                result_tx.send(Ok(())).unwrap();
                                client
                            }
                            Err(err) => {
                                result_tx.send(Err(err)).unwrap();
                                return;
                            }
                        };

                        while let Err(_) = shutdown_rx.try_recv() {
                            println!("worker tick");
                            std::thread::sleep(Duration::from_millis(500));
                        }

                        println!("deactivate client");
                        // SEGFAULT directly
                        // let _ = client.deactivate().unwrap();
                        std::thread::sleep(Duration::from_secs(3));
                        println!("client deactivated");
                        // SEGFAULT on drop
                    });

                    match result_rx.blocking_recv().unwrap() {
                        Ok(_) => {
                            let _ = status.blocking_send(Message::Ready);
                            state = State::Connected(close_channel, shutdown);
                        }
                        Err(err) => {
                            let _ = status.blocking_send(Message::Error(err));
                            state = State::Error(retry_count)
                        }
                    };
                }
                State::Connected(closed, shutdown) => {
                    while let None = closed.recv() {
                        println!("tick");
                        std::thread::sleep(Duration::from_millis(500));
                    }

                    shutdown.send(()).unwrap();
                    // println!("client deactivate");
                    // if let Err(err) = client.deactivate() {
                    //     dbg!(&err);
                    //     status.try_send(Message::Error(err.into())).unwrap();
                    // }

                    println!("before connection lost message");
                    status
                        .try_send(Message::Error(Error::ConnectionLost))
                        .unwrap();

                    state = State::Error(0);
                }
                State::Error(retry_count) => {
                    dbg!("error, retrying: {retry_count}");
                    const SLEEP_TIME_BASE: u64 = 3;

                    let timeout = retry_count * SLEEP_TIME_BASE;
                    let timeout = Duration::from_secs(timeout);

                    let _ = status.blocking_send(Message::RetryIn(timeout));
                    // tokio::time::sleep(timeout).await;
                    std::thread::sleep(timeout);

                    state = State::NotConnected(retry_count + 1);
                }
            }
        }
    }

    fn start_jack_client(
        close: Arc<signal_hook::low_level::channel::Channel<bool>>,
    ) -> Result<jack::AsyncClient<Notifications, Process>, Error> {
        let (client, _status) = jack::Client::new(
            "threading_test",
            jack::ClientOptions::default() | jack::ClientOptions::NO_START_SERVER,
        )?;

        Ok(client.activate_async(Notifications(close), Process)?)
    }

    enum State {
        NotConnected(u64),
        Connected(
            // jack::AsyncClient<Notifications, Process>,
            Arc<signal_hook::low_level::channel::Channel<bool>>,
            oneshot::Sender<()>,
        ),
        Error(u64),
    }

    struct Notifications(Arc<signal_hook::low_level::channel::Channel<bool>>);
    struct Process;

    impl jack::NotificationHandler for Notifications {
        fn thread_init(&self, _: &jack::Client) {}

        unsafe fn shutdown(&mut self, _status: jack::ClientStatus, _reason: &str) {
            self.0.send(true);
        }

        fn freewheel(&mut self, _: &jack::Client, _is_freewheel_enabled: bool) {}

        fn sample_rate(&mut self, _: &jack::Client, _srate: jack::Frames) -> jack::Control {
            jack::Control::Continue
        }

        fn client_registration(&mut self, _: &jack::Client, _name: &str, _is_registered: bool) {}

        fn port_registration(
            &mut self,
            _: &jack::Client,
            _port_id: jack::PortId,
            _is_registered: bool,
        ) {
        }

        fn port_rename(
            &mut self,
            _: &jack::Client,
            _port_id: jack::PortId,
            _old_name: &str,
            _new_name: &str,
        ) -> jack::Control {
            jack::Control::Continue
        }

        fn ports_connected(
            &mut self,
            _: &jack::Client,
            _port_id_a: jack::PortId,
            _port_id_b: jack::PortId,
            _are_connected: bool,
        ) {
        }

        fn graph_reorder(&mut self, _: &jack::Client) -> jack::Control {
            jack::Control::Continue
        }

        fn xrun(&mut self, _: &jack::Client) -> jack::Control {
            jack::Control::Continue
        }
    }

    impl jack::ProcessHandler for Process {
        fn process(
            &mut self,
            _: &jack::Client,
            _process_scope: &jack::ProcessScope,
        ) -> jack::Control {
            jack::Control::Continue
        }
    }

    #[derive(Debug, Clone, thiserror::Error)]
    pub enum Error {
        #[error("jack audio server failed: {0}")]
        Jack(#[from] jack::Error),
        #[error("lost connection to jack audio server")]
        ConnectionLost,
    }
}
