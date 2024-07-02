import asyncio
import json
import time

from kaspapy import RpcClient


async def main():
    client = await RpcClient.connect(url = "ws://localhost:17110")
    print(f'Client is connected: {client.is_connected()}')

    get_server_info_response = await client.get_server_info()
    print(get_server_info_response)

    block_dag_info_response = await client.get_block_dag_info()
    print(block_dag_info_response)

    tip_hash = block_dag_info_response['tipHashes'][0]
    get_block_request = {'hash': tip_hash, 'includeTransactions': True}
    get_block_response = await client.get_block_call(get_block_request)
    print(get_block_response)

    get_balances_by_addresses_request = {'addresses': ['kaspa:qqxn4k5dchwk3m207cmh9ewagzlwwvfesngkc8l90tj44mufcgmujpav8hakt', 'kaspa:qr5ekyld6j4zn0ngennj9nx5gpt3254fzs77ygh6zzkvyy8scmp97de4ln8v5']}
    get_balances_by_addresses_response =  await client.get_balances_by_addresses_call(get_balances_by_addresses_request)
    print(get_balances_by_addresses_response)


if __name__ == "__main__":
    asyncio.run(main())