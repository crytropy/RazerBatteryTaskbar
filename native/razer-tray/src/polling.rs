use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::thread;
use std::time::Duration;

use razer_core::frontend::FrontendSnapshot;
use razer_core::notifications::{NotificationRequest, evaluate_events};
use razer_core::service::CoreService;
use razer_core::settings::{NotificationSettings, PrimaryDevicePreference};
use razer_core::transport::BatteryTransport;

/// Only frontend-safe data leaves the worker. Raw device IDs/serials stay inside CoreService.
#[derive(Debug)]
pub struct CoreUpdate {
    pub snapshot: FrontendSnapshot,
    pub notifications: Vec<NotificationRequest>,
}

enum WorkerCommand {
    Refresh,
    SetInterval(Duration),
    SetNotifications(NotificationSettings),
    Stop,
}

/// The worker owns the transport and serializes every HID read on its own thread.
pub struct CoreWorker {
    sender: Sender<WorkerCommand>,
}

impl CoreWorker {
    pub fn start<T, F>(
        transport: T,
        interval: Duration,
        notifications: NotificationSettings,
        primary_device: PrimaryDevicePreference,
        on_update: F,
    ) -> Self
    where
        T: BatteryTransport + Send + 'static,
        F: FnMut(CoreUpdate) -> bool + Send + 'static,
    {
        assert!(!interval.is_zero(), "poll interval must be nonzero");
        let (sender, receiver) = mpsc::channel();

        thread::spawn(move || {
            let mut core = CoreService::new(transport);
            core.set_primary_device_preference(primary_device);
            run_worker(core, receiver, interval, notifications, on_update);
        });

        Self { sender }
    }

    pub fn request_refresh(&self) -> bool {
        self.sender.send(WorkerCommand::Refresh).is_ok()
    }

    pub fn set_interval(&self, interval: Duration) -> bool {
        if interval.is_zero() {
            return false;
        }
        self.sender
            .send(WorkerCommand::SetInterval(interval))
            .is_ok()
    }

    pub fn set_notifications(&self, settings: NotificationSettings) -> bool {
        self.sender
            .send(WorkerCommand::SetNotifications(settings))
            .is_ok()
    }
}

impl Drop for CoreWorker {
    fn drop(&mut self) {
        // Never block the event loop waiting for a device handle to finish closing.
        let _ = self.sender.send(WorkerCommand::Stop);
    }
}

fn apply_command(
    command: WorkerCommand,
    interval: &mut Duration,
    notifications: &mut NotificationSettings,
    refresh: &mut bool,
) -> bool {
    match command {
        WorkerCommand::Refresh => *refresh = true,
        WorkerCommand::SetInterval(next) => {
            *interval = next;
            *refresh = true;
        }
        WorkerCommand::SetNotifications(next) => *notifications = next,
        WorkerCommand::Stop => return false,
    }

    true
}

fn run_worker<T, F>(
    mut core: CoreService<T>,
    receiver: Receiver<WorkerCommand>,
    mut interval: Duration,
    mut notifications: NotificationSettings,
    mut on_update: F,
) where
    T: BatteryTransport,
    F: FnMut(CoreUpdate) -> bool,
{
    let mut refresh = true;

    loop {
        // Coalesce refresh requests and preference changes accumulated during a HID read.
        loop {
            match receiver.try_recv() {
                Ok(command) => {
                    if !apply_command(command, &mut interval, &mut notifications, &mut refresh) {
                        return;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }

        if refresh {
            refresh = false;
            let result = core.refresh();
            let update = CoreUpdate {
                snapshot: core.frontend_snapshot(),
                notifications: evaluate_events(&result.events, &notifications),
            };

            if !on_update(update) {
                return;
            }
        }

        match receiver.recv_timeout(interval) {
            Ok(command) => {
                if !apply_command(command, &mut interval, &mut notifications, &mut refresh) {
                    return;
                }
            }
            Err(RecvTimeoutError::Timeout) => refresh = true,
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, mpsc};
    use std::thread;

    use super::*;
    use razer_core::state::{DeviceReading, DeviceType};
    use razer_core::transport::TransportError;

    struct TestTransport {
        reads: Arc<AtomicUsize>,
    }

    impl BatteryTransport for TestTransport {
        fn name(&self) -> &'static str {
            "worker-test"
        }

        fn read_devices(&mut self) -> Result<Vec<DeviceReading>, TransportError> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(10));
            Ok(vec![DeviceReading {
                vendor_id: 0x1532,
                product_id: 0x00B7,
                product_name: Some("DeathAdder V3 Pro".to_string()),
                device_type: DeviceType::Mouse,
                battery: Some(78.8),
                charging: None,
                serial_number: None,
            }])
        }
    }

    #[test]
    fn initial_hid_refresh_runs_off_the_caller_thread() {
        let caller = thread::current().id();
        let (sender, receiver) = mpsc::channel();
        let reads = Arc::new(AtomicUsize::new(0));

        let worker = CoreWorker::start(
            TestTransport {
                reads: Arc::clone(&reads),
            },
            Duration::from_secs(60),
            NotificationSettings::default(),
            PrimaryDevicePreference::Auto,
            move |update| {
                sender
                    .send((thread::current().id(), update.snapshot))
                    .is_ok()
            },
        );

        let (worker_thread, snapshot) = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_ne!(worker_thread, caller);
        assert_eq!(snapshot.primary_device.unwrap().battery, Some(78.8));
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        drop(worker);
    }

    #[test]
    fn rapid_refresh_requests_are_serialized_and_worker_stays_responsive() {
        let (sender, receiver) = mpsc::channel();
        let reads = Arc::new(AtomicUsize::new(0));

        let worker = CoreWorker::start(
            TestTransport {
                reads: Arc::clone(&reads),
            },
            Duration::from_secs(60),
            NotificationSettings::default(),
            PrimaryDevicePreference::Auto,
            move |_| sender.send(()).is_ok(),
        );

        receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        for _ in 0..10 {
            assert!(worker.request_refresh());
        }

        receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        thread::sleep(Duration::from_millis(50));
        // A burst is coalesced instead of opening ten concurrent device handles.
        assert!(reads.load(Ordering::SeqCst) <= 3);
        drop(worker);
    }

    #[test]
    fn polling_interval_change_wakes_worker_without_restarting_it() {
        let (sender, receiver) = mpsc::channel();

        let worker = CoreWorker::start(
            TestTransport {
                reads: Arc::new(AtomicUsize::new(0)),
            },
            Duration::from_secs(60),
            NotificationSettings::default(),
            PrimaryDevicePreference::Auto,
            move |_| sender.send(()).is_ok(),
        );

        receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(worker.set_interval(Duration::from_millis(20)));
        receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(worker.set_notifications(NotificationSettings {
            enabled: false,
            ..NotificationSettings::default()
        }));
        drop(worker);
    }
}
