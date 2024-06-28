import asyncio
import json
import time

from kaspapy import RpcClient

async def server_info(client):
    sleep_for = 4
    await asyncio.sleep(sleep_for)

    server_info_json_str = await client.get_server_info()
    print(f'\nserver_info() slept for {sleep_for} seconds:\n{json.loads(server_info_json_str)}')

async def block_dag_info(client):
    sleep_for = 2
    await asyncio.sleep(sleep_for)

    block_dag_info_json_str = await client.get_block_dag_info()
    print(f'\nblock_dag_info() slept for {sleep_for} seconds:\n{json.loads(block_dag_info_json_str)}')

async def main():
    client = await RpcClient.connect(url = "ws://localhost:17110")
    print(f'client is connected: {client.is_connected()}')

    await asyncio.gather(
        server_info(client),
        block_dag_info(client)
    )

if __name__ == "__main__":
    start = time.time()

    asyncio.run(main())

    print(f'\ntotal execution time: {time.time() - start}')
