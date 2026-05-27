// showcase — Topo declaration host implementations (data pipeline domain)
//
// Every function here corresponds to a declaration in topo/*.topo.
// Types, constraints, and adapt mappings are implemented as Rust structs,
// traits, and free functions.
//
// Wrapped in `mod pipeline` to match `namespace pipeline` in .topo files.

pub mod pipeline_config;

pub mod pipeline {

use std::sync::{Arc, Weak};

// Re-export the std::import type
pub use crate::pipeline_config::PipelineConfig;

// -- Constraint traits (from types.topo) --------------------------

pub trait Validatable {
    fn validate(&mut self) -> bool;
    fn error_count(&self) -> i32;
}

pub trait Serializable {
    fn serialized_size(&self) -> usize;
    fn serialize(&self, dest: i32);
    fn deserialize(&mut self, src: i32) -> bool;
}

pub trait Indexable: Serializable {
    fn index_key(&self) -> i64;
    fn compare_key(&self, other: &Self) -> i32;
}

// -- Record -------------------------------------------------------

pub struct Record {
    schema: i32,
    count: i32,
    fields: Vec<i32>,
}

impl Record {
    pub fn new(schema_id: i32) -> Self {
        Record {
            schema: schema_id,
            count: 0,
            fields: vec![0; 16],
        }
    }

    pub fn field_count(&self) -> i32 {
        self.count
    }

    pub fn get_field(&self, index: i32) -> i32 {
        if index < 0 || index >= self.count {
            return 0;
        }
        self.fields[index as usize]
    }

    pub fn set_field(&mut self, index: i32, value: i32) {
        if index >= 0 && (index as usize) < self.fields.len() {
            self.fields[index as usize] = value;
            if index >= self.count {
                self.count = index + 1;
            }
        }
    }

