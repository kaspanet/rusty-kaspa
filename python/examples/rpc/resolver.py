import asyncio
from kaspa import Resolver, RpcClient

async def main():
    resolver = Resolver()

    # Connect to mainnet PNN
    client = RpcClient(resolver=resolver)
    await client.connect()
    print(f'client connected to {await client.get_current_network()}')
    await client.disconnect()

    client.set_network_id("testnet-10")
    await client.connect()
    print(f'client connected to {await client.get_current_network()}')
    await client.disconnect()

    client.set_network_id("testnet-11")
    await client.connect()
    print(f'client connected to {await client.get_current_network()}')
    await client.disconnect()

if __name__ == "__main__":
    asyncio.run(main())

    