use crate::multicast::MulticastReceiver;
use prost::Message;
use std::net::IpAddr;
use std::net::Ipv6Addr;
use std::net::SocketAddrV6;
use tansa_protocol::response_collector_service_client::ResponseCollectorServiceClient;
use tansa_protocol::Request;
use tansa_protocol::Response;
use tonic::IntoRequest;
use uuid::Uuid;

const MULTICAST_RECEIVER_BUFFER_SIZE: usize = 64;

pub async fn serve(
    multicast_address: SocketAddrV6,
    multicast_network_interface_indexes: impl IntoIterator<Item = u32>,
    service_port: u16,
) -> std::io::Result<()> {
    let multicast_receiver =
        MulticastReceiver::new(MULTICAST_RECEIVER_BUFFER_SIZE, multicast_address.port())?;
    multicast_network_interface_indexes
        .into_iter()
        .try_for_each(|i| multicast_receiver.join_multicast(multicast_address.ip(), i))?;
    loop {
        let (packet, remote_address) = multicast_receiver.receive().await?;
        let remote_ip = unwrap_ipv6(remote_address.ip());

        // TODO: Parallelize
        if let Err(e) = handle_packet(packet, remote_ip, service_port).await {
            log::error!("Failed to handle a packet from {}: {}", remote_address, e);
        }
    }
}

async fn handle_packet(
    packet: Vec<u8>,
    remote_ip: Ipv6Addr,
    local_service_port: u16,
) -> anyhow::Result<()> {
    let request = Request::decode(packet.as_slice())?;
    let response_collector_address =
        format!("http://{}:{}", remote_ip, request.response_collector_port);
    log::info!(
        "Connecting to response collector at {}",
        &response_collector_address
    );
    let mut response_collector_client =
        ResponseCollectorServiceClient::connect(response_collector_address.clone()).await?;
    let response = Response {
        request_id: request.request_id,
        response_id: Uuid::new_v4().into_bytes().into(),
        port: local_service_port.into(),
    };
    response_collector_client
        .submit_response(response.into_request())
        .await?;
    Ok(())
}

fn unwrap_ipv6(ip: IpAddr) -> Ipv6Addr {
    if let IpAddr::V6(ipv6) = ip {
        ipv6
    } else {
        panic!("Must be IPv6")
    }
}
