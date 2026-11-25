# Performance inventory (lowspec)

Target: RTX 4060 Laptop 8 GB, i7-13620H, 16 GB RAM. Rule: a change ships
only if its output is identical (or it fixes a proven difference from the
console) **and** a benchmark shows it faster. Benchmarks are best of 15 or
more runs, repeated twice; numbers are in each commit message.

Statuses: **shipped**, **measured-no-gain**, **not-identical**,
**needs-game** (only a run of the title can measure it), **once** (runs
once per frame, per submission of a few items, or at load: below what a
frame can show).

## Submission path (`host.rs`, `list.rs`, `queue.rs`)

| Item | Where | Status | Notes |
|---|---|---|---|
| Written pages into the journal without a list | `watch::writes` | shipped (upstream 78bb903) | 30 → 2 ns a call (1 page) |
| Written ranges merged in place | `host::join_ranges` | shipped (upstream 78bb903) | 64 → 30 ns (2 ranges), 395 → 273 ns (128) |
| Batch maps: address hasher | `host::Batch` | shipped | 35k adds 875–896 → 496–514 µs |
| Batch maps reused between submissions | `host::Batch::spare/park` | shipped | 864–901 → 562–572 µs a submission |
| Ask-once spans: address hasher | `host` submit | shipped | 603–609 → 498–505 µs a submission |
| Small per-list/per-flight sets: address hasher | `host` | shipped | 200 keys 4.2–4.5 → 2.8–3.0 µs |
| Write-watch answer into a reused list | `watch::writes_into`, `shadow::update_free` | shipped | 1 page 2654 → 1755 µs per 35k |
| `recycle` takes the idle lock twice | `list::recycle` | measured-no-gain | ~20 ns, a few lists a frame; the reset runs outside the lock on purpose |
| `relay_wait`, `settle` | `host` | needs-game | wait for the card; nothing to remove on the CPU side |
| `read_backs` | `host` | once | maps and copies rows; the trace is behind a switch |
| `run_deferred` steps | `host` | shipped (via Batch) | 35k refresh steps a frame go through `Batch::add` |
| `stale_under_flights` spans | `host` | measured-no-gain | a short list, linear search cheaper than a map at its size |
| `working`, `submitting`, `sweep` | `host` | once | counters; sweep once a second |
| Submission state lock per title command | `list::with_state` | measured-no-gain | uncontended lock (~15 ns); the state is shared with the submitting thread |
| Async submit / present, second queue, merged runs, 512 lists | `queue`, `queue2`, `host` | needs-game | `VICTORY_FAST=1` (opt-in, from victory-fps) |
| Known views (`VICTORY_VIEW_KNOWN`) | `watch` | not-identical | four write-watch tests miss writes with it |

## Copies of title memory (`shadow.rs`, `watch.rs`, `plaincopy.rs`)

