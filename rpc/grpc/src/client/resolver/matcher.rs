use crate::protowire::{
    self, kaspad_request, kaspad_response, GetBlockRequestMessage, GetBlockResponseMessage, KaspadRequest, KaspadResponse,
};

pub(crate) trait Matcher<T> {
    fn is_matching(&self, response: T) -> bool;
}

impl Matcher<&GetBlockResponseMessage> for GetBlockRequestMessage {
    fn is_matching(&self, response: &protowire::GetBlockResponseMessage) -> bool {
        if let Some(block) = response.block.as_ref() {
            if let Some(verbose_data) = block.verbose_data.as_ref() {
                return verbose_data.hash == self.hash;
            }
        } else if let Some(error) = response.error.as_ref() {
            // the response error message should contain the requested hash
            return error.message.contains(self.hash.as_str());
        }
        false
    }
}

impl Matcher<&kaspad_response::Payload> for GetBlockRequestMessage {
    fn is_matching(&self, response: &kaspad_response::Payload) -> bool {
        if let kaspad_response::Payload::GetBlockResponse(ref response) = response {
            return self.is_matching(response);
        }
        false
    }
}

impl Matcher<&kaspad_response::Payload> for kaspad_request::Payload {
    fn is_matching(&self, response: &kaspad_response::Payload) -> bool {
        use kaspad_request::Payload;
        match self {
            // FIXME - handle is_matching!
            Payload::GetPeerAddressesRequest(_) => todo!(),
            Payload::GetSelectedTipHashRequest(_) => todo!(),
            Payload::GetMempoolEntryRequest(_) => todo!(),
            Payload::GetMempoolEntriesByAddressesRequest(_) => todo!(),
            Payload::GetConnectedPeerInfoRequest(_) => todo!(),
            Payload::AddPeerRequest(_) => todo!(),
            Payload::SubmitTransactionRequest(_) => todo!(),
            Payload::NotifyVirtualSelectedParentChainChangedRequest(_) => todo!(),
            Payload::GetSubnetworkRequest(_) => todo!(),
            Payload::GetVirtualSelectedParentChainFromBlockRequest(_) => todo!(),
            Payload::GetVirtualSelectedParentBlueScoreRequest(_) => todo!(),
            Payload::GetBlocksRequest(_) => todo!(),
            Payload::GetBlockCountRequest(_) => todo!(),
            Payload::GetBlockDagInfoRequest(_) => todo!(),
            Payload::ResolveFinalityConflictRequest(_) => todo!(),
            Payload::NotifyFinalityConflictsRequest(_) => todo!(),
            Payload::GetMempoolEntriesRequest(_) => todo!(),
            Payload::ShutdownRequest(_) => todo!(),
            Payload::GetHeadersRequest(_) => todo!(),
            Payload::NotifyUtxosChangedRequest(_) => todo!(),
            Payload::GetUtxosByAddressesRequest(_) => todo!(),
            Payload::NotifyVirtualSelectedParentBlueScoreChangedRequest(_) => todo!(),
            Payload::BanRequest(_) => todo!(),
            Payload::UnbanRequest(_) => todo!(),
            Payload::NotifyPruningPointUtxoSetOverrideRequest(_) => todo!(),
            Payload::EstimateNetworkHashesPerSecondRequest(_) => todo!(),
            Payload::NotifyVirtualDaaScoreChangedRequest(_) => todo!(),
            Payload::GetBalanceByAddressRequest(_) => todo!(),
            Payload::GetBalancesByAddressesRequest(_) => todo!(),
            Payload::GetCoinSupplyRequest(_) => todo!(),
            // Payload::(_) => true,

            // original entries:
            Payload::SubmitBlockRequest(_) => true,
            Payload::GetBlockTemplateRequest(_) => true,
            Payload::GetBlockRequest(ref request) => request.is_matching(response),
            Payload::GetCurrentNetworkRequest(_) => true,
            Payload::NotifyBlockAddedRequest(_) => true,
            Payload::GetInfoRequest(_) => true,
            Payload::NotifyNewBlockTemplateRequest(_) => true,
            // _ => panic!("MATCHER PAYLOAD NO MATCH"),
        }
    }
}

impl Matcher<&KaspadResponse> for KaspadRequest {
    fn is_matching(&self, response: &KaspadResponse) -> bool {
        if let Some(ref response) = response.payload {
            if let Some(ref request) = self.payload {
                return request.is_matching(response);
            }
        }
        false
    }
}
