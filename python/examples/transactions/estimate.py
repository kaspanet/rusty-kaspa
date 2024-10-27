import asyncio
from kaspa import Generator, PrivateKey, Resolver, RpcClient, kaspa_to_sompi

async def main():
    private_key = PrivateKey("389840d7696e89c38856a066175e8e92697f0cf182b854c883237a50acaf1f69")

    source_address = private_key.to_keypair().to_address("testnet")
    print(f'Source Address: {source_address.to_string()}')

    client = RpcClient(resolver=Resolver(), network_id="testnet-10")
    await client.connect()

    entries = await client.get_utxos_by_addresses({"addresses": [source_address]})
    
    generator = Generator(
        network_id="testnet-10",
        entries=entries["entries"],
        outputs=[{"address": source_address, "amount": kaspa_to_sompi(0.2)}],
        priority_fee=kaspa_to_sompi(0.0002),
        change_address=source_address
    )

    estimate = generator.estimate()
    print(estimate.final_transaction_id)

    await client.disconnect()

if __name__ == "__main__":
    asyncio.run(main())