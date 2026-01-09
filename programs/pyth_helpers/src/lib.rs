//! Pyth Oracle Helpers for Origin OS Protocol
//! 
//! Provides standardized price feed loading, validation, and swap calculations.

use anchor_lang::prelude::*;
use pyth_solana_receiver_sdk::price_update::PriceUpdateV2;

/// Price data extracted from Pyth oracle
#[derive(Debug, Clone, Copy)]
pub struct PriceData {
    /// Price in base units (scaled by exponent)
    pub price: i64,
    /// Confidence interval
    pub conf: u64,
    /// Exponent (negative for decimals, e.g. -8 means 8 decimals)
    pub exponent: i32,
    /// Publish time (Unix timestamp)
    pub publish_time: i64,
}

impl PriceData {
    /// Convert price to u64, applying exponent to get value in target decimals
    pub fn price_in_decimals(&self, target_decimals: u8) -> Result<u64> {
        let exp_diff = (target_decimals as i32) + self.exponent;
        let price_abs = self.price.unsigned_abs();
        
        if exp_diff >= 0 {
            price_abs.checked_mul(10u64.pow(exp_diff as u32))
                .ok_or(error!(PythError::Overflow))
        } else {
            Ok(price_abs / 10u64.pow((-exp_diff) as u32))
        }
    }
    
    /// Get confidence as percentage of price (in bps)
    pub fn conf_ratio_bps(&self) -> u64 {
        if self.price <= 0 {
            return u64::MAX;
        }
        (self.conf as u128)
            .saturating_mul(10_000)
            .checked_div(self.price.unsigned_abs() as u128)
            .unwrap_or(u64::MAX as u128) as u64
    }
}

/// Load price from a Pyth PriceUpdateV2 account
/// 
/// # Arguments
/// * `price_update` - The Pyth price update account
/// * `feed_id` - The expected feed ID (32 bytes)
/// * `max_age_seconds` - Maximum age of price in seconds
/// 
/// # Returns
/// * `PriceData` - The extracted price data
pub fn load_price(
    price_update: &Account<PriceUpdateV2>,
    feed_id: &[u8; 32],
    max_age_seconds: u64,
) -> Result<PriceData> {
    let clock = Clock::get()?;
    
    // Get price no older than max_age
    let price = price_update
        .get_price_no_older_than(&clock, max_age_seconds, feed_id)
        .map_err(|_| error!(PythError::PriceTooOld))?;
    
    Ok(PriceData {
        price: price.price,
        conf: price.conf,
        exponent: price.exponent,
        publish_time: price.publish_time,
    })
}

/// Assert price is fresh (within max_age_seconds)
pub fn assert_fresh(publish_time: i64, max_age_seconds: u64) -> Result<()> {
    let clock = Clock::get()?;
    let age = clock.unix_timestamp.saturating_sub(publish_time);
    
    require!(
        age >= 0 && (age as u64) <= max_age_seconds,
        PythError::PriceTooOld
    );
    
    Ok(())
}

/// Assert confidence is within acceptable ratio
/// 
/// # Arguments
/// * `price` - The price value
/// * `conf` - The confidence interval
/// * `max_conf_ratio_bps` - Maximum allowed conf/price ratio in basis points
pub fn assert_conf(price: i64, conf: u64, max_conf_ratio_bps: u16) -> Result<()> {
    if price <= 0 {
        return Err(error!(PythError::InvalidPrice));
    }
    
    let conf_ratio_bps = (conf as u128)
        .saturating_mul(10_000)
        .checked_div(price.unsigned_abs() as u128)
        .unwrap_or(u64::MAX as u128);
    
    require!(
        conf_ratio_bps <= max_conf_ratio_bps as u128,
        PythError::ConfidenceTooWide
    );
    
    Ok(())
}

