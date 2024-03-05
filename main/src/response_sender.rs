use mockall::automock;
use tansa_protocol::response_collector_service_client::ResponseCollectorServiceClient;
use tansa_protocol::Response;
use tonic::IntoRequest;

#[automock]
pub trait ResponseSender {
    async fn send(
        &self,
        response: Response,
        response_collector_address: String,
    ) -> anyhow::Result<()>;
}

pub struct GrpcResponseSender;

impl ResponseSender for GrpcResponseSender {
    async fn send(
        &self,
        response: Response,
        response_collector_address: String,
    ) -> anyhow::Result<()> {
        ResponseCollectorServiceClient::connect(response_collector_address)
            .await?
            .submit_response(response.into_request())
            .await?;
        Ok(())
    }
}
