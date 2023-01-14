use kaspa_workflow_rpc_server::rpc_server_task;
use kaspa_workflow_rpc_server::result::Result;

#[tokio::main]
async fn main() -> Result<()> {

    let addr = "127.0.0.1:9292";
    rpc_server_task(addr).await?;

    Ok(())
}
