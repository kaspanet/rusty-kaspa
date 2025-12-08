# Bitmain ASIC Compatibility Analysis

## Source: kaspa-stratum-bridge-1.3.0-dev

This document summarizes key findings from reviewing the kaspa-stratum-bridge codebase to ensure proper Bitmain ASIC support in our Rust stratum implementation.

---

## Key Findings

### 1. **Extranonce Configuration**
- **Bitmain**: `extranonce = 0 bytes` (empty string)
- **IceRiver**: `extranonce = 2 bytes` (4 hex chars)
- **Detection**: User agent contains "GodMiner" (Bitmain) or "IceRiverMiner"/"BzMiner" (IceRiver)

**Code Reference:**
- `src/gostratum/default_client.go:17`: `var bitmainRegex = regexp.MustCompile(".*(GodMiner).*")`
- `src/kaspastratum/client_handler.go:17`: `var bigJobRegex = regexp.MustCompile(".*(BzMiner|IceRiverMiner).*")`
- `cmd/kaspabridge/config.yaml:71`: `extranonce_size: 0` (default, works for Bitmain)

### 2. **Subscribe Response Format**

**Bitmain Format:**
```go
[]any{nil, ctx.Extranonce, 8 - (len(ctx.Extranonce) / 2)}
// Since extranonce is empty (0 bytes), this becomes: [null, "", 8]
```

**Standard Format (IceRiver):**
```go
[]any{true, "EthereumStratum/1.0.0"}
```

**Code Reference:**
- `src/gostratum/default_client.go:142-148`

### 3. **Set Extranonce Notification Format**

**Bitmain Format:**
```go
[]any{ctx.Extranonce, 8 - (len(ctx.Extranonce) / 2)}
// Since extranonce is empty, this becomes: ["", 8]
```

**Standard Format (IceRiver):**
```go
[]any{ctx.Extranonce}
// For IceRiver with 2-byte extranonce: ["abcd"]
```

**Code Reference:**
- `src/gostratum/default_client.go:175-179`

### 4. **Job Format (Big Job vs Standard)**

**Bitmain uses "Big Job" format** (same as BzMiner and IceRiverMiner):
- Single string parameter with 80 hex characters
- Format: `prePoWHash (64 hex) + timestamp (16 hex) = 80 hex total`
- Uses `GenerateLargeJobParams()` function

**Standard format:**
- Two parameters: `[header_array, timestamp]`
- Uses `GenerateJobHeader()` function

**Code Reference:**
- `src/kaspastratum/client_handler.go:141`: `state.useBigJob = bigJobRegex.MatchString(client.RemoteApp)`
- `src/kaspastratum/client_handler.go:159-164`: Job format selection
- `src/kaspastratum/hasher.go:132-150`: `GenerateLargeJobParams()` implementation

**Important:** The regex for "big job" is `.*(BzMiner|IceRiverMiner).*` - **Bitmain is NOT in this regex**, but the code comment in `share_handler.go:216` mentions "IceRiver/Bitmain ASICs" use big job format. However, Bitmain detection is separate via `bitmainRegex` for subscribe/extranonce handling.

### 5. **Job ID Handling (Critical Bug Workaround)**

**Problem:** Bitmain/IceRiver ASICs submit shares with **incorrect job IDs**.

**Solution:** The bridge implements a workaround that loops through previous job IDs when a share is rejected due to low difficulty:

```go
// stupid hack for busted ass IceRiver/Bitmain ASICs.  Need to loop
// through job history because they submit jobs with incorrect IDs
if jobId == 1 || jobId%maxJobs == submitInfo.jobId%maxJobs+1 {
    // exhausted all previous blocks
    break
} else {
    var exists bool
    jobId--
    block, exists = state.GetJob(jobId)
    if !exists {
        // just exit loop - bad share will be recorded
        break
    }
}
```

**Code Reference:**
- `src/kaspastratum/share_handler.go:216-229`

### 6. **Nonce Parsing**

**Bitmain:** Sends nonce as **decimal string** (not hex)
**Standard:** Sends nonce as **hex string**

