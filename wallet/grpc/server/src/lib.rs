use kaspa_wallet_grpc_core::kaspawalletd::{
    kaspawalletd_server::Kaspawalletd, BroadcastRequest, BroadcastResponse, BumpFeeRequest, BumpFeeResponse,
    CreateUnsignedTransactionsRequest, CreateUnsignedTransactionsResponse, GetBalanceRequest, GetBalanceResponse,
    GetExternalSpendableUtxOsRequest, GetExternalSpendableUtxOsResponse, GetVersionRequest, GetVersionResponse, NewAddressRequest,
    NewAddressResponse, SendRequest, SendResponse, ShowAddressesRequest, ShowAddressesResponse, ShutdownRequest, ShutdownResponse,
    SignRequest, SignResponse,
};
use tonic::{Request, Response, Status};

#[derive(Debug, Default)]
pub struct KaspaWalletService {
    // Add your service state here
}

#[tonic::async_trait]
impl Kaspawalletd for KaspaWalletService {
    async fn get_balance(&self, _request: Request<GetBalanceRequest>) -> Result<Response<GetBalanceResponse>, Status> {
        let response = GetBalanceResponse { available: 0, pending: 0, address_balances: vec![] };
        Ok(Response::new(response))
    }

    async fn get_external_spendable_utx_os(
        &self,
        _request: Request<GetExternalSpendableUtxOsRequest>,
    ) -> Result<Response<GetExternalSpendableUtxOsResponse>, Status> {
        let response = GetExternalSpendableUtxOsResponse { entries: vec![] };
        Ok(Response::new(response))
    }

    async fn create_unsigned_transactions(
        &self,
        _request: Request<CreateUnsignedTransactionsRequest>,
    ) -> Result<Response<CreateUnsignedTransactionsResponse>, Status> {
        let response = CreateUnsignedTransactionsResponse { unsigned_transactions: vec![] };
        Ok(Response::new(response))
    }

    async fn show_addresses(&self, _request: Request<ShowAddressesRequest>) -> Result<Response<ShowAddressesResponse>, Status> {
        let response = ShowAddressesResponse { address: vec![] };
        Ok(Response::new(response))
    }

    async fn new_address(&self, _request: Request<NewAddressRequest>) -> Result<Response<NewAddressResponse>, Status> {
        let response = NewAddressResponse { address: "".to_string() };
        Ok(Response::new(response))
    }

    async fn shutdown(&self, _request: Request<ShutdownRequest>) -> Result<Response<ShutdownResponse>, Status> {
        let response = ShutdownResponse {};
        Ok(Response::new(response))
    }

    async fn broadcast(&self, _request: Request<BroadcastRequest>) -> Result<Response<BroadcastResponse>, Status> {
        let response = BroadcastResponse { tx_ids: vec![] };
        Ok(Response::new(response))
    }

    async fn broadcast_replacement(&self, _request: Request<BroadcastRequest>) -> Result<Response<BroadcastResponse>, Status> {
        let response = BroadcastResponse { tx_ids: vec![] };
        Ok(Response::new(response))
    }

    async fn send(&self, _request: Request<SendRequest>) -> Result<Response<SendResponse>, Status> {
        let response = SendResponse { tx_ids: vec![], signed_transactions: vec![] };
        Ok(Response::new(response))
    }

    async fn sign(&self, _request: Request<SignRequest>) -> Result<Response<SignResponse>, Status> {
        let response = SignResponse { signed_transactions: vec![] };
        Ok(Response::new(response))
    }

    async fn get_version(&self, _request: Request<GetVersionRequest>) -> Result<Response<GetVersionResponse>, Status> {
        let response = GetVersionResponse { version: "".to_string() };
        Ok(Response::new(response))
    }

    async fn bump_fee(&self, _request: Request<BumpFeeRequest>) -> Result<Response<BumpFeeResponse>, Status> {
        let response = BumpFeeResponse { transactions: vec![], tx_ids: vec![] };
        Ok(Response::new(response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_wallet_grpc_core::kaspawalletd::{
        kaspawalletd_server::KaspawalletdServer, GetBalanceRequest, GetVersionRequest, NewAddressRequest,
    };
    use std::time::Duration;
    use tonic::transport::Server;

    #[tokio::test]
    async fn test_server_basic_requests() {
        // Start server
        let addr = "[::1]:50051".parse().unwrap();
        let service = KaspaWalletService::default();

        let server_handle = tokio::spawn(async move {
            Server::builder().add_service(KaspawalletdServer::new(service)).serve(addr).await.unwrap();
        });
        tokio::time::sleep(Duration::from_secs(1)).await; // wait until server starts
                                                          // Create client
        let mut client = kaspa_wallet_grpc_core::kaspawalletd::kaspawalletd_client::KaspawalletdClient::connect("http://[::1]:50051")
            .await
            .unwrap();

        // Test GetBalance
        let balance = client.get_balance(GetBalanceRequest {}).await.unwrap();
        assert_eq!(balance.get_ref().available, 0);
        assert_eq!(balance.get_ref().pending, 0);

        // Test GetVersion
        let version = client.get_version(GetVersionRequest {}).await.unwrap();
        assert_eq!(version.get_ref().version, "");

        // Test NewAddress
        let address = client.new_address(NewAddressRequest {}).await.unwrap();
        assert_eq!(address.get_ref().address, "");

        server_handle.abort();
    }
}
