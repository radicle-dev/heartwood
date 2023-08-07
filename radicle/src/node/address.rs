mod store;
mod types;

pub use store::*;
pub use types::*;

use cyphernet::addr::HostName;
use std::net;

use crate::node::Address;

/// Address type.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressType {
    Ipv4 = 1,
    Ipv6 = 2,
    Dns = 3,
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
            HostName::Dns(_) => AddressType::Dns,
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
            3 => Ok(AddressType::Dns),
            4 => Ok(AddressType::Onion),
            _ => Err(other),
        }
    }
}
/// Check whether an IP address is globally routable.
pub fn is_routable(addr: &net::IpAddr) -> bool {
    match addr {
        net::IpAddr::V4(addr) => ipv4_is_routable(addr),
        net::IpAddr::V6(addr) => ipv6_is_routable(addr),
    }
}

/// Check whether an IP address is locally routable.
pub fn is_local(addr: &net::IpAddr) -> bool {
    match addr {
        net::IpAddr::V4(addr) => {
            addr.is_private() || addr.is_loopback() || addr.is_link_local() || addr.is_unspecified()
        }
        net::IpAddr::V6(_) => false,
    }
}

/// Check whether an IPv4 address is globally routable.
///
/// This code is adapted from the Rust standard library's `net::Ipv4Addr::is_global`. It can be
/// replaced once that function is stabilized.
fn ipv4_is_routable(addr: &net::Ipv4Addr) -> bool {
    // Check if this address is 192.0.0.9 or 192.0.0.10. These addresses are the only two
    // globally routable addresses in the 192.0.0.0/24 range.
    if u32::from(*addr) == 0xc0000009 || u32::from(*addr) == 0xc000000a {
        return true;
    }
    !addr.is_private()
        && !addr.is_loopback()
        && !addr.is_link_local()
        && !addr.is_broadcast()
        && !addr.is_documentation()
        // Make sure the address is not in 0.0.0.0/8.
        && addr.octets()[0] != 0
}

/// Check whether an IPv6 address is globally routable.
///
/// For now, this always returns `true`, as IPv6 addresses
/// are not fully supported.
fn ipv6_is_routable(_addr: &net::Ipv6Addr) -> bool {
    true
}
