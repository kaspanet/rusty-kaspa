# Rusty Kaspa Docker Setup

This directory contains Docker configuration files for running Rusty Kaspa nodes in containers.

## Quick Start

1. **Copy the environment file:**

   ```bash
   cp env.example .env
   ```

2. **Start a mainnet node:**

   ```bash
   ./docker-start.sh mainnet
   ```

3. **Start a testnet node:**
   ```bash
   ./docker-start.sh testnet
   ```

## Services Overview

### Network Profiles

- **`mainnet`**: Mainnet Kaspa node with wRPC proxy
- **`testnet`**: Testnet Kaspa node with wRPC proxy
- **`devnet`**: Devnet Kaspa node
- **`monitoring`**: Prometheus and Grafana monitoring stack

### Core Services (by profile)

#### Mainnet Profile

- **`kaspad-mainnet`**: Mainnet Kaspa node
- **`wrpc-proxy-mainnet`**: wRPC proxy for mainnet

#### Testnet Profile

- **`kaspad-testnet`**: Testnet Kaspa node
- **`wrpc-proxy-testnet`**: wRPC proxy for testnet

#### Devnet Profile

- **`kaspad-devnet`**: Devnet Kaspa node

#### Monitoring Profile

- **`prometheus`**: Metrics collection
- **`grafana`**: Dashboard visualization

## Port Configuration

### Mainnet Ports

- **16110**: gRPC API
- **17110**: wRPC Borsh API
- **18110**: wRPC JSON API
- **16111**: P2P networking

### Testnet Ports

- **16210**: gRPC API
- **17210**: wRPC Borsh API
- **18210**: wRPC JSON API
- **16211**: P2P networking

### Devnet Ports

- **16610**: gRPC API
- **17610**: wRPC Borsh API
- **18610**: wRPC JSON API
- **16611**: P2P networking

## Usage Examples

### Using the Start Script (Recommended)

```bash
# Start mainnet node with wRPC proxy
./docker-start.sh mainnet

# Start testnet node in background
./docker-start.sh -d testnet

# Start mainnet with monitoring
./docker-start.sh -m mainnet

# Build and start testnet with monitoring
./docker-start.sh -b -m testnet

# Clean up everything
./docker-start.sh -c
```

### Using Docker Compose Directly

```bash
# Start mainnet profile
docker-compose --profile mainnet up -d

# Start testnet profile
docker-compose --profile testnet up -d

# Start devnet profile
docker-compose --profile devnet up -d

# Start monitoring profile
docker-compose --profile monitoring up -d

# Start multiple profiles
docker-compose --profile mainnet,monitoring up -d
```

### View Logs

```bash
# View mainnet logs
docker-compose --profile mainnet logs -f

# View specific service logs
docker-compose logs -f kaspad-mainnet
```

### Stop Services

```bash
# Stop all services
docker-compose down

# Stop specific profile
docker-compose --profile mainnet down
```

### Rebuild Images

```bash
docker-compose build --no-cache
```

## Configuration

### Environment Variables

Copy `env.example` to `.env` and modify the configuration:

```bash
cp env.example .env
```

Key configuration options:

- `LOG_LEVEL`: Logging level (INFO, DEBUG, etc.)
- `ENABLE_UTXO_INDEX`: Enable UTXO indexing for wallet support
- `ENABLE_PERF_METRICS`: Enable performance metrics
- `OUTBOUND_TARGET`: Target number of outbound peers
- `INBOUND_LIMIT`: Maximum inbound peers
- `WRPC_ENCODING`: wRPC encoding (borsh or serde-json)

### Custom Configuration Files

Mount custom configuration files:

```yaml
volumes:
  - ./config:/app/config:ro
```

### Data Persistence

Data is automatically persisted in Docker volumes:

- `kaspad_mainnet_data`: Mainnet blockchain data
- `kaspad_testnet_data`: Testnet blockchain data
- `kaspad_devnet_data`: Devnet blockchain data

## Monitoring

### Enable Monitoring Stack

```bash
# Using start script
./docker-start.sh -m mainnet

# Or directly with docker-compose
docker-compose --profile mainnet,monitoring up -d
```

### Access Dashboards

- **Prometheus**: http://localhost:9090
- **Grafana**: http://localhost:3000 (admin/admin)

### Custom Metrics

The nodes expose metrics on their respective ports. Configure Prometheus to scrape:

- Mainnet: `kaspad-mainnet:16110`
- Testnet: `kaspad-testnet:16210`
- Devnet: `kaspad-devnet:16610`

## CLI Access

### Using kaspa-cli

```bash
# Connect to mainnet
docker-compose exec kaspad-mainnet kaspa-cli --server localhost:16110 ping

# Connect to testnet
docker-compose exec kaspad-testnet kaspa-cli --server localhost:16210 ping
```

### External CLI Access

```bash
# Mainnet
kaspa-cli --server localhost:16110 ping

# Testnet
kaspa-cli --server localhost:16210 ping
```

## Development

### Building from Source

The Dockerfile uses a multi-stage build:

1. **Builder stage**: Compiles Rust code
2. **Runtime stage**: Minimal image with binaries

### Custom Build

```bash
docker build -t rusty-kaspa:custom .
```

### Development Mode

For development, you can mount the source code:

```yaml
volumes:
  - .:/app:ro
```

## Troubleshooting

### Common Issues

1. **Port conflicts**: Ensure ports are not in use by other services
2. **Permission issues**: Check file permissions for mounted volumes
3. **Memory issues**: Increase Docker memory limits for large blockchain data

### Health Checks

Services include health checks that verify node connectivity:

```bash
docker-compose ps
```

### Logs

View detailed logs:

```bash
# All services
docker-compose logs

# Specific service
docker-compose logs kaspad-mainnet

# Follow logs
docker-compose logs -f kaspad-mainnet
```

### Reset Data

To reset blockchain data:

```bash
docker-compose down -v
docker-compose up kaspad-mainnet
```

## Security Considerations

1. **Network isolation**: Services run in isolated Docker networks
2. **Non-root user**: Containers run as non-root user `kaspa`
3. **Read-only mounts**: Configuration files are mounted read-only
4. **Health checks**: Automatic health monitoring

## Performance Tuning

### Resource Limits

Add resource limits to your services:

```yaml
deploy:
  resources:
    limits:
      cpus: "4.0"
      memory: 8G
    reservations:
      cpus: "2.0"
      memory: 4G
```

### Async Threads

Configure async threads based on your CPU cores:

```bash
# In .env
ASYNC_THREADS=8
```

### Memory Optimization

Adjust RAM scale factor:

```bash
# In .env
RAM_SCALE=1.0
```

## Production Deployment

### Recommended Production Setup

1. Use external volumes for data persistence
2. Configure proper logging
3. Set up monitoring and alerting
4. Use reverse proxy for external access
5. Implement backup strategies

### Example Production Compose

```yaml
version: "3.8"
services:
  kaspad-mainnet:
    image: rusty-kaspa:latest
    restart: unless-stopped
    volumes:
      - /data/kaspa/mainnet:/app/data
      - /logs/kaspa/mainnet:/app/logs
    environment:
      - LOG_LEVEL=INFO
      - ENABLE_UTXO_INDEX=true
    ports:
      - "16110:16110"
      - "17110:17110"
      - "18110:18110"
    deploy:
      resources:
        limits:
          cpus: "4.0"
          memory: 8G
```

## Support

For issues and questions:

1. Check the [Rusty Kaspa documentation](https://github.com/kaspanet/rusty-kaspa)
2. Review Docker logs for error messages
3. Verify configuration in `.env` file
4. Ensure sufficient system resources
