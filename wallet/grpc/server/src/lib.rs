pub mod service;

use kaspa_addresses::Version;
use kaspa_consensus_core::tx::{SignableTransaction, Transaction, UtxoEntry};
use kaspa_wallet_core::api::WalletApi;
use kaspa_wallet_core::tx::{Signer, SignerT};
use kaspa_wallet_core::{
    api::{AccountsGetUtxosRequest, AccountsSendRequest, NewAddressKind},
    prelude::Address,
    tx::{Fees, PaymentDestination, PaymentOutputs},
};
use kaspa_wallet_grpc_core::convert::{deserialize_txs, extract_tx};
use kaspa_wallet_grpc_core::kaspawalletd::{
    fee_policy::FeePolicy, kaspawalletd_server::Kaspawalletd, BroadcastRequest, BroadcastResponse, BumpFeeRequest, BumpFeeResponse,
    CreateUnsignedTransactionsRequest, CreateUnsignedTransactionsResponse, GetBalanceRequest, GetBalanceResponse,
    GetExternalSpendableUtxOsRequest, GetExternalSpendableUtxOsResponse, GetVersionRequest, GetVersionResponse, NewAddressRequest,
    NewAddressResponse, SendRequest, SendResponse, ShowAddressesRequest, ShowAddressesResponse, ShutdownRequest, ShutdownResponse,
    SignRequest, SignResponse,
};
use kaspa_wallet_grpc_core::protoserialization::PartiallySignedTransaction;
use service::Service;
use tonic::{Code, Request, Response, Status};

#[tonic::async_trait]
impl Kaspawalletd for Service {
    async fn get_balance(&self, _request: Request<GetBalanceRequest>) -> Result<Response<GetBalanceResponse>, Status> {
        let balances = self.descriptor().balance.unwrap();
        let response = GetBalanceResponse { available: balances.mature, pending: balances.pending, address_balances: vec![] };
        Ok(Response::new(response))
    }

    async fn get_external_spendable_utx_os(
        &self,
        _request: Request<GetExternalSpendableUtxOsRequest>,
    ) -> Result<Response<GetExternalSpendableUtxOsResponse>, Status> {
        let address = Address::try_from(_request.get_ref().address.clone())
            .map_err(|_| Status::new(tonic::Code::InvalidArgument, "Invalid address provided"))?;
        let request = AccountsGetUtxosRequest {
            account_id: self.descriptor().account_id,
            addresses: Some(vec![address]),
            min_amount_sompi: None,
        };
        let utxos = self.wallet().accounts_get_utxos(request).await.unwrap().utxos;
        let response = GetExternalSpendableUtxOsResponse { entries: utxos.into_iter().map(Into::into).collect() };
        Ok(Response::new(response))
    }

