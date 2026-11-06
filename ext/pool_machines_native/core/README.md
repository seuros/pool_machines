# pool-machines

> **AMS Little**, NCC-4214, *Little*-class service tender.

Engine-room flight manual: track resource checkouts, returns, and retirement.

Allocation-free `no_std` lifecycle decisions for fixed resource slots.
Register externally created resources, check out the oldest idle slot, and return
it with a generation-checked handle. Duplicate/stale returns are rejected.
Begin draining to refuse new checkouts; returned resources then retire rather
than becoming idle. Idle retirement frees a slot for external destruction.

Clock observations must not regress. Generations fail closed at exhaustion.
Handles are scoped to one pool instance/lifetime; do not exchange handles between
pools. The adapter must serialize these decisions with resource storage and
actually destroy retired resources. There is no automatic construction, health
check, async waiting, timeout driver, forced reclamation of busy resources,
or runtime ownership guarantee.
