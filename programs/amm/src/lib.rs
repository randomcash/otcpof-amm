pub mod error;
pub mod instructions;
pub mod libraries;
pub mod states;
pub mod util;

use anchor_lang::prelude::*;
use core as core_;
use instructions::*;
use states::*;

#[cfg(not(feature = "no-entrypoint"))]
solana_security_txt::security_txt! {
    name: "raydium-clmm",
    project_url: "https://raydium.io",
    contacts: "link:https://immunefi.com/bounty/raydium",
    policy: "https://immunefi.com/bounty/raydium",
    source_code: "https://github.com/raydium-io/raydium-clmm",
    preferred_languages: "en",
    auditors: "https://github.com/raydium-io/raydium-docs/blob/master/audit/OtterSec%20Q3%202022/Raydium%20concentrated%20liquidity%20(CLMM)%20program.pdf"
}

#[cfg(feature = "devnet")]
declare_id!("HAccqG6fn1hKPG39bTmZQVXj5cbFfwEJ7YMufzLJwq1c");
#[cfg(not(feature = "devnet"))]
declare_id!("CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK");

pub mod admin {
    use anchor_lang::prelude::declare_id;
    #[cfg(feature = "devnet")]
    declare_id!("5JwjUhSgwnp1VQ12tjEp3xFPsbSvvu4WknGZt42gfAa1");
    #[cfg(not(feature = "devnet"))]
    declare_id!("GThUX1Atko4tqhN2NaiTazWSeFWMuiUvfFnyJyUghFMJ");
}

#[program]
pub mod amm_v3 {

    use super::*;

    // The configuration of AMM protocol, include trade fee and protocol fee
    /// # Arguments
    ///
    /// * `ctx`- The accounts needed by instruction.
    /// * `index` - The index of amm config, there may be multiple config.
    /// * `tick_spacing` - The tickspacing binding with config, cannot be changed.
    /// * `trade_fee_rate` - Trade fee rate, can be changed.
    /// * `protocol_fee_rate` - The rate of protocol fee within trade fee.
    /// * `fund_fee_rate` - The rate of fund fee within trade fee.
    /// *  `queue_type_0` - Type for queue side 0 (Fifo = 0, Priority = 1, etc)
    /// *  `queue_type_1` - Type for queue side 1 (Fifo = 0, Priority = 1, etc)
    pub fn create_amm_config(
        ctx: Context<CreateAmmConfig>,
        index: u16,
        trade_fee_rate: u32,
        protocol_fee_rate: u32,
        fund_fee_rate: u32,
        queue_type_0: u8,
        queue_type_1: u8
    ) -> Result<()> {
        assert!(trade_fee_rate < FEE_RATE_DENOMINATOR_VALUE);
        assert!(protocol_fee_rate <= FEE_RATE_DENOMINATOR_VALUE);
        assert!(fund_fee_rate <= FEE_RATE_DENOMINATOR_VALUE);
        assert!(fund_fee_rate + protocol_fee_rate <= FEE_RATE_DENOMINATOR_VALUE);
        instructions::create_amm_config(
            ctx,
            index,
            trade_fee_rate,
            protocol_fee_rate,
            fund_fee_rate,
            queue_type_0,
            queue_type_1
        )
    }

    /// Create support token22 mint account which can create pool and send rewards with ignoring the not support extensions.
    pub fn create_support_mint_associated(ctx: Context<CreateSupportMintAssociated>) -> Result<()> {
        instructions::create_support_mint_associated(ctx)
    }

    /// Updates the owner of the amm config
    /// Must be called by the current owner or admin
    ///
    /// # Arguments
    ///
    /// * `ctx`- The context of accounts
    /// * `trade_fee_rate`- The new trade fee rate of amm config, be set when `param` is 0
    /// * `protocol_fee_rate`- The new protocol fee rate of amm config, be set when `param` is 1
    /// * `fund_fee_rate`- The new fund fee rate of amm config, be set when `param` is 2
    /// * `new_owner`- The config's new owner, be set when `param` is 3
    /// * `new_fund_owner`- The config's new fund owner, be set when `param` is 4
    /// * `param`- The value can be 0 | 1 | 2 | 3 | 4, otherwise will report a error
    ///
    pub fn update_amm_config(ctx: Context<UpdateAmmConfig>, param: u8, value: u32) -> Result<()> {
        instructions::update_amm_config(ctx, param, value)
    }

