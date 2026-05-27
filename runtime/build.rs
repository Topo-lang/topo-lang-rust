//! Build script for the `topo` crate.
//!
//! Configures native library search paths and link directives for the
//! Topo C runtime libraries (libtopo-parallel, libtopo-adaptive, libtopo-jit).
//!
//! Users can set the following environment variables:
//!   TOPO_RUNTIME_LIB_DIR — path to the directory containing the compiled
//!                           Topo runtime libraries (.a / .lib / .so / .dll)
//!
//! If TOPO_RUNTIME_LIB_DIR is not set, the build script walks up from
//! `topo-lang-rust/runtime/` looking for a sibling `build/` directory.

fn main() {
    // Determine library search path
    if let Ok(lib_dir) = std::env::var("TOPO_RUNTIME_LIB_DIR") {
        println!("cargo:rustc-link-search=native={}", lib_dir);
    } else {
        // Default: look for build output relative to the workspace.
        // Manifest dir = `<repo>/topo-lang-rust/runtime/`; the toolchain
        // `build/` directory lives at `<repo>/build/`, so two `.parent()`
        // calls land on `<repo>`. (The fallback used to walk one level
        // too far when the layout was `topo-lang/rust/topo/runtime`; the
        // current two-step walk matches the present directory tree.)
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
            .expect("CARGO_MANIFEST_DIR not set");
        let project_root = std::path::Path::new(&manifest_dir)
            .parent()  // topo-lang-rust/
            .and_then(|p| p.parent()); // <repo>/

        if let Some(root) = project_root {
            let build_dir = root.join("build");
            if build_dir.exists() {
                // CMake places libraries in build/topo-sdk/lib/ or build/
                let runtime_lib_dir = build_dir.join("topo-sdk").join("lib");
                if runtime_lib_dir.exists() {
                    println!("cargo:rustc-link-search=native={}", runtime_lib_dir.display());
                }
                println!("cargo:rustc-link-search=native={}", build_dir.display());
            }
        }
    }

    // Re-run if the env var changes
    println!("cargo:rerun-if-env-changed=TOPO_RUNTIME_LIB_DIR");

    // Link Topo runtime libraries (feature-gated)
    if std::env::var("CARGO_FEATURE_ARENA").is_ok() {
        println!("cargo:rustc-link-lib=topo-arena");
    }
    if std::env::var("CARGO_FEATURE_OBSERVE").is_ok() {
        println!("cargo:rustc-link-lib=topo-observe");
    }
    if std::env::var("CARGO_FEATURE_PARALLEL").is_ok() {
        println!("cargo:rustc-link-lib=topo-parallel");
    }
    if std::env::var("CARGO_FEATURE_ADAPTIVE").is_ok() {
        println!("cargo:rustc-link-lib=topo-adaptive");
    }
}
