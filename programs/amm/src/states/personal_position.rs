use anchor_lang::prelude::*;

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
    pub amount_0: u128,
    /// The amount of token_1 owned by this position
    pub amount_1: u128,

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
}
/// Emitted when create a new position
#[event]
#[cfg_attr(feature = "client", derive(Debug))]
pub struct CreatePersonalPositionEvent {
    /// The pool for which liquidity was added
    pub pool_state: Pubkey,

    /// The address that create the position
    pub minter: Pubkey,

    /// The owner of the position and recipient of any minted liquidity
    pub nft_owner: Pubkey,

    /// The amount of token_0 was deposit for the liquidity
    pub deposit_amount_0: u64,

    /// The amount of token_1 was deposit for the liquidity
    pub deposit_amount_1: u64,

    /// The token transfer fee for deposit_amount_0
    pub deposit_amount_0_transfer_fee: u64,

    /// The token transfer fee for deposit_amount_1
    pub deposit_amount_1_transfer_fee: u64,
}

/// Emitted when liquidity is increased.
#[event]
#[cfg_attr(feature = "client", derive(Debug))]
pub struct IncreaseLiquidityEvent {
    /// The ID of the token for which liquidity was increased
    pub position_nft_mint: Pubkey,

    /// The amount by which liquidity for the NFT position was increased
    pub liquidity: u128,

    /// The amount of token_0 that was paid for the increase in liquidity
    pub amount_0: u64,

    /// The amount of token_1 that was paid for the increase in liquidity
    pub amount_1: u64,

    /// The token transfer fee for amount_0
    pub amount_0_transfer_fee: u64,

    /// The token transfer fee for amount_1
    pub amount_1_transfer_fee: u64,
}

/// Emitted when liquidity is decreased.
#[event]
#[cfg_attr(feature = "client", derive(Debug))]
pub struct DecreaseLiquidityEvent {
    /// The ID of the token for which liquidity was decreased
    pub position_nft_mint: Pubkey,
    /// The amount by which liquidity for the position was decreased
    pub liquidity: u128,
    /// The amount of token_0 that was paid for the decrease in liquidity
    pub decrease_amount_0: u64,
    /// The amount of token_1 that was paid for the decrease in liquidity
    pub decrease_amount_1: u64,
    /// The amount of token_0 transfer fee
    pub transfer_fee_0: u64,
    /// The amount of token_1 transfer fee
    pub transfer_fee_1: u64,
}

/// Emitted when liquidity decreased or increase.
#[event]
#[cfg_attr(feature = "client", derive(Debug))]
pub struct LiquidityCalculateEvent {
    /// The pool liquidity before decrease or increase
    pub pool_liquidity: u128,
    /// The pool price when decrease or increase in liquidity
    pub pool_sqrt_price_x64: u128,
    /// The amount of token_0 that was calculated for the decrease or increase in liquidity
    pub calc_amount_0: u64,
    /// The amount of token_1 that was calculated for the decrease or increase in liquidity
    pub calc_amount_1: u64,
    /// The amount of token_0 transfer fee without trade_fee_amount_0
    pub transfer_fee_0: u64,
    /// The amount of token_1 transfer fee without trade_fee_amount_0
    pub transfer_fee_1: u64,
}

/// Emitted when tokens are collected for a position
#[event]
#[cfg_attr(feature = "client", derive(Debug))]
pub struct CollectPersonalFeeEvent {
    /// The ID of the token for which underlying tokens were collected
    pub position_nft_mint: Pubkey,

    /// The token account that received the collected token_0 tokens
    pub recipient_token_account_0: Pubkey,

    /// The token account that received the collected token_1 tokens
    pub recipient_token_account_1: Pubkey,

    /// The amount of token_0 owed to the position that was collected
    pub amount_0: u64,

    /// The amount of token_1 owed to the position that was collected
    pub amount_1: u64,
}