    /// Creates a pool for the given token pair and the initial price
    ///
    /// # Arguments
    ///
    /// * `ctx`- The context of accounts
    /// * `sqrt_price_x64` - the initial sqrt price (amount_token_1 / amount_token_0) of the pool as a Q64.64
    /// Note: The open_time must be smaller than the current block_timestamp on chain.
    pub fn create_pool<'a, 'b, 'c: 'info, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, CreatePool<'info>>,
        sqrt_price_x64: u128,
        open_time: u64,
    ) -> Result<()> {
        instructions::create_pool(ctx, sqrt_price_x64, open_time)
    }

    /// Update pool status for given value
    ///
    /// # Arguments
    ///
    /// * `ctx`- The context of accounts
    /// * `status` - The value of status
    ///
    pub fn update_pool_status(ctx: Context<UpdatePoolStatus>, status: u8) -> Result<()> {
        instructions::update_pool_status(ctx, status)
    }

    /// Creates an operation account for the program
    ///
    /// # Arguments
    ///
    /// * `ctx`- The context of accounts
    ///
    pub fn create_operation_account(ctx: Context<CreateOperationAccount>) -> Result<()> {
        instructions::create_operation_account(ctx)
    }

    /// Update the operation account
    ///
    /// # Arguments
    ///
    /// * `ctx`- The context of accounts
    /// * `param`- The value can be 0 | 1 | 2 | 3, otherwise will report a error
    /// * `keys`- update operation owner when the `param` is 0
    ///           remove operation owner when the `param` is 1
    ///           update whitelist mint when the `param` is 2
    ///           remove whitelist mint when the `param` is 3
    ///
    pub fn update_operation_account(
        ctx: Context<UpdateOperationAccount>,
        param: u8,
        keys: Vec<Pubkey>,
    ) -> Result<()> {
        instructions::update_operation_account(ctx, param, keys)
    }

    /// Creates a new position wrapped in a Token2022 NFT without relying on metadata_program and metadata_account, reduce the cost for user to create a personal position.
    ///
    /// # Arguments
    ///
    /// * `ctx` - The context of accounts
    /// * `tick_lower_index` - The low boundary of market
    /// * `tick_upper_index` - The upper boundary of market
    /// * `tick_array_lower_start_index` - The start index of tick array which include tick low
    /// * `tick_array_upper_start_index` - The start index of tick array which include tick upper
    /// * `liquidity` - The liquidity to be added, if zero, and the base_flag is specified, calculate liquidity base amount_0_max or amount_1_max according base_flag, otherwise open position with zero liquidity
    /// * `amount_0_max` - The max amount of token_0 to spend, which serves as a slippage check
    /// * `amount_1_max` - The max amount of token_1 to spend, which serves as a slippage check
    /// * `with_metadata` - The flag indicating whether to create NFT mint metadata
    /// * `base_flag` - if the liquidity specified as zero, true: calculate liquidity base amount_0_max otherwise base amount_1_max
    ///
    pub fn open_position_with_token22_nft<'a, 'b, 'c: 'info, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, OpenPositionWithToken22Nft<'info>>,
        amount_0: u64,
        amount_1: u64,
        with_metadata: bool,
    ) -> Result<()> {
        instructions::open_position_with_token22_nft(
            ctx,
            amount_0,
            amount_1,
            with_metadata,
        )
    }

    /// Close the user's position and NFT account. If the NFT mint belongs to token2022, it will also be closed and the funds returned to the NFT owner.
    ///
    /// # Arguments
    ///
    /// * `ctx` - The context of accounts
    ///
    pub fn close_position<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, ClosePosition<'info>>,
    ) -> Result<()> {
        instructions::close_position(ctx)
    }

    /// #[deprecated(note = "Use `swap_v2` instead.")]
    /// Swaps one token for as much as possible of another token across a single pool
    ///
    /// # Arguments
    ///
    /// * `ctx` - The context of accounts
    /// * `amount` - Arranged in pairs with other_amount_threshold. (amount_in, amount_out_minimum) or (amount_out, amount_in_maximum)
    /// * `other_amount_threshold` - For slippage check
    /// * `sqrt_price_limit` - The Q64.64 sqrt price √P limit. If zero for one, the price cannot
    /// * `is_base_input` - swap base input or swap base output
    ///
    pub fn swap<'a, 'b, 'c: 'info, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, SwapSingle<'info>>,
        amount: u64,
        other_amount_threshold: u64,
        sqrt_price_limit_x64: u128,
        is_base_input: bool,
    ) -> Result<()> {
        instructions::swap(
            ctx,
            amount,
            other_amount_threshold,
            sqrt_price_limit_x64,
            is_base_input,
        )
    }
}
