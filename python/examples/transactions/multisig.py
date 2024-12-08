# A very simple non-PSKT multisig example

import asyncio
from kaspa import (
    Mnemonic,
    Opcodes,
    RpcClient,
    Resolver,
    ScriptBuilder,
    SighashType,
    XPrv,
    address_from_script_public_key,
    create_multisig_address,
    create_transactions,
    kaspa_to_sompi
)

def derive(seed, account_index):
    xprv = XPrv(seed).derive_path(f"m/45'/111111'/{account_index}'")
    xpub = xprv.to_xpub()
    prv = xprv.derive_child(1).to_private_key()
    pub = xpub.derive_child(1).to_public_key()
    return prv, pub

async def main():
    seed = Mnemonic('predict cloud noise economy home stereo tag cancel adult pistol act remove equip cricket man summer neutral black art miracle foam world clown say').to_seed()

    prv1, pub1 = derive(seed, 0)
    print(f'Account 1:\n - prv: {prv1.to_string()}\n - pub: {pub1.to_string()}\n')

    prv2, pub2 = derive(seed, 1)
    print(f'Account 2:\n - prv: {prv2.to_string()}\n - pub: {pub2.to_string()}\n')

    prv3, pub3 = derive(seed, 2)
    print(f'Account 3:\n - prv: {prv3.to_string()}\n - pub: {pub3.to_string()}\n')

    # Multisig address creation - from script
    # schnorr
    redeem_script = ScriptBuilder()\
        .add_i64(2)\
        .add_data(pub1.to_x_only_public_key().to_string())\
        .add_data(pub2.to_x_only_public_key().to_string())\
        .add_data(pub3.to_x_only_public_key().to_string())\
        .add_i64(3)\
        .add_op(Opcodes.OpCheckMultiSig)
    spk = redeem_script.create_pay_to_script_hash_script()
    address = address_from_script_public_key(spk, network="testnet")
    print(f"Multisig Address: {address.to_string()}\n")
    
    # ECDSA
    # ecdsa_redeem_script = ScriptBuilder()\
    #     .add_i64(2)\
    #     .add_data(pub1.to_string())\
    #     .add_data(pub2.to_string())\
    #     .add_data(pub3.to_string())\
    #     .add_i64(3)\
    #     .add_op(Opcodes.OpCheckMultiSigECDSA)
    # ecdsa_spk = ecdsa_redeem_script.create_pay_to_script_hash_script()
    # ecdsa_address = address_from_script_public_key(ecdsa_spk, network="testnet")

    # Multisig address creation - from kaspa package
    assert address.to_string() == create_multisig_address(2, [pub1, pub2, pub3], 'testnet').to_string()
    # assert ecdsa_address.to_string() == create_multisig_address(2, [pub1, pub2, pub3], 'testnet', True).to_string()
    
    proceed = input("Send funds to address above before proceeding (enter 'y' to proceed): ")
    if proceed != 'y':
        return

    client = RpcClient(resolver=Resolver(), network_id='testnet-10')
    await client.connect(strategy='fallback')
    utxos = await client.get_utxos_by_addresses(request={'addresses': [address]})

    tx = create_transactions(
        entries=utxos['entries'],
        outputs=[{'address': 'kaspatest:prsajwtrefzex5wsmyk3rfkyzaq3wdwzczzr6jptgyzk4pacl9lzvtv8h30j9', 'amount': kaspa_to_sompi(1)}],
        change_address=address,
        priority_fee=kaspa_to_sompi(1),
        network_id='testnet-10',
        minimum_signatures=2,
        sig_op_count=3
    )

    for transaction in tx['transactions']:
        for idx, _ in enumerate(transaction.transaction.inputs):

            sig_1 = transaction.create_input_signature(idx, prv1)
            sig_2 = transaction.create_input_signature(idx, prv2)

            script_sig = ScriptBuilder()\
                .add_data(sig_1[2:])\
                .add_data(sig_2[2:])\
                .add_data(redeem_script.to_string())

            transaction.fill_input(idx, script_sig.to_string())

        print('tx id', await transaction.submit(client))

if __name__ == "__main__":
    asyncio.run(main())