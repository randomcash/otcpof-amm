use anchor_lang::prelude::*;

use crate::util::get_recent_epoch;

use super::POSITION_SEED;

#[account]
#[derive(Default, Debug)]
pub struct PersonalPositionState {
    /// Bump to identify PDA
    pub bump: [u8; 1],

    /// Mint address of the tokenized position
    pub nft_mint: Pubkey,

    /// The ID of the pool with which this token is connected
    pub pool_id: Pubkey,

    /// The amount of token_0 owned by this position
    pub amount_0: u64,
    /// The amount of token_1 owned by this position
    pub amount_1: u64,

    // account update recent epoch
    pub recent_epoch: u64,
}

impl PersonalPositionState {
    pub const LEN: usize = Self::DISCRIMINATOR.len() + std::mem::size_of::<Self>();

    pub fn seeds(&self) -> [&[u8]; 3] {
        [
            &POSITION_SEED.as_bytes(),
            self.nft_mint.as_ref(),
            self.bump.as_ref(),
        ]
    }

    pub fn initialize(
        &mut self,
        bump: u8,
        nft_mint: Pubkey,
        pool_id: Pubkey,
        amount_0: u64,
        amount_1: u64,
    ) -> Result<()> {
        self.pool_id = pool_id;
        self.bump = [bump];
        self.nft_mint = nft_mint;
        self.amount_0 = amount_0;
        self.amount_1 = amount_1;
        self.recent_epoch = get_recent_epoch()?;
        
        Ok(())
    }
}
