# 🐳 Rusty Kaspa Docker Setup

Quick Docker setup for running Rusty Kaspa nodes.

## 🚀 Quick Start

```bash
# Copy environment configuration
cp env.example .env

# Start mainnet node
./docker-start.sh mainnet

# Start testnet node
./docker-start.sh testnet

# Start with monitoring
./docker-start.sh -m mainnet
```

## 📋 Available Profiles

| Profile      | Description                  | Services                               |
| ------------ | ---------------------------- | -------------------------------------- |
| `mainnet`    | Mainnet node with wRPC proxy | `kaspad-mainnet`, `wrpc-proxy-mainnet` |
| `testnet`    | Testnet node with wRPC proxy | `kaspad-testnet`, `wrpc-proxy-testnet` |
| `devnet`     | Devnet node                  | `kaspad-devnet`                        |
| `monitoring` | Monitoring stack             | `prometheus`, `grafana`                |

## 🔧 Usage Examples

### Start Script (Recommended)

```bash
# Start mainnet
./docker-start.sh mainnet

# Start testnet in background
./docker-start.sh -d testnet

# Build and start with monitoring
./docker-start.sh -b -m mainnet

# Clean up
./docker-start.sh -c
```

### Docker Compose Direct

```bash
# Start mainnet profile
docker compose --profile mainnet up -d

# Start multiple profiles
docker compose --profile mainnet,monitoring up -d

# View logs
docker compose --profile mainnet logs -f
```

## 🌐 Ports

| Network    | gRPC  | wRPC Borsh | wRPC JSON | P2P   |
| ---------- | ----- | ---------- | --------- | ----- |
| Mainnet    | 16110 | 17110      | 18110     | 16111 |
| Testnet    | 16210 | 17210      | 18210     | 16211 |
| Devnet     | 16610 | 17610      | 18610     | 16611 |
| Monitoring | -     | -          | -         | -     |
| Prometheus | 9090  | -          | -         | -     |
| Grafana    | 3000  | -          | -         | -     |

## 📊 Monitoring

Access monitoring dashboards:

- **Prometheus**: http://localhost:9090
- **Grafana**: http://localhost:3000 (admin/admin)

## 🔍 CLI Access

```bash
# Connect to mainnet
kaspa-cli --server localhost:16110 ping

# Connect to testnet
kaspa-cli --server localhost:16210 ping

# Using Docker exec
docker-compose exec kaspad-mainnet kaspa-cli --server localhost:16110 ping
```

## 📁 Data Persistence

Data is stored in Docker volumes:

- `kaspad_mainnet_data` - Mainnet blockchain data
- `kaspad_testnet_data` - Testnet blockchain data
- `kaspad_devnet_data` - Devnet blockchain data

## ⚙️ Configuration

Edit `.env` file to customize:

- `LOG_LEVEL` - Logging level
- `ENABLE_UTXO_INDEX` - Enable UTXO indexing
- `ENABLE_PERF_METRICS` - Enable performance metrics
- `OUTBOUND_TARGET` - Target outbound peers
- `INBOUND_LIMIT` - Maximum inbound peers

## 🛠️ Troubleshooting

```bash
# Check status
./docker-start.sh --help

# View logs
docker-compose --profile mainnet logs -f

# Restart services
docker-compose --profile mainnet restart

# Reset data
./docker-start.sh -c
docker-compose --profile mainnet up -d
```

## 📚 More Information

See [DOCKER.md](DOCKER.md) for detailed documentation.
