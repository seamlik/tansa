fn main() -> std::io::Result<()> {
    tonic_build::configure().compile(&["../grpc/tw/seamlik/tansa/v1/*.proto"], &["../grpc"])
}
