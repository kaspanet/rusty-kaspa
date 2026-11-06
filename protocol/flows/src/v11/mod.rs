use crate::{flow_context::FlowContext, flow_trait::Flow, v10};
use kaspa_p2p_lib::Router;
use std::sync::Arc;

pub fn register(ctx: FlowContext, router: Arc<Router>) -> Vec<Box<dyn Flow>> {
    v10::register_flows(ctx, router, true)
}
