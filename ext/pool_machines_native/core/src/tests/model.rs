//! Randomized runs against an independent naive model.

use std::vec::Vec;

use super::*;

/// Deterministic splitmix64 stream; the tests own their draws.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }

    fn pick(&mut self, len: usize) -> usize {
        usize::try_from(self.below(len as u64)).unwrap_or(0)
    }
}

/// Independent reference: two plain lists, no slots array.
struct Model {
    /// `(slot, idle since)`.
    idle: Vec<(usize, u64)>,
    /// `(slot, handle id)`.
    busy: Vec<(usize, u64)>,
    cap: usize,
    scope: u16,
    issued: u64,
    last: u64,
    observed: u64,
    draining: bool,
}

impl Model {
    /// The id of a sequence number, spelled out here rather than borrowed
    /// from the crate under test: scope in the top 16 bits, sequence below.
    fn id(&self, sequence: u64) -> u64 {
        (u64::from(self.scope) << 48) | sequence
    }

    const fn regressed(&self, now: u64) -> Result<(), Refusal> {
        if now < self.observed {
            return Err(Refusal::TimeRegression);
        }
        Ok(())
    }

    fn register(&mut self, slot: usize, now: u64) -> Result<Registration, Refusal> {
        if self.draining {
            Err(Refusal::Draining)
        } else if now < self.observed {
            Err(Refusal::TimeRegression)
        } else if slot >= self.cap {
            Err(Refusal::UnknownSlot)
        } else if self.busy.iter().any(|b| b.0 == slot) {
            Err(Refusal::Busy)
        } else if self.idle.iter().any(|i| i.0 == slot) {
            Ok(Registration::AlreadyIdle)
        } else {
            self.idle.push((slot, now));
            self.observed = now;
            Ok(Registration::Registered)
        }
    }

    fn checkout(&mut self, now: u64) -> Result<Option<(usize, u64)>, Refusal> {
        if self.draining {
            return Err(Refusal::Draining);
        }
        self.regressed(now)?;
        let Some(&(slot, since)) = self.idle.iter().min_by_key(|i| (i.1, i.0)) else {
            self.observed = now;
            return Ok(None);
        };
        if self.issued == self.last {
            return Err(Refusal::Exhausted);
        }
        self.issued += 1;
        self.observed = now;
        self.idle.retain(|i| i != &(slot, since));
        let id = self.id(self.issued);
        self.busy.push((slot, id));
        Ok(Some((slot, id)))
    }

    fn checkin(&mut self, slot: usize, id: u64, now: u64) -> Result<Returned, Refusal> {
        if !self.busy.contains(&(slot, id)) {
            return Err(Refusal::Stale);
        }
        self.regressed(now)?;
        self.busy.retain(|b| b != &(slot, id));
        if self.draining {
            return Ok(Returned::Retire);
        }
        self.observed = now;
        self.idle.push((slot, now));
        Ok(Returned::Idle)
    }

    fn retire_idle(&mut self, now: u64, idle_for: u64) -> Result<Option<usize>, Refusal> {
        self.regressed(now)?;
        self.observed = now;
        let draining = self.draining;
        let slot = self
            .idle
            .iter()
            .filter(|i| draining || now - i.1 >= idle_for)
            .map(|i| i.0)
            .min();
        self.idle.retain(|i| Some(i.0) != slot);
        Ok(slot)
    }
}

const SLOTS: usize = 5;

/// One run: a pool and its model fed the same calls, the clock mostly
/// advancing and sometimes regressing, handles sometimes spent or forged.
fn run(seed: u64, scope: u16, start: u64, steps: usize) {
    let mut rng = Rng(seed);
    let mut pool = Pool::<SLOTS>::resumed(scope, start).expect("pool");
    let mut model = Model {
        idle: Vec::new(),
        busy: Vec::new(),
        cap: SLOTS,
        scope,
        issued: start,
        last: periscope_machines::SEQUENCE_MAX,
        observed: 0,
        draining: false,
    };
    // Every handle ever issued, plus a few no pool issued.
    let mut handles = Vec::from([
        Handle { slot: 0, key: 0 },
        Handle {
            slot: 1,
            key: model.id(start + 1),
        },
        Handle {
            slot: 2,
            key: (u64::from(scope ^ 1) << 48) | 1,
        },
        Handle {
            slot: SLOTS,
            key: model.id(1),
        },
    ]);
    for _ in 0..steps {
        let now = match rng.below(8) {
            0 => model.observed.saturating_sub(1 + rng.below(3)),
            1 => model.observed,
            _ => model.observed + rng.below(4),
        };
        let before = pool.clone();
        let unchanged = match rng.below(16) {
            0..=3 => {
                let slot = rng.pick(SLOTS + 2);
                let expected = model.register(slot, now);
                assert_eq!(pool.register(slot, at(now)), expected);
                expected != Ok(Registration::Registered)
            }
            4..=7 => {
                let expected = model.checkout(now);
                let actual = pool.checkout(at(now));
                assert_eq!(actual.map(|h| h.map(|h| (h.slot(), h.key))), expected);
                if let Ok(Some(handle)) = actual {
                    assert_eq!(handle.scope(), scope);
                    handles.push(handle);
                }
                expected.is_err()
            }
            8..=11 => {
                let handle = handles[rng.pick(handles.len())];
                let expected = model.checkin(handle.slot, handle.key, now);
                assert_eq!(pool.checkin(handle, at(now)), expected);
                expected.is_err()
            }
            12..=14 => {
                let idle_for = rng.below(6);
                let expected = model.retire_idle(now, idle_for);
                assert_eq!(pool.retire_idle(at(now), at(idle_for)), expected);
                expected.is_err()
            }
            _ => {
                if rng.below(12) != 0 {
                    continue;
                }
                let expected = if model.draining {
                    DrainBegin::AlreadyDraining
                } else {
                    DrainBegin::Begun
                };
                model.draining = true;
                assert_eq!(pool.begin_drain(), expected);
                expected == DrainBegin::AlreadyDraining
            }
        };
        if unchanged {
            assert_eq!(
                pool, before,
                "a refusal or an honoured repeat changed the pool"
            );
        }
        assert_eq!(pool.checked_out() as usize, model.busy.len());
        assert_eq!(pool.issued(), model.issued);
        assert_eq!(pool.is_draining(), model.draining);
        assert_eq!(pool.observed, at(model.observed));
    }
}

#[test]
fn random_runs_agree_with_the_model() {
    for seed in 0..300 {
        run(seed, (seed % 5) as u16, 0, 400);
    }
}

#[test]
fn random_runs_agree_with_the_model_at_the_end_of_the_sequence() {
    for seed in 0..100 {
        let left = seed % 9;
        run(seed, 3, periscope_machines::SEQUENCE_MAX - left, 300);
    }
}
