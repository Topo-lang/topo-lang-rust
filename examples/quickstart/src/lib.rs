pub mod orders {
    // --- Domain Types ---

    pub struct Order {
        order_id: i32,
        cust_id: i32,
        amount: f64,
        items: i32,
    }

    impl Order {
        pub fn new(id: i32, customer_id: i32) -> Self {
            Self { order_id: id, cust_id: customer_id, amount: 99.95, items: 3 }
        }
        pub fn id(&self) -> i32 { self.order_id }
        pub fn customer_id(&self) -> i32 { self.cust_id }
        pub fn total(&self) -> f64 { self.amount }
        pub fn item_count(&self) -> i32 { self.items }
        pub fn is_valid(&self) -> bool { self.items > 0 && self.amount > 0.0 }
    }

    pub struct PaymentResult {
        is_approved: bool,
        txn_id: i32,
        charged: f64,
    }

    impl PaymentResult {
        pub fn approved(&self) -> bool { self.is_approved }
        pub fn transaction_id(&self) -> i32 { self.txn_id }
        pub fn charged_amount(&self) -> f64 { self.charged }
    }

    pub struct ShippingQuote {
        ship_cost: f64,
        days: i32,
        carrier: i32,
    }

    impl ShippingQuote {
        pub fn cost(&self) -> f64 { self.ship_cost }
        pub fn estimated_days(&self) -> i32 { self.days }
        pub fn carrier_id(&self) -> i32 { self.carrier }
    }

    pub struct Invoice {
        inv_number: i32,
        total: f64,
        finalized: bool,
    }

    impl Invoice {
        pub fn new(order_id: i32, transaction_id: i32, total: f64) -> Self {
            Self { inv_number: order_id * 1000 + transaction_id, total, finalized: true }
        }
        pub fn number(&self) -> i32 { self.inv_number }
        pub fn grand_total(&self) -> f64 { self.total }
        pub fn is_finalized(&self) -> bool { self.finalized }
    }

    // --- Private helpers ---

    fn check_inventory(_item_id: i32, _quantity: i32) -> bool {
        true // simulated: always in stock
    }

    fn verify_address(_customer_id: i32) -> bool {
        true // simulated: address always valid
    }

    fn apply_discount(total: f64, customer_id: i32) -> f64 {
        // VIP customers (id < 100) get 10% off
        if customer_id < 100 { total * 0.9 } else { total }
    }

    // --- Internal ---

    #[doc(hidden)]
    pub(crate) fn dump_order_state(order: &Order) {
        println!("[DEBUG] Order #{}: customer={}, total={:.2}, items={}, valid={}",
                 order.id(), order.customer_id(), order.total(),
                 order.item_count(), order.is_valid());
    }

    // --- Protected ---

    pub(crate) fn validate_order(order: &Order) -> bool {
        if !order.is_valid() { return false; }
        if !check_inventory(order.id(), order.item_count()) { return false; }
        if !verify_address(order.customer_id()) { return false; }
        true
    }

    pub(crate) fn charge_payment(order: &Order) -> PaymentResult {
        let charged = apply_discount(order.total(), order.customer_id());
        PaymentResult { is_approved: true, txn_id: order.id() * 10 + 1, charged }
    }

    pub(crate) fn calculate_shipping(order: &Order) -> ShippingQuote {
        let cost = 5.0 + order.item_count() as f64 * 1.5;
        let days = if order.item_count() <= 5 { 3 } else { 7 };
        ShippingQuote { ship_cost: cost, days, carrier: 42 }
    }

    pub(crate) fn create_invoice(
        order: &Order,
        payment: &PaymentResult,
        shipping: &ShippingQuote,
    ) -> Invoice {
        let grand_total = payment.charged_amount() + shipping.cost();
        Invoice::new(order.id(), payment.transaction_id(), grand_total)
    }

    // --- Private ---

    fn send_confirmation(invoice: &Invoice) {
        println!("Confirmation: Invoice #{}, total ${:.2}",
                 invoice.number(), invoice.grand_total());
    }

    fn update_analytics(order: &Order, invoice: &Invoice) {
        println!("Analytics: order={}, invoice={}, total=${:.2}",
                 order.id(), invoice.number(), invoice.grand_total());
    }

    // --- Public ---

    pub fn process_order(order: &Order) -> Invoice {
        validate_order(order);

        let payment = charge_payment(order);
        let shipping = calculate_shipping(order);

        let invoice = create_invoice(order, &payment, &shipping);

        send_confirmation(&invoice);
        update_analytics(order, &invoice);

        invoice
    }
}
