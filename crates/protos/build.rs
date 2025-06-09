use std::error::Error;

const ZK_MESSENGER_PROTO: &str = "../../proto";
pub const OUT_DIR: &str = "src/proto";

fn main() -> Result<(), Box<dyn Error>> {
    tonic_build::configure()
        .file_descriptor_set_path(format!("{OUT_DIR}/messenger_service_descriptor.bin"))
        .build_server(true)
        .build_client(false)
        .out_dir(OUT_DIR)
        .compile_protos(
            &[proto_path("v1", "types"), proto_path("v1", "service")],
            &[ZK_MESSENGER_PROTO],
        )?;

    Ok(())
}

fn proto_path(version: &str, proto_file: &str) -> String {
    format!("{}/rpc/{version}/{proto_file}.proto", ZK_MESSENGER_PROTO)
}
