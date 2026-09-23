// A sampler of read-only node queries over one connection.
//
//   cd wasm/examples
//   npx tsx recipes/rpc/query-the-node.ts
//   npx tsx recipes/rpc/query-the-node.ts --network=mainnet

import { Resolver, RpcClient, initConsolePanicHook } from 'kaspa';
import { getNetworkId } from '../../shared/network';

initConsolePanicHook();

(async () => {
    const rpc = new RpcClient({ resolver: new Resolver(), networkId: getNetworkId() });
    await rpc.connect();
    console.log('connected to', rpc.url);

    // Node identity and sync state.
    const server = await rpc.getServerInfo();
    console.log('synced:', server.isSynced, '| version:', server.serverVersion);

    // Where the DAG currently is.
    const dag = await rpc.getBlockDagInfo();
    console.log('virtualDaaScore:', dag.virtualDaaScore, '| tips:', dag.tipHashes.length);

    // What's waiting to be mined.
    const mempool = await rpc.getMempoolEntries({ includeOrphanPool: true, filterTransactionPool: false });
    console.log('mempool transactions:', mempool.mempoolEntries.length);

    await rpc.disconnect();
})();
