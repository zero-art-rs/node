use std::io::Result;
fn main() -> Result<()> {
    // prost_build::compile_protos(&["src/proto/types.proto"], &["src/proto/"])?;

    prost_build::Config::new()
        .message_attribute(
            "zero_art_proto.Frame",
            "#[derive(utoipa::ToSchema)]"
        )
        .message_attribute(
            "zero_art_proto.FrameTBS",
            "#[derive(utoipa::ToSchema)]"
        )
        .message_attribute(
            "zero_art_proto.GroupOperation",
            "#[derive(utoipa::ToSchema)]"
        )
        .enum_attribute(
            "zero_art_proto.GroupOperation.operation",
            "#[derive(utoipa::ToSchema)]"
        )
        .compile_protos(&["src/proto/zero_art_proto.proto"], &["src/proto/"])?;

    Ok(())
}
