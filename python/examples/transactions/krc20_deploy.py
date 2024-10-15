import asyncio
import json 

from kaspa import (
    Opcodes, 
    PrivateKey,
    Resolver,
    RpcClient,
    ScriptBuilder,
    address_from_script_public_key,
    create_transaction,
    sign_transaction
)

async def main():
    private_key = PrivateKey("389840d7696e89c38856a066175e8e92697f0cf182b854c883237a50acaf1f69")
    keypair = private_key.to_keypair()
    address = keypair.to_address("kaspatest")

    ######################
    # Commit tx

    data = {
        "p": "krc-20",
        "op": "deploy",
        "tick": "PYSDK",
        "max": "112121115100107",
        "lim":" 1000",
    }

    script = ScriptBuilder()
    script.add_data(keypair.public_key)
    script.add_op(Opcodes.OpCheckSig)
    script.add_op(Opcodes.OpFalse)
    script.add_op(Opcodes.OpIf)
    script.add_data(b"kasplex")
    script.add_i64(0)
    script.add_data(json.dumps(data, separators=(',', ':')).encode('utf-8'))
    script.add_op(Opcodes.OpEndIf)
    
    print(script.to_string())
    
    p2sh_address = address_from_script_public_key(script.create_pay_to_script_hash_script(), "kaspatest")
    print(p2sh_address.to_string())

    # TODO tx submission

    ######################
    # Reveal tx
    # TODO

if __name__ == "__main__":
    asyncio.run(main())