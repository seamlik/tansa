use crate::scanner::Service;
use futures_channel::mpsc::UnboundedReceiver;
use futures_channel::mpsc::UnboundedSender;
use futures_util::stream::BoxStream;
use futures_util::StreamExt;
use mockall::automock;
use std::net::SocketAddr;
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
    fn collect(self) -> BoxStream<'static, Result<Service, tonic::transport::Error>>;
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
    fn collect(self) -> BoxStream<'static, Result<Service, tonic::transport::Error>> {
        log::info!("Collecting responses at port {}", self.get_port());
        crate::stream::join(
            self.grpc.serve_with_incoming(self.tcp),
            self.response_receiver.map(Ok::<_, tonic::transport::Error>),
        )
        .boxed()
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
        let remote_address = request.remote_addr();
        let request = request.into_inner();
        log::debug!(
            "Received {:?} from `ResponseSender` at {:?}",
            request,
            remote_address
        );

        let service_port = match request.service_port.try_into() {
            Ok(p) => p,
            Err(_) => return Err(Status::invalid_argument("Port out of range")),
        };
        if service_port == 0 {
            return Err(Status::invalid_argument("Port must not be 0"));
        }

        let mut service_address = match remote_address {
            Some(SocketAddr::V6(a)) => a,
            Some(SocketAddr::V4(_)) => return Err(Status::unimplemented("IPv4 unsupported")),
            None => return Err(Status::unimplemented("Remote address unavailable")),
        };
        service_address.set_port(service_port);

        let service = Service {
            address: service_address,
        };
        match self.response_sender.unbounded_send(service) {
            Ok(_) => Ok(tonic::Response::new(())),
            Err(_) => Err(Status::unavailable("No longer accepting responses")),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::response_sender::GrpcResponseSender;
    use crate::response_sender::ResponseSender;
    use futures_util::TryStreamExt;
    use tonic::transport::server::TcpConnectInfo;
    use tonic::IntoRequest;

    #[tokio::test]
    async fn response_collector_port_must_not_be_0() {
        let collector = GrpcResponseCollector::new().await.unwrap();
        assert_ne!(collector.get_port(), 0, "Port must not be 0");
    }

    #[tokio::test]
    async fn collect() {
        crate::test::init();

        let collector = Box::new(GrpcResponseCollector::new().await.unwrap());
        let port = collector.get_port();
        let responses = vec![Response { service_port: 1 }, Response { service_port: 2 }];

        // When
        let services: Vec<_> = crate::stream::join::<anyhow::Error, _, _, _, _, _, _>(
            submit_responses(port, responses.clone()),
            collector.collect().take(2),
        )
        .try_collect()
        .await
        .unwrap();

        // Then
        assert_eq!(services[0].address, "[::1]:1".parse().unwrap());
        assert_eq!(services[1].address, "[::1]:2".parse().unwrap());
    }

    #[tokio::test]
    async fn remote_address_unavailable() {
        crate::test::init();

        let provider = new_response_collector_service_provider();
        let request = Response { service_port: 1 }.into_request();

        // When
        let status = provider.submit_response(request).await.unwrap_err();

        // Then
        assert_eq!(status.message(), "Remote address unavailable");
    }

    #[tokio::test]
    async fn ipv4_unsupported() {
        crate::test::init();

        let provider = new_response_collector_service_provider();

        let mut request = Response { service_port: 1 }.into_request();
        let tcp_connect_info = TcpConnectInfo {
            local_addr: None,
            remote_addr: Some("1.1.1.1:1".parse().unwrap()),
        };
        request.extensions_mut().insert(tcp_connect_info);

        // When
        let status = provider.submit_response(request).await.unwrap_err();

        // Then
        assert_eq!(status.message(), "IPv4 unsupported");
    }

    #[tokio::test]
    async fn port_out_of_range() {
        crate::test::init();

        let provider = new_response_collector_service_provider();

        let mut request = Response {
            service_port: 100000,
        }
        .into_request();
        let tcp_connect_info = TcpConnectInfo {
            local_addr: None,
            remote_addr: Some("[::1]:1".parse().unwrap()),
        };
        request.extensions_mut().insert(tcp_connect_info);

        // When
        let status = provider.submit_response(request).await.unwrap_err();

        // Then
        assert_eq!(status.message(), "Port out of range");
    }

    #[tokio::test]
    async fn port_must_not_be_0() {
        crate::test::init();

        let provider = new_response_collector_service_provider();

        let mut request = Response { service_port: 0 }.into_request();
        let tcp_connect_info = TcpConnectInfo {
            local_addr: None,
            remote_addr: Some("[::1]:1".parse().unwrap()),
        };
        request.extensions_mut().insert(tcp_connect_info);

        // When
        let status = provider.submit_response(request).await.unwrap_err();

        // Then
        assert_eq!(status.message(), "Port must not be 0");
    }

    #[tokio::test]
    async fn unavailable() {
        crate::test::init();

        let provider = new_response_collector_service_provider();

        let mut request = Response { service_port: 1 }.into_request();
        let tcp_connect_info = TcpConnectInfo {
            local_addr: None,
            remote_addr: Some("[::1]:1".parse().unwrap()),
        };
        request.extensions_mut().insert(tcp_connect_info);

        // When
        let status = provider.submit_response(request).await.unwrap_err();

        // Then
        assert_eq!(status.message(), "No longer accepting responses");
    }

    async fn submit_responses(port: u16, responses: Vec<Response>) -> anyhow::Result<()> {
        let response_collector_address = format!("http://[::1]:{}", port);
        let response_sender = GrpcResponseSender;
        for response in responses {
            response_sender
                .send(response, response_collector_address.clone())
                .await?;
        }
        Ok(())
    }

    fn new_response_collector_service_provider() -> ResponseCollectorServiceProvider {
        let (response_sender, _) = futures_channel::mpsc::unbounded();
        ResponseCollectorServiceProvider { response_sender }
    }
}
