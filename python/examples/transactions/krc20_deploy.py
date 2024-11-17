import asyncio
import json 

from kaspa import (
    Opcodes, 
    PrivateKey,
    Resolver,
    RpcClient,
    ScriptBuilder,
    address_from_script_public_key,
    create_transactions,
)


async def main():
    client = RpcClient(resolver=Resolver(), network_id='testnet-10')
    await client.connect()

    private_key = PrivateKey('389840d7696e89c38856a066175e8e92697f0cf182b854c883237a50acaf1f69')
    public_key = private_key.to_public_key()
    address = public_key.to_address('testnet')
    print(f'Address: {address.to_string()}')
    print(f'XOnly Pub Key: {public_key.to_x_only_public_key().to_string()}')

    ######################
    # Commit tx

    data = {
        'p': 'krc-20',
        'op': 'deploy',
        'tick': 'TPYSDK',
        'max': '112121115100107',
        'lim': '1000',
    }

    script = ScriptBuilder()\
        .add_data(public_key.to_x_only_public_key().to_string())\
        .add_op(Opcodes.OpCheckSig)\
        .add_op(Opcodes.OpFalse)\
        .add_op(Opcodes.OpIf)\
        .add_data(b'kasplex')\
        .add_i64(0)\
        .add_data(json.dumps(data, separators=(',', ':')).encode('utf-8'))\
        .add_op(Opcodes.OpEndIf)
    print(f'Script: {script.to_string()}')
    
    p2sh_address = address_from_script_public_key(script.create_pay_to_script_hash_script(), 'testnet')
    print(f'P2SH Address: {p2sh_address.to_string()}')

    utxos = await client.get_utxos_by_addresses(request={'addresses': [address]})

    commit_txs = create_transactions(
        priority_entries=[],
        entries=utxos["entries"],
        outputs=[{ 'address': p2sh_address.to_string(), 'amount':  1 * 100_000_000 }],
        change_address=address,
        priority_fee=1 * 100_000_000,
        network_id='testnet-10'
    )

    commit_tx_id = None
    for transaction in commit_txs['transactions']:
        transaction.sign([private_key], False)
        commit_tx_id = await transaction.submit(client)
        print('Commit TX ID:', commit_tx_id)
    
    await asyncio.sleep(10)

    #####################
    # Reveal tx

    utxos = await client.get_utxos_by_addresses(request={'addresses': [address]})
    reveal_utxos = await client.get_utxos_by_addresses(request={'addresses': [p2sh_address]})

    for entry in reveal_utxos['entries']:
        if entry['outpoint']['transactionId'] == commit_tx_id:
            reveal_utxos = entry

    reveal_txs = create_transactions(
        priority_entries=[reveal_utxos],
        entries=utxos['entries'],
        outputs=[],
        change_address=address,
        priority_fee=1005 * 100_000_000,
        network_id='testnet-10'
    )

    for transaction in reveal_txs['transactions']:
        transaction.sign([private_key], False)

        commit_output = next((i for i, input in enumerate(transaction.transaction.inputs)
                            if input.signature_script == ''), None)
        
        if commit_output is not None:
            sig = transaction.create_input_signature(commit_output, private_key)
            transaction.fill_input(commit_output, script.encode_pay_to_script_hash_signature_script(sig))
        
        print('Reveal TX ID:', await transaction.submit(client))

if __name__ == '__main__':
    asyncio.run(main())
