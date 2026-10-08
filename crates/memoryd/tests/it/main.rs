mod binary;
mod bus;
mod bus_members;
mod desktop;
mod infer;
mod inferd_bus;
mod keyring;
mod peers;
mod persistence;
mod queue;
mod removals;
mod restart;
mod sandbox;
mod settings;
mod support;
// The wire samples live in almanac-core's test support.
#[path = "../../../almanac-core/tests/it/support/mod.rs"]
mod wire;
mod xdg;
