//! YouCraft's recommendation contract and its default algorithm.
//!
//! A recommendation algorithm is a pure function: it receives what a version of the contract
//! gives it and returns an order of the candidates, with no I/O and nothing else to read. The
//! default algorithm is one implementation of the contract, with no privileged access, so that
//! any other implementation, one day a WebAssembly module written by anyone, can take its place.
//! A change to the contract is a new version beside the old one, never an edit.

pub mod default;
pub mod v1;
