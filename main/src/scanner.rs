use crate::multicast::MulticastSender;
use crate::multicast::TokioMulticastSender;
use crate::response_collector::GrpcResponseCollector;
use crate::response_collector::ResponseCollector;
use futures_util::stream::BoxStream;
use futures_util::Stream;
use futures_util::StreamExt;
use futures_util::TryFutureExt;
use prost::Message;
use std::net::SocketAddrV6;
use std::sync::Arc;
use tansa_protocol::Request;
use thiserror::Error;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Service {
    pub address: SocketAddrV6,
}

pub fn scan(discovery_port: u16) -> impl Stream<Item = Result<Service, ScanError>> {
    GrpcResponseCollector::new()
        .err_into()
        .map_ok(Box::new)
        .map_ok(move |c| scan_internal(discovery_port, c, TokioMulticastSender))
        .try_flatten_stream()
}

fn scan_internal(
    discovery_port: u16,
    response_collector: Box<dyn ResponseCollector>,
    multicast_sender: impl MulticastSender + Send + 'static,
) -> BoxStream<'static, Result<Service, ScanError>> {
    if discovery_port == 0 {
        return futures_util::stream::once(async { Err(ScanError::InvalidDiscoveryPort) }).boxed();
    }

    let request = Request {
        response_collector_port: response_collector.get_port().into(),
    };
    crate::stream::join(
        send_requests(multicast_sender, request, discovery_port),
        response_collector.collect(),
    )
    .boxed()
}

async fn send_requests(
    multicast_sender: impl MulticastSender,
    request: Request,
    discovery_port: u16,
) -> Result<(), ScanError> {
    let multicast_address = SocketAddrV6::new(crate::get_discovery_ip(), discovery_port, 0, 0);
    log::debug!(
        "Sending {:?} to multicast address {}",
        request,
        multicast_address
    );
    let packet: Arc<[u8]> = request.encode_to_vec().into();
    multicast_sender
        .send(multicast_address, packet.clone())
        .await?;
    Ok(())
}

#[derive(Error, Debug)]
pub enum ScanError {
    #[error("Invalid discovery port")]
    InvalidDiscoveryPort,

    #[error("Failed to send a multicast request")]
    MulticastRequest(#[from] std::io::Error),

    #[error("Error in response collection")]
    ResponseCollection(#[from] tonic::transport::Error),
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::multicast::MockMulticastSender;
    use crate::response_collector::MockResponseCollector;
    use futures_util::FutureExt;
    use futures_util::StreamExt;
    use futures_util::TryStreamExt;
    use mockall::predicate::eq;

    #[tokio::test]
    async fn scan() {
        crate::test::init();

        let expected_services = vec![
            Service {
                address: "[::1]:1".parse().unwrap(),
            },
            Service {
                address: "[::2]:2".parse().unwrap(),
            },
        ];
        let expected_services_clone = expected_services.clone();

        let multicast_address = SocketAddrV6::new(crate::get_discovery_ip(), 1, 0, 0);
        let request = Request {
            response_collector_port: 1,
        };
        let request_packet: Arc<[u8]> = request.encode_to_vec().into();

        let mut response_collector = MockResponseCollector::new();
        response_collector
            .expect_get_port()
            .return_const(request.response_collector_port as u16);
        response_collector.expect_collect().return_once(|| {
            futures_util::stream::iter(expected_services_clone)
                .map(Ok)
                .boxed()
        });
        let response_collector = Box::new(response_collector);

        let mut multicast_sender = MockMulticastSender::new();
        multicast_sender
            .expect_send()
            .with(eq(multicast_address), eq(request_packet.clone()))
            .return_once(|_, _| async { Ok(()) }.boxed());

        // When
        let actual_services: Vec<_> = scan_internal(
            multicast_address.port(),
            response_collector,
            multicast_sender,
        )
        .take(2)
        .try_collect()
        .await
        .unwrap();

        // Then
        assert_eq!(actual_services, expected_services);
    }

    #[tokio::test]
    async fn invalid_discovery_port() {
        crate::test::init();

        // When
        let e = super::scan(0).try_collect::<Vec<_>>().await.unwrap_err();

        // Then
        if let ScanError::InvalidDiscoveryPort = e {
        } else {
            panic!("0 must be an invalid discovery port");
        }
    }
}
