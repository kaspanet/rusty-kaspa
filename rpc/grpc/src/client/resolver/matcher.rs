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
        match self {
            kaspad_request::Payload::GetBlockRequest(ref request) => request.is_matching(response),
            kaspad_request::Payload::GetCurrentNetworkRequest(_) => true,
            kaspad_request::Payload::NotifyBlockAddedRequest(_) => true,
            kaspad_request::Payload::GetInfoRequest(_) => true,
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
