// Compliant purity case, mirroring the Python vertical slice's
// `T4ZeroDeclarationCheck.COMPLIANT`: a parallel stage of two
// independent pure handlers. Every handler is a clean value->value
// Functor, so the existing forced PurityCheck must pass with zero
// hand-written .topo.

pub fn parse(raw: i64) -> i64 {
    raw + 1
}

pub fn enrich(v: i64) -> i64 {
    v * 2
}

pub fn audit(v: i64) -> i64 {
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
