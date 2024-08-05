use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use thiserror::Error;

// mod error_handling_prac1;
// mod error_prac2;
// mod ex1;
// mod ex2;
// mod ex3;
// mod ex4;
// mod ex4b;
// mod ex4b_ttl;
// mod ex4b_mcsp;
mod ex4c_refactoredTTL;
// mod ex4c_alternativeDeserialize_Impl;

fn main() {
    // ex1::mainEx1()?;
    // ex2::mainEx2();

    // ex4::mainEx4();
    // ex4b_ttl::conc_test();
    // ex4b_ttl::expire_test();
    // ex4b_mcsp::mcsp_test();

    // error_handling_prac1::error_prac_main();
    // error_handling_prac1::error_prac_main();
    // error_prac2::error_prac_main();

    ex4c_refactoredTTL::main();
    
    
}
