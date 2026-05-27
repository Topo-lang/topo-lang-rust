// Violating purity case, mirroring the Python vertical slice's
// `T4ZeroDeclarationCheck.VIOLATING`: identical to compliant.rs except
// `audit` hides a global mutable-state write, which a parallel-stage
// handler must not have. The emitted .topo and registration surface are
// the same as the compliant case — only the handler body differs — so
// detecting it is purely the existing PurityCheck's job.

static mut LOG: i64 = 0;

pub fn parse(raw: i64) -> i64 {
    raw + 1
}

pub fn enrich(v: i64) -> i64 {
    v * 2
}

pub fn audit(v: i64) -> i64 {
    // Hidden global write: this is the purity violation.
    unsafe {
        LOG += v;
    }
    v
}

pub fn total(v: i64) -> f64 {
    v as f64 + 0.5
}

pub fn build_app() -> topo::App {
    let mut app = topo::App::new("orders");
    app.handler(parse);
    app.handler(enrich);
    app.handler(audit);
    app.handler(total);
    app.flow("pipeline", parse, parallel(enrich, audit), total);
    app
}
