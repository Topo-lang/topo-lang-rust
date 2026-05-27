// Minimal Rust span-emitter for the topo-profile demo.
//
// FFI-binds the libtopo-observe C ABI (declared in topo-llvm/runtime/
// include/topo/rt/observe_rt.h) and emits three named spans following
// the `pipeline::<name>::stage<N>` convention. The fixture is built with
// rustc -g + an explicit `-l static=topo-observe` flag fed by the CMake
// glue — no Cargo needed, the binary lands directly in the build tree.

extern "C" {
    fn topo_trace_init(exporter: *const u8, sampling_rate: f64);
    fn topo_trace_span_begin(name: *const u8);
    fn topo_trace_span_end();
    fn topo_trace_shutdown();
}

#[inline(never)]
fn busy(micros: u64) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_micros(micros);
    let mut acc: u64 = 0;
    while std::time::Instant::now() < deadline {
        for i in 0..1000u64 {
            acc ^= i;
        }
    }
    std::hint::black_box(acc);
}

fn main() {
    unsafe {
        topo_trace_init(b"stdout\0".as_ptr(), 1.0);

        // Total wall ≥15 ms keeps the child alive past PipedProcess's 5 ms
        // exec-failure detection window; tighter spans would race the parent.
        topo_trace_span_begin(b"pipeline::demo::stage0\0".as_ptr());
        busy(5000);
        topo_trace_span_end();

        topo_trace_span_begin(b"pipeline::demo::stage1\0".as_ptr());
        busy(5000);
        topo_trace_span_end();

        topo_trace_span_begin(b"pipeline::demo::stage2\0".as_ptr());
        busy(5000);
        topo_trace_span_end();

        topo_trace_shutdown();
    }
}
