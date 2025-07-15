(module
  (import "kaspa" "get_balance" (func $get_balance (result i64)))
  (import "kaspa" "transfer" (func $transfer (param i32 i64) (result i32)))
  (import "kaspa" "get_state" (func $get_state (param i32) (result i32)))
  (import "kaspa" "set_state" (func $set_state (param i32 i32) (result i32)))
  
  (memory (export "memory") 1)
  
  (global $TOTAL_SUPPLY_KEY i32 (i32.const 0))
  (global $BALANCE_PREFIX i32 (i32.const 32))
  
  (func $init (param $initial_supply i64) (export "init")
    (i64.store (i32.const 64) (local.get $initial_supply))
    (call $set_state (global.get $TOTAL_SUPPLY_KEY) (i32.const 64))
    drop
  )
  
  (func $total_supply (result i64) (export "total_supply")
    (call $get_state (global.get $TOTAL_SUPPLY_KEY))
    drop
    (i64.load (i32.const 64))
  )
  
  (func $balance_of (param $account i32) (result i64) (export "balance_of")
    (i32.store (i32.const 96) (local.get $account))
    (call $get_state (i32.const 96))
    drop
    (i64.load (i32.const 128))
  )
  
  (func $transfer (param $to i32) (param $amount i64) (result i32) (export "transfer")
    (local $from_balance i64)
    (local $to_balance i64)
    
    (local.set $from_balance (call $balance_of (i32.const 0)))
    
    (if (i64.lt_u (local.get $from_balance) (local.get $amount))
      (then (return (i32.const 0)))
    )
    
    (local.set $to_balance (call $balance_of (local.get $to)))
    
    (i64.store (i32.const 160) (i64.sub (local.get $from_balance) (local.get $amount)))
    (i32.store (i32.const 192) (i32.const 0))
    (call $set_state (i32.const 192) (i32.const 160))
    drop
    
    (i64.store (i32.const 224) (i64.add (local.get $to_balance) (local.get $amount)))
    (i32.store (i32.const 256) (local.get $to))
    (call $set_state (i32.const 256) (i32.const 224))
    drop
    
    (i32.const 1)
  )
  
  (func $mint (param $to i32) (param $amount i64) (result i32) (export "mint")
    (local $current_supply i64)
    (local $to_balance i64)
    
    (local.set $current_supply (call $total_supply))
    (local.set $to_balance (call $balance_of (local.get $to)))
    
    (i64.store (i32.const 288) (i64.add (local.get $current_supply) (local.get $amount)))
    (call $set_state (global.get $TOTAL_SUPPLY_KEY) (i32.const 288))
    drop
    
    (i64.store (i32.const 320) (i64.add (local.get $to_balance) (local.get $amount)))
    (i32.store (i32.const 352) (local.get $to))
    (call $set_state (i32.const 352) (i32.const 320))
    drop
    
    (i32.const 1)
  )
)
