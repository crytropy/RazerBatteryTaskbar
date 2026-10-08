use std::collections::{HashMap, HashSet};

use crate::events::DeviceEvent;
use crate::state::{DeviceReading, DeviceState};

#[derive(Debug, Default)]
pub struct DeviceManager {
    devices: HashMap<String, DeviceState>,
}

impl DeviceManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update_from_readings(&mut self, readings: Vec<DeviceReading>) -> Vec<DeviceEvent> {
        let mut events = Vec::new();
        let mut seen_device_ids = HashSet::new();
        let mut changed = false;

        for reading in readings {
            let next_state = DeviceState::connected(reading);
            let device_id = next_state.id.clone();
            let previous_state = self.devices.get(&device_id).cloned();

            seen_device_ids.insert(device_id.clone());
            self.devices.insert(device_id, next_state.clone());

            match previous_state {
                None => {
                    events.push(DeviceEvent::Connected {
                        previous: None,
                        current: next_state,
                    });
                    changed = true;
                }
                Some(previous) if !previous.connected => {
                    events.push(DeviceEvent::Connected {
                        previous: Some(previous),
                        current: next_state,
                    });
                    changed = true;
                }
                Some(previous) => {
                    if previous.battery != next_state.battery {
                        events.push(DeviceEvent::BatteryChanged {
                            previous: previous.clone(),
                            current: next_state.clone(),
                        });
                        changed = true;
                    }

                    if previous.charging != next_state.charging {
                        events.push(DeviceEvent::ChargingChanged {
                            previous,
                            current: next_state,
                        });
                        changed = true;
                    }
                }
            }
        }

        let missing_device_ids: Vec<String> = self
            .devices
            .iter()
            .filter(|(device_id, state)| state.connected && !seen_device_ids.contains(*device_id))
            .map(|(device_id, _)| device_id.clone())
            .collect();

        for device_id in missing_device_ids {
            if let Some(previous) = self.devices.get(&device_id).cloned() {
                let current = DeviceState::disconnected(&previous);
                self.devices.insert(device_id, current.clone());

                events.push(DeviceEvent::Disconnected { previous, current });
                changed = true;
            }
        }

        if changed {
            events.push(self.devices_changed_event());
        }

        events
    }

    pub fn mark_all_disconnected(&mut self) -> Vec<DeviceEvent> {
        let mut events = Vec::new();
        let connected_ids: Vec<String> = self
            .devices
            .iter()
            .filter(|(_, state)| state.connected)
            .map(|(device_id, _)| device_id.clone())
            .collect();

        for device_id in connected_ids {
            if let Some(previous) = self.devices.get(&device_id).cloned() {
                let current = DeviceState::disconnected(&previous);
                self.devices.insert(device_id, current.clone());
                events.push(DeviceEvent::Disconnected { previous, current });
            }
        }

        if !events.is_empty() {
            events.push(self.devices_changed_event());
        }

        events
    }

    pub fn devices(&self) -> Vec<DeviceState> {
        let mut devices: Vec<DeviceState> = self.devices.values().cloned().collect();
        devices.sort_by(compare_devices);
        devices
    }

    pub fn connected_devices(&self) -> Vec<DeviceState> {
        self.devices()
            .into_iter()
            .filter(|device| device.connected)
            .collect()
    }

    pub fn primary_device(&self) -> Option<DeviceState> {
        let connected = self.connected_devices();

        connected
            .iter()
            .find(|device| device.battery.is_some())
            .cloned()
            .or_else(|| connected.first().cloned())
    }

    pub fn primary_device_for_product_id(&self, product_id: u16) -> Option<DeviceState> {
        let connected = self.connected_devices();

        connected
            .iter()
            .find(|device| device.product_id == product_id && device.battery.is_some())
            .cloned()
            .or_else(|| {
                connected
                    .iter()
                    .find(|device| device.product_id == product_id)
                    .cloned()
            })
    }

    pub fn clear(&mut self) {
        self.devices.clear();
    }

    fn devices_changed_event(&self) -> DeviceEvent {
        DeviceEvent::DevicesChanged {
            devices: self.devices(),
            primary_device: self.primary_device(),
        }
    }
}

