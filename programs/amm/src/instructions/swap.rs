use std::ops::Mul;
use anchor_lang::prelude::*;

use pyth_sdk_solana::{state::SolanaPriceAccount, Price};

use crate::events::{ChangePersonalPosition, SwapEvent};
use crate::libraries::Queue;
use crate::states::{AmmConfig, PersonalPositionState, PoolQueue, PoolState, PoolStatusBitIndex, ProtocolPositionState, POOL_QUEUE_SEED, POOL_QUEUE_SEED_SIDE_0, POOL_QUEUE_SEED_SIDE_1, POSITION_SEED};
use crate::error::ErrorCode;

#[derive(Accounts)]
pub struct Swap<'info> {
    /// Address paying to execute swap, can be anyone
    #[account(mut)]
    pub payer: Signer<'info>,

    /// Which config the pool belongs to.
    #[account(
        mut,
        constraint = pool_state_loader.load()?.amm_config == amm_config.key()
    )]
    pub amm_config: Box<Account<'info, AmmConfig>>,

    #[account(mut)]
    pub pool_state_loader: AccountLoader<'info, PoolState>,

    #[account(
        init_if_needed,
        seeds = [
            POSITION_SEED.as_bytes(),
            pool_state_loader.key().as_ref()
        ],
        bump,
        payer = payer,
        space = ProtocolPositionState::LEN
    )]
    pub protocol_position: AccountLoader<'info, ProtocolPositionState>,

    /// The account corresponding to the token_0 queue order's nft_mint.
    /// CHECK: Verified manually
    #[account(mut)]
    pub pool_head_0: Box<Account<'info, PersonalPositionState>>,

    /// The account corresponding to the token_1 queue order's nft_mint.
    /// CHECK: Verified manually
    #[account(mut)]
    pub pool_head_1: Box<Account<'info, PersonalPositionState>>,

    /// The address that holds pool order queue 0
    #[account(
        init_if_needed,
        seeds = [
            &POOL_QUEUE_SEED,
            POOL_QUEUE_SEED_SIDE_0,
            pool_state_loader.key().as_ref()
        ],
        bump,
        payer = payer,
        space = PoolQueue::LEN
    )]
    pub token_queue_0: AccountLoader<'info, PoolQueue>,

    /// The address that holds pool order queue 1
    #[account(
        init_if_needed,
        seeds = [
            &POOL_QUEUE_SEED,
            POOL_QUEUE_SEED_SIDE_1,
            pool_state_loader.key().as_ref()
        ],
        bump,
        payer = payer,
        space = PoolQueue::LEN
    )]
    pub token_queue_1: AccountLoader<'info, PoolQueue>,

    /// CHECK: Verified manually
    #[account(
        constraint = price_feed.key() == pool_state_loader.load()?.price_feed
    )]
    pub price_feed: AccountInfo<'info>,

    pub system_program: Program<'info, System>,

    pub clock: Sysvar<'info, Clock>,
}

pub fn swap(ctx: Context<Swap>) -> Result<()> {
    swap_v1(
        &ctx.accounts.payer,
        &ctx.accounts.pool_state_loader,
        &ctx.accounts.amm_config,
        &mut ctx.accounts.protocol_position,
        &mut ctx.accounts.pool_head_0,
        &mut ctx.accounts.pool_head_1,
        &mut ctx.accounts.token_queue_0,
        &mut ctx.accounts.token_queue_1,
        &ctx.accounts.price_feed,
        &ctx.accounts.clock,
    )?;

    Ok(())
}

