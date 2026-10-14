// Connect to a public Kaspa node through the public resolver.
//
//   cd wasm/examples
//   npx tsx recipes/rpc/connect-with-resolver.ts
//   npx tsx recipes/rpc/connect-with-resolver.ts --network=testnet-10

import { Resolver, RpcClient, initConsolePanicHook } from 'kaspa';
import { getNetworkId } from '../../shared/network';

initConsolePanicHook();

(async () => {
    // The resolver finds a healthy public node for the chosen network.
    const rpc = new RpcClient({ resolver: new Resolver(), networkId: getNetworkId() });

    await rpc.connect();
    console.log('connected to', rpc.url);

    const info = await rpc.getBlockDagInfo();
    console.log('virtualDaaScore:', info.virtualDaaScore);
    console.log('tipHashes:', info.tipHashes.length);

    await rpc.disconnect();
})();
