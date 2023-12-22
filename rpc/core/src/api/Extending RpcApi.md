# HOWTO Extend the RPC Api by adding a new method

As an illustration, let's pretend that we add a new `submit_block` method.

## consensus-core

1. If necessary, add a function into the ConsensusApi trait.

## consensus

1. Implement the function for Consensus

## rpc-core

1. Create an op variant in `kaspa_rpc_core::api::ops::RpcApiOps`
   (ie. `SubmitBlock`)
2. Create in `kaspa_rpc_core::model::message` a pair of request and response structures
   (ie. `SubmitBlockRequest` and `SubmitBlockResponse`).
3. Implement a constructor for the request.
4. If necessary, implement converters to handle consensus-core <-> rpc-core under `kaspa_rpc_core::convert`.
5. Add a pair of new async functions to the `kaspa_rpc_core::api::RpcApi` trait, one with detailed parameters
   and one with a unique request message.
   Implement the first as a call to the second.
   (ie. `async fn submit_block(&self, block: RpcBlock, allow_non_daa_blocks: bool) -> RpcResult<SubmitBlockResponse>` and
   `async fn submit_block_call(&self, request: SubmitBlockRequest) -> RpcResult<SubmitBlockResponse>;`)
6. Implement the function having a `_call` suffix into `kaspa_rpc_core::server::service::RpcCoreService`.

## rpc-grpc

1. In file `rpc\grpc\proto\rpc.proto`, create a request message and a response message
   (ie. `SubmitBlockRequestMessage` and `SubmitBlockResponseMessage`).
2. In file `rpc\grpc\proto\messages.proto`, add respectively a request and a response to the payload of `KaspadRequest` and `KaspadResponse`.
   (ie. `SubmitBlockRequestMessage submitBlockRequest = 1003;` and `SubmitBlockResponseMessage submitBlockResponse = 1004;`)
3. In `rpc\grpc\src\convert\message.rs`, implement converters to handle rpc-core <-> rpc-grpc.
4. If appropriate, implement a matcher in `kaspa_grpc_client::resolver::matcher`.
5. Complete the `Matcher` trait implementation for `kaspad_request::Payload`.
6. In `rpc\grpc\src\convert\kaspad.rs`, complete the `From` implementations for `RpcApiOps`.
7. In `rpc\grpc\src\convert\kaspad.rs`, add calls to `impl_into_kaspad_request!` and `impl_into_kaspad_response!`
   (ie. `impl_into_kaspad_request!(kaspa_rpc_core::SubmitBlockRequest, SubmitBlockRequestMessage, SubmitBlockRequest);` and
   `impl_into_kaspad_response!(kaspa_rpc_core::SubmitBlockResponse, SubmitBlockResponseMessage, SubmitBlockResponse);`).
8. Implement the function having a `_call` suffix into `kaspa_grpc_client::GrpcClient`.
9. In `kaspa_grpc_server::service::RpcService::message_stream`, requests handler, add an arm and implement
   a handler for the new method.

## rpc-test
1. In file `testing\integration\src\rpc_tests.rs` add a new `match` arm for your payload inside the `sanity_test` test
