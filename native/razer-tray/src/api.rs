use std::error::Error;
use std::sync::{Arc, RwLock};
use std::thread;

use razer_core::frontend::FrontendSnapshot;
use razer_core::integration::IntegrationSnapshot;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

pub const API_ADDRESS: &str = "127.0.0.1:27212";
pub const STATUS_PATH: &str = "/v1/status";
const SEELEN_ORIGIN: &str = "http://tauri.localhost";

pub type SharedIntegrationState = Arc<RwLock<String>>;

pub fn initial_state() -> SharedIntegrationState {
    let snapshot = FrontendSnapshot {
        devices: Vec::new(),
        primary_device: None,
        transport_name: "placeholder".to_string(),
        transport_ready: false,
        transport_healthy: true,
        consecutive_transport_failures: 0,
        transport_error_kind: None,
    };

    Arc::new(RwLock::new(
        IntegrationSnapshot::from_frontend(&snapshot)
            .to_json()
            .expect("empty integration snapshot must serialize"),
    ))
}

pub fn update_state(state: &SharedIntegrationState, snapshot: &FrontendSnapshot) {
    let json = match IntegrationSnapshot::from_frontend(snapshot).to_json() {
        Ok(json) => json,
        Err(error) => {
            eprintln!("failed to serialize integration snapshot: {error}");
            return;
        }
    };

    match state.write() {
        Ok(mut current) => *current = json,
        Err(error) => eprintln!("integration snapshot lock poisoned: {error}"),
    }
}

pub fn start(
    state: SharedIntegrationState,
) -> Result<thread::JoinHandle<()>, Box<dyn Error + Send + Sync>> {
    let server = Server::http(API_ADDRESS)?;

    Ok(thread::spawn(move || {
        for request in server.incoming_requests() {
            respond(request, &state);
        }
    }))
}

fn respond(request: Request, state: &SharedIntegrationState) {
    if request.method() != &Method::Get {
        send_response(
            request,
            StatusCode(405),
            "Method Not Allowed",
            "text/plain; charset=UTF-8",
        );
        return;
    }

    if request.url() != STATUS_PATH {
        send_response(
            request,
            StatusCode(404),
            "Not Found",
            "text/plain; charset=UTF-8",
        );
        return;
    }

    let body = match state.read() {
        Ok(current) => current.clone(),
        Err(error) => {
            eprintln!("integration snapshot read lock poisoned: {error}");
            send_response(
                request,
                StatusCode(503),
                r#"{"error":"integration state unavailable"}"#,
                "application/json; charset=UTF-8",
            );
            return;
        }
    };

    send_response(
        request,
        StatusCode(200),
        body,
        "application/json; charset=UTF-8",
    );
}

fn send_response(
    request: Request,
    status: StatusCode,
    body: impl Into<String>,
    content_type: &'static str,
) {
    let response = Response::from_string(body)
        .with_status_code(status)
        .with_header(header("Content-Type", content_type))
        .with_header(header("Cache-Control", "no-store"))
        .with_header(header("Access-Control-Allow-Origin", SEELEN_ORIGIN));

    if let Err(error) = request.respond(response) {
        eprintln!("failed to send integration API response: {error}");
    }
}

fn header(name: &'static str, value: &'static str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes())
        .expect("static HTTP header must contain valid ASCII")
}

#[cfg(test)]
mod tests {
    use super::*;
    use razer_core::frontend::FrontendDeviceState;
    use razer_core::state::DeviceType;

    #[test]
    fn initial_api_state_is_valid_versioned_json() {
        let state = initial_state();
        let json = state.read().unwrap();

        assert!(json.contains(r#""schemaVersion":1"#));
        assert!(json.contains(r#""transportName":"placeholder""#));
        assert!(json.contains(r#""transportReady":false"#));
        assert!(json.contains(r#""devices":[]"#));
    }

    #[test]
    fn update_state_publishes_latest_frontend_snapshot_without_private_ids() {
        let state = initial_state();
        let snapshot = FrontendSnapshot {
            devices: vec![FrontendDeviceState {
                vendor_id: 0x1532,
                product_id: 0x00AB,
                name: Some("Razer Basilisk V3 Pro Wireless".to_string()),
                device_type: DeviceType::Mouse,
                battery: Some(72.5),
                charging: None,
                connected: true,
            }],
            primary_device: None,
            transport_name: "scripted".to_string(),
            transport_ready: true,
            transport_healthy: true,
            consecutive_transport_failures: 0,
            transport_error_kind: None,
        };

        update_state(&state, &snapshot);

        let json = state.read().unwrap();
        assert!(json.contains(r#""battery":72.5"#));
        assert!(!json.contains("serial"));
        assert!(!json.contains("usb:1532"));
    }
}
