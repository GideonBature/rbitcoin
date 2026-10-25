//! Test harness exec of the operator CLI. Integration tests cannot see
//! `CARGO_BIN_EXE` for the `rbitcoin-node` package, and a workspace `cargo test`
//! may run those tests before that package links its bin. This main is
//! the `rbitcoin-node` main and nothing else.

fn main() -> std::process::ExitCode {
    rbitcoin_node::cli_main(std::env::args_os(), rbitcoin_node::NodeEnv::from_env())
}
