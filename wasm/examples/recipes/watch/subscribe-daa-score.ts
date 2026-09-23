// Subscribe to the virtual DAA score, which advances roughly once per second.
//
//   cd wasm/examples
//   npx tsx recipes/watch/subscribe-daa-score.ts
//
// Ctrl-C to stop.

import { Resolver, RpcClient, initConsolePanicHook } from 'kaspa';
import { getNetworkId } from '../../shared/network';

initConsolePanicHook();

(async () => {
    const rpc = new RpcClient({ resolver: new Resolver(), networkId: getNetworkId() });

    rpc.addEventListener('virtual-daa-score-changed', (event) => {
        console.log('DAA score:', event.data.virtualDaaScore);
    });

    // Subscribe only after the connection is established.
    rpc.addEventListener('connect', async () => {
        console.log('connected to', rpc.url);
        await rpc.subscribeVirtualDaaScoreChanged();
    });

    await rpc.connect();

    process.on('SIGINT', async () => {
        await rpc.disconnect();
        process.exit(0);
    });
})();
