# Changelog

## [0.3.0](https://github.com/seuros/pool_machines/compare/pool-machines-v0.2.2...pool-machines-v0.3.0) (2026-11-13)


### ⚠ BREAKING CHANGES

* converted to the fleet conventions. One exhaustive Refusal per crate, Result from every operation that can refuse, a refusal changes nothing, provable idempotent repeats are honoured, Ruby mirrors the core. No compatibility aliases. Requires flow-machines >= 0.3. Handles are issued through periscope-machines; Ruby answers symbols and no longer raises on a stale handle.

### Features

* one Refusal, scoped handles, and refusals that leave the clock alone ([f9e176a](https://github.com/seuros/pool_machines/commit/f9e176ae029835068e43bf4af75c3967fea56545))

## [0.2.2](https://github.com/seuros/pool_machines/compare/pool-machines-v0.2.1...pool-machines-v0.2.2) (2026-11-06)


### Bug Fixes

* crates.io keywords and categories for the core ([e65b159](https://github.com/seuros/pool_machines/commit/e65b1592ae2868c0acea23df562fab5b43dab71f))

## [0.2.1](https://github.com/seuros/pool_machines/compare/pool-machines-v0.2.0...pool-machines-v0.2.1) (2026-11-06)


### Bug Fixes

* keep test sources out of the packaged gem and crate ([d256e29](https://github.com/seuros/pool_machines/commit/d256e29c8cb52218216e7c796057a7fef3205b00))

## [0.2.0](https://github.com/seuros/pool_machines/compare/pool-machines-v0.1.0...pool-machines-v0.2.0) (2026-11-06)


### Features

* initial extraction ([030ecb6](https://github.com/seuros/pool_machines/commit/030ecb6977a3559cd397225ab2d38137a46d39b7))
