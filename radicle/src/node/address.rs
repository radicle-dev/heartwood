mod store;
mod types;

pub use store::*;
pub use types::*;

use std::net;

use cyphernet::addr::HostName;

use crate::node::Address;

/// Address type.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressType {
    Ipv4 = 1,
    Ipv6 = 2,
    Hostname = 3,
    Onion = 4,
}

impl From<AddressType> for u8 {
    fn from(other: AddressType) -> Self {
        other as u8
    }
}

impl From<&Address> for AddressType {
    fn from(a: &Address) -> Self {
        match a.host {
            HostName::Ip(net::IpAddr::V4(_)) => AddressType::Ipv4,
            HostName::Ip(net::IpAddr::V6(_)) => AddressType::Ipv6,
            HostName::Dns(_) => AddressType::Hostname,
            HostName::Tor(_) => AddressType::Onion,
            _ => todo!(), // FIXME(cloudhead): Maxim will remove `non-exhaustive`
        }
    }
}

impl TryFrom<u8> for AddressType {
    type Error = u8;

    fn try_from(other: u8) -> Result<Self, Self::Error> {
        match other {
            1 => Ok(AddressType::Ipv4),
            2 => Ok(AddressType::Ipv6),
            3 => Ok(AddressType::Hostname),
            4 => Ok(AddressType::Onion),
            _ => Err(other),
        }
    }
}
