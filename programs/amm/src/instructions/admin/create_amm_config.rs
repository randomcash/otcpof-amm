use crate::error::ErrorCode;
use crate::events::ConfigChangeEvent;
use crate::libraries::QueueType;
use crate::states::*;
use anchor_lang::prelude::*;
use std::ops::DerefMut;

#[derive(Accounts)]
#[instruction(index: u16)]
pub struct CreateAmmConfig<'info> {
    /// Address to be set as protocol owner.
    #[account(
        mut,
        address = crate::admin::id() @ ErrorCode::NotApproved
    )]
    pub owner: Signer<'info>,

    /// Initialize config state account to store protocol owner address and fee rates.
    #[account(
        init_if_needed,
        seeds = [
            AMM_CONFIG_SEED.as_bytes(),
            &index.to_be_bytes()
        ],
        bump,
        payer = owner,
        space = AmmConfig::LEN
    )]
    pub amm_config: Account<'info, AmmConfig>,

    pub system_program: Program<'info, System>,
}

pub fn create_amm_config(
    ctx: Context<CreateAmmConfig>,
    index: u16,
    trade_fee_rate: u32,
    protocol_fee_rate: u32,
    fund_fee_rate: u32,
    queue_type_0: u8,
    queue_type_1: u8,
    price_feed_max_age: u64
) -> Result<()> {
    let amm_config = ctx.accounts.amm_config.deref_mut();
    amm_config.owner = ctx.accounts.owner.key();
    amm_config.bump = ctx.bumps.amm_config;
    amm_config.index = index;
    amm_config.trade_fee_rate = trade_fee_rate;
    amm_config.protocol_fee_rate = protocol_fee_rate;
    amm_config.fund_fee_rate = fund_fee_rate;
    amm_config.fund_owner = ctx.accounts.owner.key();
    amm_config.queue_type_0 = QueueType::from(queue_type_0);
    amm_config.queue_type_1 = QueueType::from(queue_type_1);
    amm_config.price_feed_max_age = price_feed_max_age;

    emit!(ConfigChangeEvent {
        index: amm_config.index,
        owner: ctx.accounts.owner.key(),
        protocol_fee_rate: amm_config.protocol_fee_rate,
        trade_fee_rate: amm_config.trade_fee_rate,
        fund_fee_rate: amm_config.fund_fee_rate,
        fund_owner: amm_config.fund_owner,
        queue_type_0: amm_config.queue_type_0,
        queue_type_1: amm_config.queue_type_1,
        price_feed_max_age: amm_config.price_feed_max_age
    });

    Ok(())
}
