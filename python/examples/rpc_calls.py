import asyncio

from kaspa import Resolver, RpcClient


async def main():
    client = RpcClient(resolver=Resolver(), network="testnet", network_suffix=11)
    await client.connect()

    ###
    # Get some sample data for request parameters
    ###
    block_dag_info_response = await client.get_block_dag_info()
    tip_hashes = block_dag_info_response["tipHashes"]
    
    block = await client.get_block(request={
        "hash": tip_hashes[0],
        "includeTransactions": True
    })
    
    addresses = []
    transaction_ids = []
    subnetwork_ids = set()
    for tx in block["block"]["transactions"]:
        transaction_ids.append(tx["verboseData"]["transactionId"])
        subnetwork_ids.add(tx["subnetworkId"])

        for output in tx["outputs"]:
            addresses.append(output["verboseData"]["scriptPublicKeyAddress"])
    addresses = list(set(addresses))

    ###
    # Sample requests
    ###
    await client.get_block_count()
        
    await client.get_block_dag_info()
        
    await client.get_coin_supply()
        
    await client.get_connected_peer_info()
        
    await client.get_info()
        
    await client.get_peer_addresses()
    
    await client.get_metrics(request={
        "processMetrics": True,
        "connectionMetrics": True,
        "bandwidthMetrics": True,
        "consensusMetrics": True,
        "storageMetrics": True,
        "customMetrics": True,
    })
    
    await client.get_connections(request={
        "includeProfileData": True
    })
        
    await client.get_sink()
        
    await client.get_sink_blue_score()
        
    await client.ping()
        
    # await client.shutdown()
        
    await client.get_server_info()
        
    await client.get_sync_status()
        
    # await client.add_peer(request=)
        
    # await client.ban(request=)
        
    await client.estimate_network_hashes_per_second(request={
        "windowSize": 1000, 
        "startHash": block_dag_info_response["tipHashes"][0]
    })

    await client.get_balance_by_address(request={
        "address": addresses[0]
    })
        
    await client.get_balances_by_addresses(request={
        "addresses": addresses
    })
        
    await client.get_block(request={
        "hash": block_dag_info_response["tipHashes"][0],
        "includeTransactions": True
    })
        
    await client.get_blocks(request={
        "lowHash": block_dag_info_response["pruningPointHash"],
        "includeBlocks": True,
        "includeTransactions": True,
    })
        
    await client.get_block_template(request={
        "payAddress": addresses[0],
        "extraData": list("my miner name is...".encode('utf-8'))
    })
        
    # await client.get_current_block_color(request={
    #     "hash": block_dag_info_response["pruningPointHash"]
    # })
        
    await client.get_daa_score_timestamp_estimate(request={
        "daaScores": [block_dag_info_response["virtualDaaScore"]]
    })
        
    await client.get_fee_estimate(request={})
        
    await client.get_fee_estimate_experimental(request={
        "verbose": True
    })
        
    await client.get_current_network(request={})
        
    # await client.get_headers(request={
    #     "startHash": block_dag_info_response["tipHashes"][0],
    #     "limit": 5,
    #     "isAscending": True
    # })
        
    mempool_entries = await client.get_mempool_entries(request={
        "includeOrphanPool": False,
        "includeOrphanPool": False,
        "filterTransactionPool": False,
    })
        
    await client.get_mempool_entries_by_addresses(request={
        "addresses": addresses,
        "includeOrphanPool": False,
        "filterTransactionPool": False,
    })

    if len(mempool_entries) > 0:
        try:
            await client.get_mempool_entry(request={
                "transactionId": mempool_entries["mempoolEntries"][0]["transaction"]["verboseData"]["transactionId"],
                "includeOrphanPool": False,
                "filterTransactionPool": False,
            })
        except Exception as e:
            print(e)

    # await client.get_subnetwork(request={
    #     "subnetworkId": list(subnetwork_ids)[0]
    # })
        
    await client.get_utxos_by_addresses(request={
        "addresses": addresses
    })

    await client.get_virtual_chain_from_block(request={
        "startHash": tip_hashes[0],
        "includeAcceptedTransactionIds": True
    })

    # await client.resolve_finality_conflict(request)

    # await client.submit_block(request)

    # await client.submit_transaction(request)

    # await client.submit_transaction_replacement(request)

    # await client.unban(request)

if __name__ == "__main__":
    asyncio.run(main())

