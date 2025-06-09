pub mod rpc {
    pub mod v1 {
        include!("proto/rpc.v1.rs");

        pub const FILE_DESCRIPTOR_SET: &[u8] =
            include_bytes!("proto/messenger_service_descriptor.bin");
    }
}

#[cfg(feature = "util")]
pub mod util;