pub fn swap_v1<'a, 'info>(
    payer: &'a Signer<'info>,
    pool_state_loader: &'a AccountLoader<'info, PoolState>,
    amm_config: &'a Box<Account<'info, AmmConfig>>,
    protocol_position_loader: &'a mut AccountLoader<'info, ProtocolPositionState>,
    pool_head_0: &'a mut Box<Account<'info, PersonalPositionState>>,
    pool_head_1: &'a mut Box<Account<'info, PersonalPositionState>>,
    token_queue_0_loader: &'a mut AccountLoader<'info, PoolQueue>,
    token_queue_1_loader: &'a mut AccountLoader<'info, PoolQueue>,
    price_feed: &'a AccountInfo<'info>,
    clock: &'a Sysvar<'info, Clock>,
) -> Result<()> {
    let pool_state = pool_state_loader.load()?;
    if !pool_state.get_status_by_bit(PoolStatusBitIndex::OpenPositionOrIncreaseLiquidity) {
        return err!(ErrorCode::NotApproved);
    }

    let mut token_queue_0 = token_queue_0_loader.load_mut()?;
    let mut token_queue_1 = token_queue_1_loader.load_mut()?;
    let mut protocol_position = protocol_position_loader.load_mut()?;
    
    let mut head_0 = token_queue_0.has_next();
    let mut head_1 = token_queue_1.has_next();
    let price_feed = SolanaPriceAccount::account_info_to_feed(&price_feed)
        .map_err(|err| { 
            msg!(&err.to_string());
            ErrorCode::PriceFeedErr
        })?;

    // Get the current price (optionally check status)
    let price = price_feed
        .get_price_no_older_than(clock.unix_timestamp, amm_config.price_feed_max_age)
        .ok_or(ErrorCode::PriceFeedErr)?;

    while head_0 && head_1 {
        let (swap_vol_1, swap_vol_0) = execute_swap(pool_head_0, pool_head_1, price)?;
        protocol_position.amount_0 -= swap_vol_0;
        protocol_position.amount_1 -= swap_vol_1;

        emit!(SwapEvent {
            minter: payer.key(),
            pool_id: pool_state_loader.key(),
            position_0: pool_head_0.key(),
            position_1: pool_head_1.key(),
            vol_swap_0: swap_vol_0,
            vol_swap_1: swap_vol_1,
            liquidity_0: protocol_position.liquidity_0,
            liquidity_1: protocol_position.liquidity_1
        });
        emit!(ChangePersonalPosition { 
            pool_id: pool_state_loader.key(), 
            payer: payer.key(), 
            position_account: pool_head_0.nft_mint, 
            deposited_amount_0: pool_head_0.amount_0, 
            deposited_amount_1: pool_head_0.amount_1
        });
        emit!(ChangePersonalPosition { 
            pool_id: pool_state_loader.key(), 
            payer: payer.key(), 
            position_account: pool_head_1.nft_mint, 
            deposited_amount_0: pool_head_1.amount_0, 
            deposited_amount_1: pool_head_1.amount_1
        });

        // Renomove from queue and burn position NFT if either order is filled
        if pool_head_0.amount_0 == 0 {
            token_queue_0.pop();
        }
        if pool_head_1.amount_1 == 0 {
            token_queue_1.pop();
        }

        head_0 = token_queue_0.has_next();
        head_1 = token_queue_1.has_next();
    }

    Ok(())
}

//TODO: move this to safe math
pub fn execute_swap(
    a: &mut PersonalPositionState,
    b: &mut PersonalPositionState,
    price: Price,
) -> Result<(u64, u64)> {
    let normalized_price = price.price as f64 * 10f64.powi(price.expo);
    if normalized_price == 0.0 {
        return Err(ErrorCode::PriceFeedErr.into());
    }
    //Volume to fill
    let vtf = normalized_price.mul(a.amount_0 as f64).ceil() as u64; //TODO: check for overflows

    if vtf >= b.amount_0 {
        let swap_vol_0 = normalized_price.mul(b.amount_1 as f64).ceil() as u64;
        let swap_vol_1 = b.amount_1;

        a.amount_0 -= swap_vol_0;
        a.amount_1 += swap_vol_1;

        b.amount_0 += swap_vol_0;
        b.amount_1 -= swap_vol_1;

        Ok((swap_vol_0, swap_vol_1))
    } else {
        let swap_vol_0 = a.amount_0;
        let swap_vol_1 = vtf;

        a.amount_0 -= swap_vol_0;
        b.amount_1 -= swap_vol_1;

        a.amount_1 += swap_vol_1;
        b.amount_0 -= swap_vol_0;

        Ok((swap_vol_0, swap_vol_1))
    }
}
