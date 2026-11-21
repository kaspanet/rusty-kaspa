pub mod policy;
pub mod service;

use kaspa_wallet_core::api::NewAddressKind;
use kaspa_wallet_core::api::WalletApi;
use kaspa_wallet_grpc_core::convert::deserialize_txs;
use kaspa_wallet_grpc_core::kaspawalletd::{
    BroadcastRequest, BroadcastResponse, BumpFeeRequest, BumpFeeResponse, CreateUnsignedTransactionsRequest,
    CreateUnsignedTransactionsResponse, GetBalanceRequest, GetBalanceResponse, GetExternalSpendableUtxOsRequest,
    GetExternalSpendableUtxOsResponse, GetVersionRequest, GetVersionResponse, NewAddressRequest, NewAddressResponse, SendRequest,
    SendResponse, ShowAddressesRequest, ShowAddressesResponse, ShutdownRequest, ShutdownResponse, SignRequest, SignResponse,
    kaspawalletd_server::Kaspawalletd,
};
use kaspa_wallet_grpc_core::pst::decode_partially_signed_transaction;
use service::Service;
use tonic::{Request, Response, Status};

#[tonic::async_trait]
impl Kaspawalletd for Service {
    async fn get_balance(&self, _request: Request<GetBalanceRequest>) -> Result<Response<GetBalanceResponse>, Status> {
        let _guard = self.read_guard().await;
        let balances = self.balances().await?;
        let response = GetBalanceResponse {
            available: balances.available,
            pending: balances.pending,
            address_balances: balances.address_balances,
        };
        Ok(Response::new(response))
    }

    async fn get_external_spendable_utx_os(
        &self,
        request: Request<GetExternalSpendableUtxOsRequest>,
    ) -> Result<Response<GetExternalSpendableUtxOsResponse>, Status> {
        let _guard = self.read_guard().await;
        let entries = self.external_spendable_utxos(request.into_inner().address).await?;
        Ok(Response::new(GetExternalSpendableUtxOsResponse { entries }))
    }

    async fn create_unsigned_transactions(
        &self,
        request: Request<CreateUnsignedTransactionsRequest>,
    ) -> Result<Response<CreateUnsignedTransactionsResponse>, Status> {
        let _guard = self.write_guard().await;
        let unsigned_transactions =
            self.create_unsigned_transactions(request.into_inner()).await?.into_iter().map(|pst| pst.encode_to_vec()).collect();
        Ok(Response::new(CreateUnsignedTransactionsResponse { unsigned_transactions }))
    }

    async fn show_addresses(&self, _request: Request<ShowAddressesRequest>) -> Result<Response<ShowAddressesResponse>, Status> {
        let _guard = self.read_guard().await;
        self.ensure_synced()?;
        let addresses = self.receive_addresses()?.iter().map(|addr| addr.to_string()).collect::<Vec<String>>();
        let response = ShowAddressesResponse { address: addresses };
        Ok(Response::new(response))
    }

    async fn new_address(&self, _request: Request<NewAddressRequest>) -> Result<Response<NewAddressResponse>, Status> {
        let _guard = self.write_guard().await;
        self.ensure_synced()?;
        let address = self
            .wallet()
            .accounts_create_new_address(self.descriptor()?.account_id, NewAddressKind::Receive)
            .await
            .map_err(|err| Status::internal(err.to_string()))?
            .address;
        let response = NewAddressResponse { address: address.to_string() };
        Ok(Response::new(response))
    }

    async fn shutdown(&self, _request: Request<ShutdownRequest>) -> Result<Response<ShutdownResponse>, Status> {
        self.initiate_shutdown();
        Ok(Response::new(ShutdownResponse {}))
    }

