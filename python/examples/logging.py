import asyncio
import logging
import kaspa

FORMAT = '%(levelname)s %(name)s %(asctime)-15s %(filename)s:%(lineno)d %(message)s'
logging.basicConfig(format=FORMAT)
logging.getLogger().setLevel(logging.DEBUG)

async def main():
    client = kaspa.RpcClient(resolver=kaspa.Resolver())
    await client.connect()
    print(await client.get_info())

if __name__ == "__main__":
    asyncio.run(main())