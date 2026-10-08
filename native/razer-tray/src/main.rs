#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::error::Error;
use std::thread;
use std::time::Duration;

use razer_core::frontend::FrontendSnapshot;
use razer_core::notifications::evaluate_events;
use razer_core::service::CoreService;
use razer_core::settings::{AppSettings, SettingsStore};
use razer_core::state::DeviceReading;
use razer_core::transport::{BatteryTransport, TransportError};
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::WindowId;

#[derive(Debug)]
enum UserEvent {
    Menu(MenuEvent),
    Tick,
}

struct PlaceholderTransport;

impl BatteryTransport for PlaceholderTransport {
    fn name(&self) -> &'static str {
        "placeholder"
    }

    fn read_devices(&mut self) -> Result<Vec<DeviceReading>, TransportError> {
        Ok(Vec::new())
    }
}

struct TrayApplication {
    core: CoreService<PlaceholderTransport>,
    settings: AppSettings,
    tray: Option<TrayIcon>,
    status_item: Option<MenuItem>,
    refresh_item: Option<MenuItem>,
    quit_item: Option<MenuItem>,
}

impl TrayApplication {
    fn new(settings: AppSettings) -> Self {
        Self {
            core: CoreService::new(PlaceholderTransport),
            settings,
            tray: None,
            status_item: None,
            refresh_item: None,
            quit_item: None,
        }
    }

    fn create_tray(&mut self) -> Result<(), Box<dyn Error>> {
        let menu = Menu::new();
        let status_item = MenuItem::new("Starting native core…", false, None);
        let refresh_item = MenuItem::with_id("refresh", "Refresh", true, None);
        let quit_item = MenuItem::with_id("quit", "Quit", true, None);

        menu.append(&status_item)?;
        menu.append(&refresh_item)?;
        menu.append(&quit_item)?;

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_icon(build_battery_icon(None)?)
            .with_tooltip("RazerBatteryTaskbar Native")
            .build()?;

        self.tray = Some(tray);
        self.status_item = Some(status_item);
        self.refresh_item = Some(refresh_item);
        self.quit_item = Some(quit_item);

        Ok(())
    }

    fn refresh(&mut self) {
        let result = self.core.refresh();
        let snapshot = self.core.frontend_snapshot();

        for notification in evaluate_events(&result.events, &self.settings.notifications) {
            eprintln!(
                "notification pending: {} — {}",
                notification.title, notification.body
            );
        }

        self.render(&snapshot);
    }

    fn render(&self, snapshot: &FrontendSnapshot) {
        let status = format_status(snapshot);

        if let Some(item) = &self.status_item {
            item.set_text(&status);
        }

        if let Some(tray) = &self.tray {
            let _ = tray.set_tooltip(Some(&status));
            let _ = tray.set_icon(Some(
                build_battery_icon(
                    snapshot
                        .primary_device
                        .as_ref()
                        .and_then(|device| device.battery),
                )
                .expect("generated tray icon must be valid"),
            ));
        }
    }

    fn handle_menu(&mut self, event_loop: &ActiveEventLoop, event: MenuEvent) {
        if self
            .refresh_item
            .as_ref()
            .is_some_and(|item| event.id == *item.id())
        {
            self.refresh();
            return;
        }

        if self
            .quit_item
            .as_ref()
            .is_some_and(|item| event.id == *item.id())
        {
            event_loop.exit();
        }
    }
}

impl ApplicationHandler<UserEvent> for TrayApplication {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        if self.tray.is_none() {
            self.create_tray()
                .expect("native Windows tray should initialize");
            self.refresh();
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Menu(event) => self.handle_menu(event_loop, event),
            UserEvent::Tick => self.refresh(),
        }
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {
    }
}

fn load_settings() -> AppSettings {
    match SettingsStore::for_current_user().and_then(|store| store.load()) {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!("failed to load settings; using defaults: {error}");
            AppSettings::default()
        }
    }
}

fn format_status(snapshot: &FrontendSnapshot) -> String {
    match snapshot.primary_device.as_ref() {
        Some(device) if device.connected => match device.battery {
            Some(battery) => format!(
                "{} — {battery:.1}%",
                device.name.as_deref().unwrap_or("Razer device")
            ),
            None => format!(
                "{} — Battery unavailable",
                device.name.as_deref().unwrap_or("Razer device")
            ),
        },
        _ => "Native shell ready — HID transport pending".to_string(),
    }
}

fn build_battery_icon(battery: Option<f32>) -> Result<Icon, tray_icon::BadIcon> {
    const WIDTH: u32 = 16;
    const HEIGHT: u32 = 16;

    let mut rgba = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
    let level = battery.unwrap_or(0.0).clamp(0.0, 100.0);
    let fill_width = ((level / 100.0) * 9.0).round() as u32;

    let mut set_pixel = |x: u32, y: u32, value: u8| {
        let index = ((y * WIDTH + x) * 4) as usize;
        rgba[index] = value;
        rgba[index + 1] = value;
        rgba[index + 2] = value;
        rgba[index + 3] = 255;
    };

    for x in 2..13 {
        set_pixel(x, 4, 220);
        set_pixel(x, 11, 220);
    }

    for y in 4..=11 {
        set_pixel(2, y, 220);
        set_pixel(12, y, 220);
    }

    for y in 6..=9 {
        set_pixel(13, y, 220);
    }

    for x in 3..(3 + fill_width) {
        for y in 5..11 {
            set_pixel(x, y, 255);
        }
    }

    Icon::from_rgba(rgba, WIDTH, HEIGHT)
}

fn main() -> Result<(), Box<dyn Error>> {
    let settings = load_settings();
    let poll_interval = Duration::from_secs(settings.poll_interval_seconds);

    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;

    let menu_proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_proxy.send_event(UserEvent::Menu(event));
    }));

    let tick_proxy = event_loop.create_proxy();
    thread::spawn(move || {
        loop {
            thread::sleep(poll_interval);

            if tick_proxy.send_event(UserEvent::Tick).is_err() {
                break;
            }
        }
    });

    let mut application = TrayApplication::new(settings);
    event_loop.run_app(&mut application)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_shell_status_is_explicit_about_pending_hid_transport() {
        let snapshot = FrontendSnapshot {
            devices: Vec::new(),
            primary_device: None,
            transport_healthy: true,
            consecutive_transport_failures: 0,
        };

        assert_eq!(
            format_status(&snapshot),
            "Native shell ready — HID transport pending"
        );
    }

    #[test]
    fn generated_icons_are_valid_at_extreme_battery_levels() {
        assert!(build_battery_icon(Some(0.0)).is_ok());
        assert!(build_battery_icon(Some(100.0)).is_ok());
        assert!(build_battery_icon(None).is_ok());
    }
}
