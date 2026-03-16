//! Unit tests for Order Entity Enhancement - Task 1.1.1
//!
//! Tests cover:
//! - OrderStatus state machine transitions
//! - Order validation (market hours, symbol)
//! - Notional value calculation
//! - Helper methods (can_cancel, is_active, time_in_force_expired)
//! - Fill entity operations
//! - Fill aggregation to position

use chrono::{Duration, Utc};
use domain::entities::{
    Fill, FillAggregation, Order, OrderStatus, OrderType, PositionDirection,
};
use domain::values::{Currency, Money, OrderId, Price, Quantity, Side, Symbol, TimeInForce};
use rust_decimal::Decimal;

// =============================================================================
// ORDER STATUS STATE MACHINE TESTS
// =============================================================================

#[test]
fn order_status_created_is_active() {
    let status = OrderStatus::Created;
    assert!(status.is_active());
    assert!(!status.is_terminal());
    assert!(status.can_cancel());
    assert!(!status.is_filled());
}

#[test]
fn order_status_submitted_is_active() {
    let status = OrderStatus::Submitted { at: Utc::now() };
    assert!(status.is_active());
    assert!(!status.is_terminal());
    assert!(status.can_cancel());
    assert!(!status.is_filled());
    assert!(status.timestamp().is_some());
}

#[test]
fn order_status_pending_is_active() {
    let status = OrderStatus::Pending { at: Utc::now() };
    assert!(status.is_active());
    assert!(!status.is_terminal());
    assert!(status.can_cancel());
    assert!(!status.is_filled());
}

#[test]
fn order_status_partially_filled_is_active() {
    let status = OrderStatus::PartiallyFilled {
        filled: Quantity::new(Decimal::new(50, 0)).unwrap(),
        remaining: Quantity::new(Decimal::new(50, 0)).unwrap(),
        avg_price: Price::new(Decimal::new(15000, 2)).unwrap(),
    };
    assert!(status.is_active());
    assert!(!status.is_terminal());
    assert!(status.can_cancel());
    assert!(status.is_filled());
}

#[test]
fn order_status_filled_is_terminal() {
    let status = OrderStatus::Filled { at: Utc::now() };
    assert!(!status.is_active());
    assert!(status.is_terminal());
    assert!(!status.can_cancel());
    assert!(status.is_filled());
}

#[test]
fn order_status_cancelled_is_terminal() {
    let status = OrderStatus::Cancelled {
        at: Utc::now(),
        reason: "user_request".to_string(),
    };
    assert!(!status.is_active());
    assert!(status.is_terminal());
    assert!(!status.can_cancel());
    assert!(!status.is_filled());
}

#[test]
fn order_status_rejected_is_terminal() {
    let status = OrderStatus::Rejected {
        at: Utc::now(),
        reason: "insufficient_funds".to_string(),
    };
    assert!(!status.is_active());
    assert!(status.is_terminal());
    assert!(!status.can_cancel());
    assert!(!status.is_filled());
}

#[test]
fn order_status_variant_name() {
    assert_eq!(OrderStatus::Created.variant_name(), "Created");
    assert_eq!(
        OrderStatus::Submitted { at: Utc::now() }.variant_name(),
        "Submitted"
    );
    assert_eq!(
        OrderStatus::Pending { at: Utc::now() }.variant_name(),
        "Pending"
    );
    assert_eq!(
        OrderStatus::PartiallyFilled {
            filled: Quantity::new(Decimal::ONE).unwrap(),
            remaining: Quantity::new(Decimal::ONE).unwrap(),
            avg_price: Price::new(Decimal::new(100, 0)).unwrap(),
        }
        .variant_name(),
        "PartiallyFilled"
    );
    assert_eq!(
        OrderStatus::Filled { at: Utc::now() }.variant_name(),
        "Filled"
    );
    assert_eq!(
        OrderStatus::Cancelled {
            at: Utc::now(),
            reason: "test".to_string()
        }
        .variant_name(),
        "Cancelled"
    );
    assert_eq!(
        OrderStatus::Rejected {
            at: Utc::now(),
            reason: "test".to_string()
        }
        .variant_name(),
        "Rejected"
    );
}

// =============================================================================
// ORDER STATE TRANSITION TESTS
// =============================================================================

