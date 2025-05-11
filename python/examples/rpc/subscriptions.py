import asyncio

from kaspa import RpcClient, Resolver


def subscription_callback(event, name, **kwargs):
    # print(event['nonexistent key'])
    
    # try:
    #     print(event['nonexistent key'])
    # except KeyError:
    #     print('caught key error exception')

    print(f"{name} | {event}")

def block_added_handler(event):
    print(f"block_added_handler: {event}")

async def rpc_subscriptions(client: RpcClient):
    # client.add_event_listener("all", subscription_callback, callback_id=1, kwarg1="Im a kwarg!!")
    client.add_event_listener("all", subscription_callback, name="all")
    client.add_event_listener("block-added", block_added_handler)

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

async def main():
    client = RpcClient(resolver=Resolver(), network_id="mainnet")
    
    await client.connect()
    print(f"Client is connected: {client.is_connected}")

    await rpc_subscriptions(client)
    await client.disconnect()


if __name__ == "__main__":
    asyncio.run(main())