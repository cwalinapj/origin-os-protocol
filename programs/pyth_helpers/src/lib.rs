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
}