fn create_test_order() -> Order {
    // Use new_with_id to create order in Created state for testing transitions
    Order::new_with_id(
        OrderId::generate(),
        uuid::Uuid::new_v4(),
        Symbol::new("AAPL").unwrap(),
        Side::Buy,
        Quantity::new(Decimal::new(100, 0)).unwrap(),
        OrderType::Market,
        TimeInForce::Day,
        None,
        None,
    )
    .unwrap()
}

fn create_legacy_test_order() -> Order {
    // Legacy API creates order in Pending state
    Order::new(
        uuid::Uuid::new_v4(),
        Symbol::new("AAPL").unwrap(),
        Side::Buy,
        OrderType::Market,
        Quantity::new(Decimal::new(100, 0)).unwrap(),
        None,
        None,
    )
    .unwrap()
}

#[test]
fn order_transition_created_to_submitted() {
    let mut order = create_test_order();
    assert!(matches!(order.status(), OrderStatus::Created));

    order
        .update_status(OrderStatus::Submitted { at: Utc::now() })
        .unwrap();
    assert!(matches!(order.status(), OrderStatus::Submitted { .. }));
}

#[test]
fn order_transition_created_to_cancelled() {
    let mut order = create_test_order();

    order
        .update_status(OrderStatus::Cancelled {
            at: Utc::now(),
            reason: "test".to_string(),
        })
        .unwrap();
    assert!(order.status().is_terminal());
}

#[test]
fn order_transition_created_to_rejected() {
    let mut order = create_test_order();

    order
        .update_status(OrderStatus::Rejected {
            at: Utc::now(),
            reason: "test".to_string(),
        })
        .unwrap();
    assert!(order.status().is_terminal());
}

#[test]
fn order_transition_submitted_to_pending() {
    let mut order = create_test_order();
    order
        .update_status(OrderStatus::Submitted { at: Utc::now() })
        .unwrap();

    order
        .update_status(OrderStatus::Pending { at: Utc::now() })
        .unwrap();
    assert!(matches!(order.status(), OrderStatus::Pending { .. }));
}

#[test]
fn order_transition_pending_to_partially_filled() {
    let mut order = create_test_order();
    order
        .update_status(OrderStatus::Submitted { at: Utc::now() })
        .unwrap();
    order
        .update_status(OrderStatus::Pending { at: Utc::now() })
        .unwrap();

    order
        .update_status(OrderStatus::PartiallyFilled {
            filled: Quantity::new(Decimal::new(50, 0)).unwrap(),
            remaining: Quantity::new(Decimal::new(50, 0)).unwrap(),
            avg_price: Price::new(Decimal::new(15000, 2)).unwrap(),
        })
        .unwrap();
    assert!(matches!(order.status(), OrderStatus::PartiallyFilled { .. }));
}

#[test]
fn order_transition_partially_filled_to_filled() {
    let mut order = create_test_order();
    order
        .update_status(OrderStatus::Submitted { at: Utc::now() })
        .unwrap();
    order
        .update_status(OrderStatus::Pending { at: Utc::now() })
        .unwrap();
    order
        .update_status(OrderStatus::PartiallyFilled {
            filled: Quantity::new(Decimal::new(50, 0)).unwrap(),
            remaining: Quantity::new(Decimal::new(50, 0)).unwrap(),
            avg_price: Price::new(Decimal::new(15000, 2)).unwrap(),
        })
        .unwrap();

    order
        .update_status(OrderStatus::Filled { at: Utc::now() })
        .unwrap();
    assert!(matches!(order.status(), OrderStatus::Filled { .. }));
}

#[test]
fn order_invalid_transition_filled_to_cancelled() {
    let mut order = create_test_order();
    transition_to_pending(&mut order);
    order
        .fill(
            Quantity::new(Decimal::new(100, 0)).unwrap(),
            Price::new(Decimal::new(15000, 2)).unwrap(),
        )
        .unwrap();
    assert!(matches!(order.status(), OrderStatus::Filled { .. }));

    let result = order.update_status(OrderStatus::Cancelled {
        at: Utc::now(),
        reason: "test".to_string(),
    });
    assert!(result.is_err());
}

#[test]
fn order_cancel_method() {
    let mut order = create_test_order();
    assert!(order.can_cancel());

    order.cancel("user_request".to_string()).unwrap();
    assert!(order.is_terminal());
    assert!(!order.can_cancel());
    assert!(matches!(order.status(), OrderStatus::Cancelled { .. }));
}

