# Running Kaspa Archive Nodes

This guide explains how to run a Kaspa archive node with HDD-optimized RocksDB configuration.

## What is an Archive Node?

An **archive node** stores the complete blockchain history, including all pruned data that normal nodes discard. Archive nodes can be useful for specialized use cases such as:

- **Blockchain explorers** - Complete transaction history access
- **Research and analytics** - Historical data analysis
- **Compliance and auditing** - Data retention requirements

**Important**: Running archive nodes is resource-intensive and usually requires specific system optimization beyond standard node setup. Archive functionality is provided as-is for advanced users with specific requirements. Most users should run standard pruned nodes instead.

Normal Kaspa nodes are **pruned** and only keep recent blocks (determined by pruning depth).

## Storage Requirements

### Minimum Requirements
- **Storage:** 500GB HDD minimum (2TB+ recommended)
- **RAM:** 4GB minimum (8GB+ recommended with `--rocksdb-preset=hdd`)
- **CPU:** 4 cores
- **Network:** Stable connection with sufficient bandwidth

## RocksDB Presets

Kaspad provides two RocksDB configuration presets optimized for different storage types:

### Default Preset (SSD/NVMe)
```bash
kaspad --archival
# or explicitly:
kaspad --archival --rocksdb-preset=default
```

**Configuration:**
- 64MB write buffer
- Standard compression
- Optimized for fast storage (SSD/NVMe)
- Lower memory footprint

**Best for:** Archive nodes on SSD/NVMe storage

### HDD Preset
```bash
kaspad --archival --rocksdb-preset=hdd
```

**Configuration:**
- **256MB write buffer** (4x default) - Better write batching for HDDs
- **BlobDB enabled** - Separates large values, reduces write amplification
- **Aggressive compression:**
  - LZ4 for L0-L4 (fast compression for hot data)
  - ZSTD level 22 for L5+ (maximum compression for cold data)
  - 64KB dictionary compression with 8MB training
- **12 MB/s rate limiter** - Prevents I/O spikes
- **256MB LRU block cache** (default, scales with --ram-scale)
- **Level 0 compaction trigger: 1 file** - Minimizes write amplification
- **4MB read-ahead** - Optimized for sequential HDD reads
- **Partitioned Bloom filters** - Memory-efficient filtering

**Best for:** Archive nodes on HDD storage

**Memory requirements:** 4GB minimum (basic usage), 8GB+ recommended (public RPC)

## Quick Start

### Basic Archive Node (SSD/NVMe)
```bash
# Default preset, suitable for SSD
kaspad --archival \
  --rpclisten-borsh=0.0.0.0:17110 \
  --rpclisten-json=0.0.0.0:18110
```

### HDD-Optimized Archive Node
```bash
# HDD preset with HDD optimizations
kaspad --archival \
  --rocksdb-preset=hdd \
  --ram-scale=1.0 \
  --rpclisten-borsh=0.0.0.0:17110 \
  --rpclisten-json=0.0.0.0:18110
```

## Performance Tuning

### System-Level Optimizations (Linux)

For optimal HDD performance, tune kernel parameters:

```bash
# /etc/sysctl.d/90-kaspad-archive.conf
vm.dirty_ratio = 40
vm.dirty_background_ratio = 20
vm.dirty_expire_centisecs = 12000
vm.dirty_writeback_centisecs = 1500
vm.swappiness = 10
vm.vfs_cache_pressure = 50
```

Apply with: `sudo sysctl -p /etc/sysctl.d/90-kaspad-archive.conf`

Configure I/O scheduler for HDD (mq-deadline):
```bash
echo "mq-deadline" | sudo tee /sys/block/sda/queue/scheduler
echo "4096" | sudo tee /sys/block/sda/queue/read_ahead_kb
```

### RAM Scaling

Adjust memory allocation based on available RAM:

```bash
# Limited RAM (4GB system)
kaspad --archival --rocksdb-preset=hdd --ram-scale=0.2

# Normal RAM (8GB system)
kaspad --archival --rocksdb-preset=hdd --ram-scale=0.5

# High RAM (16GB+ system)
kaspad --archival --rocksdb-preset=hdd --ram-scale=1.0
```

**Note:** HDD preset requires ~4GB minimum even with `--ram-scale=0.2`. The cache size scales with `--ram-scale` (minimum 64MB).

### RocksDB Cache Configuration

The HDD preset uses a 256MB block cache by default, which automatically scales with `--ram-scale`:

