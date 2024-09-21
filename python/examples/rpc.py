import asyncio
import json
import time
import os

from kaspa import RpcClient, Resolver


def subscription_callback(event, name, **kwargs):
    print(f"{name} | {event}")

async def rpc_subscriptions(client):
    # client.add_event_listener("all", subscription_callback, callback_id=1, kwarg1="Im a kwarg!!")
    client.add_event_listener("all", subscription_callback, name="all")

    await client.subscribe_virtual_daa_score_changed()
    await client.subscribe_virtual_chain_changed(True)
    await client.subscribe_block_added()
    await client.subscribe_new_block_template()

    await asyncio.sleep(5)

    client.remove_event_listener("all")
    print("Removed all event listeners. Sleeping for 5 seconds before unsubscribing. Should see nothing print.")

    await asyncio.sleep(5)

    await client.unsubscribe_virtual_daa_score_changed()
    await client.unsubscribe_virtual_chain_changed(True)
    await client.unsubscribe_block_added()
    await client.unsubscribe_new_block_template()


async def rpc_calls(client):
    get_server_info_response = await client.get_server_info_call()
    print(get_server_info_response)

    block_dag_info_response = await client.get_block_dag_info_call()
    print(block_dag_info_response)

    tip_hash = block_dag_info_response["tipHashes"][0]
    get_block_request = {"hash": tip_hash, "includeTransactions": True}
    get_block_response = await client.get_block_call(get_block_request)
    print(get_block_response)

    get_balances_by_addresses_request = {"addresses": ["kaspa:qqxn4k5dchwk3m207cmh9ewagzlwwvfesngkc8l90tj44mufcgmujpav8hakt", "kaspa:qr5ekyld6j4zn0ngennj9nx5gpt3254fzs77ygh6zzkvyy8scmp97de4ln8v5"]}
    get_balances_by_addresses_response =  await client.get_balances_by_addresses_call(get_balances_by_addresses_request)
    print(get_balances_by_addresses_response)

async def main():
    # rpc_host = os.environ.get("KASPA_RPC_HOST")
    # client = RpcClient(url=f"ws://{rpc_host}:17210")
    client = RpcClient(resolver=Resolver(), network="testnet", network_suffix=11)
    await client.connect()
    print(f"Client is connected: {client.is_connected}")

    await rpc_calls(client)
    await rpc_subscriptions(client)

    await client.disconnect()


if __name__ == "__main__":
    asyncio.run(main())