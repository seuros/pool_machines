use magnus::{Error, Ruby};

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    let module = ruby.define_module("PoolMachines")?;
    crate::pool::define(ruby, module)
}
