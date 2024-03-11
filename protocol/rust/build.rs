fn main() -> std::io::Result<()> {
    let protos: Vec<_> = ["response_collector_service", "discovery_packet"]
        .into_iter()
        .map(|name| format!("../grpc/tw/seamlik/tansa/v1/{}.proto", name))
        .collect();
    tonic_build::configure().compile(&protos, &["../grpc"])
}