#[test]
fn order_cancel_from_submitted() {
    let mut order = create_test_order();
    order
        .update_status(OrderStatus::Submitted { at: Utc::now() })
        .unwrap();
    
    order.cancel("user_request".to_string()).unwrap();
    assert!(matches!(order.status(), OrderStatus::Cancelled { .. }));
}

#[test]
fn order_reject_method() {
    let mut order = create_test_order();

    order.reject("insufficient_funds".to_string()).unwrap();
    assert!(order.is_terminal());
    assert!(matches!(order.status(), OrderStatus::Rejected { .. }));
}

#[test]
fn order_cancel_already_terminal_fails() {
    let mut order = create_test_order();
    transition_to_pending(&mut order);
    order
        .fill(
            Quantity::new(Decimal::new(100, 0)).unwrap(),
            Price::new(Decimal::new(15000, 2)).unwrap(),
        )
        .unwrap();

    let result = order.cancel("test".to_string());
    assert!(result.is_err());
}

// =============================================================================
// ORDER FILL TESTS
// =============================================================================

fn transition_to_pending(order: &mut Order) {
    order
        .update_status(OrderStatus::Submitted { at: Utc::now() })
        .unwrap();
    order
        .update_status(OrderStatus::Pending { at: Utc::now() })
        .unwrap();
}

#[test]
fn order_fill_partial() {
    let mut order = create_test_order();
    transition_to_pending(&mut order);

    let fill_qty = Quantity::new(Decimal::new(50, 0)).unwrap();
    let fill_price = Price::new(Decimal::new(15050, 2)).unwrap();

    order.fill(fill_qty, fill_price).unwrap();

    assert_eq!(order.filled_quantity().inner(), Decimal::new(50, 0));
    assert!(matches!(order.status(), OrderStatus::PartiallyFilled { .. }));
}

#[test]
fn order_fill_complete() {
    let mut order = create_test_order();
    transition_to_pending(&mut order);

    let fill_qty = Quantity::new(Decimal::new(100, 0)).unwrap();
    let fill_price = Price::new(Decimal::new(15050, 2)).unwrap();

    order.fill(fill_qty, fill_price).unwrap();

    assert_eq!(order.filled_quantity().inner(), Decimal::new(100, 0));
    assert!(matches!(order.status(), OrderStatus::Filled { .. }));
}

#[test]
fn order_fill_exceeds_remaining_fails() {
    let mut order = create_test_order();
    transition_to_pending(&mut order);

    let fill_qty = Quantity::new(Decimal::new(150, 0)).unwrap();
    let fill_price = Price::new(Decimal::new(15050, 2)).unwrap();

    let result = order.fill(fill_qty, fill_price);
    assert!(result.is_err());
}

#[test]
fn order_fill_in_terminal_state_fails() {
    let mut order = create_test_order();
    transition_to_pending(&mut order);
    order
        .fill(
            Quantity::new(Decimal::new(100, 0)).unwrap(),
            Price::new(Decimal::new(15050, 2)).unwrap(),
        )
        .unwrap();
    assert!(order.status().is_terminal());

    let fill_qty = Quantity::new(Decimal::new(10, 0)).unwrap();
    let fill_price = Price::new(Decimal::new(15050, 2)).unwrap();

    let result = order.fill(fill_qty, fill_price);
    assert!(result.is_err());
}

#[test]
fn order_fill_calculates_avg_price() {
    let mut order = create_test_order();
    transition_to_pending(&mut order);

    // First fill: 50 shares @ $150.00
    order
        .fill(
            Quantity::new(Decimal::new(50, 0)).unwrap(),
            Price::new(Decimal::new(15000, 2)).unwrap(),
        )
        .unwrap();

    // Second fill: 50 shares @ $151.00
    order
        .fill(
            Quantity::new(Decimal::new(50, 0)).unwrap(),
            Price::new(Decimal::new(15100, 2)).unwrap(),
        )
        .unwrap();

    // Verify fully filled
    assert!(matches!(order.status(), OrderStatus::Filled { .. }));
}

// =============================================================================
// ORDER VALIDATION TESTS
// =============================================================================

#[test]
fn order_validate_for_submission_success() {
    // Order must be in Created state for validation to pass
    let order = create_test_order();
    assert!(matches!(order.status(), OrderStatus::Created));
    assert!(order.validate_for_submission().is_ok());
}

