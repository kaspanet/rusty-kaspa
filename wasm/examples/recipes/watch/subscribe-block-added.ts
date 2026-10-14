// Subscribe to new blocks as they are added to the DAG.
//
//   cd wasm/examples
//   npx tsx recipes/watch/subscribe-block-added.ts
//
// Ctrl-C to stop.

import { Resolver, RpcClient, initConsolePanicHook } from 'kaspa';
import { getNetworkId } from '../../shared/network';

initConsolePanicHook();

(async () => {
    const rpc = new RpcClient({ resolver: new Resolver(), networkId: getNetworkId() });

    rpc.addEventListener('block-added', (event) => {
        console.log('block:', event.data.block.header.hash);
    });

    rpc.addEventListener('connect', async () => {
        console.log('connected to', rpc.url);
        await rpc.subscribeBlockAdded();
    });

    await rpc.connect();

    process.on('SIGINT', async () => {
        await rpc.disconnect();
        process.exit(0);
    });
})();
