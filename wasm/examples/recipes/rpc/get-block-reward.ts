// Look up the block reward (coinbase subsidy + fees) for a single block.
//
//   cd wasm/examples
//   npx tsx recipes/rpc/get-block-reward.ts
//   npx tsx recipes/rpc/get-block-reward.ts --network=mainnet

import { Resolver, RpcClient, sompiToKaspaString, initConsolePanicHook } from 'kaspa';
import { getNetworkId } from '../../shared/network';

initConsolePanicHook();

(async () => {
    const rpc = new RpcClient({ resolver: new Resolver(), networkId: getNetworkId() });
    await rpc.connect();
    console.log('connected to', rpc.url);

    // Reward info is keyed by block hash. The sink (chain tip) has no reward yet,
    // so walk back the selected-parent chain to a settled, confirmed block.
    let hash = (await rpc.getBlockDagInfo()).sink;
    for (let i = 0; i < 10; i++) {
        const { block } = await rpc.getBlock({ hash, includeTransactions: false });
        hash = block.verboseData.selectedParentHash;
    }
    console.log('querying reward for block:', hash);

    const info = await rpc.getBlockRewardInfo({ hash });
    console.log('blockColor:', info.blockColor, '| confirmations:', info.confirmationCount);
    if (info.rewardAmount != null) {
        console.log('reward:', sompiToKaspaString(info.rewardAmount), '| sompi:', info.rewardAmount);
    }

    await rpc.disconnect();
})();