```bash
# Default cache (256MB)
kaspad --archival --rocksdb-preset=hdd

# Scaled cache with ram-scale=0.2 (~51MB cache)
kaspad --archival --rocksdb-preset=hdd --ram-scale=0.2

# Custom cache size (useful for public RPC nodes)
kaspad --archival --rocksdb-preset=hdd --rocksdb-cache-size=2048  # 2GB cache
```

**Cache Sizing Guidelines:**

| Node Type | Recommended Cache | Example |
|-----------|------------------|---------|
| Private storage-only | 64-256MB (default) | `--ram-scale=0.2` to `1.0` |
| Private local queries | 256-512MB | `--rocksdb-cache-size=512` |
| Public RPC (light) | 512MB-1GB | `--rocksdb-cache-size=1024` |
| Public RPC (heavy) | 2-4GB | `--rocksdb-cache-size=2048` |

**Memory Budget Example (8GB system):**
- System: ~1GB
- Kaspad core: ~2GB
- RocksDB cache: 256MB (default)
- RocksDB write buffers: 512MB (2x256MB)
- Remaining: ~4GB (file cache, network buffers, etc.)

## Monitoring

### Check Archive Status
```bash
# Using kaspa-cli (if installed)
kaspa-cli getinfo

# Check logs
journalctl -u kaspad -f

# Check disk usage
du -sh ~/.kaspa/kaspa-mainnet/datadir/
```

### Performance Metrics
```bash
# Enable performance metrics
kaspad --archival --rocksdb-preset=hdd --perf-metrics --perf-metrics-interval-sec=60
```

### Disk I/O Monitoring
```bash
# Monitor disk activity
iostat -x 5

# Check write patterns
iotop -o
```

## Docker Deployment

### Docker Compose Example

**docker-compose.yml:**
```yaml
version: '3.8'

services:
  kaspad-archive:
    image: kaspanet/kaspad:latest
    container_name: kaspad-archive
    restart: unless-stopped
    command:
      - --archival
      - --rocksdb-preset=hdd
      - --ram-scale=1.0
      - --rpclisten-borsh=0.0.0.0:17110
      - --rpclisten-json=0.0.0.0:18110
      - --utxoindex
    volumes:
      - /mnt/hdd/kaspa-archive:/app/data
    ports:
      - "16111:16111"  # P2P
      - "17110:17110"  # RPC Borsh
      - "18110:18110"  # RPC JSON
    environment:
      - KASPAD_APPDIR=/app/data
```

Run with: `docker-compose up -d`

### Docker with System Optimizations

For HDD optimization, configure host kernel parameters before starting the container.

**docker-run.sh:**
```bash
#!/bin/bash

# Apply system tuning
sudo sysctl -w vm.dirty_ratio=40
sudo sysctl -w vm.swappiness=10
sudo sysctl -w vm.vfs_cache_pressure=50

# Set I/O scheduler
echo "mq-deadline" | sudo tee /sys/block/sda/queue/scheduler

# Run container
docker run -d \
  --name kaspad-archive \
  --restart unless-stopped \
  -v /mnt/hdd/kaspa-archive:/app/data \
  -p 16111:16111 \
  -p 17110:17110 \
  -p 18110:18110 \
  kaspanet/kaspad:latest \
    --archival \
    --rocksdb-preset=hdd \
    --ram-scale=1.0 \
    --rpclisten-borsh=0.0.0.0:17110 \
    --rpclisten-json=0.0.0.0:18110 \
    --appdir=/app/data
```

## Systemd Service

**Example systemd service for HDD archive node:**

**/etc/systemd/system/kaspad-archive.service:**
```ini
[Unit]
Description=Kaspa Archive Node (HDD-optimized)
After=network.target

[Service]
Type=simple
User=kaspa
Group=kaspa
ExecStart=/usr/local/bin/kaspad \
  --archival \
  --rocksdb-preset=hdd \
  --ram-scale=1.0 \
  --appdir=/mnt/hdd/kaspa-archive \
  --rpclisten-borsh=0.0.0.0:17110 \
  --rpclisten-json=0.0.0.0:18110 \
  --utxoindex
Restart=always
RestartSec=10
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
```

Enable and start:
```bash
sudo systemctl daemon-reload
sudo systemctl enable kaspad-archive
sudo systemctl start kaspad-archive
sudo systemctl status kaspad-archive
```

## Troubleshooting

