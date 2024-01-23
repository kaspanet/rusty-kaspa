use crate::imports::*;
use crate::result::Result;
use crate::tests::RpcCoreMock;
use crate::tx::generator::test::*;
use crate::tx::*;
use crate::utils::*;
use crate::utxo::*;

#[tokio::test]
async fn test_utxo_subsystem_bootstrap() -> Result<()> {
    let network_id = NetworkId::with_suffix(NetworkType::Testnet, 10);
    let rpc_api_mock = Arc::new(RpcCoreMock::new());
    let processor = UtxoProcessor::new(Some(rpc_api_mock.clone().into()), Some(network_id), None, None);
    let _context = UtxoContext::new(&processor, UtxoContextBinding::default());

    processor.mock_set_connected(true);
    processor.handle_daa_score_change(1).await?;
    // println!("daa score: {:?}", processor.current_daa_score());
    // context.register_addresses(&[output_address(network_id.into())]).await?;
    Ok(())
}

#[test]
fn test_utxo_generator_empty_utxo_noop() -> Result<()> {
    let network_id = NetworkId::with_suffix(NetworkType::Testnet, 11);
    let output_address = output_address(network_id.into());

    let payment_output = PaymentOutput::new(output_address, kaspa_to_sompi(2.0));
    let generator = make_generator(network_id, &[10.0], &[], Fees::SenderPays(0), change_address, payment_output.into()).unwrap();
    let _tx = generator.generate_transaction().unwrap();
    // println!("tx: {:?}", tx);
    // assert!(tx.is_none());
    Ok(())
}
