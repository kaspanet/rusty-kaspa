import asyncio
from kaspa import (
    Keypair,
    PrivateKey,
    RpcClient,
    Resolver,
    PaymentOutput,
    create_transaction,
    sign_transaction
)

async def main():
    private_key = PrivateKey("389840d7696e89c38856a066175e8e92697f0cf182b854c883237a50acaf1f69")
    keypair = private_key.to_keypair()
    address = keypair.to_address(network="kaspatest")
    print(address.to_string())

    client = RpcClient(resolver=Resolver(), network="testnet", network_suffix=10)
    await client.connect()
    print(f"Client is connected: {client.is_connected}")

    utxos = await client.get_utxos_by_addresses({"addresses": [address]})
    utxos = utxos["entries"]

    utxos = sorted(utxos, key=lambda x: x['utxoEntry']['amount'], reverse=True)
    total = sum(item['utxoEntry']['amount'] for item in utxos)

    fee_rates = await client.get_fee_estimate()
    fee = int(fee_rates["estimate"]["priorityBucket"]["feerate"])

    output_amount = int(total - fee)

    change_address = address
    outputs = [
        {"address": change_address, "amount": output_amount},
    ]

    tx = create_transaction(utxos, outputs, 0, None, 1)
    tx_signed = sign_transaction(tx, [private_key], True)

    print(await client.submit_transaction({
        "transaction": tx_signed,
        "allow_orphan": True
    }))

if __name__ == "__main__":
    asyncio.run(main())