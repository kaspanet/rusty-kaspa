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
            // TODO: implement a matcher for each payload variant supporting request/response pairing
            Payload::GetPeerAddressesRequest(_) => true,
            Payload::GetSelectedTipHashRequest(_) => true,
            Payload::GetMempoolEntryRequest(_) => true,
            Payload::GetMempoolEntriesByAddressesRequest(_) => true,
            Payload::GetConnectedPeerInfoRequest(_) => true,
            Payload::AddPeerRequest(_) => true,
            Payload::SubmitTransactionRequest(_) => true,
            Payload::NotifyVirtualSelectedParentChainChangedRequest(_) => true,
            Payload::GetSubnetworkRequest(_) => true,
            Payload::GetVirtualSelectedParentChainFromBlockRequest(_) => true,
            Payload::GetVirtualSelectedParentBlueScoreRequest(_) => true,
            Payload::GetBlocksRequest(_) => true,
            Payload::GetBlockCountRequest(_) => true,
            Payload::GetBlockDagInfoRequest(_) => true,
            Payload::ResolveFinalityConflictRequest(_) => true,
            Payload::NotifyFinalityConflictRequest(_) => true,
            Payload::GetMempoolEntriesRequest(_) => true,
            Payload::ShutdownRequest(_) => true,
            Payload::GetHeadersRequest(_) => true,
            Payload::NotifyUtxosChangedRequest(_) => true,
            Payload::GetUtxosByAddressesRequest(_) => true,
            Payload::NotifyVirtualSelectedParentBlueScoreChangedRequest(_) => true,
            Payload::BanRequest(_) => true,
            Payload::UnbanRequest(_) => true,
            Payload::NotifyPruningPointUtxoSetOverrideRequest(_) => true,
            Payload::EstimateNetworkHashesPerSecondRequest(_) => true,
            Payload::NotifyVirtualDaaScoreChangedRequest(_) => true,
            Payload::GetBalanceByAddressRequest(_) => true,
            Payload::GetBalancesByAddressesRequest(_) => true,
            Payload::GetCoinSupplyRequest(_) => true,
            Payload::PingRequest(_) => true,
            Payload::GetProcessMetricsRequest(_) => true,

            // original entries:
            Payload::SubmitBlockRequest(_) => true,
            Payload::GetBlockTemplateRequest(_) => true,
            Payload::GetBlockRequest(ref request) => request.is_matching(response),
            Payload::GetCurrentNetworkRequest(_) => true,
            Payload::NotifyBlockAddedRequest(_) => true,
            Payload::GetInfoRequest(_) => true,
            Payload::NotifyNewBlockTemplateRequest(_) => true,

            Payload::StopNotifyingUtxosChangedRequest(_) => true,
            Payload::StopNotifyingPruningPointUtxoSetOverrideRequest(_) => true,
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
