use crate::multicast::MulticastReceiver;
use prost::Message;
use std::net::IpAddr;
use std::net::Ipv6Addr;
use std::net::SocketAddrV6;
use tansa_protocol::response_collector_service_client::ResponseCollectorServiceClient;
use tansa_protocol::Request;
use tansa_protocol::Response;
use tansa_protocol::SocketAddress;
use tokio::net::UdpSocket;
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
    let local_service_ip = find_local_ip_for_remote_ip(remote_ip).await?;
    log::info!(
        "Connecting to response collector at {}",
        &response_collector_address
    );
    let mut response_collector_client =
        ResponseCollectorServiceClient::connect(response_collector_address.clone()).await?;
    let response = Response {
        request_id: request.request_id,
        response_id: Uuid::new_v4().into_bytes().into(),
        service_address: Some(SocketAddress {
            ipv6: local_service_ip.octets().into(),
            port: local_service_port.into(),
        }),
    };
    response_collector_client
        .submit_response(response.into_request())
        .await?;
    Ok(())
}

async fn find_local_ip_for_remote_ip(remote_ip: Ipv6Addr) -> std::io::Result<Ipv6Addr> {
    let socket = UdpSocket::bind("[::]:0").await?;
    socket
        .connect(SocketAddrV6::new(remote_ip, 1, 0, 0))
        .await?;
    Ok(unwrap_ipv6(socket.local_addr()?.ip()))
}

fn unwrap_ipv6(ip: IpAddr) -> Ipv6Addr {
    if let IpAddr::V6(ipv6) = ip {
        ipv6
    } else {
        panic!("Must be IPv6")
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn find_local_ip() -> anyhow::Result<()> {
        let localhost = "::1".parse()?;

        // When
        let local_ip = find_local_ip_for_remote_ip(localhost).await?;

        // Then
        assert_ne!(local_ip, Ipv6Addr::UNSPECIFIED);

        Ok(())
    }
}
