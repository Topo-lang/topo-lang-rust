# Topo Quickstart: Order Processing (Rust)

## What This Example Does

An order processing system with clear API boundaries and execution stages:

- **public** (`pub`): `process_order()` -- the single entry point
- **protected** (`pub(crate)`): `validate_order()`, `charge_payment()`, `calculate_shipping()`, `create_invoice()` -- reusable processing components
- **private** (no modifier): `send_confirmation()`, `update_analytics()`, `check_inventory()`, `verify_address()`, `apply_discount()` -- implementation details
- **internal** (`#[doc(hidden)] pub`): `dump_order_state()` -- debug/test helper

### What the Declarations Mean

The `.topo` file declares a 4-stage processing pipeline:

```
                        +-------------------+
          Stage 1:      | validate_order()  |
                        +---------+---------+
                                  |
                    +-------------+-------------+
                    |                           |
          Stage 2:  | charge_payment()    calculate_shipping() |
                    |  (parallel-safe)      (parallel-safe)    |
                    +-------------+-------------+
                                  |
          Stage 3:      +---------+---------+
                        | create_invoice()  |
                        +---------+---------+
                                  |
                    +-------------+-------------+
                    |                           |
          Stage 4:  | send_confirmation() update_analytics()   |
                    |  (parallel-safe)      (parallel-safe)    |
                    +-------------+-------------+
```

- **Stage 1**: `validate_order` -- must complete before anything else
- **Stage 2**: `charge_payment` + `calculate_shipping` -- independent operations that can safely run in parallel
- **Stage 3**: `create_invoice` -- depends on both stage-2 results
- **Stage 4**: `send_confirmation` + `update_analytics` -- independent post-processing

Topo enforces these constraints:
- Stage 2 cannot start before stage 1 completes
- `charge_payment` and `calculate_shipping` must not share mutable state (they are declared parallel-safe)
- Code outside the `orders` module cannot call `private` functions like `send_confirmation`

## Try It

### Step 1: Validate declarations

```sh
topo --check topo-lang-rust/examples/quickstart/topo/processor.topo
```

### Step 2: Verify completeness

```sh
topo-test --project topo-lang-rust/examples/quickstart --check-completeness
```

Expected output:

```
[PASS] Completeness: 11 host symbol(s) checked, 11 .topo function(s) -- all OK
```

### Step 3: Add an undeclared function --> ERROR

Add a new function to `src/lib.rs` inside `pub mod orders`:

```rust
pub fn cancel_order(_order_id: i32) -> bool { true }
```

Run the check again:

```sh
topo-test --project topo-lang-rust/examples/quickstart --check-completeness
```

```
ERROR: symbol 'orders::cancel_order' exists in host code
       but is not declared in .topo
```

**Why it matters**: Someone added a new function but forgot to update the contract.
Topo catches this before compilation.

### Step 4: Remove a declared function --> WARNING

Delete the `calculate_shipping()` function from `src/lib.rs`
(keep the `.topo` declaration).

```sh
topo-test --project topo-lang-rust/examples/quickstart --check-completeness
```

```
WARNING: function 'orders::calculate_shipping' is declared in .topo
         but not found in host code
```

### Step 5: Visibility mismatch

Change `send_confirmation()` from private (no modifier) to `pub`:

```rust
pub fn send_confirmation(invoice: &Invoice) { ... }
```

```sh
topo-test --project topo-lang-rust/examples/quickstart --check-completeness
```

```
WARNING: function 'orders::send_confirmation' is declared private in .topo
         but has public visibility in host code
```

Now change `process_order()` from `pub` to private (remove `pub`):

```sh
topo-test --project topo-lang-rust/examples/quickstart --check-completeness
```

```
ERROR: function 'orders::process_order' is declared public in .topo
       but has private visibility in host code
```

### Step 6: Restore and pass

Undo all changes. Run the check again:

```sh
topo-test --project topo-lang-rust/examples/quickstart --check-completeness
```

```
[PASS] Completeness: all OK
```

### Step 7: Verify stage isolation

Stage isolation verifies that stage N does not depend on stage N+1 outputs.
This requires a test command that exercises the pipeline:

```sh
topo-test --project topo-lang-rust/examples/quickstart \
          --check-isolation \
          --test-cmd "echo 'pipeline-test-placeholder'"
```

Expected: each stage passes independently — stage 2 functions (`charge_payment`,
`calculate_shipping`) do not call stage 3 or 4 functions.

### Step 8: Verify parallel safety

Purity check verifies that functions in the same stage (declared parallel-safe)
do not access shared mutable state:

```sh
topo-test --project topo-lang-rust/examples/quickstart \
          --check-purity \
          --test-cmd "echo 'pipeline-test-placeholder'"
```

Expected: stage 2 (`charge_payment` + `calculate_shipping`) and stage 4
(`send_confirmation` + `update_analytics`) pass — no shared global state.

## What You Learned

| Violation | Severity | Topo Report |
|-----------|----------|-------------|
| Code has function, `.topo` does not declare it | ERROR | "exists in host code but not declared" |
| `.topo` declares function, code does not implement it | WARNING | "declared but not found in host code" |
| `.topo` says public, code is private | ERROR | "declared public but private" |
| `.topo` says private, code is public | WARNING | "declared private but public" |
| Stage N calls stage N+1 function | ERROR | "stage isolation violated" |
| Parallel-stage function writes global state | ERROR | "purity violation" |

## What's Next

- **Showcase example**: demonstrates all Topo features -- constraints, templates, pipeline DAG, lifetime management, priority, ownership
