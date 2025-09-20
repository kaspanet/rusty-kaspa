import asyncio
from kaspa import Resolver, RpcClient

async def main():
    client = RpcClient(resolver=Resolver())
    await client.connect()

    balances = await client.get_balances_by_addresses(request={
        "addresses": ["kaspa:qpamkvhgh0kzx50gwvvp5xs8ktmqutcy3dfs9dc3w7lm9rq0zs76vf959mmrp"]
    })

    print(balances)

    await client.disconnect()

if __name__ == "__main__":
    asyncio.run(main())

