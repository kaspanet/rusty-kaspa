// Follow the virtual selected parent chain (VSPC) from the node's sink,
// printing added/removed chain blocks and their accepted transactions.
//
//   cd wasm/examples
//   npx tsx recipes/rpc/get-virtual-chain.ts
//   npx tsx recipes/rpc/get-virtual-chain.ts --network=mainnet
//
// Ctrl-C to stop.

import { Resolver, RpcClient, initConsolePanicHook } from 'kaspa';
import { getNetworkId } from '../../shared/network';

initConsolePanicHook();

const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

(async () => {
    const rpc = new RpcClient({ resolver: new Resolver(), networkId: getNetworkId() });
    await rpc.connect();
    console.log('connected to', rpc.url);

    // Start from the node's current sink (tip of the selected chain).
    let startHash = (await rpc.getBlockDagInfo()).sink;
    console.log('starting from sink:', startHash);

    while (true) {
        const vspc = await rpc.getVirtualChainFromBlockV2({
            startHash,
            minConfirmationCount: 10,
            dataVerbosityLevel: 'High',
        });

        for (const hash of vspc.removedChainBlockHashes) console.log('- removed', hash);
        for (const hash of vspc.addedChainBlockHashes) {
            console.log('+ added  ', hash);
            startHash = hash; // advance so the next poll only returns new blocks
        }

        await delay(10000);
    }
})();
