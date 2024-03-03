use crate::SocketAddress;
use std::net::SocketAddrV6;
use std::num::TryFromIntError;
use thiserror::Error;

impl TryFrom<SocketAddress> for SocketAddrV6 {
    type Error = SocketAddressConvertError;

    fn try_from(value: SocketAddress) -> Result<Self, Self::Error> {
        let ipv6_array: [u8; 16] = value
            .ipv6
            .try_into()
            .map_err(|_| SocketAddressConvertError::Ipv6BytesLengthMismatch)?;
        Ok(Self::new(ipv6_array.into(), value.port.try_into()?, 0, 0))
    }
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum SocketAddressConvertError {
    #[error("IPv6 bytes length mismatch")]
    Ipv6BytesLengthMismatch,

    #[error("Port out of range")]
    PortOutOfRange(#[from] TryFromIntError),
}

#[cfg(test)]
mod test {
    use std::net::Ipv6Addr;

    use super::*;

    #[test]
    fn convert_success() {
        let input_addr = SocketAddress {
            ipv6: (0..16).map(|_| 0).collect(),
            port: 12345,
        };
        let expected_addr = SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 12345, 0, 0);

        // When
        let result: Result<SocketAddrV6, _> = input_addr.try_into();

        // Then
        assert_eq!(expected_addr, result.unwrap());
    }

    #[test]
    fn ipv6_bytes_length_mismatch() {
        let addr = SocketAddress {
            ipv6: vec![0, 0],
            port: 0,
        };

        // When
        let result: Result<SocketAddrV6, _> = addr.try_into();

        // Then
        assert_eq!(
            SocketAddressConvertError::Ipv6BytesLengthMismatch,
            result.unwrap_err()
        );
    }

    #[test]
    fn port_out_of_range() {
        let addr = SocketAddress {
            ipv6: (0..16).collect(),
            port: 100000,
        };

        // When
        let result: Result<SocketAddrV6, _> = addr.try_into();

        // Then
        if let SocketAddressConvertError::PortOutOfRange(_) = result.unwrap_err() {
        } else {
            panic!("Expecting `PortOutOfRange`")
        }
    }
}
