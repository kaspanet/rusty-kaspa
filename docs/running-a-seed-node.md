# Running a seed node

A public network needs a few always-on nodes that new nodes can find. Until a DNS seeder exists, new nodes
can only discover peers through `--addpeer` / `--connect`, so run at least 2–3 seed nodes on different hosts.

## 1. Run the node

Requirements: a host with a static public IP, a few GB of RAM, and the P2P port open to inbound TCP.

| Network      | Flag                         | P2P port | RPC port |
|--------------|------------------------------|----------|----------|
| mainnet      | (none)                       | 16111    | 16110    |
| testnet 13   | `--testnet --netsuffix=13`   | 16711    | 16210    |

```bash
# testnet 13 example
aerod --testnet --netsuffix=13 \
      --listen=0.0.0.0:16711 \
      --externalip=<PUBLIC_IP>:16711 \
      --utxoindex --maxinpeers=400
```

Notes:

- The **very first** node of a brand-new network must be started with `--enable-unsynced-mining`, otherwise it
  refuses to mine because it has no peers and thinks it is not synced.
- Open the P2P port in the firewall and cloud security group. Do not expose RPC (16110/16210) publicly unless you
  also pass `--saferpc` and understand the exposure.
- Seed nodes should stay up and keep a stable IP. Use `--archival` only if you also want to serve full history.
- Use the Docker image (`docker/Dockerfile.aerod`) or a systemd unit with `Restart=always`.

## 2. Let others find it

- **Now:** publish the `<ip>:<port>` of each seed so people can start with `aerod --addpeer=<ip>:<port>`.
- **Soon:** run a DNS seeder (a service that crawls the network and answers A/AAAA queries with live peer IPs) on a
  domain you control, then list the seeder hostname(s) in `dns_seeders` for the network in
  `consensus/core/src/config/params.rs`, for example:

  ```rust
  dns_seeders: &["seed1.example-aero.org", "seed2.example-aero.org"],
  ```

  Rebuild and release a new version. Nodes try these automatically at startup unless `--nodnsseed` is passed;
  `--dnsseed=<host>` overrides the list for a single run.

## 3. Check it works

From a *different* machine, with a fresh data dir:

```bash
aerod --testnet --netsuffix=13 --addpeer=<seed_ip>:16711 --appdir=/tmp/aero-fresh
```

It should connect, finish IBD (initial block download) and report as synced. Test this on a real remote machine, not just localhost.
