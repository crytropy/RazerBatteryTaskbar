use std::collections::HashSet;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::OnceLock;

use serde::Deserialize;

use crate::state::DeviceType;

const PRODUCT_DATABASE_JSON: &str = include_str!("../../../src/devices/razer-products.json");

/// How a product ID reaches the host. A wireless mouse exposes two product IDs
/// for the same physical device: the receiver (or the mouse over the receiver)
/// and the mouse itself when it is on its USB cable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinkKind {
    Wired,
    Wireless,
}

impl LinkKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "wired" => Some(Self::Wired),
            "wireless" => Some(Self::Wireless),
            _ => None,
        }
    }
}

/// Optional `pairing` entry of the shared database: product IDs that name the
/// same `model` are the wired / wireless twins of one logical device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelPairing {
    pub model: String,
    pub link: LinkKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductDefinition {
    pub product_id: u16,
    pub name: String,
    pub device_type: DeviceType,
    pub transaction_id: u8,
    pub pairing: Option<ModelPairing>,
}

/// One physical device as the core tracks it. Paired wired / wireless product
/// IDs resolve to the same logical device; its identity is the wireless
/// (receiver) product ID and its name is the base model name, so plugging the
/// cable in never looks like a different device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalDevice {
    /// Canonical product ID used for the device id and the reading.
    pub product_id: u16,
    /// User-visible name without a "Wired" / "Wireless" suffix when paired.
    pub name: String,
    pub wired: Option<u16>,
    pub wireless: Option<u16>,
}

impl LogicalDevice {
    /// Every product ID this device can enumerate as, most preferred first.
    /// The wired twin wins: while the mouse is on its cable the receiver is
    /// usually still enumerated but answers `deviceNotResponding`.
    pub fn product_ids(&self) -> Vec<u16> {
        let ids: Vec<u16> = self.wired.into_iter().chain(self.wireless).collect();

        if ids.is_empty() {
            vec![self.product_id]
        } else {
            ids
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawModelPairing {
    model: String,
    link: String,
}

#[derive(Debug, Deserialize)]
struct RawProductDefinition {
    #[serde(rename = "productId")]
    product_id: String,
    name: String,
    #[serde(rename = "type")]
    device_type: String,
    #[serde(rename = "transactionId")]
    transaction_id: String,
    #[serde(default)]
    pairing: Option<RawModelPairing>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceDatabaseError {
    InvalidJson(String),
    InvalidHex { field: &'static str, value: String },
    DuplicateProductId(u16),
    InvalidDeviceType(String),
    TransactionIdOutOfRange(u16),
    InvalidLink(String),
    DuplicateLink { model: String, link: LinkKind },
}

impl Display for DeviceDatabaseError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidJson(message) => {
                write!(formatter, "invalid device database JSON: {message}")
            }
            Self::InvalidHex { field, value } => {
                write!(formatter, "{field} must be a hexadecimal string: {value}")
            }
            Self::DuplicateProductId(product_id) => {
                write!(formatter, "duplicate Razer product ID: 0x{product_id:04X}")
            }
            Self::InvalidDeviceType(device_type) => {
                write!(formatter, "unsupported Razer device type: {device_type}")
            }
            Self::TransactionIdOutOfRange(transaction_id) => {
                write!(
                    formatter,
                    "transaction ID is out of byte range: 0x{transaction_id:04X}"
                )
            }
            Self::InvalidLink(link) => {
                write!(formatter, "pairing link must be wired or wireless: {link}")
            }
            Self::DuplicateLink { model, link } => {
                write!(
                    formatter,
                    "model {model} lists more than one {link:?} product ID"
                )
            }
        }
    }
}

impl Error for DeviceDatabaseError {}

fn parse_hex_u16(value: &str, field: &'static str) -> Result<u16, DeviceDatabaseError> {
    let digits = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .ok_or_else(|| DeviceDatabaseError::InvalidHex {
            field,
            value: value.to_string(),
        })?;

    u16::from_str_radix(digits, 16).map_err(|_| DeviceDatabaseError::InvalidHex {
        field,
        value: value.to_string(),
    })
}

pub fn load_product_database() -> Result<Vec<ProductDefinition>, DeviceDatabaseError> {
    let raw: Vec<RawProductDefinition> = serde_json::from_str(PRODUCT_DATABASE_JSON)
        .map_err(|error| DeviceDatabaseError::InvalidJson(error.to_string()))?;

    let mut seen_product_ids = HashSet::new();
    let mut seen_links = HashSet::new();
    let mut products = Vec::with_capacity(raw.len());

    for definition in raw {
        let product_id = parse_hex_u16(&definition.product_id, "productId")?;
        let transaction_id = parse_hex_u16(&definition.transaction_id, "transactionId")?;
        let device_type = DeviceType::parse(&definition.device_type);

        if device_type == DeviceType::Unknown && definition.device_type != "unknown" {
            return Err(DeviceDatabaseError::InvalidDeviceType(
                definition.device_type,
            ));
        }

        if transaction_id > u8::MAX as u16 {
            return Err(DeviceDatabaseError::TransactionIdOutOfRange(transaction_id));
        }

        if !seen_product_ids.insert(product_id) {
            return Err(DeviceDatabaseError::DuplicateProductId(product_id));
        }

        let pairing = definition
            .pairing
            .map(|pairing| {
                let link = LinkKind::parse(&pairing.link)
                    .ok_or(DeviceDatabaseError::InvalidLink(pairing.link))?;

                if !seen_links.insert((pairing.model.clone(), link)) {
                    return Err(DeviceDatabaseError::DuplicateLink {
                        model: pairing.model,
                        link,
                    });
                }

                Ok(ModelPairing {
                    model: pairing.model,
                    link,
                })
            })
            .transpose()?;

        products.push(ProductDefinition {
            product_id,
            name: definition.name,
            device_type,
            transaction_id: transaction_id as u8,
            pairing,
        });
    }

    Ok(products)
}

pub fn product_database() -> &'static [ProductDefinition] {
    static DATABASE: OnceLock<Vec<ProductDefinition>> = OnceLock::new();

    DATABASE
        .get_or_init(|| {
            load_product_database()
                .expect("embedded Razer product database must be valid at build/runtime")
        })
        .as_slice()
}

pub fn get_product(product_id: u16) -> Option<&'static ProductDefinition> {
    product_database()
        .iter()
        .find(|product| product.product_id == product_id)
}