    async fn create_unsigned_transactions(
        &self,
        request: Request<CreateUnsignedTransactionsRequest>,
    ) -> Result<Response<CreateUnsignedTransactionsResponse>, Status> {
        let CreateUnsignedTransactionsRequest { address, amount, from, use_existing_change_address, is_send_all, fee_policy } =
            request.into_inner();
        let to_address = Address::try_from(address).map_err(|err| Status::invalid_argument(err.to_string()))?;
        let (fee_rate, max_fee) = self.calculate_fee_limits(fee_policy).await?;
        let from_addresses = from
            .iter()
            .map(|a| Address::try_from(a.as_str()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| Status::invalid_argument(err.to_string()))?;
        let transactions =
            self.unsigned_txs(to_address, amount, use_existing_change_address, is_send_all, fee_rate, max_fee, from_addresses).await?;
        let unsigned_transactions =
            transactions.into_iter().map(|tx| PartiallySignedTransaction::from_unsigned(tx).encode_to_vec()).collect();
        Ok(Response::new(CreateUnsignedTransactionsResponse { unsigned_transactions }))
    }

    async fn show_addresses(&self, _request: Request<ShowAddressesRequest>) -> Result<Response<ShowAddressesResponse>, Status> {
        let addresses = self.receive_addresses().iter().map(|addr| addr.to_string()).collect::<Vec<String>>();
        let response = ShowAddressesResponse { address: addresses };
        Ok(Response::new(response))
    }

    async fn new_address(&self, _request: Request<NewAddressRequest>) -> Result<Response<NewAddressResponse>, Status> {
        let address = self
            .wallet()
            .accounts_create_new_address(self.descriptor().account_id, NewAddressKind::Receive)
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
        let request = request.into_inner();
        let txs = deserialize_txs(request.transactions, request.is_domain, self.use_ecdsa())?;
        let mut tx_ids: Vec<String> = Vec::with_capacity(txs.len());
        for tx in txs {
            let tx_id =
                self.wallet().rpc_api().submit_transaction(tx, false).await.map_err(|e| Status::new(Code::Internal, e.to_string()))?;
            tx_ids.push(tx_id.to_string());
        }
        Ok(Response::new(BroadcastResponse { tx_ids }))
    }

    async fn broadcast_replacement(&self, request: Request<BroadcastRequest>) -> Result<Response<BroadcastResponse>, Status> {
        let request = request.into_inner();
        let txs = deserialize_txs(request.transactions, request.is_domain, self.use_ecdsa())?;
        let mut tx_ids: Vec<String> = Vec::with_capacity(txs.len());
        for (i, tx) in txs.into_iter().enumerate() {
            // Once the first transaction is added to the mempool, the transactions that depend
            // on the replaced transaction will be removed, so there's no need to submit them
            // as RBF transactions.
            let tx_id = if i == 0 {
                let submit_transaction_replacement_response = self
                    .wallet()
                    .rpc_api()
                    .submit_transaction_replacement(tx)
                    .await
                    .map_err(|e| Status::new(Code::Internal, e.to_string()))?;
                submit_transaction_replacement_response.transaction_id
            } else {
                self.wallet().rpc_api().submit_transaction(tx, false).await.map_err(|e| Status::new(Code::Internal, e.to_string()))?
            };
            tx_ids.push(tx_id.to_string());
        }
        Ok(Response::new(BroadcastResponse { tx_ids }))
    }

    async fn send(&self, _request: Request<SendRequest>) -> Result<Response<SendResponse>, Status> {
        let acc = self.wallet().account().map_err(|err| Status::internal(err.to_string()))?;
        if acc.minimum_signatures() != 1 {
            return Err(Status::unimplemented("Only single signature wallets are supported"));
        }
        if acc.receive_address().map_err(|err| Status::internal(err.to_string()))?.version == Version::PubKeyECDSA {
            return Err(Status::unimplemented("Ecdsa wallets are not supported yet"));
        }

        // todo call unsigned tx and sign it to be consistent

        let data = _request.get_ref();
        let fee_rate_estimate = self.wallet().fee_rate_estimate().await.unwrap();
        let fee_rate = data.fee_policy.and_then(|policy| match policy.fee_policy.unwrap() {
            FeePolicy::MaxFeeRate(rate) => Some(fee_rate_estimate.normal.feerate.min(rate)),
            FeePolicy::ExactFeeRate(rate) => Some(rate),
            _ => None, // TODO: we dont support maximum_amount policy so think if we should supply default fee_rate_estimate or just 1 on this case...
        });
        let request = AccountsSendRequest {
            account_id: self.descriptor().account_id,
            wallet_secret: data.password.clone().into(),
            payment_secret: None,
            destination: PaymentDestination::PaymentOutputs(PaymentOutputs::from((
                Address::try_from(data.to_address.clone()).unwrap(),
                data.amount,
            ))),
            fee_rate,
            priority_fee_sompi: Fees::SenderPays(0),
            payload: None,
        };
        let result = self
            .wallet()
            .accounts_send(request)
            .await
            .map_err(|err| Status::new(tonic::Code::Internal, format!("Generator: {}", err)))?;
        let final_transaction = result.final_transaction_id.unwrap().to_string();
        // todo return all transactions
        let response = SendResponse { tx_ids: vec![final_transaction], signed_transactions: vec![] };
        Ok(Response::new(response))
    }

    async fn sign(&self, request: Request<SignRequest>) -> Result<Response<SignResponse>, Status> {
        if self.use_ecdsa() {
            return Err(Status::unimplemented("Ecdsa signing is not supported yet"));
        }
        let SignRequest { unsigned_transactions, password } = request.into_inner();
        let account = self.wallet().account().map_err(|err| Status::internal(err.to_string()))?;
        let txs = unsigned_transactions
            .iter()
            .map(|tx| extract_tx(tx.as_slice(), self.use_ecdsa()))
            // todo convert directly to consensus::transaction
            .map(|r| r
                .and_then(|rtx| Transaction::try_from(rtx)
                .map_err(|err| Status::internal(err.to_string()))))
            .collect::<Result<Vec<_>, _>>()?;
        let utxos = account.clone().get_utxos(None, None).await.map_err(|err| Status::internal(err.to_string()))?;
        let signable_txs: Vec<SignableTransaction> = txs
            .into_iter()
            .map(|tx| {
                let utxos = tx
                    .inputs
                    .iter()
                    .map(|input| {
                        utxos
                            .iter()
                            .find(|utxo| utxo.outpoint != input.previous_outpoint)
                            .map(UtxoEntry::from)
                            .ok_or(Status::invalid_argument(format!("Wallet does not have mature utxo for input {input:?}")))
                    })
                    .collect::<Result<_, Status>>()?;
                Ok(SignableTransaction::with_entries(tx, utxos))
            })
            .collect::<Result<_, Status>>()?;
        let addresses: Vec<_> = account.utxo_context().addresses().iter().map(|addr| addr.as_ref().clone()).collect();
        let signer = Signer::new(
            account.clone(),
            account.prv_key_data(password.into()).await.map_err(|err| Status::internal(err.to_string()))?,
            None,
        );
        let _signed_txs = signable_txs.into_iter().map(|tx| signer.try_sign(tx, addresses.as_slice()));
        // todo fill all required fields, serialize and return
        todo!()
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