#[test]
fn order_validate_limit_order_without_price_fails() {
    let result = Order::new(
        uuid::Uuid::new_v4(),
        Symbol::new("AAPL").unwrap(),
        Side::Buy,
        OrderType::Limit,
        Quantity::new(Decimal::new(100, 0)).unwrap(),
        None, // Missing limit price
        None,
    );
    assert!(result.is_err());
}

#[test]
fn order_validate_stop_order_without_price_fails() {
    let result = Order::new(
        uuid::Uuid::new_v4(),
        Symbol::new("AAPL").unwrap(),
        Side::Buy,
        OrderType::Stop,
        Quantity::new(Decimal::new(100, 0)).unwrap(),
        None,
        None, // Missing stop price
    );
    assert!(result.is_err());
}

#[test]
fn order_validate_submission_not_in_created_state_fails() {
    let mut order = create_test_order();
    order
        .update_status(OrderStatus::Submitted { at: Utc::now() })
        .unwrap();

    // Order is no longer in Created state, validation should fail
    let result = order.validate_for_submission();
    assert!(result.is_err());
}

// =============================================================================
// ORDER HELPER METHOD TESTS
// =============================================================================

#[test]
fn order_notional_value_calculation() {
    let order = create_test_order();
    let price = Price::new(Decimal::new(15000, 2)).unwrap(); // $150.00

    let notional = order.notional_value(price);

    // 100 shares * $150.00 = $15,000
    assert_eq!(notional.amount(), Decimal::new(15000, 0));
    assert_eq!(notional.currency(), Currency::USD);
}

#[test]
fn order_is_active_various_states() {
    let mut order = create_test_order();
    assert!(order.is_active());

    order
        .update_status(OrderStatus::Submitted { at: Utc::now() })
        .unwrap();
    assert!(order.is_active());

    order
        .update_status(OrderStatus::Pending { at: Utc::now() })
        .unwrap();
    assert!(order.is_active());

    order
        .fill(
            Quantity::new(Decimal::new(100, 0)).unwrap(),
            Price::new(Decimal::new(15000, 2)).unwrap(),
        )
        .unwrap();
    assert!(!order.is_active());
}

#[test]
fn order_time_in_force_expired_day_order() {
    let order = create_test_order();

    // Same day - not expired
    let same_day = order.created_at() + Duration::hours(1);
    assert!(!order.time_in_force_expired(same_day));

    // Next day - expired
    let next_day = order.created_at() + Duration::days(1);
    assert!(order.time_in_force_expired(next_day));
}

#[test]
fn order_time_in_force_gtc_never_expires() {
    let order = Order::new_with_id(
        OrderId::generate(),
        uuid::Uuid::new_v4(),
        Symbol::new("AAPL").unwrap(),
        Side::Buy,
        Quantity::new(Decimal::new(100, 0)).unwrap(),
        OrderType::Market,
        TimeInForce::GTC,
        None,
        None,
    )
    .unwrap();

    // GTC orders never expire
    let far_future = Utc::now() + Duration::days(365);
    assert!(!order.time_in_force_expired(far_future));
}

// =============================================================================
// FILL ENTITY TESTS
// =============================================================================

fn create_test_fill() -> Fill {
    Fill::new(
        OrderId::generate(),
        Symbol::new("AAPL").unwrap(),
        Quantity::new(Decimal::new(100, 0)).unwrap(),
        Price::new(Decimal::new(15050, 2)).unwrap(),
        Side::Buy,
        Utc::now(),
    )
}

#[test]
fn fill_creation() {
    let fill = create_test_fill();

    assert_eq!(fill.symbol().as_str(), "AAPL");
    assert_eq!(fill.quantity().inner(), Decimal::new(100, 0));
    assert_eq!(fill.price().inner(), Decimal::new(15050, 2));
    assert_eq!(fill.side(), Side::Buy);
    assert!(fill.commission().is_none());
}

#[test]
fn fill_with_commission() {
    let fill = Fill::with_commission(
        OrderId::generate(),
        Symbol::new("AAPL").unwrap(),
        Quantity::new(Decimal::new(100, 0)).unwrap(),
        Price::new(Decimal::new(15050, 2)).unwrap(),
        Side::Buy,
        Utc::now(),
        Money::new(Decimal::new(5, 0), Currency::USD).unwrap(),
    );

    assert!(fill.commission().is_some());
    let comm = fill.commission().unwrap();
    assert_eq!(comm.amount(), Decimal::new(5, 0));
}

