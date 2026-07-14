// Look up the balance and UTXOs of one or more addresses.
//
//   cd wasm/examples
//   npx tsx recipes/rpc/get-balance.ts --address=kaspatest:...
//   npx tsx recipes/rpc/get-balance.ts --network=mainnet --address=kaspa:...

import { Resolver, RpcClient, sompiToKaspaString, initConsolePanicHook } from 'kaspa';
import { getNetworkId } from '../../shared/network';

initConsolePanicHook();

(async () => {
    const addressFlag = process.argv.slice(2).find((a) => a.startsWith('--address='));
    const address = addressFlag && addressFlag.split('=')[1];
    if (!address) {
        console.error('Pass an address: --address=kaspatest:...');
        process.exit(1);
    }

    const rpc = new RpcClient({ resolver: new Resolver(), networkId: getNetworkId() });
    await rpc.connect();

    const { entries } = await rpc.getBalancesByAddresses({ addresses: [address] });
    for (const entry of entries) {
        console.log(`${entry.address}: ${sompiToKaspaString(entry.balance)}`);
    }

    const utxos = await rpc.getUtxosByAddresses([address]);
    console.log(`${utxos.entries.length} UTXO(s)`);

    await rpc.disconnect();
})();
