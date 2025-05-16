use crate::{libraries::liquidity_math, util::get_recent_epoch};
use anchor_lang::prelude::*;

/// Seed to derive account address and signature
pub const POSITION_SEED: &str = "position";

/// Protocol Position
#[account(zero_copy(unsafe))]
#[repr(C, packed)]
#[derive(Default, Debug)]
pub struct ProtocolPositionState {
    /// Bump to identify PDA
    pub bump: [u8; 1],

    /// The ID of the pool with which this token is connected
    pub pool_id: Pubkey,

    /// The amount of tokens_0 deposited in the protocol
    pub amount_0: u64,
    /// The amount of tokens_1 deposited in the protocol
    pub amount_1: u64,

    /// tokens_0 on queue_0, ready to be swapped
    pub liquidity_0: u64,
    /// tokens_1 on queue_1, ready to be swapped
    pub liquidity_1: u64,

    /// The current price of the pool as a sqrt(token_1/token_0) Q64.64 value
    pub sqrt_price_x64: u128,

    /// The amounts in and out of swap token_0 and token_1
    pub swap_in_amount_token_0: u64,
    pub swap_out_amount_token_1: u64,
    pub swap_in_amount_token_1: u64,
    pub swap_out_amount_token_0: u64,

    // account update recent epoch
    pub recent_epoch: u64,
}

impl ProtocolPositionState {
    pub const LEN: usize = Self::DISCRIMINATOR.len() + std::mem::size_of::<Self>();

    pub fn seeds(&self) -> [&[u8]; 3] {
        [
            &POSITION_SEED.as_bytes(),
            self.pool_id.as_ref(),
            self.bump.as_ref(),
        ]
    }

    pub fn initialize(&mut self, bump: u8, pool_id: Pubkey) -> Result<()> {
        self.pool_id = pool_id;
        self.bump = [bump];
        self.amount_0 = 0;
        self.amount_1 = 0;
        self.liquidity_0 = 0;
        self.liquidity_1 = 0;
        self.sqrt_price_x64 = 0;
        self.swap_in_amount_token_0 = 0;
        self.swap_out_amount_token_1 = 0;
        self.swap_in_amount_token_1 = 0;
        self.swap_out_amount_token_0 = 0;
        self.recent_epoch = get_recent_epoch()?;
        Ok(())
    }

    pub fn update(
        &mut self,
        amount_delta_0: i64,
        amount_delta_1: i64,
        liquidity_delta_0: i64,
        liquidity_delta_1: i64,
    ) -> Result<()> {
        self.amount_0 = liquidity_math::add_delta_64(self.amount_0, amount_delta_0)?;
        self.amount_1 = liquidity_math::add_delta_64(self.amount_1, amount_delta_1)?;
        self.liquidity_0 = liquidity_math::add_delta_64(self.liquidity_0, liquidity_delta_0)?;
        self.liquidity_1 = liquidity_math::add_delta_64(self.liquidity_1, liquidity_delta_1)?;
        self.recent_epoch = get_recent_epoch()?;
        Ok(())
    }
}


/// Emitted pool liquidity change when increase and decrease liquidity
#[event]
#[cfg_attr(feature = "client", derive(Debug))]
pub struct LiquidityChangeEvent {
    /// The pool for swap
    pub pool_state: Pubkey,

    /// The liquidity of the pool before liquidity change
    pub liquidity_before_0: u64,
    /// The liquidity of the pool before liquidity change
    pub liquidity_before_1: u64,

    /// The liquidity of the pool after liquidity change
    pub liquidity_after_0: u64,
    /// The liquidity of the pool after liquidity change
    pub liquidity_after_1: u64,
}
