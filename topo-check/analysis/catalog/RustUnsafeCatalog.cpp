#include "RustUnsafeCatalog.h"

#include <unordered_set>

namespace topo::check {

UnsafeLevel RustUnsafeCatalog::classifyCall(const std::string& pattern) {
    // Level 4: Language escape mechanisms
    static const std::unordered_set<std::string> escape = {
        "transmute", "std::mem::transmute",
        "from_raw", "into_raw", "as_ptr", "as_mut_ptr",
        // mem family (transmute_copy / zeroed / uninitialized / forget)
        "transmute_copy", "std::mem::transmute_copy", "core::mem::transmute_copy",
        "zeroed", "std::mem::zeroed", "core::mem::zeroed",
        "uninitialized", "std::mem::uninitialized", "core::mem::uninitialized",
        "forget", "std::mem::forget", "core::mem::forget",
        // ptr family (read / write / copy / drop_in_place / swap_nonoverlapping)
        "read", "std::ptr::read", "core::ptr::read",
        "read_unaligned", "std::ptr::read_unaligned", "core::ptr::read_unaligned",
        "read_volatile", "std::ptr::read_volatile", "core::ptr::read_volatile",
        "write", "std::ptr::write", "core::ptr::write",
        "write_unaligned", "std::ptr::write_unaligned", "core::ptr::write_unaligned",
        "write_volatile", "std::ptr::write_volatile", "core::ptr::write_volatile",
        "copy", "std::ptr::copy", "core::ptr::copy",
        "copy_nonoverlapping", "std::ptr::copy_nonoverlapping", "core::ptr::copy_nonoverlapping",
        "swap_nonoverlapping", "std::ptr::swap_nonoverlapping", "core::ptr::swap_nonoverlapping",
        "drop_in_place", "std::ptr::drop_in_place", "core::ptr::drop_in_place",
        // MaybeUninit (raw access into uninitialized memory)
        "assume_init", "MaybeUninit::assume_init",
        "MaybeUninit::as_ptr", "MaybeUninit::as_mut_ptr",
        // Box raw round-trip
        "Box::from_raw", "Box::into_raw", "Box::leak",
        // Rc / Arc raw round-trip + unchecked mutation
        "Rc::from_raw", "Rc::into_raw",
        "Arc::from_raw", "Arc::into_raw", "Arc::get_mut_unchecked", "get_mut_unchecked",
        // Vec / String raw round-trip
        "Vec::from_raw_parts", "String::from_raw_parts", "from_raw_parts",
        // slice raw constructors (also reachable as std::slice::from_raw_parts)
        "slice::from_raw_parts", "std::slice::from_raw_parts", "core::slice::from_raw_parts",
        "slice::from_raw_parts_mut", "std::slice::from_raw_parts_mut",
        "core::slice::from_raw_parts_mut", "from_raw_parts_mut",
    };
    if (escape.count(pattern)) return UnsafeLevel::Escape;

    // Note: `unsafe` blocks are detected at syntax level by the extractor,
    // not through call classification. The extractor marks them as Escape directly.

    // Level 1: System calls
    static const std::unordered_set<std::string> systemCalls = {
        "File::open", "File::create",
        "fs::read", "fs::write", "fs::read_to_string", "fs::remove_file",
        "TcpListener::bind", "TcpStream::connect", "UdpSocket::bind",
        "Command::new", "Command::spawn", "Command::output",
        "Library::new",
        // stdout/stderr output macros (process boundary I/O)
        "println", "eprintln", "print", "eprint",
        "write!", "writeln!",
        // Buffered I/O wrappers (construct file-descriptor-backed writers/readers)
        "BufWriter::new", "BufReader::new",
        // tokio async filesystem operations
        "tokio::fs::read", "tokio::fs::write", "tokio::fs::remove_file",
        "tokio::fs::create_dir", "tokio::fs::copy", "tokio::fs::rename",
        "tokio::fs::read_to_string",
        // tokio async network operations
        "tokio::net::TcpListener::bind", "tokio::net::TcpStream::connect",
        "tokio::net::UdpSocket::bind",
    };
    if (systemCalls.count(pattern)) return UnsafeLevel::System;

    return UnsafeLevel::Safe;
}

UnsafeLevel RustUnsafeCatalog::classifyImport(const std::string& path) {
    // Level 1: System crates/modules
    static const std::unordered_set<std::string> system = {
        "std::fs", "std::io", "std::net", "std::process",
        "std::os",
        "tokio", "async-std", "smol",
    };
    if (system.count(path)) return UnsafeLevel::System;

    // Level 4: FFI escape
    if (path == "libc") return UnsafeLevel::Escape;

    // Level 4: core:: equivalents of dangerous std:: modules
    static const std::unordered_set<std::string> coreEscape = {
        "core::ptr", "core::mem", "core::intrinsics", "core::arch",
    };
    if (coreEscape.count(path)) return UnsafeLevel::Escape;

    // Level 3: Web framework crates
    static const std::unordered_set<std::string> input = {
        "actix_web", "actix-web", "axum", "warp", "rocket", "hyper",
    };
    if (input.count(path)) return UnsafeLevel::Input;

    // Level 2: anything not std::
    if (path.substr(0, 5) != "std::" && path != "core" && path != "alloc") {
        return UnsafeLevel::Dep;
    }

    return UnsafeLevel::Safe;
}

} // namespace topo::check
