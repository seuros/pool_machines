# Changelog

## [0.5.1](https://github.com/seuros/pool_machines/compare/pool_machines/v0.5.0...pool_machines/v0.5.1) (2026-11-14)


### Bug Fixes

* **deps:** require matryona-machines &gt;= 0.4 ([cff1f7a](https://github.com/seuros/pool_machines/commit/cff1f7a938b6d5f2e47e0e298a1ecdc34eec47fb))

## [0.5.0](https://github.com/seuros/pool_machines/compare/pool_machines/v0.4.0...pool_machines/v0.5.0) (2026-11-14)


### ⚠ BREAKING CHANGES

* name the received class when checkin refuses a handle

### Features

* name the received class when checkin refuses a handle ([572deb9](https://github.com/seuros/pool_machines/commit/572deb9b679f92af73a91f3cde859ba8125525ba))

## [0.4.0](https://github.com/seuros/pool_machines/compare/pool_machines/v0.3.0...pool_machines/v0.4.0) (2026-11-14)


### ⚠ BREAKING CHANGES

* adopt matryona-machines 0.2 (Class::pin, exact_typed, symbols!)

### Features

* adopt matryona-machines 0.2 (Class::pin, exact_typed, symbols!) ([46a30f5](https://github.com/seuros/pool_machines/commit/46a30f5f8e7e2ed55852af8a510e036bf2b0904f))

## [0.3.0](https://github.com/seuros/pool_machines/compare/pool_machines/v0.2.0...pool_machines/v0.3.0) (2026-11-14)


### ⚠ BREAKING CHANGES

* drop flow-machines, record no clock on a retiring checkin, Handle sealed, resolved at load and checked by exact class

### Features

* drop flow-machines, record no clock on a retiring checkin, Handle sealed, resolved at load and checked by exact class ([0af2455](https://github.com/seuros/pool_machines/commit/0af2455e73f0279d0c5bdcd8473f489633ae0ae3))

## [0.2.0](https://github.com/seuros/pool_machines/compare/pool_machines/v0.1.3...pool_machines/v0.2.0) (2026-11-13)


### ⚠ BREAKING CHANGES

* converted to the fleet conventions. One exhaustive Refusal per crate, Result from every operation that can refuse, a refusal changes nothing, provable idempotent repeats are honoured, Ruby mirrors the core. No compatibility aliases. Requires flow-machines >= 0.3. Handles are issued through periscope-machines; Ruby answers symbols and no longer raises on a stale handle.

### Features

* one Refusal, scoped handles, and refusals that leave the clock alone ([f9e176a](https://github.com/seuros/pool_machines/commit/f9e176ae029835068e43bf4af75c3967fea56545))

## [0.1.3](https://github.com/seuros/pool_machines/compare/pool_machines/v0.1.2...pool_machines/v0.1.3) (2026-11-06)


### Bug Fixes

* crates.io keywords and categories for the core ([e65b159](https://github.com/seuros/pool_machines/commit/e65b1592ae2868c0acea23df562fab5b43dab71f))

## [0.1.2](https://github.com/seuros/pool_machines/compare/pool_machines/v0.1.1...pool_machines/v0.1.2) (2026-11-06)


### Bug Fixes

* keep test sources out of the packaged gem and crate ([d256e29](https://github.com/seuros/pool_machines/commit/d256e29c8cb52218216e7c796057a7fef3205b00))

## [0.1.1](https://github.com/seuros/pool_machines/compare/pool_machines-v0.1.0...pool_machines/v0.1.1) (2026-11-06)


### Features

* initial extraction ([030ecb6](https://github.com/seuros/pool_machines/commit/030ecb6977a3559cd397225ab2d38137a46d39b7))