    // TODO: Consider implementing parallel transaction processing in the future:
    // - Server-side configuration processes messages sequentially
    // - It might be possible to start processing a new message before writing the response to the socket
    // - New parameters like allow_parallel should be introduced
    // - Client behavior should be considered as they may expect sequential processing until the first error when sending batches
    async fn broadcast(&self, request: Request<BroadcastRequest>) -> Result<Response<BroadcastResponse>, Status> {
        let _guard = self.write_guard().await;
        let request = request.into_inner();
        let txs = deserialize_txs(request.transactions, request.is_domain, self.use_ecdsa())?;
        let mut tx_ids: Vec<String> = Vec::with_capacity(txs.len());
        for tx in txs {
            let tx_id = self.submit_transaction(tx).await?;
            tx_ids.push(tx_id.to_string());
        }
        Ok(Response::new(BroadcastResponse { tx_ids }))
    }

    async fn broadcast_replacement(&self, request: Request<BroadcastRequest>) -> Result<Response<BroadcastResponse>, Status> {
        let _guard = self.write_guard().await;
        let request = request.into_inner();
        let txs = deserialize_txs(request.transactions, request.is_domain, self.use_ecdsa())?;
        let mut tx_ids: Vec<String> = Vec::with_capacity(txs.len());
        for (i, tx) in txs.into_iter().enumerate() {
            // Once the first transaction is added to the mempool, the transactions that depend
            // on the replaced transaction will be removed, so there's no need to submit them
            // as RBF transactions.
            let tx_id = if i == 0 { self.submit_transaction_replacement(tx).await? } else { self.submit_transaction(tx).await? };
            tx_ids.push(tx_id.to_string());
        }
        Ok(Response::new(BroadcastResponse { tx_ids }))
    }

    async fn send(&self, request: Request<SendRequest>) -> Result<Response<SendResponse>, Status> {
        let _guard = self.write_guard().await;
        let SendRequest { to_address, amount, password, from, use_existing_change_address, is_send_all, fee_policy } =
            request.into_inner();
        self.ensure_payment_secret_not_required().await?;
        let unsigned_transactions = self
            .create_unsigned_transactions(CreateUnsignedTransactionsRequest {
                address: to_address,
                amount,
                from,
                use_existing_change_address,
                is_send_all,
                fee_policy,
            })
            .await?;
        let signed_transactions = self
            .sign_partially_signed_transactions(unsigned_transactions, password)
            .await?
            .into_iter()
            .map(|pst| pst.encode_to_vec())
            .collect::<Vec<_>>();
        let transactions = deserialize_txs(signed_transactions.clone(), false, self.use_ecdsa())?;
        let mut tx_ids: Vec<String> = Vec::with_capacity(transactions.len());
        for transaction in transactions {
            match self.submit_transaction(transaction).await {
                Ok(tx_id) => tx_ids.push(tx_id.to_string()),
                Err(err) => {
                    return Err(Status::new(
                        err.code(),
                        format!("{}; already submitted transactions: [{}]", err.message(), tx_ids.join(", ")),
                    ));
                }
            }
        }
        Ok(Response::new(SendResponse { tx_ids, signed_transactions }))
    }

    async fn sign(&self, request: Request<SignRequest>) -> Result<Response<SignResponse>, Status> {
        let _guard = self.write_guard().await;
        let SignRequest { unsigned_transactions, password } = request.into_inner();

        let unsigned_transactions = unsigned_transactions
            .iter()
            .map(|tx| decode_partially_signed_transaction(tx.as_slice()))
            .collect::<Result<Vec<_>, Status>>()?;

        let signed_transactions = self
            .sign_partially_signed_transactions(unsigned_transactions, password)
            .await?
            .into_iter()
            .map(|pst| pst.encode_to_vec())
            .collect();
        Ok(Response::new(SignResponse { signed_transactions }))
    }

    async fn get_version(&self, _request: Request<GetVersionRequest>) -> Result<Response<GetVersionResponse>, Status> {
        let response = GetVersionResponse { version: env!("CARGO_PKG_VERSION").to_string() };
        Ok(Response::new(response))
    }

    async fn bump_fee(&self, _request: Request<BumpFeeRequest>) -> Result<Response<BumpFeeResponse>, Status> {
        // wallet api doesnt support RBF, requires manual implementation
        Err(Status::unimplemented("Bump fee is not implemented yet"))
    }
}
