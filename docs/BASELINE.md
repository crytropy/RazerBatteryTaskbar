# Legacy baseline

This document records the behavior of the upstream Electron implementation before the modernization work begins.

## Existing behavior

- Runs as an Electron tray application.
- Polls battery state every 30 seconds.
- Queries one supported Razer USB device.
- Displays battery state using 0-100% tray icons in 10% increments.
- Displays the precise battery percentage in the tray tooltip.
- Provides a single tray command: Quit.

## Supported product IDs inherited from upstream

The current device table includes Mouse Dock Pro, Basilisk V3 Pro, Basilisk V3 X HyperSpeed, DeathAdder V2/V3 models, HyperPolling Wireless Dongle, Basilisk Ultimate, Naga V2 Pro, Viper V2 Pro, Viper Ultimate, BlackShark V2 Pro (2020/2023), and Cobra Pro variants.

## Known legacy issues addressed by Phase 1

- Battery values were converted to strings with `toFixed(1)` before state handling.
- A disconnected device could leave the previous battery icon/tooltip visible.
- The vendor check did not actually compare `device.vendorId` with Razer's vendor ID.
- The product transaction ID table was not explicitly used when building the battery request.
- USB interfaces/devices were not explicitly released/closed after each query.
- `setInterval` could allow overlapping polls if a battery query stalled.
- Device/protocol/tray responsibilities were all contained in `src/main.js`.

## Deliberately deferred

- Multi-device support.
- Hotplug event handling.
- Charging-state support.
- Native Rust core.
- Seelen UI integration.
