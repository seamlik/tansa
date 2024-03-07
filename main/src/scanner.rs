use crate::multicast::MulticastSender;
use crate::multicast::TokioMulticastSender;
use crate::response_collector::GrpcResponseCollector;
use crate::response_collector::ResponseCollector;
use futures_util::TryStream;
use prost::Message;
use std::net::SocketAddrV6;
use std::sync::Arc;
use tansa_protocol::Request;
use thiserror::Error;

pub struct Service {
    pub address: SocketAddrV6,
}

pub struct Scanner {
    service_name: String,
    multicast_network_interface_indexes: Vec<u32>,
    response_collector: Box<dyn ResponseCollector>,
    multicast_sender: Arc<dyn MulticastSender>,
}

impl Scanner {
    pub async fn new(
        service_name: String,
        multicast_network_interface_indexes: impl IntoIterator<Item = u32>,
    ) -> std::io::Result<Self> {
        Self::new_internal(
            service_name,
            multicast_network_interface_indexes,
            GrpcResponseCollector::new_boxed().await?,
            Arc::new(TokioMulticastSender),
        )
    }
    fn new_internal(
        service_name: String,
        multicast_network_interface_indexes: impl IntoIterator<Item = u32>,
        response_collector: Box<dyn ResponseCollector>,
        multicast_sender: Arc<dyn MulticastSender>,
    ) -> std::io::Result<Self> {
        Ok(Self {
            service_name,
            multicast_network_interface_indexes: multicast_network_interface_indexes
                .into_iter()
                .collect(),
            response_collector,
            multicast_sender,
        })
    }
    pub fn scan(self) -> impl TryStream<Ok = Service, Error = ScanError> {
        let request = Request {
            service_name: self.service_name,
            response_collector_port: self.response_collector.get_port().into(),
        };
        let request_packet: Arc<[u8]> = request.encode_to_vec().into();
        let multicast_address = crate::get_multicast_address();

        let send_request_task = self
            .multicast_network_interface_indexes
            .iter()
            .map(|inter| {
                self.multicast_sender
                    .send(*inter, multicast_address, request_packet.clone())
            });
        let send_request_task = futures_util::future::try_join_all(send_request_task);

        crate::stream::join(send_request_task, self.response_collector.collect())
    }
}

#[derive(Error, Debug)]
pub enum ScanError {
    #[error("Failed in sending a multicast request")]
    MulticastRequest(#[from] std::io::Error),

    #[error("Error in response collection")]
    ResponseCollection(#[from] tonic::transport::Error),
}
