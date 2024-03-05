use crate::multicast::MulticastSender;
use crate::multicast::TokioMulticastSender;
use crate::response_collector::GrpcResponseCollector;
use crate::response_collector::ResponseCollector;
use futures_util::FutureExt;
use futures_util::StreamExt;
use futures_util::TryFuture;
use futures_util::TryFutureExt;
use futures_util::TryStream;
use futures_util::TryStreamExt;
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
    response_collector: GrpcResponseCollector,
}

impl Scanner {
    pub async fn new(
        service_name: String,
        multicast_network_interface_indexes: impl IntoIterator<Item = u32>,
    ) -> std::io::Result<Self> {
        let response_collector = GrpcResponseCollector::new().await?;
        Ok(Self {
            service_name,
            multicast_network_interface_indexes: multicast_network_interface_indexes
                .into_iter()
                .collect(),
            response_collector,
        })
    }
    pub fn scan(self) -> impl TryStream<Ok = Service, Error = ScanError> {
        Self::scan_internal(
            self.service_name,
            &self.multicast_network_interface_indexes,
            Arc::new(TokioMulticastSender),
            self.response_collector,
        )
    }
    fn scan_internal(
        service_name: String,
        multicast_network_interface_indexes: &[u32],
        multicast_sender: Arc<dyn MulticastSender>,
        response_collector: impl ResponseCollector,
    ) -> impl TryStream<Ok = Service, Error = ScanError> {
        let request = Request {
            service_name,
            response_collector_port: response_collector.get_port().into(),
        };
        let request_packet: Arc<[u8]> = request.encode_to_vec().into();
        let multicast_address = crate::get_multicast_address();

        let send_request_task = multicast_network_interface_indexes
            .iter()
            .map(|inter| multicast_sender.send(*inter, multicast_address, request_packet.clone()));
        let send_request_task = futures_util::future::try_join_all(send_request_task);

        join(send_request_task, response_collector.collect())
    }
}

#[derive(Error, Debug)]
pub enum ScanError {
    #[error("Failed in sending a multicast request")]
    MulticastRequest(#[from] std::io::Error),

    #[error("Error in response collection")]
    ResponseCollection(#[from] tonic::transport::Error),
}

fn join<F, S>(future: F, stream: S) -> impl TryStream<Ok = Service, Error = ScanError>
where
    F: TryFuture<Ok = Vec<()>, Error = std::io::Error> + Send + 'static,
    S: TryStream<Ok = Service, Error = tonic::transport::Error> + Send + 'static,
{
    let wrapped_future = future.map_ok(|_| None).map_err(Into::into).into_stream();
    let wrapped_stream = stream.map_ok(Some).map_err(Into::into);
    futures_util::stream::select(wrapped_future, wrapped_stream)
        .filter_map(|item| async { item.transpose() })
}
