use crate::multicast::MulticastSender;
use crate::multicast::TokioMulticastSender;
use crate::response_collector::GrpcResponseCollector;
use crate::response_collector::ResponseCollector;
use futures_util::Stream;
use prost::Message;
use std::net::SocketAddrV6;
use std::sync::Arc;
use tansa_protocol::Request;
use thiserror::Error;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Service {
    pub address: SocketAddrV6,
}

pub struct Scanner {
    service_name: String,
    response_collector: Box<dyn ResponseCollector>,
    multicast_sender: Arc<dyn MulticastSender + Send + Sync>,
}

impl Scanner {
    pub async fn new(service_name: String) -> std::io::Result<Self> {
        Self::new_internal(
            service_name,
            Box::new(GrpcResponseCollector::new().await?),
            Arc::new(TokioMulticastSender),
        )
    }
    fn new_internal(
        service_name: String,
        response_collector: Box<dyn ResponseCollector>,
        multicast_sender: Arc<dyn MulticastSender + Send + Sync>,
    ) -> std::io::Result<Self> {
        Ok(Self {
            service_name,
            response_collector,
            multicast_sender,
        })
    }
    pub fn scan(self) -> impl Stream<Item = Result<Service, ScanError>> {
        let request = Request {
            service_name: self.service_name,
            response_collector_port: self.response_collector.get_port().into(),
        };
        crate::stream::join(
            Self::send_requests(self.multicast_sender, request),
            self.response_collector.collect(),
        )
    }

    async fn send_requests(
        multicast_sender: Arc<dyn MulticastSender + Send + Sync>,
        request: Request,
    ) -> Result<(), ScanError> {
        let multicast_address = crate::get_multicast_address();
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
}

#[derive(Error, Debug)]
pub enum ScanError {
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

        let multicast_address = crate::get_multicast_address();
        let request = Request {
            service_name: "SERVICE".into(),
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

        let mut multicast_sender = MockMulticastSender::new();
        multicast_sender
            .expect_send()
            .with(eq(multicast_address), eq(request_packet.clone()))
            .return_once(|_, _| async { Ok(()) }.boxed());

        let scanner = Scanner::new_internal(
            request.service_name,
            Box::new(response_collector),
            Arc::new(multicast_sender),
        )
        .unwrap();

        // When
        let actual_services: Vec<_> = scanner.scan().take(2).try_collect().await.unwrap();

        // Then
        assert_eq!(actual_services, expected_services);
    }
}
