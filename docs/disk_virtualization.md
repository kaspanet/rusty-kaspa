# Disk Virtualization for Kaspa Nodes

This guide documents disk virtualization approaches for optimizing Kaspa nodes, including testing results and production recommendations for both regular and archive nodes.

## Overview: Disk Virtualization for Kaspa Nodes

### What is Disk Virtualization?

Disk virtualization involves placing high-frequency database operations (like Write-Ahead Logs) on fast storage while keeping bulk data on slower, cheaper storage.

### Why It Matters

RocksDB uses a Write-Ahead Log (WAL) to ensure durability. Every write operation must be committed to the WAL before being acknowledged.

**Use Cases by Node Type:**

|     Node Type      | WAL Storage | Bulk Data | Primary Benefit                   |
|--------------------|-------------|-----------|-----------------------------------|
| **Regular (NVMe)** | tmpfs (RAM) | NVMe      | Reduce NVMe wear, max performance |
| **Archive (HDD)**  | NVMe/SSD    | HDD       | Eliminate HDD seek latency        |

**For Regular Nodes:**
- **Challenge**: NVMe write endurance (24/7 operation)
- **Solution**: tmpfs for WAL reduces wear while maximizing performance
- **Trade-off**: Database recreation on crash is acceptable:
  1. Crashes are infrequent (power loss, kernel panic)
  2. Fast resync (2-4 hours)
  3. No data loss - rebuilds from network peers

**For Archive Nodes:**
- **Challenge**: HDD seek latency bottleneck (5-15ms per write)
- **Solution**: NVMe/SSD for WAL eliminates seek penalty
- **Trade-off**: Database recreation NOT acceptable (27+ hour resync)

## Understanding RocksDB Write Path

### How RocksDB Handles Writes

RocksDB's write path has three components:

```
Application Write
    ↓
1. WAL (Write-Ahead Log)    ← SYNCHRONOUS disk write (BOTTLENECK on HDD)
    ↓
2. Memtable                 ← In-memory write buffer (already does write-back caching!)
    ↓
3. SST Files                ← Async flush when memtable full
```

**Key Points:**

1. **WAL is synchronous** - Every write must be written to disk before acknowledging to the application. This ensures durability (crash recovery).

2. **Memtable is already a write-back cache** - Writes go to memory first, then flush to SST files asynchronously. RocksDB already does this optimization.

3. **WAL is the bottleneck on HDD** - Because WAL writes are synchronous and HDDs have 5-15ms seek time, each write operation is slow.

4. **You cannot make WAL asynchronous** - This would break crash recovery and lose data on power failure.

**Important Clarification: Synchronous vs Durable**

WAL writes are **always synchronous** from the application's perspective (the app waits for the write to complete). However:

- **Durable storage** (disk/NVMe/SSD): Write persists on crash → **safe**
- **Volatile storage** (tmpfs/RAM): Write lost on crash → **not durable**

tmpfs doesn't make WAL "asynchronous" - it makes it **non-durable**. The write operation is still synchronous (blocking), but the data is stored in volatile memory rather than persistent storage.

**This is the key trade-off:**
- **Regular nodes**: Non-durable WAL acceptable (fast resync, no data loss from peers)
- **Archive nodes**: Durable WAL mandatory (resync too expensive)

### Implementation: `--rocksdb-wal-dir` Flag

We implemented the `--rocksdb-wal-dir` command-line option to specify WAL directory location.

