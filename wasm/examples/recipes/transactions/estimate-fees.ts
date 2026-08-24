// Estimate the fee and mass for a transaction.
//
//   cd wasm/examples
//   npx tsx recipes/transactions/estimate-fees.ts
//
// A synthetic UTXO and a throwaway key let this run with no network and no
// funds. In a real app the entries come from rpc.getUtxosByAddresses().

import {
    Mnemonic,
    XPrv,
    PrivateKeyGenerator,
    Generator,
    payToAddressScript,
    kaspaToSompi,
    sompiToKaspaString,
    initConsolePanicHook,
} from 'kaspa';
import { getNetworkId } from '../../shared/network';

initConsolePanicHook();

const networkId = getNetworkId();

const xprv = new XPrv(Mnemonic.random().toSeed());
const privateKey = new PrivateKeyGenerator(xprv, false, 0n).receiveKey(0);
const address = privateKey.toAddress(networkId);

// One synthetic UTXO worth 500 KAS to spend from.
const entries = [{
    address,
    outpoint: { transactionId: '1b84324c701b16c1cfbbd713a5ff87edf78bc5c92a92866f86d7e32ab5cd387d', index: 0 },
    scriptPublicKey: payToAddressScript(address),
    amount: kaspaToSompi('500'),
    isCoinbase: true,
    blockDaaScore: 0n,
}];

const generator = new Generator({
    entries,
    outputs: [{ address: address.toString(), amount: kaspaToSompi('1.5') }],
    priorityFee: kaspaToSompi('0.0001'),
    changeAddress: address.toString(),
    networkId,
});

(async () => {
    // estimate() simulates building the transactions without submitting anything.
    const estimate = await generator.estimate();
    console.log('transactions:', estimate.transactions);
    console.log('mass:        ', estimate.mass);
    console.log('total fees:  ', sompiToKaspaString(estimate.fees));
})();
