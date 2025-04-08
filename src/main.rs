use std::time::Duration;

fn main() {
    let (client, _status) =
        jack::Client::new("deadlock_client", jack::ClientOptions::default()).unwrap();

    let client = client
        .activate_async(Notifications, ProcessHandler {})
        .unwrap();

    println!("async client running...");

    // shutdown server
    std::thread::sleep(Duration::from_secs(5));

    println!("sleep ended...");

    // deadlock
    client.deactivate().unwrap();
}

struct Notifications;
struct ProcessHandler;

impl jack::NotificationHandler for Notifications {}

impl jack::ProcessHandler for ProcessHandler {
    fn process(&mut self, _: &jack::Client, _process_scope: &jack::ProcessScope) -> jack::Control {
        jack::Control::Continue
    }
}
