use std::{
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

fn main() -> ! {
    let closed = Arc::new(AtomicBool::new(false));
    let mut state = State::NotConnected(0);

    loop {
        match state {
            State::NotConnected(retry_count) => {
                println!("trying to connect, retry #{retry_count}");
                match audio::start_jack_client(closed.clone()) {
                    Ok(client) => state = State::Connected(client),
                    Err(err) => {
                        dbg!(err);
                        state = State::Error(retry_count);
                    }
                }
            }
            State::Connected(client) => {
                while closed.load(std::sync::atomic::Ordering::Relaxed) != true {
                    println!("tick");
                    std::thread::sleep(Duration::from_millis(500));
                }

                // deadlocks here
                println!("deactivate client");
                match client.deactivate() {
                    Ok((client, _notification, _handler)) => {
                        dbg!(&client);
                    }
                    Err(err) => {
                        dbg!(err);
                    }
                }
                println!("client deactivated");

                state = State::Error(0);
            }
            State::Error(retry_count) => {
                println!("error, retrying: {retry_count}");
                const SLEEP_TIME_BASE: u64 = 3;

                let timeout = retry_count * SLEEP_TIME_BASE;
                let timeout = Duration::from_secs(timeout);

                std::thread::sleep(timeout);

                state = State::NotConnected(retry_count + 1);
            }
        }
    }
}

enum State {
    NotConnected(u64),
    Connected(jack::AsyncClient<audio::Notifications, audio::Process>),
    Error(u64),
}

mod audio {
    use std::sync::{Arc, atomic::AtomicBool};

    pub(crate) fn start_jack_client(
        // close: Arc<signal_hook::low_level::channel::Channel<bool>>,
        close: Arc<AtomicBool>,
    ) -> Result<jack::AsyncClient<Notifications, Process>, Error> {
        println!("start jack client");

        let (client, _status) =
            jack::Client::new("threading_test", jack::ClientOptions::default())?;

        println!("start jack client, register port");
        client.register_port("rust_in_l", jack::AudioIn::default())?;

        println!("start jack client, activate async");
        Ok(client.activate_async(Notifications(close), Process)?)
    }

    // struct Notifications(Arc<signal_hook::low_level::channel::Channel<bool>>);
    pub(crate) struct Notifications(pub Arc<AtomicBool>);
    pub(crate) struct Process;

    impl jack::NotificationHandler for Notifications {
        fn thread_init(&self, _: &jack::Client) {}

        unsafe fn shutdown(&mut self, _status: jack::ClientStatus, _reason: &str) {
            self.0.store(true, std::sync::atomic::Ordering::Relaxed);
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