/// Calculate conservative minimum output for a swap
/// 
/// Uses worst-case pricing: sell at (price - conf), buy at (price + conf),
/// then apply slippage tolerance.
/// 
/// # Arguments
/// * `amount_in` - Input amount in token's native units
/// * `price_in` - Input token price data
/// * `price_out` - Output token price data
/// * `slippage_bps` - Slippage tolerance in basis points
/// 
/// # Returns
/// * Minimum acceptable output amount
pub fn conservative_min_out(
    amount_in: u64,
    price_in: &PriceData,
    price_out: &PriceData,
    slippage_bps: u16,
) -> Result<u64> {
    // Validate prices
    require!(price_in.price > 0, PythError::InvalidPrice);
    require!(price_out.price > 0, PythError::InvalidPrice);
    
    // Conservative sell price: price - conf (worst case for seller)
    let sell_price = (price_in.price as u64)
        .saturating_sub(price_in.conf);
    
    // Conservative buy price: price + conf (worst case for buyer)
    let buy_price = (price_out.price as u64)
        .saturating_add(price_out.conf);
    
    if buy_price == 0 {
        return Err(error!(PythError::InvalidPrice));
    }
    
    // Normalize exponents: convert to common scale
    let exp_diff = price_in.exponent - price_out.exponent;
    
    let value_in_out_units = if exp_diff >= 0 {
        // price_in has larger exponent (fewer decimals)
        (amount_in as u128)
            .checked_mul(sell_price as u128)
            .ok_or(error!(PythError::Overflow))?
            .checked_mul(10u128.pow(exp_diff as u32))
            .ok_or(error!(PythError::Overflow))?
            .checked_div(buy_price as u128)
            .ok_or(error!(PythError::Overflow))?
    } else {
        // price_out has larger exponent
        (amount_in as u128)
            .checked_mul(sell_price as u128)
            .ok_or(error!(PythError::Overflow))?
            .checked_div(buy_price as u128)
            .ok_or(error!(PythError::Overflow))?
            .checked_div(10u128.pow((-exp_diff) as u32))
            .ok_or(error!(PythError::Overflow))?
    };
    
    // Apply slippage (reduce output by slippage_bps)
    let slippage_factor = 10_000u128.saturating_sub(slippage_bps as u128);
    let min_out = value_in_out_units
        .checked_mul(slippage_factor)
        .ok_or(error!(PythError::Overflow))?
        .checked_div(10_000)
        .ok_or(error!(PythError::Overflow))?;
    
    Ok(min_out as u64)
}

/// Validate price update meets all constraints
pub fn validate_price(
    price_update: &Account<PriceUpdateV2>,
    feed_id: &[u8; 32],
    max_age_seconds: u64,
    max_conf_ratio_bps: u16,
) -> Result<PriceData> {
    let data = load_price(price_update, feed_id, max_age_seconds)?;
    assert_conf(data.price, data.conf, max_conf_ratio_bps)?;
    Ok(data)
}

/// Convert USD value to token amount using price
pub fn usd_to_token_amount(
    usd_value: u64,
    usd_decimals: u8,
    price_data: &PriceData,
    token_decimals: u8,
) -> Result<u64> {
    require!(price_data.price > 0, PythError::InvalidPrice);
    
    // USD value in smallest units * 10^token_decimals / price
    let exp_adjustment = (token_decimals as i32) - (usd_decimals as i32) + price_data.exponent;
    
    let result = if exp_adjustment >= 0 {
        (usd_value as u128)
            .checked_mul(10u128.pow(exp_adjustment as u32))
            .ok_or(error!(PythError::Overflow))?
            .checked_div(price_data.price.unsigned_abs() as u128)
            .ok_or(error!(PythError::Overflow))?
    } else {
        (usd_value as u128)
            .checked_div(price_data.price.unsigned_abs() as u128)
            .ok_or(error!(PythError::Overflow))?
            .checked_div(10u128.pow((-exp_adjustment) as u32))
            .ok_or(error!(PythError::Overflow))?
    };
    
    Ok(result as u64)
}