**Code Reference:**
- Our Rust implementation already handles this via `Encoding::Bitmain` vs `Encoding::BigHeader`

### 7. **Difficulty Requirements**

**Bitmain requires:**
- `pow2_clamp: true` - Difficulty must be power of 2 (e.g., 64, 128, 256, 512, 1024, 2048, 4096, 8192)
- Without pow2 clamping, Bitmain ASICs experience higher error rates

**Code Reference:**
- `cmd/kaspabridge/config.yaml:26-31`: `pow2_clamp: false` (but README says it's required for Bitmain)
- `src/kaspastratum/stratum_server.go:93-95`: Clamping logic on initial difficulty
- `src/kaspastratum/share_handler.go:510-512`: Clamping in vardiff updates

### 8. **Template Distribution Rate Limiting**

**Important:** The bridge implements a 250ms delay between template distributions to prevent overloading ASICs:

```go
if c.lastTemplateTime.After(time.Now().Add(-250 * time.Millisecond)) {
    // skip templates if new ones arrive within a threshold of the last one sent out
    // to not overload the machines with new jobs. KA box pros and some other machines
    // are known to have issues with getting jobs too frequently.
    return
}
```

**Code Reference:**
- `src/kaspastratum/client_handler.go:89-94`

### 9. **Client Connection Spacing**

When distributing new blocks to multiple clients, the bridge adds a 500 microsecond delay between clients:

```go
if clientcount > 0 {
    time.Sleep(500 * time.Microsecond)
}
```

**Code Reference:**
- `src/kaspastratum/client_handler.go:103-105`

---

## Comparison with Our Rust Implementation

### ✅ Already Implemented Correctly:
1. **Extranonce size detection** - We detect Bitmain and set extranonce = 0
2. **Subscribe response format** - We send `[null, extranonce, size]` for Bitmain
3. **Set extranonce format** - We send `[extranonce, size]` for Bitmain
4. **Nonce parsing** - We handle decimal (Bitmain) vs hex (standard) encoding
5. **Difficulty clamping** - We have `clamp_pow2` support
6. **Miner type detection** - We detect Bitmain via user agent

### ❌ Missing/Needs Fix:
1. **Big Job format detection** - We need to check if Bitmain uses big job format
2. **Job ID workaround** - We need to implement the job ID looping workaround for incorrect job IDs
3. **Template distribution rate limiting** - We should add 250ms delay between template distributions
4. **Client connection spacing** - We should add microsecond delays when broadcasting to multiple clients

---

## Action Items

1. **Verify Big Job Format for Bitmain**
   - Check if Bitmain actually uses big job format (80-char hex string) or standard format
   - The bridge code suggests Bitmain might use big job, but it's not explicitly in the regex

2. **Implement Job ID Workaround**
   - Add logic to loop through previous job IDs when a share is rejected
   - This is critical for Bitmain/IceRiver compatibility

3. **Add Template Distribution Rate Limiting**
   - Implement 250ms minimum delay between template distributions
   - Prevents ASIC overload

4. **Add Client Broadcast Spacing**
   - Add 500 microsecond delay between client notifications
   - Prevents network congestion

5. **Verify Extranonce Handling**
   - Ensure Bitmain gets extranonce = 0 bytes (empty string)
   - Ensure subscribe response sends `[null, "", 8]`
   - Ensure set_extranonce sends `["", 8]`

---

## Code References

### Key Files:
- `src/gostratum/default_client.go` - Subscribe, authorize, extranonce handling
- `src/kaspastratum/client_handler.go` - Client connection, job distribution, big job detection
- `src/kaspastratum/share_handler.go` - Share validation, job ID workaround
- `src/kaspastratum/hasher.go` - Job format generation (big job vs standard)
- `src/kaspastratum/mining_state.go` - Mining state management
- `cmd/kaspabridge/config.yaml` - Configuration defaults

### Key Functions:
- `HandleSubscribe()` - Subscribe response format
- `SendExtranonce()` - Extranonce notification format
- `GenerateLargeJobParams()` - Big job format (80-char hex)
- `GenerateJobHeader()` - Standard job format (array + timestamp)
- `HandleSubmit()` - Share validation with job ID workaround