    pub fn schema_id(&self) -> i32 {
        self.schema
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

impl Drop for Record {
    fn drop(&mut self) {
        // cleanup
    }
}

// -- Schema -------------------------------------------------------

pub struct Schema {
    schema_id: i32,
    field_count: i32,
}

impl Schema {
    pub fn new(id: i32) -> Self {
        Schema {
            schema_id: id,
            field_count: 8,
        }
    }

    pub fn field_type(&self, _index: i32) -> i32 {
        1 // all fields are i32 type
    }

    pub fn accepts(&self, record: &Record) -> bool {
        record.schema_id() == self.schema_id
    }

    pub fn id(&self) -> i32 {
        self.schema_id
    }

    pub fn default_schema() -> Schema {
        Schema::new(0)
    }
}

// -- Batch --------------------------------------------------------

pub struct Batch {
    count: i32,
    cap: i32,
    records: Vec<Record>,
}

impl Batch {
    pub fn new(capacity: i32) -> Self {
        Batch {
            count: 0,
            cap: capacity,
            records: Vec::with_capacity(capacity as usize),
        }
    }

    pub fn add(&mut self, record: Record) {
        if self.count < self.cap {
            self.records.push(record);
            self.count += 1;
        }
    }

    pub fn get(&self, index: i32) -> &Record {
        &self.records[index as usize]
    }

    pub fn size(&self) -> i32 {
        self.count
    }

    pub fn is_full(&self) -> bool {
        self.count >= self.cap
    }

    pub fn clear(&mut self) {
        self.records.clear();
        self.count = 0;
    }
}

impl Drop for Batch {
    fn drop(&mut self) {
        // cleanup
    }
}

// -- DataSource / FileSource (inheritance via composition) ---------

pub struct DataSource {
    total_bytes: i64,
}

impl DataSource {
    pub fn new() -> Self {
        DataSource { total_bytes: 0 }
    }

    pub fn read(&mut self, batch: &mut Batch) -> i32 {
        let mut r = Record::new(0);
        r.set_field(0, 42);
        batch.add(r);
        self.total_bytes += 64;
        1
    }

    pub fn has_more(&self) -> bool {
        true
    }

    pub fn bytes_read(&self) -> i64 {
        self.total_bytes
    }
}

pub struct FileSource {
    fd: i32,
    offset: i64,
    total_size: i64,
    source: DataSource,
}

impl FileSource {
    pub fn new(path: i32) -> Self {
        FileSource {
            fd: path,
            offset: 0,
            total_size: 1024 * 1024,
            source: DataSource::new(),
        }
    }

    pub fn file_size(&self) -> i64 {
        self.total_size
    }

    pub fn progress(&self) -> f64 {
        if self.total_size == 0 {
            return 1.0;
        }
        self.offset as f64 / self.total_size as f64
    }

    // Delegated methods from DataSource
    pub fn read(&mut self, batch: &mut Batch) -> i32 {
        self.source.read(batch)
    }

    pub fn has_more(&self) -> bool {
        self.source.has_more()
    }

    pub fn bytes_read(&self) -> i64 {
        self.source.bytes_read()
    }
}

// -- Constraint adaptation: Validatable for Record ----------------

pub fn record_validate(record: &mut Record) -> bool {
    !record.is_empty() && record.schema_id() >= 0
}

pub fn record_error_count(record: &Record) -> i32 {
    let mut errors = 0;
    for i in 0..record.field_count() {
        if record.get_field(i) < 0 {
            errors += 1;
        }
    }
    errors
}

impl Validatable for Record {
    fn validate(&mut self) -> bool {
        record_validate(self)
    }

    fn error_count(&self) -> i32 {
        record_error_count(self)
    }
}

// -- Constraint adaptation: Serializable for Record ---------------

pub fn record_serialized_size(record: &Record) -> usize {
    8 + record.field_count() as usize * std::mem::size_of::<i32>()
}

pub fn record_serialize(record: &Record, dest: i32) {
    println!(
        "serialize Record(schema={}, fields={}) -> dest {}",
        record.schema_id(),
        record.field_count(),
        dest
    );
}

pub fn record_deserialize(record: &mut Record, _src: i32) -> bool {
    record.set_field(0, 1);
    true
}

impl Serializable for Record {
    fn serialized_size(&self) -> usize {
        record_serialized_size(self)
    }

    fn serialize(&self, dest: i32) {
        record_serialize(self, dest);
    }

    fn deserialize(&mut self, src: i32) -> bool {
        record_deserialize(self, src)
    }
}

// -- Constraint adaptation: Indexable for Record ------------------

pub fn record_index_key(record: &Record) -> i64 {
    record.schema_id() as i64 * 100000 + record.get_field(0) as i64
}

pub fn record_compare_key(a: &Record, b: &Record) -> i32 {
    let ka = record_index_key(a);
    let kb = record_index_key(b);
    if ka < kb {
        -1
    } else if ka > kb {
        1
    } else {
        0
    }
}

impl Indexable for Record {
    fn index_key(&self) -> i64 {
        record_index_key(self)
    }

    fn compare_key(&self, other: &Self) -> i32 {
        record_compare_key(self, other)
    }
}

// -- Cache<T> template (generic struct) ---------------------------

pub struct Cache<T: Serializable> {
    count: i32,
    max_size: i32,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: Serializable> Cache<T> {
    pub fn new() -> Self {
        Cache {
            count: 0,
            max_size: 1024,
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn get(&self, _key: i64, _out: &mut T) -> bool {
        self.count > 0
    }

    pub fn put(&mut self, _key: i64, _value: &T) {
        if self.count < self.max_size {
            self.count += 1;
        }
    }

    pub fn evict(&mut self, _key: i64) {
        if self.count > 0 {
            self.count -= 1;
        }
    }

    pub fn size(&self) -> i32 {
        self.count
    }

    pub fn clear(&mut self) {
        self.count = 0;
    }
}

// -- Index<T> template (generic struct) ---------------------------

pub struct Index<T: Indexable> {
    count: i32,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: Indexable> Index<T> {
    pub fn new() -> Self {
        Index {
            count: 0,
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn insert(&mut self, _item: &T) {
        self.count += 1;
    }

    pub fn lookup(&self, _key: i64, _out: &mut T) -> bool {
        self.count > 0
    }

    pub fn remove(&mut self, _key: i64) {
        if self.count > 0 {
            self.count -= 1;
        }
    }

    pub fn size(&self) -> i32 {
        self.count
    }
}

// -- Compile-time conditional -------------------------------------

pub fn use_compact_format() {
    println!("using compact record format (32-bit)");
}

pub fn use_wide_format() {
    println!("using wide record format (64-bit)");
}

// -- Ownership functions ------------------------------------------

pub fn submit_batch(batch: Batch) {
    println!("submitted batch with {} records", batch.size());
}

pub fn query_cache(cache: Arc<Cache<Record>>, key: i64) -> i32 {
    let mut r = Record::new(0);
    if cache.get(key, &mut r) {
        r.schema_id()
    } else {
        -1
    }
}

pub fn check_source(source: Weak<DataSource>) -> bool {
    match source.upgrade() {
        Some(s) => s.has_more(),
        None => false,
    }
}

// -- Private helpers ----------------------------------------------

fn compact_cache(threshold: i32) {
    println!("compacting cache entries below threshold {}", threshold);
}

fn rebuild_index_segment(segment_id: i32) {
    println!("rebuilding index segment {}", segment_id);
}

// == pipeline.topo implementations ================================

// -- Pipeline stage functions (protected) -------------------------

pub fn ingest(source_id: i32) -> i32 {
    println!("ingest: reading from source {}", source_id);
    source_id * 100 + 42
}

pub fn validate(raw_data: i32) -> i32 {
    println!("validate: checking data {}", raw_data);
    if raw_data <= 0 { 0 } else { raw_data }
}

pub fn transform(validated: i32) -> i32 {
    println!("transform: processing {}", validated);
    validated * 2 + 1
}

pub fn build_index(validated: i32) -> i32 {
    println!("build_index: indexing {}", validated);
    validated % 1000
}

pub fn store(transformed: i32, indexed: i32) -> i32 {
    println!("store: saving transformed={} indexed={}", transformed, indexed);
    transformed + indexed
}

pub fn notify(store_result: i32) -> i32 {
    println!("notify: pipeline complete, result={}", store_result);
    store_result
}

// -- Pipeline entry (public, critical priority) -------------------

pub fn run_pipeline(source_id: i32) -> i32 {
    let raw = ingest(source_id);
    let validated = validate(raw);
    let transformed = transform(validated);
    let indexed = build_index(validated);
    let stored = store(transformed, indexed);
    notify(stored)
}

// -- Priority-differentiated operations ---------------------------

pub fn flush_buffer(buffer_id: i32) {
    println!("flush_buffer: flushing buffer {} [critical]", buffer_id);
}

pub fn compact_storage() {
    println!("compact_storage: reclaiming space [low priority]");
}

pub fn merge_indices() {
    println!("merge_indices: consolidating index segments [low priority]");
}

pub fn collect_metrics() {
    println!("collect_metrics: gathering stats [background]");
}

pub fn archive_processed(batch_id: i32) {
    println!("archive_processed: archiving batch {} [background]", batch_id);
}

// -- Private pipeline helpers -------------------------------------

fn parse_raw(data: i32) -> i32 {
    data & 0x7FFFFFFF
}

fn build_segment(data: i32, segment_size: i32) -> i32 {
    data / segment_size
}

fn apply_schema(parsed: i32, _schema: &Schema) -> i32 {
    parsed
}

// == session.topo implementations =================================

// -- Lifetime boundary: connection --------------------------------

pub fn connect() {
    println!("connect: establishing connection");
}

pub fn disconnect() {
    println!("disconnect: closing connection");
}

// -- Lifetime boundary: transaction -------------------------------

pub fn begin_transaction() {
    println!("begin_transaction: starting transaction");
}

pub fn end_transaction() {
    println!("end_transaction: committing transaction");
}

// -- Staged session operations (protected) ------------------------

pub fn load_schemas(conn: i32) -> i32 {
    println!("load_schemas: loading from connection {}", conn);
    conn + 10
}

pub fn warm_cache(conn: i32) -> i32 {
    println!("warm_cache: preloading cache from connection {}", conn);
    conn + 20
}

pub fn process_batches(schemas: i32, cache: i32) -> i32 {
    println!("process_batches: schemas={} cache={}", schemas, cache);
    schemas * 2 + cache
}

pub fn report_metrics(count: i32) {
    println!("report_metrics: processed {} records", count);
}

// -- Multi-return (public) ----------------------------------------

pub fn get_status() -> (i32, i32, i32) {
    (1024, 3, 47)
}

// -- Nested namespace: metrics ------------------------------------

pub mod metrics {
    pub fn record_latency(stage_id: i32, ms: f64) {
        println!("metrics: stage {} latency {:.2} ms", stage_id, ms);
    }

    pub fn record_throughput(records_per_sec: i32) {
        println!("metrics: throughput {} rec/s", records_per_sec);
    }

    pub fn get_p99_latency() -> f64 {
        12.5
    }

    pub fn reset_all() {
        println!("metrics: reset all counters");
    }
}

// -- Private session helpers --------------------------------------

fn rollback_on_error() {
    println!("rollback_on_error: reverting changes");
}

fn flush_buffers() {
    println!("flush_buffers: flushing pending writes");
}

fn retry_connection(max_attempts: i32) -> bool {
    println!("retry_connection: up to {} attempts", max_attempts);
    max_attempts > 0
}

// -- Internal -----------------------------------------------------

pub(crate) fn dump_session_state() {
    println!("dump_session_state: [debug]");
}

pub(crate) fn trace_pipeline(batch_id: i32) {
    println!("trace_pipeline: batch {} [debug]", batch_id);
}

// -- Entry point (public) -----------------------------------------

pub fn run() {
    connect();
    let conn = 1;
    let schemas = load_schemas(conn);
    let cache = warm_cache(conn);
    begin_transaction();
    let count = process_batches(schemas, cache);
    end_transaction();
    report_metrics(count);
    disconnect();
}

} // mod pipeline
