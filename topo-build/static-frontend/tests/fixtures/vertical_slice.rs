// Canonical vertical-slice topo-app program, expressed in Rust via the
// registration API. Same handlers / flow as the Python reference
// `topo-lang-python/runtime/test/test_vertical_slice.py::build_app`
// (parse -> validate -> persist over an OrderRec), so the static path's
// output can be compared graph-for-graph against the runtime bridge.
//
// The static path never runs this file; it is parsed statically with `syn`.

topo::record! { OrderRec { id: i64, amount: f64 } }

pub fn parse(raw: String) -> OrderRec {
    OrderRec {
        id: raw.len() as i64,
        amount: 1.0,
    }
}

pub fn validate(order: OrderRec) -> OrderRec {
    order
}

pub fn persist(order: OrderRec) -> bool {
    let _ = order;
    true
}

pub fn build_app() -> topo::App {
    let mut app = topo::App::new("orders");
    app.handler(parse);
    app.handler(validate);
    app.handler(persist);
    app.flow("order_pipeline", parse, validate, persist);
    app
}
