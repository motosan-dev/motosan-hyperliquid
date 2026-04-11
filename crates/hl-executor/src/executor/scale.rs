use rust_decimal::Decimal;

use hl_types::{HlError, OrderResponse, OrderWire, Tif};

use super::OrderExecutor;

impl OrderExecutor {
    /// Place a scale order — distributes `total_size` across `num_orders` limit
    /// orders evenly spaced between `price_low` and `price_high`.
    ///
    /// This is a client-side convenience that generates multiple limit orders and
    /// submits them via [`Self::bulk_order`]. It is **not** a special exchange
    /// action.
    ///
    /// # Arguments
    ///
    /// * `symbol` — Market symbol (e.g. `"BTC"`, `"ETH-PERP"`).
    /// * `is_buy` — `true` for buy orders, `false` for sell orders.
    /// * `total_size` — Total position size to distribute across all orders.
    /// * `price_low` — Lowest price in the range (inclusive).
    /// * `price_high` — Highest price in the range (inclusive).
    /// * `num_orders` — Number of orders to generate (must be >= 2).
    /// * `tif` — Time-in-force for all generated orders.
    /// * `vault` — Optional vault address.
    ///
    /// # Errors
    ///
    /// Returns [`HlError::Validation`] if inputs are invalid (e.g. `num_orders < 2`,
    /// `price_low >= price_high`, `total_size <= 0`).
    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(skip(self))]
    pub async fn place_scale_order(
        &self,
        symbol: &str,
        is_buy: bool,
        total_size: Decimal,
        price_low: Decimal,
        price_high: Decimal,
        num_orders: u32,
        tif: Tif,
        vault: Option<&str>,
    ) -> Result<Vec<OrderResponse>, HlError> {
        // --- Validation ---
        if num_orders < 2 {
            return Err(HlError::Validation(
                "scale order requires at least 2 orders".into(),
            ));
        }
        if price_low >= price_high {
            return Err(HlError::Validation(
                "price_low must be less than price_high".into(),
            ));
        }
        if total_size <= Decimal::ZERO {
            return Err(HlError::Validation("total_size must be positive".into()));
        }

        let asset_idx = self.resolve_asset(symbol)?;
        let size_per_order = total_size / Decimal::from(num_orders);
        let price_step = (price_high - price_low) / Decimal::from(num_orders - 1);

        let mut orders = Vec::with_capacity(num_orders as usize);
        for i in 0..num_orders {
            let price = price_low + price_step * Decimal::from(i);
            let builder = if is_buy {
                OrderWire::limit_buy(asset_idx, price, size_per_order)
            } else {
                OrderWire::limit_sell(asset_idx, price, size_per_order)
            };
            let order = builder.tif(tif).build()?;
            orders.push(order);
        }

        self.bulk_order(orders, vault).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hl_types::OrderTypeWire;
    use std::str::FromStr;

    /// Helper: build expected orders the same way the implementation does and
    /// verify the math is correct.
    #[test]
    fn scale_order_price_distribution() {
        let price_low = Decimal::from(90000);
        let price_high = Decimal::from(100000);
        let num_orders: u32 = 5;
        let total_size = Decimal::from(10);

        let size_per_order = total_size / Decimal::from(num_orders);
        let price_step = (price_high - price_low) / Decimal::from(num_orders - 1);

        assert_eq!(size_per_order, Decimal::from(2));
        assert_eq!(price_step, Decimal::from(2500));

        let mut prices = Vec::new();
        for i in 0..num_orders {
            prices.push(price_low + price_step * Decimal::from(i));
        }
        assert_eq!(
            prices,
            vec![
                Decimal::from(90000),
                Decimal::from(92500),
                Decimal::from(95000),
                Decimal::from(97500),
                Decimal::from(100000),
            ]
        );
    }

    #[test]
    fn scale_order_two_orders_uses_endpoints() {
        let price_low = Decimal::from(1000);
        let price_high = Decimal::from(2000);
        let num_orders: u32 = 2;

        let price_step = (price_high - price_low) / Decimal::from(num_orders - 1);
        assert_eq!(price_step, Decimal::from(1000));

        let p0 = price_low + price_step * Decimal::from(0u32);
        let p1 = price_low + price_step * Decimal::from(1u32);
        assert_eq!(p0, Decimal::from(1000));
        assert_eq!(p1, Decimal::from(2000));
    }

    #[test]
    fn scale_order_builds_valid_order_wires() {
        let asset_idx = 3u32;
        let price_low = Decimal::from(50000);
        let price_high = Decimal::from(60000);
        let num_orders = 3u32;
        let total_size = Decimal::from_str("0.03").unwrap();
        let tif = Tif::Gtc;

        let size_per_order = total_size / Decimal::from(num_orders);
        let price_step = (price_high - price_low) / Decimal::from(num_orders - 1);

        let mut orders = Vec::new();
        for i in 0..num_orders {
            let price = price_low + price_step * Decimal::from(i);
            let order = OrderWire::limit_buy(asset_idx, price, size_per_order)
                .tif(tif)
                .build()
                .unwrap();
            orders.push(order);
        }

        assert_eq!(orders.len(), 3);
        assert_eq!(orders[0].limit_px, "50000");
        assert_eq!(orders[1].limit_px, "55000");
        assert_eq!(orders[2].limit_px, "60000");

        for order in &orders {
            assert_eq!(order.asset, 3);
            assert!(order.is_buy);
            assert_eq!(order.sz, "0.01");
            assert!(order.order_type.is_limit());
            if let OrderTypeWire::Limit(ref l) = order.order_type {
                assert_eq!(l.tif, Tif::Gtc);
            }
        }
    }

    #[test]
    fn scale_order_sell_side() {
        let order = OrderWire::limit_sell(0, Decimal::from(100), Decimal::from(1))
            .tif(Tif::Alo)
            .build()
            .unwrap();
        assert!(!order.is_buy);
        if let OrderTypeWire::Limit(ref l) = order.order_type {
            assert_eq!(l.tif, Tif::Alo);
        }
    }
}