/// Resolve a product ID to the logical device it belongs to, following the
/// database's wired / wireless pairing. Unpaired products are their own
/// logical device.
pub fn logical_device(product_id: u16) -> Option<LogicalDevice> {
    let product = get_product(product_id)?;

    let Some(pairing) = &product.pairing else {
        return Some(LogicalDevice {
            product_id: product.product_id,
            name: product.name.clone(),
            wired: None,
            wireless: None,
        });
    };

    let mut wired = None;
    let mut wireless = None;

    for twin in product_database() {
        let Some(twin_pairing) = &twin.pairing else {
            continue;
        };

        if twin_pairing.model == pairing.model {
            match twin_pairing.link {
                LinkKind::Wired => wired = Some(twin.product_id),
                LinkKind::Wireless => wireless = Some(twin.product_id),
            }
        }
    }

    Some(LogicalDevice {
        product_id: wireless.or(wired).unwrap_or(product.product_id),
        name: pairing.model.clone(),
        wired,
        wireless,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_the_shared_product_database() {
        let products = load_product_database().unwrap();

        assert_eq!(products.len(), 24);

        let basilisk = products
            .iter()
            .find(|product| product.product_id == 0x00AB)
            .unwrap();

        assert_eq!(basilisk.name, "Razer Basilisk V3 Pro Wireless");
        assert_eq!(basilisk.device_type, DeviceType::Mouse);
        assert_eq!(basilisk.transaction_id, 0x1F);

        let basilisk_x = products
            .iter()
            .find(|product| product.product_id == 0x0083)
            .unwrap();

        assert_eq!(basilisk_x.name, "Razer Basilisk X HyperSpeed");
    }

    #[test]
    fn global_lookup_uses_the_same_embedded_database() {
        assert_eq!(
            get_product(0x0555).unwrap().device_type,
            DeviceType::Headset,
        );
        assert!(get_product(0xFFFF).is_none());
    }

    #[test]
    fn wired_and_wireless_twins_resolve_to_one_logical_device() {
        let wireless = logical_device(0x00B7).unwrap();
        let wired = logical_device(0x00B6).unwrap();

        assert_eq!(wired, wireless);
        assert_eq!(wireless.product_id, 0x00B7);
        assert_eq!(wireless.name, "Razer DeathAdder V3 Pro");
        assert_eq!(wireless.wired, Some(0x00B6));
        assert_eq!(wireless.wireless, Some(0x00B7));
        assert_eq!(wireless.product_ids(), vec![0x00B6, 0x00B7]);

        assert_eq!(
            get_product(0x00B6).unwrap().pairing,
            Some(ModelPairing {
                model: "Razer DeathAdder V3 Pro".to_string(),
                link: LinkKind::Wired,
            })
        );
    }

    #[test]
    fn unpaired_product_is_its_own_logical_device() {
        let basilisk_x = logical_device(0x0083).unwrap();

        assert_eq!(basilisk_x.product_id, 0x0083);
        assert_eq!(basilisk_x.name, "Razer Basilisk X HyperSpeed");
        assert_eq!(basilisk_x.wired, None);
        assert_eq!(basilisk_x.wireless, None);
        assert_eq!(basilisk_x.product_ids(), vec![0x0083]);
        assert!(logical_device(0xFFFF).is_none());
    }
}
