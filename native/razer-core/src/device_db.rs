use std::collections::HashSet;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::OnceLock;

use serde::Deserialize;

use crate::state::DeviceType;

const PRODUCT_DATABASE_JSON: &str =
    include_str!("../../../src/devices/razer-products.json");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductDefinition {
    pub product_id: u16,
    pub name: String,
    pub device_type: DeviceType,
    pub transaction_id: u8,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceDatabaseError {
    InvalidJson(String),
    InvalidHex {
        field: &'static str,
        value: String,
    },
    DuplicateProductId(u16),
    InvalidDeviceType(String),
    TransactionIdOutOfRange(u16),
}

impl Display for DeviceDatabaseError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidJson(message) => write!(formatter, "invalid device database JSON: {message}"),
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
                write!(formatter, "transaction ID is out of byte range: 0x{transaction_id:04X}")
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
    let mut products = Vec::with_capacity(raw.len());

    for definition in raw {
        let product_id = parse_hex_u16(&definition.product_id, "productId")?;
        let transaction_id = parse_hex_u16(&definition.transaction_id, "transactionId")?;
        let device_type = DeviceType::parse(&definition.device_type);

        if device_type == DeviceType::Unknown && definition.device_type != "unknown" {
            return Err(DeviceDatabaseError::InvalidDeviceType(definition.device_type));
        }

        if transaction_id > u8::MAX as u16 {
            return Err(DeviceDatabaseError::TransactionIdOutOfRange(transaction_id));
        }

        if !seen_product_ids.insert(product_id) {
            return Err(DeviceDatabaseError::DuplicateProductId(product_id));
        }

        products.push(ProductDefinition {
            product_id,
            name: definition.name,
            device_type,
            transaction_id: transaction_id as u8,
        });
    }

    Ok(products)
}

pub fn product_database() -> &'static [ProductDefinition] {
    static DATABASE: OnceLock<Vec<ProductDefinition>> = OnceLock::new();

    DATABASE
        .get_or_init(|| {
            load_product_database().expect(
                "embedded Razer product database must be valid at build/runtime",
            )
        })
        .as_slice()
}

pub fn get_product(product_id: u16) -> Option<&'static ProductDefinition> {
    product_database()
        .iter()
        .find(|product| product.product_id == product_id)
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
    }

    #[test]
    fn global_lookup_uses_the_same_embedded_database() {
        assert_eq!(
            get_product(0x0555).unwrap().device_type,
            DeviceType::Headset,
        );
        assert!(get_product(0xFFFF).is_none());
    }
}