| Item | Where | Status | Notes |
|---|---|---|---|
| Plain copy instead of the kernel's | `watch::try_copy` | shipped (upstream, on by default) | |
| Streaming stores from 1 MiB | `watch::try_copy` asm | shipped | 1 MiB 5.9–8.7 → 7.3–10.4 GB/s; 16 MiB 2.5–8.7 → 3.6–11.1 |
| 64 KiB pieces of `rep movsb` | `watch::try_copy` | measured-no-gain | same speed as one `rep movsb` |
| Merge scratch not zeroed before each read | `shadow` card merges | shipped (after AnyXboxX #224) | 64 KiB 2.23 → 1.13 µs |
| Same for `card_merge_relay` | `shadow` | not-identical | the scratch is used after a failed read |
| Changed runs 64 bytes at a time | `shadow::changed_runs` | shipped (fxceless c36971b) | 48 MB, 12 of 16 differ 37.8–38.3 → 28.4–29.3 ms; all differ 15.7–16.4 → 4.0 |
| Near runs joined before upload (`VICTORY_COPY_COALESCE`) | `shadow` | needs-game | AnyXboxX 08f8a1b, never run by its author; assumes the gap bytes match the card's |
| Every title buffer's copy in card memory (`VICTORY_CARD_BUFFERS_MIN_KB`) | `shadow` | needs-game | fxceless: card time 68 → 50 ms a frame, but more VRAM on an 8 GB card |
| GPU compare by groups of 64 threads | `host` card hash | shipped (upstream, on by default) | fxceless: 22–29 → 5.5–8.5 ms a frame |
| `copy_stale` page loop | `shadow` | measured-no-gain | one lock, a few 64 KiB pages a refresh |
| `VICTORY_COPY_PLAIN` (second plain copy) | `plaincopy` | measured-no-gain | superseded by `watch::try_copy` |

## Draws and dispatches (`draw.rs`)

| Item | Where | Status | Notes |
|---|---|---|---|
| Root constants on the stack | `State::bind` | shipped | 27.3–27.6 → 2.7–2.9 ns (4 dwords) |
| Fresh table slots as a bit set | `State::tables` | shipped (AnyXboxX 0f7b43f did the same) | 2 CBVs 35.2–35.5 → 8.0–8.1 ns |
| Render target handles without a list | `list::targets` | shipped | 26.7–27.4 → 0.6–1.1 ns |
| `rtvs` list in `set_targets` | `State::set_targets` | measured-no-gain | each call also makes views through the driver, far dearer |
| Heap list in `bind` | `State::bind` | once | only when the heaps change |
| Per-frame heap pass maps (`RANGES`, `WHOLE`) | `State::bind` | once | a few heaps, once a frame |
| Dispatches over 65,535 groups in parts | `draw`, `indirect` | shipped (AnyXboxX 65ed4cd) | correctness: the bottom tiles of the indirect light were stale |
| Op text, slot text, target names | `op_text`, `slots_text` | once | diagnostics builds only |

## Descriptor heaps (`descriptor.rs`, `descriptor/host.rs`, `heap_pass.rs`)

| Item | Where | Status | Notes |
|---|---|---|---|
| Record fingerprint, four independent lanes | `record_print` | measured-no-gain | 1.34–1.37 → 1.55–1.62 ns a record: the serial form already overlaps records |
| One lock per page of records | `mirror_copied_page` | shipped (upstream) | |
| Mirroring the changed records | `HostHeap::mirror` | needs-game | the driver's view creation dominates |

## Shaders (`victory-shader`)

| Item | Where | Status | Notes |
|---|---|---|---|
| Translation | `emit`, `lower` | once | per program, cached on disk; prewarm (`translate`) at install |
| 64-bit compares on their dwords | `lower` | shipped (AnyXboxX) | correctness |
| `v_interp_p2_f16` op_sel/omod/clamp | `lower` | shipped (AnyXboxX) | correctness |
| Group base for split dispatches | `lower` | shipped (AnyXboxX a43c89e) | correctness |
| NaN guard on colour exports | `lower` | not-identical | changes colours (NaN to 0, infinities to 65504) |
| PSV0 interpolation, wave32 halves | `emit` | needs-game | AnyXboxX 5fd09a7, bc7968e: conflict-heavy, picture changes to be checked in a run |

## Textures (`texture.rs`, `victory-formats`)

| Item | Where | Status | Notes |
|---|---|---|---|
| Big textures hashed on several threads | `texture::content_hash` | shipped (upstream) | |
| XG layouts as Microsoft's library | `victory-hle` `xg` | shipped (AnyXboxX) | correctness; watch for texture artifacts |
| Staging buffer pool | `texture` | needs-game | AnyXboxX #216, off by default there; trades VRAM |
| Detiling | `victory-formats` | needs-game | runs on uploads; per-texture cost depends on the title's traffic |

## GPU side

| Item | Status | Notes |
|---|---|---|
| Frame time on the card (passes, barriers, copy-back) | needs-game | the F9 overlay and `cardmeter` show it in a run |
| Series S profile (lower resolution) | needs-game | faster, but a black screen on some PCs (upstream fe12291) |

## System services (`victory-hle`)

| Item | Where | Status | Notes |
|---|---|---|---|
| Kernel objects (events, threads, waits) | `modules` | measured-no-gain | native Windows handles, no map of ours in between |
| Memory mapping maps (`pages`, `views`, `shown`) | `xmem` | needs-game | per mapping call, not per draw; the title's rate decides |
| DirectStorage lanes and failures | `dstorage` | needs-game | per request while streaming |
| Async calls map | `xasync` | once | a few calls a frame |
| Module names | `host` | once | at load |
| Audio blocks | `audio` | needs-game | per voice buffer |

## Pipelines (`pipeline/make.rs`)

| Item | Where | Status | Notes |
|---|---|---|---|
| A waiter woken only by its own pipeline | `Pending::completion` | shipped (upstream; AnyXboxX 0f7b43f did the same) | |
