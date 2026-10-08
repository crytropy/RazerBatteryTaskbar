use crate::diagnostics::DiagnosticsSnapshot;
use crate::events::DeviceEvent;
use crate::frontend::{FrontendDeviceState, FrontendSnapshot};
use crate::manager::DeviceManager;
use crate::settings::PrimaryDevicePreference;
use crate::state::DeviceState;
use crate::transport::BatteryTransport;

pub const DEFAULT_FAILURE_THRESHOLD: u32 = 3;

#[derive(Debug, Clone, PartialEq)]
pub struct RefreshSnapshot {
    pub events: Vec<DeviceEvent>,
    pub devices: Vec<DeviceState>,
    pub primary_device: Option<DeviceState>,
    pub transport_error: Option<String>,
    pub consecutive_transport_failures: u32,
}

pub struct CoreService<T: BatteryTransport> {
    transport: T,
    manager: DeviceManager,
    failure_threshold: u32,
    primary_device_preference: PrimaryDevicePreference,
    consecutive_transport_failures: u32,
    last_transport_error: Option<String>,
}

impl<T: BatteryTransport> CoreService<T> {
    pub fn new(transport: T) -> Self {
        Self::with_failure_threshold(transport, DEFAULT_FAILURE_THRESHOLD)
    }

    pub fn with_failure_threshold(transport: T, failure_threshold: u32) -> Self {
        assert!(
            failure_threshold > 0,
            "failure threshold must be at least 1"
        );

        Self {
            transport,
            manager: DeviceManager::new(),
            failure_threshold,
            primary_device_preference: PrimaryDevicePreference::Auto,
            consecutive_transport_failures: 0,
            last_transport_error: None,
        }
    }

    pub fn refresh(&mut self) -> RefreshSnapshot {
        let (events, transport_error) = match self.transport.read_devices() {
            Ok(readings) => {
                self.consecutive_transport_failures = 0;
                self.last_transport_error = None;
                (self.manager.update_from_readings(readings), None)
            }
            Err(error) => {
                self.consecutive_transport_failures += 1;
                let message = error.to_string();
                self.last_transport_error = Some(message.clone());

                let events = if self.consecutive_transport_failures >= self.failure_threshold {
                    self.manager.mark_all_disconnected()
                } else {
                    Vec::new()
                };

                (events, Some(message))
            }
        };

        let mut events = events;
        self.apply_primary_preference_to_events(&mut events);
        let primary_device = self.selected_primary_device();

        RefreshSnapshot {
            events,
            devices: self.manager.devices(),
            primary_device,
            transport_error,
            consecutive_transport_failures: self.consecutive_transport_failures,
        }
    }

    pub fn devices(&self) -> Vec<DeviceState> {
        self.manager.devices()
    }

    pub fn primary_device(&self) -> Option<DeviceState> {
        self.selected_primary_device()
    }

    pub fn primary_device_preference(&self) -> &PrimaryDevicePreference {
        &self.primary_device_preference
    }

    pub fn set_primary_device_preference(&mut self, preference: PrimaryDevicePreference) {
        self.primary_device_preference = preference;
    }

    pub fn frontend_snapshot(&self) -> FrontendSnapshot {
        FrontendSnapshot {
            devices: self
                .manager
                .devices()
                .iter()
                .map(FrontendDeviceState::from)
                .collect(),
            primary_device: self
                .selected_primary_device()
                .as_ref()
                .map(FrontendDeviceState::from),
            transport_healthy: self.consecutive_transport_failures == 0,
            consecutive_transport_failures: self.consecutive_transport_failures,
        }
    }

    pub fn diagnostics(&self) -> DiagnosticsSnapshot {
        DiagnosticsSnapshot {
            core_version: env!("CARGO_PKG_VERSION"),
            transport_name: self.transport.name().to_string(),
            consecutive_transport_failures: self.consecutive_transport_failures,
            last_transport_error: self.last_transport_error.clone(),
            devices: self.manager.devices(),
            primary_device: self.selected_primary_device(),
        }
    }

    fn selected_primary_device(&self) -> Option<DeviceState> {
        match self.primary_device_preference {
            PrimaryDevicePreference::Auto => self.manager.primary_device(),
            PrimaryDevicePreference::ProductId(product_id) => self
                .manager
                .primary_device_for_product_id(product_id)
                .or_else(|| self.manager.primary_device()),
        }
    }

    fn apply_primary_preference_to_events(&self, events: &mut [DeviceEvent]) {
        let selected = self.selected_primary_device();

        for event in events {
            if let DeviceEvent::DevicesChanged { primary_device, .. } = event {
                *primary_device = selected.clone();
            }
        }
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;
    use crate::events::DeviceEvent;
    use crate::state::{DeviceReading, DeviceType};
    use crate::transport::TransportError;

    struct ScriptedTransport {
        results: VecDeque<Result<Vec<DeviceReading>, TransportError>>,
    }

    impl ScriptedTransport {
        fn new(results: Vec<Result<Vec<DeviceReading>, TransportError>>) -> Self {
            Self {
                results: results.into(),
            }
        }
    }

    impl BatteryTransport for ScriptedTransport {
        fn name(&self) -> &'static str {
            "scripted"
        }

        fn read_devices(&mut self) -> Result<Vec<DeviceReading>, TransportError> {
            self.results.pop_front().unwrap_or_else(|| Ok(Vec::new()))
        }
    }