**Flexibility:**
- Point to NVMe/SSD partition for hybrid storage (archive nodes)
- Point to tmpfs mount for RAM-based storage (regular nodes)
- Point to any filesystem path (user's choice)

**Safety Features:**
- Auto-generates unique subdirectories per database (consensus, meta, utxoindex)
- Prevents race conditions between databases (experienced with tmpfs)
- Works with both `default` and `archive` presets

**For Regular Nodes - tmpfs Trade-off:**

Database recreation on crash is acceptable because:
1. **Infrequent**: Crashes rare (power loss, kernel panic)
2. **Fast**: 2-4 hour resync
3. **No data loss**: Rebuilds from network peers
4. **Benefit**: Eliminates NVMe wear from continuous WAL writes 

### Why `--rocksdb-wal-dir` is the Correct Solution

Since we cannot make WAL asynchronous without losing durability, the only way to speed up writes is to **put the WAL on fast storage**:

**With `--rocksdb-wal-dir`:**
```
WAL → NVMe/SSD (fast, <1ms writes)
Memtable → RAM (fast, already optimized)
SST Files → HDD (slow but OK, async flush)
```

**Result:**
- Fast synchronous WAL writes (no HDD seek penalty)
- Full crash safety maintained
- Bulk data on cheap HDD storage
- No additional complexity

**Without WAL optimization:**
```
WAL → HDD (slow, 5-15ms per write)
Memtable → RAM (fast, but doesn't help WAL)
SST Files → HDD (slow but OK, async)
```

**Result:**
- Every write waits for HDD seek
- Memtable optimization wasted
- Write throughput limited by disk

### Why "In-Process Write-Back Cache" is Not Needed

Some might ask: "Why not add another caching layer in kaspad?"

**The answer:**
- RocksDB's memtable **already is** a write-back cache
- Adding another layer would either:
  - **Duplicate memtable** (no benefit)
  - **Make WAL non-durable** (loses crash safety)
- `--rocksdb-wal-dir` solves the problem correctly

**tmpfs for WAL** makes the WAL non-durable (data in RAM only, lost on crash), which is why it requires database recreation after power loss or crashes.

## Tested Approaches

### Option 1: `--rocksdb-wal-dir` with NVMe/SSD (✅ RECOMMENDED FOR ARCHIVAL NODES USING HDDs)

**Status**: Implemented and safe for production

The `--rocksdb-wal-dir` flag allows you to specify a custom directory for RocksDB Write-Ahead Logs, enabling hybrid storage configurations.

**Features:**
- Custom WAL directory on separate storage device
- Auto-generated unique subdirectories per database (consensus, meta, utxoindex)
- No corruption risk
- Works with both `default` and `archive` presets

**Example Configuration:**

```bash
# Create WAL directory on NVMe/SSD
mkdir -p /mnt/nvme/kaspa-wal

# Run kaspad with WAL on fast storage, data on HDD
kaspad --archival \
       --rocksdb-preset=archive \
       --rocksdb-wal-dir=/mnt/nvme/kaspa-wal \
       --appdir=/mnt/hdd/kaspa-data
```

**Benefits:**
- Fast write bursts to NVMe WAL (microsecond latency)
- Bulk data storage on cheaper HDD
- Optimal I/O distribution
- Cost-effective for large archives
- No data loss on restart/crash

**When to Use:**
- Production archive nodes on HDD storage
- Systems with available NVMe/SSD capacity (2-8GB recommended for WAL)
- When budget allows for hybrid storage setup

### Option 2: tmpfs WAL — RAM-backed filesystem (✅ RECOMMENDED FOR REGULAR NVME NODES)

**Status**: Node type dependent - acceptable for regular nodes, NOT for archive nodes

tmpfs stores data entirely in RAM, providing the fastest possible I/O but with volatile storage. This is a valid operational choice for standard nodes with clear trade-offs.
**Note** : tmpfs is a linux filesystem, alternatives exists  (lmDisk on Windows). DYOR but the principle remains the same. 

**Important Distinction:**
- ❌ **Archive nodes**: tmpfs NOT recommended (27+ hour resync)
- ✅ **Standard (pruned) nodes**: tmpfs acceptable with proper setup

For **standard (non-archival) pruned nodes**, tmpfs can be a valid choice to reduce NVMe wear while maintaining high performance.

**Decision Matrix: Should You Use tmpfs?**

|         Criteria       | Archive Node | Standard Node |
|------------------------|--------------|---------------|
| **Resync Time**        | 27+ hours | 2-4 hours |
| **Resync Acceptable?** | ❌ NO | ✅ YES |
| **Data Criticality**   | High (historical) | Low (rebuilds from peers) |
| **Crash Impact**       | Database corruption → 27h resync | Database corruption → 2-4h resync |
| **Power Loss Impact**  | Database lost → Unacceptable downtime | Database lost → Acceptable downtime |
| **tmpfs Recommendation** | ❌ Never use | ✅ Valid choice |
| **Primary Use Case**   | Explorers, research, analytics | General P2P, development, testing |

**Key Trade-off:**
- **Risk**: Power loss or crash → Database corruption → Full resync required
- **Benefit**: Eliminates NVMe wear, maximum performance
- **Acceptable for standard nodes**: Yes - crashes are infrequent, resync is fast (2-4h), no data loss (rebuilds from network)

**Tested Configuration:**

Tested and validated with:
- **tmpfs size**: 3GB (sufficient for WAL)
- **Total RAM**: ~7-9GB (4-6GB kaspad + 3GB tmpfs)
- **Node type**: Standard pruned node (non-archival)
- **Uptime**: 24/7 operation

**Setup Example:**

```bash
# Create 3GB tmpfs for standard node WAL
sudo mkdir -p /mnt/tmpfs-kaspad-wal
sudo mount -t tmpfs -o size=3G tmpfs /mnt/tmpfs-kaspad-wal

# Add to /etc/fstab for persistence
echo "tmpfs /mnt/tmpfs-kaspad-wal tmpfs size=3G,mode=1777 0 0" | sudo tee -a /etc/fstab

# Run standard node (NOT archival!)
kaspad \
  --rocksdb-wal-dir=/mnt/tmpfs-kaspad-wal/wal \
  --rpclisten-borsh=0.0.0.0:17110 \
  --rpclisten-json=0.0.0.0:18110
```

**Benefits:**
- ✅ **Reduced NVMe wear** - Important for 24/7 operation
- ✅ **Maximum write performance** - RAM-speed WAL writes
- ✅ **Low memory overhead** - Only 3GB tested
- ✅ **Fast resync** - 2-4 hours acceptable for standard nodes
- ✅ **Cost effective** - No need for separate NVMe for WAL

**Trade-offs:**
- ⚠️ **Crash = Database loss** - Requires full resync (2-4 hours)
- ⚠️ **Mempool state lost** - Need to rebuild from network
- ⚠️ **Network bandwidth** - Each crash uses ~50-100GB download
- ⚠️ **Temporary downtime** - Node offline during resync

**When to Use:**
- Standard (pruned) nodes only
- Fast network for resync
- Node not critical for services/mining
- NVMe wear is a concern (24/7 operation)

**When NOT to Use:**
- ❌ Archive nodes (resync too long)
- ❌ Mining nodes (uptime critical)
- ❌ Service provider nodes (reliability critical)
- ❌ Slow network connection (resync expensive)

**Monitoring:**

```bash
# Check tmpfs usage
df -h /mnt/tmpfs-kaspad-wal

# Watch for growth (should stay ~1-2GB)
watch -n 60 "du -sh /mnt/tmpfs-kaspad-wal/*"

# Monitor for unexpected restarts
journalctl -u kaspad -f
```

**Risk Mitigation:**
1. **Fast network** - Ensure quick resync capability
2. **Monitor stability** - Address if crashes become frequent
3. **Alert on restart** - Detect when resync is needed
4. **Backup strategy** - Consider periodic database snapshots for faster recovery

**Testing Notes:**
Automatic crash recovery for tmpfs-based standard nodes was prototyped during development. The mechanism detects database corruption on startup and initiates automatic resync. However, this feature requires extensive testing and careful evaluation before production use. For now, manual recovery (delete database, restart) is the recommended approach.

**Comparison: tmpfs vs NVMe WAL for Standard Nodes**

| Approach | Cost | Performance | Safety | NVMe Wear |
|----------|------|-------------|--------|-----------|
| **tmpfs WAL** | Low (RAM) | Fastest | Crash = resync | Minimal |
| **NVMe WAL** | Medium (NVMe) | Fast | Full safety | Higher |
| **HDD WAL** | Low | Slow | Full safety | N/A |


## Performance Benchmarks

### Test Environment

**Hardware:**
- CPU: Multi-core x86_64
- RAM: 78GB total
- Storage: Seagate ST12000NM001G 12TB HDD
- Network: 8 outbound peers (including local peer connection)

**System Optimizations:**
- I/O Scheduler: mq-deadline (optimized for HDD)
- Read-ahead: 4096 KB
- Kernel tuning: `vm.dirty_ratio=40`, `vm.swappiness=10`

### Baseline: Archive Preset Only (HDD)

**Configuration:**
```bash
kaspad --archival \
       --rocksdb-preset=archive \
       --appdir=/mnt/hdd/kaspa-data \
       --ram-scale=0.5
```

**Results (3 hour test):**

| Metric | Value |
|--------|-------|
| **Sync Rate** | 3.67% per hour |
| **Headers/sec** | 500-550 (average) |
| **Database Growth** | 11.7 GB/hour |
| **Total Database Size** | 35 GB at 11% completion |
| **Estimated Full Sync** | ~27 hours |
| **Memory Usage** | 9.3 GB peak, 7-9 GB average |
| **Swap Usage** | 1.9 GB |
| **CPU Utilization** | ~6% average |
| **Disk I/O** | 95-99% utilization during sync |

**Key Observations:**
- Archive preset provides good memory control (9.3GB vs ~29GB without preset)
- HDD utilization very high during header sync
- Sync progressing steadily without bottlenecks
- System tuning effective (stable performance)

### Hybrid Setup: Archive + NVMe WAL (Recommended for HDD)

**Test Configuration:**
- Hardware: 12TB Seagate HDD, 78GB RAM
- Storage: HDD for data, NVMe for WAL (`/opt/kaspa/wal`)
- Preset: archive-tiered (archive optimizations + tiered storage)
- Date: November 28, 2025

**Results (Full Sync from Genesis):**

| Metric | Value |
|--------|-------|
| **Total Sync Time** | **4h 50m** (4:50:25) |
| **vs Baseline** | **82% faster** (27h → 4h 50m) |
| **Speedup** | **5.6x** |
| **Start Time** | 09:46:25 CET |
| **Completion Time** | 14:36:50 CET |
| **Configuration** | `--archival --rocksdb-preset=archive-tiered --rocksdb-wal-dir=/opt/kaspa/wal` |

**Key Observations:**
- ✅ NVMe WAL eliminates HDD seek latency bottleneck
- ✅ Archive preset keeps memory controlled (~8-12GB)
- ✅ Consistent sync speed throughout
- ✅ No I/O stutters or bottlenecks
- ✅ Production-proven configuration

### Comparison: Default vs Archive Preset

| Configuration | Memory Peak | Sync Performance | Database Size | Compression |
|---------------|-------------|------------------|---------------|-------------|
| **Default (SSD)** | ~29GB | Optimized for fast storage | Standard | LZ4 only |
| **Archive (HDD)** | ~9.3GB | Optimized for sequential I/O | ~30-50% smaller | LZ4 + ZSTD |

**Archive Preset Improvements:**
- 68% reduction in peak memory (29GB → 9.3GB)
- 30-50% better compression (ZSTD on bottommost level)
- 96% fewer SST files (256MB files vs default)
- Smoother I/O (12 MB/s rate limiting prevents spikes)
- Better caching (2GB block cache)

## Step-by-Step Setup Guide

### Recommended: NVMe/SSD WAL Directory

**Prerequisites:**
- Available NVMe/SSD storage (10-50GB recommended)
- Root access for mounting (if needed)

**Step 1: Prepare WAL Storage**

```bash
# Check available NVMe/SSD storage
df -h /mnt/nvme

# Create WAL directory
sudo mkdir -p /mnt/nvme/kaspa-wal
sudo chown kaspa:kaspa /mnt/nvme/kaspa-wal
```

**Step 2: Run Kaspad with Hybrid Storage**

```bash
kaspad --archival \
       --rocksdb-preset=archive \
       --rocksdb-wal-dir=/mnt/nvme/kaspa-wal \
       --appdir=/mnt/hdd/kaspa-data
```

**Step 3: Verify Setup**

Check logs for confirmation:
```bash
journalctl -u kaspad -f | grep -i "wal\|preset"
```

You should see:
```
Using RocksDB preset: archive - Archive preset - optimized for HDD
Custom WAL directory: /mnt/nvme/kaspa-wal
```

**Step 4: Monitor Performance**

```bash
# Check WAL directory size
du -sh /mnt/nvme/kaspa-wal/*

# Monitor I/O distribution
iostat -x 5

# Check database growth
du -sh /mnt/hdd/kaspa-data/kaspa-mainnet/datadir2
```

### Advanced: tmpfs Setup for Standard Nodes

**For standard (pruned) nodes only - NOT for archive nodes**

**Step 1: Create tmpfs Mount**

```bash
# Create mount point
sudo mkdir -p /mnt/tmpfs-kaspad

# Mount tmpfs (8GB example)
sudo mount -t tmpfs -o size=8G tmpfs /mnt/tmpfs-kaspad

# Verify mount
df -h /mnt/tmpfs-kaspad
```

**Step 2: Make tmpfs Persistent (Optional)**

Add to `/etc/fstab`:
```
tmpfs /mnt/tmpfs-kaspad tmpfs size=8G,mode=1777 0 0
```

**Step 3: Run Kaspad with tmpfs WAL**

```bash
# Standard (pruned) node with tmpfs WAL
kaspad --rocksdb-wal-dir=/mnt/tmpfs-kaspad/wal \
       --rpclisten-borsh=0.0.0.0:17110 \
       --rpclisten-json=0.0.0.0:18110
```

**⚠️ CRITICAL WARNINGS:**
- Data in tmpfs is lost on restart/crash
- Database will become corrupted and require deletion
- Manual recovery required (no automatic mechanism yet)
- Only use for standard nodes (NOT archive nodes - 27+ hour resync)

**Recovery Procedure (if corruption occurs):**

```bash
# Stop kaspad
sudo systemctl stop kaspad

# Delete corrupted database
rm -rf /mnt/hdd/kaspa-data/kaspa-mainnet/datadir2

# Clear tmpfs WAL
rm -rf /mnt/tmpfs-kaspad/wal/*

# Restart kaspad (will resync from genesis)
sudo systemctl start kaspad
```


## Monitoring & Metrics

### Key Metrics to Track

**Sync Progress:**
```bash
# Via RPC
curl -s http://localhost:16310 --data-binary '{
  "jsonrpc":"2.0",
  "id":"1",
  "method":"getBlockDagInfoRequest",
  "params":[]
}' -H 'content-type: application/json'
```

**WAL Directory Usage:**
```bash
# Check WAL size
watch -n 60 "du -sh /mnt/nvme/kaspa-wal/* && df -h /mnt/nvme"
```

**Database Growth:**
```bash
# Track database size over time
watch -n 300 "du -sh /mnt/hdd/kaspa-data/kaspa-mainnet/datadir2/*"
```

**I/O Statistics:**
```bash
# Monitor I/O per device
iostat -x 5 /dev/nvme0n1 /dev/sda
```

**Memory Usage:**
```bash
# Watch kaspad memory
watch -n 60 "ps aux | grep kaspad | grep -v grep"
```

### Expected Resource Usage

**With `--rocksdb-preset=archive` + NVMe WAL:**

| Resource | Expected Range |
|----------|----------------|
| RAM | 8-12 GB peak |
| WAL Storage | 10-50 GB |
| Database Growth | 10-15 GB/hour (during header sync) |
| Sync Time (full) | 20-30 hours (estimated) |

## Troubleshooting

### WAL Directory Issues

**Problem: Permission denied**
```bash
sudo chown -R kaspa:kaspa /mnt/nvme/kaspa-wal
sudo chmod 755 /mnt/nvme/kaspa-wal
```

**Problem: WAL directory full**
```bash
# Check usage
df -h /mnt/nvme

# Increase WAL partition or move to larger storage
# Stop kaspad, move WAL, update --rocksdb-wal-dir, restart
```

### tmpfs Corruption Recovery

**Problem: Database won't start after restart**
```bash
# Delete corrupted database
rm -rf $APPDIR/kaspa-mainnet/datadir2

# Clear tmpfs WAL
rm -rf /mnt/tmpfs-kaspad/wal/*

# Restart (will resync)
systemctl restart kaspad
```

### Performance Issues

**Problem: Slower than expected sync**

Check:
1. I/O scheduler: Should be `mq-deadline` for HDD
   ```bash
   cat /sys/block/sda/queue/scheduler
   ```

2. Read-ahead: Should be 4096 KB or higher
   ```bash
   blockdev --getra /dev/sda
   ```

3. Kernel tuning: Check `vm.dirty_ratio`, `vm.swappiness`
   ```bash
   sysctl vm.dirty_ratio vm.swappiness
   ```

## Recommendations Summary

### Archive Nodes (Production)

**Recommended Configuration:**
```bash
kaspad --archival \
       --rocksdb-preset=archive \
       --rocksdb-wal-dir=/mnt/nvme/kaspa-wal \
       --appdir=/mnt/hdd/kaspa-data
```

✅ **DO:**
- Use `--rocksdb-preset=archive` for HDD deployments
- Use `--rocksdb-wal-dir` with NVMe/SSD for hybrid setups
- Allocate 16GB+ RAM (8GB minimum)
- Apply system-level optimizations (I/O scheduler, kernel tuning)
- Monitor WAL directory usage regularly
- Plan for 500GB-2TB+ storage

❌ **DON'T:**
- Use tmpfs for WAL storage (27+ hour resync NOT acceptable)
- Run without `--rocksdb-preset=archive` on HDDs
- Ignore memory requirements (will cause OOM)

### Regular Nodes (Standard/Pruned)

**Recommended Configuration (NVMe wear reduction):**
```bash
# Option 1: tmpfs WAL (fastest, non-durable)
kaspad --rocksdb-wal-dir=/mnt/tmpfs-kaspad-wal

# Option 2: NVMe WAL (fast, durable)
kaspad --rocksdb-wal-dir=/mnt/nvme/kaspa-wal
```

✅ **DO:**
- Consider tmpfs for WAL (NVMe wear reduction, 2-4h resync acceptable)
- Allocate 7-9GB RAM total (4-6GB kaspad + 3GB tmpfs)
- Ensure fast network connection for resync
- Monitor for unexpected restarts
- Set up alerts for database corruption/restart events

❌ **DON'T:**
- Use tmpfs for mining or service provider nodes (uptime critical)
- Use tmpfs with slow network (resync expensive)
- Ignore crash frequency (if crashes frequent, use durable WAL)

### Development Nodes

**Recommended Configuration (maximum flexibility):**
```bash
# Fast development iteration
kaspad --rocksdb-wal-dir=/mnt/tmpfs-dev-wal
```

✅ **DO:**
- Use tmpfs for maximum performance (fast resync OK)
- Experiment with different configurations
- Use smaller databases for testing
- Automate resync recovery

❌ **DON'T:**
- Use development setups for production
- Rely on data persistence across restarts

### Cost-Benefit Analysis

**Archive Node (HDD only):**
- Cost: Low (just HDD storage)
- Setup: Simple (single flag: `--rocksdb-preset=archive`)
- Performance: Good (3.67%/hour sync rate, ~27h full sync)
- Reliability: High

**Archive Node (HDD + NVMe WAL):**
- Cost: Medium (HDD + small NVMe for WAL)
- Setup: Moderate (requires separate partition/mount)
- Performance: **82% faster** (4h 50m vs 27h baseline, 5.6x speedup)
- Reliability: High
- **Tested:** Nov 28, 2025 - Full sync from genesis with archive-tiered preset

**Standard Node (tmpfs WAL):**
- Cost: Low (just RAM, 3GB tested)
- Setup: Simple (tmpfs mount + flag)
- Performance: Fastest (RAM-speed writes)
- Reliability: Medium (2-4h resync on crash - acceptable for standard nodes)
- Trade-off: Database recreation required after crash/power loss

## References

- [Issue #681](https://github.com/kaspanet/rusty-kaspa/issues/681) - HDD Archive Node Optimization
- [RocksDB Tuning Guide](https://github.com/facebook/rocksdb/wiki/RocksDB-Tuning-Guide)
- Archive Preset Configuration: `database/src/db/rocksdb_preset.rs`
- Implementation: Based on testing by @Callidon and community feedback


