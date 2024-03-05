use crate::scanner::Service;
use futures_channel::mpsc::UnboundedReceiver;
use futures_channel::mpsc::UnboundedSender;
use futures_util::Future;
use futures_util::FutureExt;
use futures_util::Stream;
use futures_util::StreamExt;
use futures_util::TryStreamExt;
use mockall::automock;
use std::net::IpAddr;
use std::net::SocketAddrV6;
use tansa_protocol::response_collector_service_server::ResponseCollectorService;
use tansa_protocol::response_collector_service_server::ResponseCollectorServiceServer;
use tansa_protocol::Response;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::server::Router;
use tonic::transport::Server;
use tonic::Status;
use tower_layer::Identity;

#[automock]
pub trait ResponseCollector {
    fn get_port(&self) -> u16;
    fn collect(
        self,
    ) -> impl Stream<Item = Result<Service, tonic::transport::Error>> + Send + 'static;
}

pub struct GrpcResponseCollector {
    tcp: TcpListenerStream,
    port: u16,
    grpc: Router<Identity>,
    response_receiver: UnboundedReceiver<Service>,
}

impl GrpcResponseCollector {
    pub async fn new() -> std::io::Result<Self> {
        let tcp = TcpListener::bind("[::]:0").await?;
        let port = tcp.local_addr()?.port();
        let tcp = TcpListenerStream::new(tcp);
        let (response_sender, response_receiver) = futures_channel::mpsc::unbounded();
        let grpc_service_provider = ResponseCollectorServiceProvider { response_sender };

        let (mut health_reporter, health_server) = tonic_health::server::health_reporter();
        health_reporter
            .set_serving::<ResponseCollectorServiceServer<ResponseCollectorServiceProvider>>()
            .await;

        let grpc = Server::builder()
            .add_service(health_server)
            .add_service(ResponseCollectorServiceServer::new(grpc_service_provider));

        Ok(Self {
            tcp,
            port,
            grpc,
            response_receiver,
        })
    }
}

impl ResponseCollector for GrpcResponseCollector {
    fn get_port(&self) -> u16 {
        self.port
    }
    fn collect(
        self,
    ) -> impl Stream<Item = Result<Service, tonic::transport::Error>> + Send + 'static {
        join(
            self.grpc.serve_with_incoming(self.tcp),
            self.response_receiver,
        )
    }
}

struct ResponseCollectorServiceProvider {
    response_sender: UnboundedSender<Service>,
}

#[tonic::async_trait]
impl ResponseCollectorService for ResponseCollectorServiceProvider {
    async fn submit_response(
        &self,
        request: tonic::Request<Response>,
    ) -> Result<tonic::Response<()>, Status> {
        let remote_ip = match request.remote_addr().map(|addr| addr.ip()) {
            Some(IpAddr::V6(ip)) => ip,
            Some(IpAddr::V4(_)) => return Err(Status::unimplemented("IPv4 unsupported")),
            None => return Err(Status::unimplemented("IP unavailable")),
        };
        let request = request.into_inner();
        let remote_service_port = match request.service_port.try_into() {
            Ok(p) => p,
            Err(_) => return Err(Status::invalid_argument("Port out of range")),
        };
        let service = Service {
            address: SocketAddrV6::new(remote_ip, remote_service_port, 0, 0),
        };
        match self.response_sender.unbounded_send(service) {
            Ok(_) => Ok(tonic::Response::new(())),
            Err(_) => Err(Status::internal("No longer accepting responses")),
        }
    }
}

fn join(
    future: impl Future<Output = Result<(), tonic::transport::Error>> + Send + 'static,
    stream: impl Stream<Item = Service> + Send + 'static,
) -> impl Stream<Item = Result<Service, tonic::transport::Error>> + Send + 'static {
    let wrapped_future = future.into_stream().map_ok(|_| None);
    let wrapped_stream = stream.map(|response| Ok(Some(response)));
    futures_util::stream::select(wrapped_future, wrapped_stream)
        .filter_map(|item| async { item.transpose() })
}
