use quickstart_rust::orders::{Order, process_order};

fn main() {
    let order = Order::new(42, 7);
    println!("Processing order #{} for customer #{}...",
             order.id(), order.customer_id());

    let invoice = process_order(&order);

    println!("Done: Invoice #{}, grand total ${:.2}",
             invoice.number(), invoice.grand_total());
}
