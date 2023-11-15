use crate::pb::kaspad_message::Payload as KaspadMessagePayload;

#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, Hash, PartialEq)]
pub enum KaspadMessagePayloadType {
    Addresses = 0,
    Block,
    Transaction,
    BlockLocator,
    RequestAddresses,
    RequestRelayBlocks,
    RequestTransactions,
    IbdBlock,
    InvRelayBlock,
    InvTransactions,
    Ping,
    Pong,
    Verack,
    Version,
    TransactionNotFound,
    Reject,
    PruningPointUtxoSetChunk,
    RequestIbdBlocks,
    UnexpectedPruningPoint,
    IbdBlockLocator,
    IbdBlockLocatorHighestHash,
    RequestNextPruningPointUtxoSetChunk,
    DonePruningPointUtxoSetChunks,
    IbdBlockLocatorHighestHashNotFound,
    BlockWithTrustedData,
    DoneBlocksWithTrustedData,
    RequestPruningPointAndItsAnticone,
    BlockHeaders,
    RequestNextHeaders,
    DoneHeaders,
    RequestPruningPointUtxoSet,
    RequestHeaders,
    RequestBlockLocator,
    PruningPoints,
    RequestPruningPointProof,
    PruningPointProof,
    Ready,
    BlockWithTrustedDataV4,
    TrustedData,
    RequestIbdChainBlockLocator,
    IbdChainBlockLocator,
    RequestAntipast,
    RequestNextPruningPointAndItsAnticoneBlocks,
}

impl From<&KaspadMessagePayload> for KaspadMessagePayloadType {
    fn from(payload: &KaspadMessagePayload) -> Self {
        match payload {
            KaspadMessagePayload::Addresses(_) => KaspadMessagePayloadType::Addresses,
            KaspadMessagePayload::Block(_) => KaspadMessagePayloadType::Block,
            KaspadMessagePayload::Transaction(_) => KaspadMessagePayloadType::Transaction,
            KaspadMessagePayload::BlockLocator(_) => KaspadMessagePayloadType::BlockLocator,
            KaspadMessagePayload::RequestAddresses(_) => KaspadMessagePayloadType::RequestAddresses,
            KaspadMessagePayload::RequestRelayBlocks(_) => KaspadMessagePayloadType::RequestRelayBlocks,
            KaspadMessagePayload::RequestTransactions(_) => KaspadMessagePayloadType::RequestTransactions,
            KaspadMessagePayload::IbdBlock(_) => KaspadMessagePayloadType::IbdBlock,
            KaspadMessagePayload::InvRelayBlock(_) => KaspadMessagePayloadType::InvRelayBlock,
            KaspadMessagePayload::InvTransactions(_) => KaspadMessagePayloadType::InvTransactions,
            KaspadMessagePayload::Ping(_) => KaspadMessagePayloadType::Ping,
            KaspadMessagePayload::Pong(_) => KaspadMessagePayloadType::Pong,
            KaspadMessagePayload::Verack(_) => KaspadMessagePayloadType::Verack,
            KaspadMessagePayload::Version(_) => KaspadMessagePayloadType::Version,
            KaspadMessagePayload::TransactionNotFound(_) => KaspadMessagePayloadType::TransactionNotFound,
            KaspadMessagePayload::Reject(_) => KaspadMessagePayloadType::Reject,
            KaspadMessagePayload::PruningPointUtxoSetChunk(_) => KaspadMessagePayloadType::PruningPointUtxoSetChunk,
            KaspadMessagePayload::RequestIbdBlocks(_) => KaspadMessagePayloadType::RequestIbdBlocks,
            KaspadMessagePayload::UnexpectedPruningPoint(_) => KaspadMessagePayloadType::UnexpectedPruningPoint,
            KaspadMessagePayload::IbdBlockLocator(_) => KaspadMessagePayloadType::IbdBlockLocator,
            KaspadMessagePayload::IbdBlockLocatorHighestHash(_) => KaspadMessagePayloadType::IbdBlockLocatorHighestHash,
            KaspadMessagePayload::RequestNextPruningPointUtxoSetChunk(_) => {
                KaspadMessagePayloadType::RequestNextPruningPointUtxoSetChunk
            }
            KaspadMessagePayload::DonePruningPointUtxoSetChunks(_) => KaspadMessagePayloadType::DonePruningPointUtxoSetChunks,
            KaspadMessagePayload::IbdBlockLocatorHighestHashNotFound(_) => {
                KaspadMessagePayloadType::IbdBlockLocatorHighestHashNotFound
            }
            KaspadMessagePayload::BlockWithTrustedData(_) => KaspadMessagePayloadType::BlockWithTrustedData,
            KaspadMessagePayload::DoneBlocksWithTrustedData(_) => KaspadMessagePayloadType::DoneBlocksWithTrustedData,
            KaspadMessagePayload::RequestPruningPointAndItsAnticone(_) => KaspadMessagePayloadType::RequestPruningPointAndItsAnticone,
            KaspadMessagePayload::BlockHeaders(_) => KaspadMessagePayloadType::BlockHeaders,
            KaspadMessagePayload::RequestNextHeaders(_) => KaspadMessagePayloadType::RequestNextHeaders,
            KaspadMessagePayload::DoneHeaders(_) => KaspadMessagePayloadType::DoneHeaders,
            KaspadMessagePayload::RequestPruningPointUtxoSet(_) => KaspadMessagePayloadType::RequestPruningPointUtxoSet,
            KaspadMessagePayload::RequestHeaders(_) => KaspadMessagePayloadType::RequestHeaders,
            KaspadMessagePayload::RequestBlockLocator(_) => KaspadMessagePayloadType::RequestBlockLocator,
            KaspadMessagePayload::PruningPoints(_) => KaspadMessagePayloadType::PruningPoints,
            KaspadMessagePayload::RequestPruningPointProof(_) => KaspadMessagePayloadType::RequestPruningPointProof,
            KaspadMessagePayload::PruningPointProof(_) => KaspadMessagePayloadType::PruningPointProof,
            KaspadMessagePayload::Ready(_) => KaspadMessagePayloadType::Ready,
            KaspadMessagePayload::BlockWithTrustedDataV4(_) => KaspadMessagePayloadType::BlockWithTrustedDataV4,
            KaspadMessagePayload::TrustedData(_) => KaspadMessagePayloadType::TrustedData,
            KaspadMessagePayload::RequestIbdChainBlockLocator(_) => KaspadMessagePayloadType::RequestIbdChainBlockLocator,
            KaspadMessagePayload::IbdChainBlockLocator(_) => KaspadMessagePayloadType::IbdChainBlockLocator,
            KaspadMessagePayload::RequestAntipast(_) => KaspadMessagePayloadType::RequestAntipast,
            KaspadMessagePayload::RequestNextPruningPointAndItsAnticoneBlocks(_) => {
                KaspadMessagePayloadType::RequestNextPruningPointAndItsAnticoneBlocks
            }
        }
    }
}
