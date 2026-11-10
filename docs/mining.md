# Mining Aero

Aero uses **RandomX** proof of work (CPU-friendly) at 4 blocks per second. The block reward is 0.1 ARO, halving
every 500,000,000 blocks, with a 100,000,000 ARO hard cap and no premine.

## Solo mining with `aero-miner`

1. Run a synced node with RPC enabled (default) and note its gRPC address:

   ```bash
   aerod --testnet --netsuffix=13 --addpeer=<seed_ip>:16711
   ```

2. Create an address with the wallet (`aero-wallet`; prefix `aero` on mainnet, `aerotest` on testnets).

3. Start the miner:

   ```bash
   aero-miner -s 127.0.0.1:16210 -a <your-address>
   ```

   | Flag | Meaning |
   |------|---------|
   | `-s, --rpcserver` | gRPC `host:port` of your node (mainnet default 16110, testnet 16210) |
   | `-a, --mining-address` | address that receives rewards |
   | `-t, --threads` | mining threads (default: all CPUs) |
   | `--light` | RandomX light mode (256 MiB): far slower, for machines with under ~2.5 GiB free RAM |
   | `--mine-unsynced` | mine even if the node reports it is not synced (only for the first node of a new network) |

Fast mode builds a ~2 GiB dataset at startup, so expect a pause before the first hash.

## Pool / many miners: `stratum-bridge`

`stratum-bridge` (in `bridge/`) exposes a Stratum endpoint in front of your node so many miners can connect to one
node. See `bridge/docs/README.md` for configuration.

## Starting a new network from genesis

The first node must run with `--enable-unsynced-mining`, and the first miner with `--mine-unsynced`. Difficulty
starts very low and adjusts as hash rate arrives, so early blocks will come in bursts until it settles. Early
networks have little hash power and are cheap to 51%-attack; do not treat early testnet/mainnet blocks as final.
