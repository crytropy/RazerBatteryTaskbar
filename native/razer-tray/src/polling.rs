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
    SetNotifications(NotificationSettings),
    SetPrimaryDevice(PrimaryDevicePreference),
    Stop,
}

/// What the worker owes the frontend after draining its command queue.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Pending {
    /// Read the transport, then publish.
    refresh: bool,
    /// Publish the current snapshot; no HID read is needed for it.
    publish: bool,
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

    pub fn set_notifications(&self, settings: NotificationSettings) -> bool {
        self.sender
            .send(WorkerCommand::SetNotifications(settings))
            .is_ok()
    }

    /// Re-point the primary device on the running core. The worker answers
    /// with a fresh snapshot built from the devices it already knows, so the
    /// icon switches at once and no extra HID handle is opened.
    pub fn set_primary_device(&self, preference: PrimaryDevicePreference) -> bool {
        self.sender
            .send(WorkerCommand::SetPrimaryDevice(preference))
            .is_ok()
    }
}

impl Drop for CoreWorker {
    fn drop(&mut self) {
        // Never block the event loop waiting for a device handle to finish closing.
        let _ = self.sender.send(WorkerCommand::Stop);
    }
}

fn apply_command<T: BatteryTransport>(
    command: WorkerCommand,
    core: &mut CoreService<T>,
    notifications: &mut NotificationSettings,
    pending: &mut Pending,
) -> bool {
    match command {
        WorkerCommand::Refresh => pending.refresh = true,
        WorkerCommand::SetNotifications(next) => *notifications = next,
        WorkerCommand::SetPrimaryDevice(preference) => {
            core.set_primary_device_preference(preference);
            pending.publish = true;
        }
        WorkerCommand::Stop => return false,
    }

    true
}

