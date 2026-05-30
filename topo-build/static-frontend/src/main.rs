//! `topo-app-static-rust` CLI: statically analyze a Rust topo-app
//! source file and print the equivalent `.topo` to stdout.
//!
//! This is the `topo-build`-side static analysis front-end the topo-app
//! design assigns to the compile-time path ("scan framework API calls at
//! compile time → build the logic graph → emit `.topo`"). It executes
//! none of the analyzed program.

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let path = match args.next() {
        Some(p) => p,
        None => {
            eprintln!(
                "usage: topo-app-static-rust <app.rs>\n\
                 statically scans the Rust topo-app registration surface \
                 and emits equivalent .topo (no execution)"
            );
            return ExitCode::from(2);
        }
    };

    let src = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read {path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    match topo_app_static_rust::analyze(&src) {
        Ok(graph) => {
            print!("{}", topo_app_static_rust::emit_topo(&graph));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("static analysis failed: {e}");
            ExitCode::FAILURE
        }
    }
}