### High Disk I/O
**Symptoms:** System slow, high `iowait`

**Solutions:**
1. Verify HDD preset is active:
   ```bash
   journalctl -u kaspad-archive | grep "RocksDB preset"
   # Should show: "Using RocksDB preset: hdd"
   ```
2. Check I/O scheduler: `cat /sys/block/sda/queue/scheduler` (should be `mq-deadline`)
3. Verify kernel tuning: `sysctl vm.dirty_ratio vm.swappiness`
4. Lower `--ram-scale` if swapping occurs

### Out of Memory
**Symptoms:** Process killed by OOM

**Solutions:**
1. HDD preset needs minimum 4GB RAM
2. Reduce `--ram-scale` and cache size:
   ```bash
   # 4GB system
   kaspad --archival --rocksdb-preset=hdd --ram-scale=0.2

   # 8GB system
   kaspad --archival --rocksdb-preset=hdd --ram-scale=0.5

   # Custom cache reduction (if needed)
   kaspad --archival --rocksdb-preset=hdd --rocksdb-cache-size=64
   ```
3. Check swap: `free -h` (4GB+ swap recommended for low-RAM systems)
4. Consider default preset on SSD instead

### Slow Sync Speed
**Expected:** 10-20 blocks/sec on HDD with HDD preset

**If slower:**
1. Verify HDD not failing: `sudo smartctl -a /dev/sda`
2. Check disk utilization: `iostat -x 5` (should be ~70-95%)
3. Ensure system tuning applied
4. Monitor memory: HDD preset uses more RAM but reduces disk I/O

### Preset Not Applied
**Symptom:** Performance same as before

**Check:**
```bash
# Verify flag in service config
systemctl cat kaspad-archive | grep rocksdb-preset

# Check startup logs
journalctl -u kaspad-archive -n 100 | grep -i rocksdb

# Should see:
# "Using RocksDB preset: hdd - HDD preset - optimized for HDD"
```

## Trade-offs

### Archive Preset vs Default

| Aspect | Default (SSD) | Archive (HDD) |
|--------|---------------|---------------|
| Write throughput | ~200 MB/s | ~100-150 MB/s |
| Memory usage | ~4-6 GB | ~4-8 GB (scales with cache) |
| Storage (pruned baseline) | ~40 GB | ~35 GB (better compression) |
| Sync time (HDD) | 2-3 days | 1-2 days |
| Write amplification | ~20x | ~8-10x |
| CPU usage | Low | Medium (compression) |

### When NOT to Use Archive Preset

- **SSD/NVMe storage** - Default preset is faster
- **Limited RAM (<4GB)** - May cause OOM
- **CPU-constrained** - Compression uses more CPU
- **Low disk space** - BlobDB and caching use more temporary space

## Best Practices

1. **Use separate mount for archive data** - Protects system from filling up
2. **Monitor disk health** - HDDs wear out; use SMART monitoring
3. **Plan for growth** - ~10-20 GB/month, size accordingly
4. **Backup strategy** - Archive data is valuable; back up regularly
5. **Dual-node setup** - Fast node for queries, archive for history
6. **System tuning** - Essential for HDD performance

## Performance Comparison

Based on real-world testing:

**Before (Default on HDD):**
- Sync time: ~3-5 days
- Frequent swap usage (10+ GB)
- Write amplification: ~20x
- Disk utilization: 60-80% (not bottlenecked)

**After (Archive Preset on HDD with default 256MB cache):**
- Sync time: ~1.5-2 days
- Minimal swap usage (<100 MB)
- Write amplification: ~8-10x
- Disk utilization: 95-99% (fully utilized)
- 30-50% improvement in write throughput
- Memory usage: ~4-6GB (reduced from 8-12GB with configurable cache)

## Summary

For **HDD-based archive nodes**, use:
```bash
kaspad --archival --rocksdb-preset=hdd
```

This enables HDD-optimized RocksDB configuration, providing:
- ✓ 30-50% faster sync times on HDD
- ✓ Reduced write amplification (50-60% reduction)
- ✓ Better disk utilization (95%+ vs 60-80%)
- ✓ Minimal swap usage despite larger working set
- ✓ Configurable cache size (scales with --ram-scale, or set explicitly)
- ! Requires 4GB+ RAM (8GB+ for public RPC)
- ! Uses more CPU for compression

For **SSD/NVMe**, the default preset is optimal.

---

**Last updated:** January 2026
**Applies to:** Kaspad v1.0.0+