fn compare_devices(left: &DeviceState, right: &DeviceState) -> std::cmp::Ordering {
    right
        .connected
        .cmp(&left.connected)
        .then_with(|| {
            left.device_type
                .priority()
                .cmp(&right.device_type.priority())
        })
        .then_with(|| {
            left.name
                .as_deref()
                .unwrap_or(&left.id)
                .cmp(right.name.as_deref().unwrap_or(&right.id))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::DeviceType;

    fn reading(
        product_id: u16,
        name: &str,
        device_type: DeviceType,
        serial: &str,
        battery: Option<f32>,
        charging: Option<bool>,
    ) -> DeviceReading {
        DeviceReading {
            vendor_id: 0x1532,
            product_id,
            product_name: Some(name.to_string()),
            device_type,
            battery,
            charging,
            serial_number: Some(serial.to_string()),
        }
    }

    #[test]
    fn tracks_multiple_devices_and_selects_a_readable_primary_device() {
        let mut manager = DeviceManager::new();

        manager.update_from_readings(vec![
            reading(
                0x00AB,
                "Razer Basilisk V3 Pro Wireless",
                DeviceType::Mouse,
                "MOUSE-1",
                None,
                None,
            ),
            reading(
                0x0555,
                "Razer Blackshark V2 Pro",
                DeviceType::Headset,
                "HEADSET-1",
                Some(55.0),
                None,
            ),
        ]);

        assert_eq!(manager.connected_devices().len(), 2);
        assert_eq!(manager.primary_device().unwrap().battery, Some(55.0));
    }

    #[test]
    fn selects_a_requested_product_before_the_automatic_primary_device() {
        let mut manager = DeviceManager::new();

        manager.update_from_readings(vec![
            reading(0x00AB, "Mouse", DeviceType::Mouse, "M", Some(80.0), None),
            reading(
                0x0555,
                "Headset",
                DeviceType::Headset,
                "H",
                Some(60.0),
                None,
            ),
        ]);

        assert_eq!(manager.primary_device().unwrap().product_id, 0x00AB);
        assert_eq!(
            manager
                .primary_device_for_product_id(0x0555)
                .unwrap()
                .product_id,
            0x0555
        );
        assert!(manager.primary_device_for_product_id(0x9999).is_none());
    }

    #[test]
    fn emits_state_transition_events_in_compatibility_order() {
        let mut manager = DeviceManager::new();

        let first = manager.update_from_readings(vec![reading(
            0x00AB,
            "Razer Basilisk V3 Pro Wireless",
            DeviceType::Mouse,
            "MOUSE-1",
            Some(80.0),
            None,
        )]);
        assert!(matches!(first[0], DeviceEvent::Connected { .. }));
        assert!(matches!(first[1], DeviceEvent::DevicesChanged { .. }));

        let changed = manager.update_from_readings(vec![reading(
            0x00AB,
            "Razer Basilisk V3 Pro Wireless",
            DeviceType::Mouse,
            "MOUSE-1",
            Some(70.0),
            Some(true),
        )]);
        assert!(matches!(changed[0], DeviceEvent::BatteryChanged { .. }));
        assert!(matches!(changed[1], DeviceEvent::ChargingChanged { .. }));
        assert!(matches!(changed[2], DeviceEvent::DevicesChanged { .. }));

        let disconnected = manager.update_from_readings(Vec::new());
        assert!(matches!(disconnected[0], DeviceEvent::Disconnected { .. }));
        assert!(matches!(
            disconnected[1],
            DeviceEvent::DevicesChanged { .. }
        ));
    }

    #[test]
    fn stable_order_prefers_mouse_then_headset_then_dock_then_dongle() {
        let mut manager = DeviceManager::new();

        manager.update_from_readings(vec![
            reading(
                0x0088,
                "Dock receiver",
                DeviceType::Dongle,
                "D",
                Some(90.0),
                None,
            ),
            reading(
                0x0555,
                "Headset",
                DeviceType::Headset,
                "H",
                Some(80.0),
                None,
            ),
            reading(0x00A4, "Dock", DeviceType::Dock, "K", Some(100.0), None),
            reading(0x00AB, "Mouse", DeviceType::Mouse, "M", Some(70.0), None),
        ]);

        let types: Vec<_> = manager
            .devices()
            .into_iter()
            .map(|device| device.device_type)
            .collect();

        assert_eq!(
            types,
            vec![
                DeviceType::Mouse,
                DeviceType::Headset,
                DeviceType::Dock,
                DeviceType::Dongle,
            ],
        );
    }
}