    fn mouse_reading(battery: f32) -> DeviceReading {
        DeviceReading {
            vendor_id: 0x1532,
            product_id: 0x00AB,
            product_name: Some("Razer Basilisk V3 Pro Wireless".to_string()),
            device_type: DeviceType::Mouse,
            battery: Some(battery),
            charging: None,
            serial_number: Some("MOUSE-1".to_string()),
        }
    }

    fn headset_reading(battery: f32) -> DeviceReading {
        DeviceReading {
            vendor_id: 0x1532,
            product_id: 0x0555,
            product_name: Some("Razer BlackShark V2 Pro".to_string()),
            device_type: DeviceType::Headset,
            battery: Some(battery),
            charging: None,
            serial_number: Some("HEADSET-1".to_string()),
        }
    }

    #[test]
    fn successful_refresh_updates_the_manager_and_resets_transport_health() {
        let transport = ScriptedTransport::new(vec![Ok(vec![mouse_reading(80.0)])]);
        let mut service = CoreService::new(transport);

        let refresh = service.refresh();

        assert_eq!(refresh.consecutive_transport_failures, 0);
        assert_eq!(refresh.devices.len(), 1);
        assert_eq!(refresh.primary_device.unwrap().battery, Some(80.0));
        assert!(matches!(refresh.events[0], DeviceEvent::Connected { .. }));
    }

    #[test]
    fn configured_primary_device_preference_is_used_across_snapshots_and_events() {
        let transport = ScriptedTransport::new(vec![Ok(vec![
            mouse_reading(80.0),
            headset_reading(55.0),
        ])]);
        let mut service = CoreService::new(transport);
        service.set_primary_device_preference(PrimaryDevicePreference::ProductId(0x0555));

        let refresh = service.refresh();

        assert_eq!(refresh.primary_device.as_ref().unwrap().product_id, 0x0555);
        assert_eq!(service.primary_device().unwrap().product_id, 0x0555);
        assert_eq!(
            service
                .frontend_snapshot()
                .primary_device
                .as_ref()
                .unwrap()
                .product_id,
            0x0555
        );

        let event_primary = refresh
            .events
            .iter()
            .find_map(|event| match event {
                DeviceEvent::DevicesChanged { primary_device, .. } => primary_device.as_ref(),
                _ => None,
            })
            .unwrap();

        assert_eq!(event_primary.product_id, 0x0555);
    }

    #[test]
    fn unavailable_primary_device_preference_falls_back_to_automatic_selection() {
        let transport = ScriptedTransport::new(vec![Ok(vec![mouse_reading(80.0)])]);
        let mut service = CoreService::new(transport);
        service.set_primary_device_preference(PrimaryDevicePreference::ProductId(0x0555));

        service.refresh();

        assert_eq!(service.primary_device().unwrap().product_id, 0x00AB);
    }

    #[test]
    fn transient_transport_errors_preserve_existing_connected_state() {
        let transport = ScriptedTransport::new(vec![
            Ok(vec![mouse_reading(80.0)]),
            Err(TransportError::new("temporary failure")),
            Err(TransportError::new("temporary failure")),
        ]);
        let mut service = CoreService::new(transport);

        service.refresh();
        let first_error = service.refresh();
        let second_error = service.refresh();

        assert_eq!(first_error.consecutive_transport_failures, 1);
        assert!(first_error.events.is_empty());
        assert!(first_error.devices[0].connected);

        assert_eq!(second_error.consecutive_transport_failures, 2);
        assert!(second_error.events.is_empty());
        assert!(second_error.devices[0].connected);
    }

    #[test]
    fn third_consecutive_transport_error_marks_devices_disconnected() {
        let transport = ScriptedTransport::new(vec![
            Ok(vec![mouse_reading(80.0)]),
            Err(TransportError::new("failure 1")),
            Err(TransportError::new("failure 2")),
            Err(TransportError::new("failure 3")),
        ]);
        let mut service = CoreService::new(transport);

        service.refresh();
        service.refresh();
        service.refresh();
        let failed = service.refresh();

        assert_eq!(failed.consecutive_transport_failures, 3);
        assert!(!failed.devices[0].connected);
        assert!(matches!(failed.events[0], DeviceEvent::Disconnected { .. }));
        assert!(matches!(
            failed.events[1],
            DeviceEvent::DevicesChanged { .. }
        ));
    }

    #[test]
    fn successful_scan_after_an_error_clears_health_state() {
        let transport = ScriptedTransport::new(vec![
            Err(TransportError::new("temporary failure")),
            Ok(vec![mouse_reading(75.0)]),
        ]);
        let mut service = CoreService::new(transport);

        assert_eq!(service.refresh().consecutive_transport_failures, 1);
        let recovered = service.refresh();

        assert_eq!(recovered.consecutive_transport_failures, 0);
        assert_eq!(recovered.transport_error, None);
        assert_eq!(recovered.primary_device.unwrap().battery, Some(75.0));
        assert_eq!(service.diagnostics().last_transport_error, None);
    }

    #[test]
    fn frontend_snapshot_is_serial_free_and_reports_transport_health() {
        let transport = ScriptedTransport::new(vec![Ok(vec![mouse_reading(66.0)])]);
        let mut service = CoreService::new(transport);
        service.refresh();

        let snapshot = service.frontend_snapshot();

        assert!(snapshot.transport_healthy);
        assert_eq!(snapshot.devices.len(), 1);
        assert_eq!(
            snapshot.primary_device.as_ref().unwrap().battery,
            Some(66.0)
        );

        let debug = format!("{snapshot:?}");
        assert!(!debug.contains("MOUSE-1"));
        assert!(!debug.contains("usb:1532"));
    }
}
