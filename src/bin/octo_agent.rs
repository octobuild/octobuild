use daemon::Daemon;
use daemon::DaemonRunner;
use daemon::State;
use std::sync::mpsc::Receiver;

fn main() {
    env_logger::init();

    let daemon = Daemon {
        name: "octobuild_agent".to_string(),
    };

    daemon
        .run(move |rx: Receiver<State>| {
            // Just consume all signals until channel is closed
            rx.iter().for_each(drop);
        })
        .unwrap();
}
