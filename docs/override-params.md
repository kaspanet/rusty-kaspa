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
  "prior_ghostdag_k": 124,
  "timestamp_deviation_tolerance": 600,
  "prior_target_time_per_block": 1000,
  "prior_difficulty_window_size": 2641,
  "min_difficulty_window_size": 150,
  "prior_max_block_parents": 81,
  "prior_mergeset_size_limit": 1240,
  "prior_merge_depth": 3600,
  "prior_finality_depth": 86400,
  "prior_pruning_depth": 185798,
  "coinbase_payload_script_public_key_max_len": 150,
  "max_coinbase_payload_len": 204,
  "prior_max_tx_inputs": 1000000000,
  "prior_max_tx_outputs": 1000000000,
  "prior_max_signature_script_len": 1000000000,
  "prior_max_script_public_key_len": 1000000000,
  "mass_per_tx_byte": 1,
  "mass_per_script_pub_key_byte": 10,
  "mass_per_sig_op": 1000,
  "max_block_mass": 500000,
  "storage_mass_parameter": 10000,
  "deflationary_phase_daa_score": 15519600,
  "pre_deflationary_phase_base_subsidy": 50000000000,
  "prior_coinbase_maturity": 100,
  "skip_proof_of_work": true,
  "max_block_level": 254,
  "pruning_proof_m": 1000,
  "crescendo": {
    "past_median_time_sampled_window_size": 27,
    "sampled_difficulty_window_size": 661,
    "target_time_per_block": 100,
    "ghostdag_k": 124,
    "past_median_time_sample_rate": 10,
    "difficulty_sample_rate": 2,
    "max_block_parents": 16,
    "mergeset_size_limit": 248,
    "merge_depth": 36000,
    "finality_depth": 432000,
    "pruning_depth": 1080000,
    "max_tx_inputs": 1000,
    "max_tx_outputs": 1000,
    "max_signature_script_len": 10000,
    "max_script_public_key_len": 10000,
    "coinbase_maturity": 200
  },
  "crescendo_activation": 0
}
```

All high level (non-nested) fields are optional, and if omitted, their default values in the respective network will be used. The sub-fields of the `crescendo` won't be overriden by the network default value, but instead will be set to 0 if not specified (this is a temporary behavior that will be changed once Crescendo activation logic is cleaned).
## Available parameters
| Field                                      | Description                |
|---------------------------------------------|----------------------------|
| prior_ghostdag_k                           | Pre-crescendo GHOSTDAG K parameter       |
| timestamp_deviation_tolerance              | Timestamp deviation tolerance |
| prior_target_time_per_block                | Pre-crescendo target time per block |
| prior_difficulty_window_size                | Pre-crescendo difficulty window size |
| min_difficulty_window_size                  | Minimum difficulty window size |
| prior_max_block_parents                     | Pre-crescendo max block parents    |
| prior_mergeset_size_limit                   | Pre-crescendo mergeset size limit  |
| prior_merge_depth                           | Pre-crescendo merge depth          |
| prior_finality_depth                        | Pre-crescendo finality depth      |
| prior_pruning_depth                         | Pre-crescendo pruning depth       |
| coinbase_payload_script_public_key_max_len  | Coinbase payload script public key max length |
| max_coinbase_payload_len                    | Maximum coinbase payload length |
| prior_max_tx_inputs                         | Pre-crescendo max transaction inputs |
| prior_max_tx_outputs                        | Pre-crescendo max transaction outputs |
| prior_max_signature_script_len              | Pre-crescendo max signature script length |
| prior_max_script_public_key_len             | Pre-crescendo max script public key length |
| mass_per_tx_byte                            | Mass per transaction byte     |
| mass_per_script_pub_key_byte                | Mass per script public key byte |
| mass_per_sig_op                             | Mass per signature operation  |
| max_block_mass                              | Maximum block mass            |
| storage_mass_parameter                      | Storage mass parameter        |
| deflationary_phase_daa_score                | Deflationary phase DAA score  |
| pre_deflationary_phase_base_subsidy         | Pre-deflationary phase base subsidy |
| prior_coinbase_maturity                     | Pre-crescendo coinbase maturity       |
| skip_proof_of_work                          | Whether to skip proof of work checks            |
| max_block_level                             | Maximum block level           |
| pruning_proof_m                             | Pruning proof M parameter                        |
| crescendo                                   | Post-crescendo parameters            |
| crescendo_activation                        | Crescendo DAA score                        |

**crescendo sub-fields:**

| Field                              | Description                |
|-------------------------------------|----------------------------|
| past_median_time_sampled_window_size| Post-crescendo median time window size |
| sampled_difficulty_window_size      | Post-crescendo difficulty window size      |
| target_time_per_block               | Post-crescendo target time per block               |
| ghostdag_k                          | Post-crescendo ghostdag K                          |
| past_median_time_sample_rate        | Post-crescendo median time sample rate        |
| difficulty_sample_rate              | Post-crescendo difficulty sample rate              |
| max_block_parents                   | Post-crescendo maximum block parents                   |
| mergeset_size_limit                 | Post-crescendo mergeset size limit                 |
| merge_depth                         | Post-crescendo merge depth                         |
| finality_depth                      | Post-crescendo finality depth                      |
| pruning_depth                       | Post-crescendo pruning depth                       |
| max_tx_inputs                       | Post-crescendo maximum transaction inputs          |
| max_tx_outputs                      | Post-crescendo maximum transaction outputs         |
| max_signature_script_len            | Post-crescendo maximum signature script length     |
| max_script_public_key_len           | Post-crescendo maximum script public key length    |
| coinbase_maturity                   | Post-crescendo coinbase maturity                   |

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