import { NetworkId } from 'kaspa';

/**
 * Resolve the network an example runs against.
 *
 * Resolution order: `--network=<id>` flag, then `KASPA_NETWORK`, then the
 * `testnet-10` default.
 */
export function getNetworkId(): NetworkId {
    const flag = process.argv.slice(2).find((a) => a.startsWith('--network='));
    const id = (flag && flag.split('=')[1]) || process.env.KASPA_NETWORK || 'testnet-10';
    return new NetworkId(id);
}
