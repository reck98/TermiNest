//! Virtual Socket (Vsock / AF_HYPERV / AF_VSOCK) transport models and address contracts.

/// Known CID addresses for standard virtio-vsock.
pub mod cid {
    /// Hypervisor host CID.
    pub const HOST: u32 = 2;
    /// Standard guest CID default in TermiNest configs.
    pub const DEFAULT_GUEST: u32 = 3;
}

/// Standard TermiNest Vsock port assignments.
pub mod ports {
    /// Primary TermiNest multiplexed wire protocol port.
    pub const WIRE_PROTOCOL: u32 = 10001;
    /// Auxiliary serial or debug port.
    pub const DEBUG_PORT: u32 = 10002;
}

/// Represents an abstract Vsock endpoint address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VsockAddress {
    /// Context Identifier (CID).
    pub cid: u32,
    /// Logical port number.
    pub port: u32,
}

impl VsockAddress {
    pub const fn new(cid: u32, port: u32) -> Self {
        Self { cid, port }
    }

    pub const fn host(port: u32) -> Self {
        Self {
            cid: cid::HOST,
            port,
        }
    }

    pub const fn default_guest() -> Self {
        Self {
            cid: cid::DEFAULT_GUEST,
            port: ports::WIRE_PROTOCOL,
        }
    }
}

impl std::fmt::Display for VsockAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "vsock://{}:{}", self.cid, self.port)
    }
}

/// Windows Hyper-V Socket (AF_HYPERV) GUID definitions.
pub mod hyperv_guid {
    /// Well-known template GUID for mapping a 32-bit port number to an AF_HYPERV ServiceId:
    /// `{PORT-facb-11e6-bd58-64006a561569}`.
    pub const PORT_GUID_TEMPLATE_SUFFIX: &str = "-facb-11e6-bd58-64006a561569";

    /// Parent partition (Host OS) GUID.
    pub const HV_GUID_PARENT: &str = "a42e7cda-d03f-480c-9cc2-a4de20abb878";

    /// Generates the AF_HYPERV ServiceId GUID string for a given 32-bit port.
    pub fn port_to_service_guid(port: u32) -> String {
        format!("{:08x}{}", port, PORT_GUID_TEMPLATE_SUFFIX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vsock_address_display() {
        let addr = VsockAddress::new(3, 10001);
        assert_eq!(addr.to_string(), "vsock://3:10001");
    }

    #[test]
    fn test_hyperv_port_guid_generation() {
        let port = 10001; // 0x2711
        let guid = hyperv_guid::port_to_service_guid(port);
        assert_eq!(guid, "00002711-facb-11e6-bd58-64006a561569");
    }
}
