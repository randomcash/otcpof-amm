use anchor_lang::prelude::*;

use crate::libraries::QueueType;

/// Emitted when create or update a config
#[event]
#[cfg_attr(feature = "client", derive(Debug))]
pub struct ConfigChangeEvent {
    pub index: u16,
    pub owner: Pubkey,
    pub protocol_fee_rate: u32,
    pub trade_fee_rate: u32,
    pub fund_fee_rate: u32,
    pub fund_owner: Pubkey,
    pub queue_type_0: QueueType,
    pub queue_type_1: QueueType,
    pub price_feed_max_age: u64,
}

/// Emitted when a pool is created and initialized with a starting price
///
#[event]
#[cfg_attr(feature = "client", derive(Debug))]
pub struct PoolCreatedEvent {
    /// The first token of the pool by address sort order
    pub token_mint_0: Pubkey,

    /// The second token of the pool by address sort order
    pub token_mint_1: Pubkey,

    /// The address of the created pool
    pub pool_state: Pubkey,

    /// The initial sqrt price of the pool, as a Q64.64
    pub sqrt_price_x64: u128,

    /// Vault of token_0
    pub token_vault_0: Pubkey,
    /// Vault of token_1
    pub token_vault_1: Pubkey,
}


/// Emitted when create a new position
#[event]
#[cfg_attr(feature = "client", derive(Debug))]
pub struct CreatePersonalPositionEvent {
    /// The pool for which liquidity was added
    pub pool_id: Pubkey,

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

/// Emitted when create a new position
#[event]
#[cfg_attr(feature = "client", derive(Debug))]
pub struct ChangePersonalPosition {
    /// The pool from which liquidity was swapped
    pub pool_id: Pubkey,

    /// Payer who executed swaps
    pub payer: Pubkey,

    /// Position address which changed
    pub position_account: Pubkey,

    /// The amount of token_0 was deposit for the liquidity
    pub deposited_amount_0: u64,

    /// The amount of token_1 was deposit for the liquidity
    pub deposited_amount_1: u64,
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

/// Emitted when create a new position
#[event]
#[cfg_attr(feature = "client", derive(Debug))]
pub struct PositionPushedToQueueEvent {
    /// The pool for which liquidity was added
    pub pool_id: Pubkey,

    /// The address that create the position
    pub minter: Pubkey,

    /// The owner of the position and recipient of any minted liquidity
    pub nft_owner: Pubkey,

    /// Position address
    pub position_address: Pubkey,

    /// Position in queue
    pub queue_position: u8,
}

#[event]
#[cfg_attr(feature = "client", derive(Debug))]
pub struct SwapEvent {
    /// The address that altered the position
    pub minter: Pubkey,

    /// The ID of the pool with which this token is connected
    pub pool_id: Pubkey,

    /// Pubkey for NFT of token_0
    pub position_0: Pubkey,

    /// Pubkey for NFT of token_1
    pub position_1: Pubkey,

    /// Liquidity of token 0 added/removed by this event
    pub vol_swap_0: u64,

    /// Liquidity of token 1 added/removed by this event
    pub vol_swap_1: u64,

    /// The amount of token_0 owned by this position, on queue
    pub liquidity_0: u64,

    /// The amount of token_1 owned by this position, on queue
    pub liquidity_1: u64,
}