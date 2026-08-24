// Build a transaction and serialize it to a plain object.
//
//   cd wasm/examples
//   npx tsx recipes/transactions/serialize-a-transaction.ts

import {
    Mnemonic,
    XPrv,
    PrivateKeyGenerator,
    payToAddressScript,
    createTransactions,
    kaspaToSompi,
    initConsolePanicHook,
} from 'kaspa';
import { getNetworkId } from '../../shared/network';

initConsolePanicHook();

const networkId = getNetworkId();

const xprv = new XPrv(Mnemonic.random().toSeed());
const privateKey = new PrivateKeyGenerator(xprv, false, 0n).receiveKey(1);
const address = privateKey.toAddress(networkId);

const entries = [{
    address,
    outpoint: { transactionId: '1b84324c701b16c1cfbbd713a5ff87edf78bc5c92a92866f86d7e32ab5cd387d', index: 0 },
    scriptPublicKey: payToAddressScript(address),
    amount: kaspaToSompi('500'),
    isCoinbase: true,
    blockDaaScore: 0n,
}];

(async () => {
    const { transactions } = await createTransactions({
        entries,
        outputs: [{ address: address.toString(), amount: kaspaToSompi('4') }],
        changeAddress: address.toString(),
        priorityFee: 0n,
        networkId,
    });

    for (const pending of transactions) {
        // serializeToSafeJSON() -> a JSON string (BigInts encoded as strings)
        // you can store or hand to another service for signing/submission.
        console.log(pending.serializeToSafeJSON());
    }
})();