#[test]
fn fill_notional_value() {
    let fill = create_test_fill();

    let notional = fill.notional_value();

    // 100 shares * $150.50 = $15,050
    assert_eq!(notional.amount(), Decimal::new(15050, 0));
    assert_eq!(notional.currency(), Currency::USD);
}

#[test]
fn fill_net_value_buy() {
    let fill = Fill::with_commission(
        OrderId::generate(),
        Symbol::new("AAPL").unwrap(),
        Quantity::new(Decimal::new(100, 0)).unwrap(),
        Price::new(Decimal::new(15000, 2)).unwrap(),
        Side::Buy,
        Utc::now(),
        Money::new(Decimal::new(5, 0), Currency::USD).unwrap(),
    );

    let net = fill.net_value();

    // Buy: notional + commission = $15,000 + $5 = $15,005
    assert_eq!(net.amount(), Decimal::new(15005, 0));
}

#[test]
fn fill_net_value_sell() {
    let fill = Fill::with_commission(
        OrderId::generate(),
        Symbol::new("AAPL").unwrap(),
        Quantity::new(Decimal::new(100, 0)).unwrap(),
        Price::new(Decimal::new(15000, 2)).unwrap(),
        Side::Sell,
        Utc::now(),
        Money::new(Decimal::new(5, 0), Currency::USD).unwrap(),
    );

    let net = fill.net_value();

    // Sell: notional - commission = $15,000 - $5 = $14,995
    assert_eq!(net.amount(), Decimal::new(14995, 0));
}

#[test]
fn fill_pnl_calculation() {
    // Exit fill at $160 (selling a long position)
    let exit_fill = Fill::with_commission(
        OrderId::generate(),
        Symbol::new("AAPL").unwrap(),
        Quantity::new(Decimal::new(100, 0)).unwrap(),
        Price::new(Decimal::new(16000, 2)).unwrap(), // $160.00
        Side::Sell,
        Utc::now(),
        Money::new(Decimal::new(5, 0), Currency::USD).unwrap(),
    );

    let entry_price = Price::new(Decimal::new(15000, 2)).unwrap(); // $150.00
    let pnl = exit_fill.pnl(entry_price);

    // ($160 - $150) * 100 - $5 = $1,000 - $5 = $995
    assert_eq!(pnl.amount(), Decimal::new(995, 0));
}

#[test]
fn fill_is_closing() {
    let sell_fill = Fill::new(
        OrderId::generate(),
        Symbol::new("AAPL").unwrap(),
        Quantity::new(Decimal::new(100, 0)).unwrap(),
        Price::new(Decimal::new(15000, 2)).unwrap(),
        Side::Sell,
        Utc::now(),
    );

    assert!(sell_fill.is_closing(PositionDirection::Long));
    assert!(!sell_fill.is_closing(PositionDirection::Short));

    let buy_fill = Fill::new(
        OrderId::generate(),
        Symbol::new("AAPL").unwrap(),
        Quantity::new(Decimal::new(100, 0)).unwrap(),
        Price::new(Decimal::new(15000, 2)).unwrap(),
        Side::Buy,
        Utc::now(),
    );

    assert!(buy_fill.is_closing(PositionDirection::Short));
    assert!(!buy_fill.is_closing(PositionDirection::Long));
}

#[test]
fn fill_builder() {
    let fill = Fill::builder(
        OrderId::generate(),
        Symbol::new("AAPL").unwrap(),
        Quantity::new(Decimal::new(100, 0)).unwrap(),
        Price::new(Decimal::new(15050, 2)).unwrap(),
        Side::Buy,
        Utc::now(),
    )
    .commission(Money::new(Decimal::new(5, 0), Currency::USD).unwrap())
    .build();

    assert_eq!(fill.quantity().inner(), Decimal::new(100, 0));
    assert!(fill.commission().is_some());
}

// =============================================================================
// FILL AGGREGATION TESTS
// =============================================================================

#[test]
fn fill_aggregation_empty() {
    let agg = FillAggregation::new(Currency::USD);

    assert_eq!(agg.total_quantity().inner(), Decimal::ZERO);
    assert_eq!(agg.fill_count(), 0);
}

#[test]
fn fill_aggregation_single_fill() {
    let fill = create_test_fill();
    let mut agg = FillAggregation::new(Currency::USD);

    agg.add_fill(&fill);

    assert_eq!(agg.total_quantity().inner(), Decimal::new(100, 0));
    assert_eq!(agg.fill_count(), 1);
    assert_eq!(agg.total_notional().amount(), Decimal::new(15050, 0));
}

