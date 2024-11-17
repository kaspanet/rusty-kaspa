import asyncio
from kaspa import (
    Generator,
    PrivateKey,
    PublicKey,
    Resolver,
    RpcClient,
    kaspa_to_sompi
)

async def main():
    private_key = PrivateKey("389840d7696e89c38856a066175e8e92697f0cf182b854c883237a50acaf1f69")
    source_address = private_key.to_keypair().to_address("testnet")
    print(f'Source Address: {source_address.to_string()}')

    client = RpcClient(resolver=Resolver(), network_id="testnet-10")
    await client.connect()

    entries = await client.get_utxos_by_addresses({"addresses": [source_address]})
    entries = entries["entries"]

    entries = sorted(entries, key=lambda x: x['utxoEntry']['amount'], reverse=True)
    total = sum(item['utxoEntry']['amount'] for item in entries)

    generator = Generator(
        network_id="testnet-10",
        entries=entries,
        outputs=[
            {"address": source_address, "amount": kaspa_to_sompi(10)},
            {"address": source_address, "amount": kaspa_to_sompi(10)},
            {"address": source_address, "amount": kaspa_to_sompi(10)}
        ],
        change_address=source_address,
        priority_fee=kaspa_to_sompi(10),
    )

    for pending_tx in generator:
        print(pending_tx.sign([private_key]))
        tx_id = await pending_tx.submit(client)
        print(tx_id)

    print(generator.summary().transactions)

if __name__ == "__main__":
    asyncio.run(main())