/// Convert token amount to USD value using price
pub fn token_amount_to_usd(
    token_amount: u64,
    token_decimals: u8,
    price_data: &PriceData,
    usd_decimals: u8,
) -> Result<u64> {
    require!(price_data.price > 0, PythError::InvalidPrice);
    
    // token_amount * price / 10^(token_decimals - usd_decimals + exponent)
    let exp_adjustment = (token_decimals as i32) - (usd_decimals as i32) + price_data.exponent;
    
    let result = if exp_adjustment >= 0 {
        (token_amount as u128)
            .checked_mul(price_data.price.unsigned_abs() as u128)
            .ok_or(error!(PythError::Overflow))?
            .checked_div(10u128.pow(exp_adjustment as u32))
            .ok_or(error!(PythError::Overflow))?
    } else {
        (token_amount as u128)
            .checked_mul(price_data.price.unsigned_abs() as u128)
            .ok_or(error!(PythError::Overflow))?
            .checked_mul(10u128.pow((-exp_adjustment) as u32))
            .ok_or(error!(PythError::Overflow))?
    };
    
    Ok(result as u64)
}

#[error_code]
pub enum PythError {
    #[msg("Price is too old")]
    PriceTooOld,
    #[msg("Price confidence interval is too wide")]
    ConfidenceTooWide,
    #[msg("Invalid price (zero or negative)")]
    InvalidPrice,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("Feed ID mismatch")]
    FeedIdMismatch,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Confidence Ratio Tests ====================

    #[test]
    fn test_conf_ratio_bps() {
        let data = PriceData {
            price: 10000,
            conf: 100, // 1%
            exponent: -8,
            publish_time: 0,
        };
        assert_eq!(data.conf_ratio_bps(), 100); // 100 bps = 1%
    }

    #[test]
    fn test_conf_ratio_bps_zero_price() {
        let data = PriceData {
            price: 0,
            conf: 100,
            exponent: -8,
            publish_time: 0,
        };
        assert_eq!(data.conf_ratio_bps(), u64::MAX);
    }

    #[test]
    fn test_conf_ratio_bps_negative_price() {
        let data = PriceData {
            price: -10000,
            conf: 100,
            exponent: -8,
            publish_time: 0,
        };
        assert_eq!(data.conf_ratio_bps(), u64::MAX);
    }

    // ==================== assert_conf Tests (Confidence Ratio Rejection) ====================

    #[test]
    fn test_assert_conf_within_limit() {
        // 1% confidence (100 bps), limit is 200 bps - should pass
        let result = assert_conf(10000, 100, 200);
        assert!(result.is_ok());
    }

    #[test]
    fn test_assert_conf_at_limit() {
        // Exactly at 100 bps limit - should pass
        let result = assert_conf(10000, 100, 100);
        assert!(result.is_ok());
    }

    #[test]
    fn test_assert_conf_exceeds_limit() {
        // 2% confidence (200 bps), limit is 100 bps - should fail
        let result = assert_conf(10000, 200, 100);
        assert!(result.is_err());
        // Verify it's the right error
        let err = result.unwrap_err();
        assert!(err.to_string().contains("ConfidenceTooWide") ||
                err.to_string().contains("6001")); // Error code for ConfidenceTooWide
    }

    #[test]
    fn test_assert_conf_zero_price_rejected() {
        // Zero price should be rejected regardless of confidence
        let result = assert_conf(0, 100, 1000);
        assert!(result.is_err());
    }

    #[test]
    fn test_assert_conf_negative_price_rejected() {
        // Negative price should be rejected
        let result = assert_conf(-10000, 100, 1000);
        assert!(result.is_err());
    }

    #[test]
    fn test_assert_conf_high_confidence_rejected() {
        // 50% confidence (5000 bps) - way too wide
        let result = assert_conf(10000, 5000, 100);
        assert!(result.is_err());
    }

    #[test]
    fn test_assert_conf_zero_confidence_passes() {
        // Zero confidence is always acceptable
        let result = assert_conf(10000, 0, 100);
        assert!(result.is_ok());
    }

    // ==================== Price Decimals Tests ====================

    #[test]
    fn test_price_in_decimals() {
        let data = PriceData {
            price: 12345678, // $0.12345678 with exp -8
            conf: 1000,
            exponent: -8,
            publish_time: 0,
        };

        // Convert to 6 decimals
        let price_6 = data.price_in_decimals(6).unwrap();
        assert_eq!(price_6, 123456); // Truncated to 6 decimals
    }

    #[test]
    fn test_price_in_decimals_scale_up() {
        let data = PriceData {
            price: 12345678,
            conf: 1000,
            exponent: -8,
            publish_time: 0,
        };

        // Convert to 10 decimals (scale up by 100)
        let price_10 = data.price_in_decimals(10).unwrap();
        assert_eq!(price_10, 1234567800);
    }

    // ==================== conservative_min_out Tests ====================

    #[test]
    fn test_conservative_min_out_same_exponent() {
        // SOL at $100 (conf $1), USDC at $1 (conf $0.001)
        // Selling 1 SOL should give ~99 USDC (worst case)
        let price_sol = PriceData {
            price: 10000000000, // $100.00 with exp -8
            conf: 100000000,    // $1.00 confidence
            exponent: -8,
            publish_time: 0,
        };
        let price_usdc = PriceData {
            price: 100000000,   // $1.00 with exp -8
            conf: 100000,       // $0.001 confidence
            exponent: -8,
            publish_time: 0,
        };

        // 1 SOL (1e9 lamports)
        let amount_in = 1_000_000_000u64;
        let slippage_bps = 100; // 1%

        let min_out = conservative_min_out(amount_in, &price_sol, &price_usdc, slippage_bps).unwrap();

        // Sell price: 100 - 1 = $99
        // Buy price: 1 + 0.001 = $1.001
        // Raw: 1e9 * 99 / 1.001 = ~98,901,098,901
        // After 1% slippage: ~97,912,087,912
        // Should be less than 100 USDC (100e9)
        assert!(min_out < 100_000_000_000);
        assert!(min_out > 90_000_000_000); // But reasonably close
    }

    #[test]
    fn test_conservative_min_out_different_exponents() {
        // Token A at $50 (exp -6), Token B at $25 (exp -9)
        let price_a = PriceData {
            price: 50000000,     // $50 with exp -6
            conf: 500000,        // $0.50 confidence
            exponent: -6,
            publish_time: 0,
        };
        let price_b = PriceData {
            price: 25000000000,  // $25 with exp -9
            conf: 250000000,     // $0.25 confidence
            exponent: -9,
            publish_time: 0,
        };

        let amount_in = 1_000_000u64; // 1 token A
        let slippage_bps = 50; // 0.5%

        let min_out = conservative_min_out(amount_in, &price_a, &price_b, slippage_bps).unwrap();

        // Should get roughly 2x token B (since A is ~2x price of B)
        // But with conservative pricing, will be less
        assert!(min_out > 0);
    }

    #[test]
    fn test_conservative_min_out_zero_slippage() {
        let price_in = PriceData {
            price: 100000000,
            conf: 1000000,
            exponent: -8,
            publish_time: 0,
        };
        let price_out = PriceData {
            price: 100000000,
            conf: 1000000,
            exponent: -8,
            publish_time: 0,
        };

        let amount_in = 1_000_000_000u64;
        let slippage_bps = 0;

        let min_out = conservative_min_out(amount_in, &price_in, &price_out, slippage_bps).unwrap();

        // With equal prices but confidence, output should be less than input
        // sell_price = 100M - 1M = 99M
        // buy_price = 100M + 1M = 101M
        // min_out = 1e9 * 99 / 101 = ~980,198,019
        assert!(min_out < amount_in);
        assert!(min_out > 900_000_000);
    }

    #[test]
    fn test_conservative_min_out_high_slippage() {
        let price_in = PriceData {
            price: 100000000,
            conf: 0,
            exponent: -8,
            publish_time: 0,
        };
        let price_out = PriceData {
            price: 100000000,
            conf: 0,
            exponent: -8,
            publish_time: 0,
        };

        let amount_in = 1_000_000_000u64;
        let slippage_bps = 1000; // 10% slippage

        let min_out = conservative_min_out(amount_in, &price_in, &price_out, slippage_bps).unwrap();

        // With zero confidence and equal prices, should be exactly 90% of input
        assert_eq!(min_out, 900_000_000);
    }

    #[test]
    fn test_conservative_min_out_invalid_price_in() {
        let price_in = PriceData {
            price: 0, // Invalid
            conf: 0,
            exponent: -8,
            publish_time: 0,
        };
        let price_out = PriceData {
            price: 100000000,
            conf: 0,
            exponent: -8,
            publish_time: 0,
        };

        let result = conservative_min_out(1000, &price_in, &price_out, 100);
        assert!(result.is_err());
    }

    #[test]
    fn test_conservative_min_out_invalid_price_out() {
        let price_in = PriceData {
            price: 100000000,
            conf: 0,
            exponent: -8,
            publish_time: 0,
        };
        let price_out = PriceData {
            price: -100000000, // Invalid (negative)
            conf: 0,
            exponent: -8,
            publish_time: 0,
        };

        let result = conservative_min_out(1000, &price_in, &price_out, 100);
        assert!(result.is_err());
    }

    #[test]
    fn test_conservative_min_out_confidence_wider_than_price() {
        // Edge case: confidence is larger than price itself
        let price_in = PriceData {
            price: 100000000,
            conf: 200000000, // Conf > price, sell_price saturates to 0
            exponent: -8,
            publish_time: 0,
        };
        let price_out = PriceData {
            price: 100000000,
            conf: 0,
            exponent: -8,
            publish_time: 0,
        };

        let min_out = conservative_min_out(1_000_000_000, &price_in, &price_out, 100).unwrap();

        // sell_price saturates to 0, so min_out should be 0
        assert_eq!(min_out, 0);
    }

    // ==================== USD/Token Conversion Tests ====================

    #[test]
    fn test_usd_to_token_amount() {
        // $100 USD at $50/token = 2 tokens
        let price = PriceData {
            price: 5000000000, // $50.00 with exp -8
            conf: 0,
            exponent: -8,
            publish_time: 0,
        };

        // $100 in 6 decimals = 100_000_000
        let usd_value = 100_000_000u64;
        let token_amount = usd_to_token_amount(usd_value, 6, &price, 9).unwrap();

        // Should be 2 tokens with 9 decimals = 2_000_000_000
        assert_eq!(token_amount, 2_000_000_000);
    }

    #[test]
    fn test_token_amount_to_usd() {
        // 2 tokens at $50/token = $100
        let price = PriceData {
            price: 5000000000, // $50.00 with exp -8
            conf: 0,
            exponent: -8,
            publish_time: 0,
        };

        // 2 tokens with 9 decimals
        let token_amount = 2_000_000_000u64;
        let usd_value = token_amount_to_usd(token_amount, 9, &price, 6).unwrap();

        // Should be $100 in 6 decimals = 100_000_000
        assert_eq!(usd_value, 100_000_000);
    }

    #[test]
    fn test_usd_conversion_zero_price_rejected() {
        let price = PriceData {
            price: 0,
            conf: 0,
            exponent: -8,
            publish_time: 0,
        };

        let result = usd_to_token_amount(100_000_000, 6, &price, 9);
        assert!(result.is_err());

        let result = token_amount_to_usd(2_000_000_000, 9, &price, 6);
        assert!(result.is_err());
    }

    // ==================== Staleness Tests (assert_fresh logic) ====================
    // Note: assert_fresh requires Clock::get() which needs Solana runtime.
    // These tests document the expected behavior; integration tests cover actual execution.

    // The staleness check in assert_fresh:
    // 1. Gets current unix_timestamp from Clock
    // 2. Calculates age = current_time - publish_time
    // 3. Requires age >= 0 AND age <= max_age_seconds
    // 4. Returns PythError::PriceTooOld if stale

    // Integration test scenarios that should be covered:
    // - Fresh price (publish_time = now) -> OK
    // - Price exactly at max_age -> OK
    // - Price 1 second over max_age -> PriceTooOld error
    // - Future price (publish_time > now) -> age < 0 -> PriceTooOld error
}
