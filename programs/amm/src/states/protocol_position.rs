use crate::{libraries::liquidity_math, util::get_recent_epoch};
use anchor_lang::prelude::*;

/// Seed to derive account address and signature
pub const POSITION_SEED: &str = "position";

/// Protocol Position
#[account]
#[derive(Default, Debug)]
pub struct ProtocolPositionState {
    /// Bump to identify PDA
    pub bump: u8,

    /// The ID of the pool with which this token is connected
    pub pool_id: Pubkey,

    /// The amount of tokens_0 deposited in the protocol
    pub amount_0: u128,
    /// The amount of tokens_1 deposited in the protocol
    pub amount_1: u128,

    /// tokens_0 on queue_0, ready to be swapped
    pub liquidity_0: u128,
    /// tokens_1 on queue_1, ready to be swapped
    pub liquidity_1: u128,

    // account update recent epoch
    pub recent_epoch: u64,
}

impl ProtocolPositionState {
    pub const LEN: usize = Self::DISCRIMINATOR.len() + std::mem::size_of::<Self>();

    pub fn update(
        &mut self,
        amount_delta_0: i128,
        amount_delta_1: i128,
        liquidity_delta_0: i128,
        liquidity_delta_1: i128
    ) -> Result<()> {
        self.amount_0 = liquidity_math::add_delta(self.amount_0, amount_delta_0)?;
        self.amount_1 = liquidity_math::add_delta(self.amount_1, amount_delta_1)?;
        self.liquidity_0 = liquidity_math::add_delta(self.liquidity_0, liquidity_delta_0)?;
        self.liquidity_1 = liquidity_math::add_delta(self.liquidity_1, liquidity_delta_1)?;
        self.recent_epoch = get_recent_epoch()?;
        Ok(())
    }
}