fn run_worker<T, F>(
    mut core: CoreService<T>,
    receiver: Receiver<WorkerCommand>,
    interval: Duration,
    mut notifications: NotificationSettings,
    mut on_update: F,
) where
    T: BatteryTransport,
    F: FnMut(CoreUpdate) -> bool,
{
    let mut pending = Pending {
        refresh: true,
        publish: false,
    };

    loop {
        // Coalesce refresh requests and preference changes accumulated during a HID read.
        loop {
            match receiver.try_recv() {
                Ok(command) => {
                    if !apply_command(command, &mut core, &mut notifications, &mut pending) {
                        return;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }

        if pending.refresh {
            pending = Pending::default();
            let result = core.refresh();
            let update = CoreUpdate {
                snapshot: core.frontend_snapshot(),
                notifications: evaluate_events(&result.events, &notifications),
            };

            if !on_update(update) {
                return;
            }
        } else if pending.publish {
            pending = Pending::default();
            let update = CoreUpdate {
                snapshot: core.frontend_snapshot(),
                notifications: Vec::new(),
            };

            if !on_update(update) {
                return;
            }
        }

        match receiver.recv_timeout(interval) {
            Ok(command) => {
                if !apply_command(command, &mut core, &mut notifications, &mut pending) {
                    return;
                }
            }
            Err(RecvTimeoutError::Timeout) => pending.refresh = true,
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

    fn mouse() -> DeviceReading {
        DeviceReading {
            vendor_id: 0x1532,
            product_id: 0x00B7,
            product_name: Some("DeathAdder V3 Pro".to_string()),
            device_type: DeviceType::Mouse,
            battery: Some(78.8),
            charging: None,
            serial_number: None,
        }
    }

    fn headset() -> DeviceReading {
        DeviceReading {
            vendor_id: 0x1532,
            product_id: 0x0527,
            product_name: Some("BlackShark V2 Pro".to_string()),
            device_type: DeviceType::Headset,
            battery: Some(41.0),
            charging: None,
            serial_number: None,
        }
    }

    /// Replays scripted scans in order and repeats the last one forever.
    struct ScriptedTransport {
        scans: Vec<Vec<DeviceReading>>,
        reads: Arc<AtomicUsize>,
    }

    impl BatteryTransport for ScriptedTransport {
        fn name(&self) -> &'static str {
            "worker-scripted"
        }

        fn read_devices(&mut self) -> Result<Vec<DeviceReading>, TransportError> {
            let index = self.reads.fetch_add(1, Ordering::SeqCst);
            Ok(self.scans[index.min(self.scans.len() - 1)].clone())
        }
    }

    fn primary_product_id(snapshot: &FrontendSnapshot) -> Option<u16> {
        snapshot
            .primary_device
            .as_ref()
            .map(|device| device.product_id)
    }

    #[test]
    fn primary_device_change_republishes_the_snapshot_without_a_hid_read() {
        let (sender, receiver) = mpsc::channel();
        let reads = Arc::new(AtomicUsize::new(0));

        let worker = CoreWorker::start(
            ScriptedTransport {
                scans: vec![vec![mouse(), headset()]],
                reads: Arc::clone(&reads),
            },
            Duration::from_secs(60),
            NotificationSettings::default(),
            PrimaryDevicePreference::Auto,
            move |update| sender.send(update.snapshot).is_ok(),
        );

        let snapshot = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(
            primary_product_id(&snapshot),
            Some(0x00B7),
            "automatic picks the mouse"
        );
        assert_eq!(snapshot.devices.len(), 2);

        assert!(worker.set_primary_device(PrimaryDevicePreference::ProductId(0x0527)));
        let snapshot = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(primary_product_id(&snapshot), Some(0x0527));
        assert_eq!(
            reads.load(Ordering::SeqCst),
            1,
            "switching the primary device must not open another HID handle"
        );

        assert!(worker.set_primary_device(PrimaryDevicePreference::Auto));
        let snapshot = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(primary_product_id(&snapshot), Some(0x00B7));
        drop(worker);
    }

    /// Ticket #9: the preferred device unplugged falls back to the core's
    /// default ordering (Mouse first) and comes back by itself on reconnect.
    #[test]
    fn preferred_device_offline_falls_back_to_default_ordering_and_returns_on_reconnect() {
        let (sender, receiver) = mpsc::channel();

        let worker = CoreWorker::start(
            ScriptedTransport {
                scans: vec![
                    vec![mouse(), headset()],
                    vec![mouse()],
                    vec![mouse(), headset()],
                ],
                reads: Arc::new(AtomicUsize::new(0)),
            },
            Duration::from_secs(60),
            NotificationSettings::default(),
            PrimaryDevicePreference::ProductId(0x0527),
            move |update| sender.send(update.snapshot).is_ok(),
        );

        let snapshot = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(
            primary_product_id(&snapshot),
            Some(0x0527),
            "preferred headset wins"
        );

        assert!(worker.request_refresh());
        let snapshot = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(
            primary_product_id(&snapshot),
            Some(0x00B7),
            "mouse is the fallback"
        );
        assert!(
            snapshot
                .devices
                .iter()
                .any(|device| device.product_id == 0x0527 && !device.connected),
            "the unplugged headset is still listed as disconnected"
        );

        assert!(worker.request_refresh());
        let snapshot = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(
            primary_product_id(&snapshot),
            Some(0x0527),
            "preference is restored"
        );
        drop(worker);
    }

    /// A receiver unplug fires several HID interface notifications; the
    /// debouncer must turn the burst into a single worker read.
    #[cfg(windows)]
    #[test]
    fn usb_device_change_burst_coalesces_into_one_rescan() {
        use crate::device_events::{DeviceChangeDebouncer, TimerOutcome};
        use std::time::Instant;

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
        assert_eq!(reads.load(Ordering::SeqCst), 1);

        let mut debouncer = DeviceChangeDebouncer::default();
        let now = Instant::now();
        let mut armed = 0;
        for offset_ms in [0, 10, 20, 150, 300, 600] {
            if debouncer
                .on_event(now + Duration::from_millis(offset_ms))
                .is_some()
            {
                armed += 1;
            }
        }
        assert_eq!(armed, 1, "a burst arms exactly one timer");

        let TimerOutcome::Rescan { wait_until } = debouncer.on_timer(now + Duration::from_secs(10))
        else {
            panic!("the burst deadline has passed");
        };
        assert!(worker.request_refresh());
        receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        thread::sleep(Duration::from_millis(50));
        assert_eq!(reads.load(Ordering::SeqCst), 2, "six events cost one read");

        // The settle rescan is the only other read the burst produces.
        let follow_up = wait_until.expect("a follow-up rescan is scheduled");
        assert_eq!(
            debouncer.on_timer(follow_up),
            TimerOutcome::Rescan { wait_until: None }
        );
        assert!(worker.request_refresh());
        receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        thread::sleep(Duration::from_millis(50));
        assert_eq!(reads.load(Ordering::SeqCst), 3);
        drop(worker);
    }
}
