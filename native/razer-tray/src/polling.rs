use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

pub struct PollScheduler {
    sender: Sender<Duration>,
}

impl PollScheduler {
    pub fn start<F>(initial_interval: Duration, mut on_tick: F) -> Self
    where
        F: FnMut() -> bool + Send + 'static,
    {
        let (sender, receiver) = mpsc::channel();

        thread::spawn(move || {
            let mut interval = initial_interval;

            loop {
                match receiver.recv_timeout(interval) {
                    Ok(new_interval) => {
                        interval = new_interval;

                        if !on_tick() {
                            break;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        if !on_tick() {
                            break;
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        });

        Self { sender }
    }

    pub fn set_interval(&self, interval: Duration) -> bool {
        self.sender.send(interval).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    #[test]
    fn changing_interval_wakes_scheduler_and_triggers_tick() {
        let (tick_sender, tick_receiver) = mpsc::channel();
        let scheduler = PollScheduler::start(Duration::from_secs(60), move || {
            tick_sender.send(()).is_ok()
        });

        assert!(scheduler.set_interval(Duration::from_millis(1)));
        assert!(tick_receiver.recv_timeout(Duration::from_secs(1)).is_ok());
    }
}
