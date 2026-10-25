use rbitcoin_node::process_main;
use std::process::ExitCode;

#[global_allocator]
#[doc(hidden)]
pub static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[doc(hidden)]
pub fn main() -> ExitCode {
    process_main()
}
