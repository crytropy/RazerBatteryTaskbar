use crate::state::DeviceState;

#[derive(Debug, Clone, PartialEq)]
pub enum DeviceEvent {
    Connected {
        previous: Option<DeviceState>,
        current: DeviceState,
    },
    Disconnected {
        previous: DeviceState,
        current: DeviceState,
    },
    BatteryChanged {
        previous: DeviceState,
        current: DeviceState,
    },
    ChargingChanged {
        previous: DeviceState,
        current: DeviceState,
    },
    DevicesChanged {
        devices: Vec<DeviceState>,
        primary_device: Option<DeviceState>,
    },
}