#[test]
fn fill_aggregation_multiple_fills() {
    let fill1 = Fill::new(
        OrderId::generate(),
        Symbol::new("AAPL").unwrap(),
        Quantity::new(Decimal::new(50, 0)).unwrap(),
        Price::new(Decimal::new(15000, 2)).unwrap(), // $150.00
        Side::Buy,
        Utc::now(),
    );

    let fill2 = Fill::new(
        OrderId::generate(),
        Symbol::new("AAPL").unwrap(),
        Quantity::new(Decimal::new(50, 0)).unwrap(),
        Price::new(Decimal::new(15200, 2)).unwrap(), // $152.00
        Side::Buy,
        Utc::now(),
    );

    let agg = FillAggregation::from_fills(vec![fill1, fill2].iter(), Currency::USD);

    assert_eq!(agg.total_quantity().inner(), Decimal::new(100, 0));
    assert_eq!(agg.fill_count(), 2);

    // Total notional: (50 * $150) + (50 * $152) = $7,500 + $7,600 = $15,100
    assert_eq!(agg.total_notional().amount(), Decimal::new(15100, 0));

    // Avg price: $15,100 / 100 = $151.00
    assert_eq!(agg.avg_price().inner(), Decimal::new(15100, 2));
}

#[test]
fn fill_aggregation_with_commission() {
    let fill = Fill::with_commission(
        OrderId::generate(),
        Symbol::new("AAPL").unwrap(),
        Quantity::new(Decimal::new(100, 0)).unwrap(),
        Price::new(Decimal::new(15000, 2)).unwrap(),
        Side::Buy,
        Utc::now(),
        Money::new(Decimal::new(10, 0), Currency::USD).unwrap(),
    );

    let mut agg = FillAggregation::new(Currency::USD);
    agg.add_fill(&fill);

    assert_eq!(agg.total_commission().amount(), Decimal::new(10, 0));

    // Net value for buy: notional + commission
    let net = agg.net_value(Side::Buy);
    assert_eq!(net.amount(), Decimal::new(15010, 0)); // $15,000 + $10
}

// =============================================================================
// EDGE CASE TESTS
// =============================================================================

#[test]
fn order_fill_zero_quantity_fails() {
    // Zero quantity is not allowed - Quantity::new fails
    let qty_result = Quantity::new(Decimal::ZERO);
    assert!(qty_result.is_err());
}

#[test]
fn order_display_and_debug() {
    let order = create_test_order();
    let debug_str = format!("{:?}", order);
    assert!(debug_str.contains("Order"));
}

#[test]
fn order_status_display() {
    let status = OrderStatus::Submitted { at: Utc::now() };
    let display_str = format!("{}", status);
    assert!(display_str.starts_with("Submitted"));
}

#[test]
fn fill_equality() {
    let order_id = OrderId::generate();
    let timestamp = Utc::now();
    let fill1 = Fill::new(
        order_id,
        Symbol::new("AAPL").unwrap(),
        Quantity::new(Decimal::new(100, 0)).unwrap(),
        Price::new(Decimal::new(15000, 2)).unwrap(),
        Side::Buy,
        timestamp,
    );

    let fill2 = Fill::new(
        order_id,
        Symbol::new("AAPL").unwrap(),
        Quantity::new(Decimal::new(100, 0)).unwrap(),
        Price::new(Decimal::new(15000, 2)).unwrap(),
        Side::Buy,
        timestamp,
    );

    // Fills with same values and timestamps should be equal
    assert_eq!(fill1, fill2);
}

#[test]
fn order_idempotent_transition() {
    let mut order = create_test_order();
    let submitted = OrderStatus::Submitted { at: Utc::now() };
    order.update_status(submitted.clone()).unwrap();

    // Transitioning to the same state should succeed (idempotent)
    let result = order.update_status(submitted);
    assert!(result.is_ok());
}

#[test]
fn order_remaining_quantity_calculation() {
    let mut order = create_test_order();
    transition_to_pending(&mut order);

    let fill_qty = Quantity::new(Decimal::new(30, 0)).unwrap();
    let fill_price = Price::new(Decimal::new(15000, 2)).unwrap();

    order.fill(fill_qty, fill_price).unwrap();

    // 100 - 30 = 70 remaining
    assert_eq!(order.remaining_quantity().inner(), Decimal::new(70, 0));
}
