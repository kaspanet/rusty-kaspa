# Using `--override-params-file`

The `--override-params-file` flag lets you run `kaspad` with a custom set of
consensus parameters loaded from a JSON file. This is primarily useful for
local development, testing alternative fork schedules, or simulating extreme
network conditions. **Overriding consensus parameters on mainnet is blocked
and will make the node exit at startup.**

## Quick start

1. Pick a non-mainnet network flag (for example `--testnet`, `--devnet`, or
	 `--simnet`).
2. Create a JSON file containing the parameters you want to override. Any
	 field you omit keeps its default value for the chosen network.
3. Launch `kaspad` with the flag:

	 ```bash
	 kaspad --devnet --override-params-file /path/to/overrides.json
	 ```

If the file cannot be read or parsed, `kaspad` prints the error and exits.

### Example override file

```json
{
  "timestamp_deviation_tolerance": 600,
  "pre_crescendo_target_time_per_block": 1000,
  "past_median_time_window_size": 27,
  "difficulty_window_size": 661,
  "min_difficulty_window_size": 150,
  "coinbase_payload_script_public_key_max_len": 150,
  "max_coinbase_payload_len": 204,
  "mass_per_tx_byte": 1,
  "mass_per_script_pub_key_byte": 10,
  "mass_per_sig_op": 1000,
  "max_block_mass": 500000,
  "storage_mass_parameter": 10000,
  "deflationary_phase_daa_score": 15519600,
  "pre_deflationary_phase_base_subsidy": 50000000000,
  "skip_proof_of_work": true,
  "max_block_level": 254,
  "pruning_proof_m": 1000,
  "blockrate": {
    "target_time_per_block": 100,
    "ghostdag_k": 124,
    "past_median_time_sample_rate": 10,
    "difficulty_sample_rate": 2,
    "max_block_parents": 16,
    "mergeset_size_limit": 248,
    "merge_depth": 36000,
    "finality_depth": 432000,
    "pruning_depth": 1080000,
    "coinbase_maturity": 200
  },
  "crescendo_activation": 0
}
```

All high level (non-nested) fields are optional, and if omitted, their default values in the respective network will be used. 
The `blockrate` field must either be absent or provided in full with all subfields (missing subfields will default to zero and not to default network params). This is
because they have logical relations and should be modified as a unit.  

## Available parameters
| Field                                       | Description                |
|---------------------------------------------|----------------------------|
| timestamp_deviation_tolerance               | Timestamp deviation tolerance |
| pre_crescendo_target_time_per_block         | Pre-crescendo target time per block |
| past_median_time_window_size                | Past median time window size |
| difficulty_window_size                      | Difficulty window size |
| min_difficulty_window_size                  | Minimum difficulty window size |
| coinbase_payload_script_public_key_max_len  | Coinbase payload script public key max length |
| max_coinbase_payload_len                    | Maximum coinbase payload length |
| max_tx_inputs                               | Max transaction inputs |
| max_tx_outputs                              | Max transaction outputs |
| max_signature_script_len                    | Max signature script length |
| max_script_public_key_len                   | Max script public key length |
| mass_per_tx_byte                            | Mass per transaction byte     |
| mass_per_script_pub_key_byte                | Mass per script public key byte |
| mass_per_sig_op                             | Mass per signature operation  |
| max_block_mass                              | Maximum block mass            |
| storage_mass_parameter                      | Storage mass parameter        |
| deflationary_phase_daa_score                | Deflationary phase DAA score  |
| pre_deflationary_phase_base_subsidy         | Pre-deflationary phase base subsidy |
| skip_proof_of_work                          | Whether to skip proof of work checks            |
| max_block_level                             | Maximum block level           |
| pruning_proof_m                             | Pruning proof M parameter                        |
| blockrate                                   | Blockrate-related parameters            |
| crescendo_activation                        | Crescendo DAA score                        |

**blockrate sub-fields:**

| Field                              | Description                |
|-------------------------------------|----------------------------|
| target_time_per_block               | Target time per block               |
| ghostdag_k                          | Ghostdag K                          |
| past_median_time_sample_rate        | Median time sample rate        |
| difficulty_sample_rate              | Difficulty sample rate              |
| max_block_parents                   | Maximum block parents                   |
| mergeset_size_limit                 | Mergeset size limit                 |
| merge_depth                         | Merge depth                         |
| finality_depth                      | Finality depth                      |
| pruning_depth                       | Pruning depth                       |
| coinbase_maturity                   | Coinbase maturity                   |

Refer to the source definition in
`consensus/core/src/config/params.rs` for the full list of available fields and
their meaning.

## Use simpa params

If you want to run `kaspad` with the simpa generated database, you'll need to ask it to generate an override params file as well, so you can run `kaspad` with the same parameters.

This can be done by passing the `--override-params-output` param, for example:
```bash
cargo run --release --bin simpa -- --override-params-output overrides.json -o=/path/to/simpa/database
```

And then launch kaspad with:
```bash
kaspad --simnet
```

and immedeiately close it. This will create the simnet datadir at `~/.rusty-kaspa/kaspa-simnet`.

You can then override it with the simpa database by running:
```bash
rm -rf ~/.rusty-kaspa/kaspa-simnet/datadir/consensus/consensus-001/
mv /path/to/simpa/database ~/.rusty-kaspa/kaspa-simnet/datadir/consensus/consensus-001/
```

And finally launch kaspad with:
```bash
kaspad --simnet --override-params-file overrides.json
```