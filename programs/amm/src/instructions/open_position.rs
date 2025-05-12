use crate::error::ErrorCode;
use crate::states::*;
use crate::util::*;
use anchor_lang::prelude::*;
use anchor_lang::solana_program;
use anchor_lang::system_program;
use anchor_lang::system_program::{transfer, Transfer};
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::metadata::mpl_token_metadata::types::Creator;
use anchor_spl::token::{Mint, Token, TokenAccount};
use anchor_spl::token_2022::spl_token_2022::extension::{
    BaseStateWithExtensions, StateWithExtensions,
};
use anchor_spl::token_2022::Token2022;
use anchor_spl::token_2022::{
    self,
    spl_token_2022::{self, instruction::AuthorityType},
};
use anchor_spl::token_interface;
use anchor_spl::token_interface::spl_token_metadata_interface;
use mpl_token_metadata::types::DataV2;
use std::cell::RefMut;
#[cfg(feature = "enable-log")]
use std::convert::identity;
use std::ops::DerefMut;

#[derive(Accounts)]
#[instruction(tick_lower_index: i32, tick_upper_index: i32,tick_array_lower_start_index:i32,tick_array_upper_start_index:i32)]
pub struct OpenPosition<'info> {
    /// Pays to mint the position
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: Receives the position NFT
    pub position_nft_owner: UncheckedAccount<'info>,

    /// Unique token mint address
    #[account(
        init,
        mint::decimals = 0,
        mint::authority = pool_state.key(),
        payer = payer,
    )]
    pub position_nft_mint: Box<Account<'info, Mint>>,

    /// Token account where position NFT will be minted
    /// This account created in the contract by cpi to avoid large stack variables
    #[account(
        init,
        associated_token::mint = position_nft_mint,
        associated_token::authority = position_nft_owner,
        payer = payer,
    )]
    pub position_nft_account: Box<Account<'info, TokenAccount>>,

    /// To store metaplex metadata
    /// CHECK: Safety check performed inside function body
    #[account(mut)]
    pub metadata_account: UncheckedAccount<'info>,

    /// Add liquidity for this pool
    #[account(mut)]
    pub pool_state: AccountLoader<'info, PoolState>,

    /// Store the information of market marking in range
    #[account(
        init_if_needed,
        seeds = [
            POSITION_SEED.as_bytes(),
            pool_state.key().as_ref(),
            &tick_lower_index.to_be_bytes(),
            &tick_upper_index.to_be_bytes(),
        ],
        bump,
        payer = payer,
        space = ProtocolPositionState::LEN
    )]
    pub protocol_position: Box<Account<'info, ProtocolPositionState>>,

    /// personal position state
    #[account(
        init,
        seeds = [POSITION_SEED.as_bytes(), position_nft_mint.key().as_ref()],
        bump,
        payer = payer,
        space = PersonalPositionState::LEN
    )]
    pub personal_position: Box<Account<'info, PersonalPositionState>>,

    /// The token_0 account deposit token to the pool
    #[account(
        mut,
        token::mint = token_vault_0.mint
    )]
    pub token_account_0: Box<Account<'info, TokenAccount>>,

    /// The token_1 account deposit token to the pool
    #[account(
        mut,
        token::mint = token_vault_1.mint
    )]
    pub token_account_1: Box<Account<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_0
    #[account(
        mut,
        constraint = token_vault_0.key() == pool_state.load()?.token_vault_0
    )]
    pub token_vault_0: Box<Account<'info, TokenAccount>>,

    /// The address that holds pool tokens for token_1
    #[account(
        mut,
        constraint = token_vault_1.key() == pool_state.load()?.token_vault_1
    )]
    pub token_vault_1: Box<Account<'info, TokenAccount>>,

    /// Sysvar for token mint and ATA creation
    pub rent: Sysvar<'info, Rent>,

    /// Program to create the position manager state account
    pub system_program: Program<'info, System>,

    /// Program to create mint account and mint tokens
    pub token_program: Program<'info, Token>,
    /// Program to create an ATA for receiving position NFT
    pub associated_token_program: Program<'info, AssociatedToken>,

    // /// Program to create NFT metadata
    // /// CHECK: Metadata program address constraint applied
    //pub metadata_program: Program<'info, Metadata>,
    // remaining account
    // #[account(
    //     seeds = [
    //         POOL_TICK_ARRAY_BITMAP_SEED.as_bytes(),
    //         pool_state.key().as_ref(),
    //     ],
    //     bump
    // )]
    // pub tick_array_bitmap: AccountLoader<'info, TickArrayBitmapExtension>,
}

pub fn open_position_v1<'a, 'b, 'c: 'info, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, OpenPosition<'info>>,
    liquidity: u128,
    amount_0_max: u64,
    amount_1_max: u64,
    with_metadata: bool,
    base_flag: Option<bool>,
) -> Result<()> {
    open_position(
        &ctx.accounts.payer,
        &ctx.accounts.position_nft_owner,
        &ctx.accounts.position_nft_mint.to_account_info(),
        &ctx.accounts.position_nft_account.to_account_info(),
        Some(&ctx.accounts.metadata_account),
        &ctx.accounts.pool_state,
        &mut ctx.accounts.protocol_position,
        &mut ctx.accounts.personal_position,
        &ctx.accounts.token_account_0.to_account_info(),
        &ctx.accounts.token_account_1.to_account_info(),
        &ctx.accounts.token_vault_0.to_account_info(),
        &ctx.accounts.token_vault_1.to_account_info(),
        &ctx.accounts.rent,
        &ctx.accounts.system_program,
        &ctx.accounts.token_program,
        &ctx.accounts.associated_token_program,
        //Some(&ctx.accounts.metadata_program),
        None,
        None,
        None,
        &ctx.remaining_accounts,
        ctx.bumps.protocol_position,
        ctx.bumps.personal_position,
        liquidity,
        amount_0_max,
        amount_1_max,
        with_metadata,
        base_flag,
        false,
    )
}

pub fn open_position<'a, 'b, 'c: 'info, 'info>(
    payer: &'b Signer<'info>,
    position_nft_owner: &'b UncheckedAccount<'info>,
    position_nft_mint: &'b AccountInfo<'info>,
    position_nft_account: &'b AccountInfo<'info>,
    metadata_account: Option<&'b UncheckedAccount<'info>>,
    pool_state_loader: &'b AccountLoader<'info, PoolState>,
    protocol_position: &'b mut Box<Account<'info, ProtocolPositionState>>,
    personal_position: &'b mut Box<Account<'info, PersonalPositionState>>,
    token_account_0: &'b AccountInfo<'info>,
    token_account_1: &'b AccountInfo<'info>,
    token_vault_0: &'b AccountInfo<'info>,
    token_vault_1: &'b AccountInfo<'info>,
    rent: &'b Sysvar<'info, Rent>,
    system_program: &'b Program<'info, System>,
    token_program: &'b Program<'info, Token>,
    _associated_token_program: &'b Program<'info, AssociatedToken>,
    //metadata_program: Option<&'b Program<'info, Metadata>>,
    token_program_2022: Option<&'b Program<'info, Token2022>>,
    vault_0_mint: Option<Box<InterfaceAccount<'info, token_interface::Mint>>>,
    vault_1_mint: Option<Box<InterfaceAccount<'info, token_interface::Mint>>>,
    remaining_accounts: &'c [AccountInfo<'info>],
    protocol_position_bump: u8,
    personal_position_bump: u8,
    liquidity: u128,
    amount_0_max: u64,
    amount_1_max: u64,
    with_metadata: bool,
    base_flag: Option<bool>,
    use_metadata_extension: bool,
) -> Result<()> {
    let mut liquidity = liquidity;
    {
        let pool_state = &mut pool_state_loader.load_mut()?;
        if !pool_state.get_status_by_bit(PoolStatusBitIndex::OpenPositionOrIncreaseLiquidity) {
            return err!(ErrorCode::NotApproved);
        }
        
        // check if protocol position is initialized
        let protocol_position = protocol_position.deref_mut();
        if protocol_position.pool_id == Pubkey::default() {
            protocol_position.bump = protocol_position_bump;
            protocol_position.pool_id = pool_state_loader.key();
        }

        let (amount_0, amount_1, amount_0_transfer_fee, amount_1_transfer_fee) = add_liquidity(
            payer,
            token_account_0,
            token_account_1,
            token_vault_0,
            token_vault_1,
            protocol_position,
            token_program_2022,
            token_program,
            vault_0_mint,
            vault_1_mint,
            pool_state,
            &mut liquidity,
            amount_0_max,
            amount_1_max,
            base_flag,
        )?;

        // let personal_position = &mut personal_position;
        personal_position.bump = [personal_position_bump];
        personal_position.nft_mint = position_nft_mint.key();
        personal_position.pool_id = pool_state_loader.key();

        personal_position.fee_growth_inside_0_last_x64 =
            protocol_position.fee_growth_inside_0_last_x64;
        personal_position.fee_growth_inside_1_last_x64 =
            protocol_position.fee_growth_inside_1_last_x64;

        // update rewards, must update before update liquidity
        personal_position.update_rewards(protocol_position.reward_growth_inside, false)?;
        personal_position.liquidity = liquidity;

        emit!(CreatePersonalPositionEvent {
            pool_state: pool_state_loader.key(),
            minter: payer.key(),
            nft_owner: position_nft_owner.key(),
            liquidity: liquidity,
            deposit_amount_0: amount_0,
            deposit_amount_1: amount_1,
            deposit_amount_0_transfer_fee: amount_0_transfer_fee,
            deposit_amount_1_transfer_fee: amount_1_transfer_fee
        });
    }

    mint_nft_and_remove_mint_authority(
        payer,
        pool_state_loader,
        personal_position,
        position_nft_mint,
        position_nft_account,
        metadata_account,
        //metadata_program,
        token_program,
        token_program_2022,
        system_program,
        rent,
        with_metadata,
        use_metadata_extension,
    )
}

/// Add liquidity to an initialized pool
pub fn add_liquidity<'b, 'c: 'info, 'info>(
    payer: &'b Signer<'info>,
    token_account_0: &'b AccountInfo<'info>,
    token_account_1: &'b AccountInfo<'info>,
    token_vault_0: &'b AccountInfo<'info>,
    token_vault_1: &'b AccountInfo<'info>,
    protocol_position: &mut ProtocolPositionState,
    token_program_2022: Option<&Program<'info, Token2022>>,
    token_program: &'b Program<'info, Token>,
    vault_0_mint: Option<Box<InterfaceAccount<'info, token_interface::Mint>>>,
    vault_1_mint: Option<Box<InterfaceAccount<'info, token_interface::Mint>>>,
    pool_state: &mut RefMut<PoolState>,
    liquidity: &mut u128,
    amount_0_max: u64,
    amount_1_max: u64,
    base_flag: Option<bool>,
) -> Result<(u64, u64, u64, u64)> {
    if *liquidity == 0 {
        if base_flag.is_none() {
            // when establishing a new position , liquidity allows for further additions
            return Ok((0, 0, 0, 0));
        }
        if base_flag.unwrap() {
            // must deduct transfer fee before calculate liquidity
            // because only v2 instruction support token_2022, vault_0_mint must be exist
            let amount_0_transfer_fee =
                get_transfer_fee(vault_0_mint.clone().unwrap(), amount_0_max).unwrap();
            // TODO: add liquidity calculation
            /* *liquidity = liquidity_math::get_liquidity_from_single_amount_0(
                pool_state.sqrt_price_x64,
                tick_math::get_sqrt_price_at_tick(tick_lower_index)?,
                tick_math::get_sqrt_price_at_tick(tick_upper_index)?,
                amount_0_max.checked_sub(amount_0_transfer_fee).unwrap(),
            ); */
            #[cfg(feature = "enable-log")]
            msg!(
                "liquidity: {}, amount_0_max:{}, amount_0_transfer_fee:{}",
                *liquidity,
                amount_0_max,
                amount_0_transfer_fee
            );
        } else {
            // must deduct transfer fee before calculate liquidity
            // because only v2 instruction support token_2022, vault_1_mint must be exist
            let amount_1_transfer_fee =
                get_transfer_fee(vault_1_mint.clone().unwrap(), amount_1_max).unwrap();
            // TODO: add liquidity calculation
            /* *liquidity = liquidity_math::get_liquidity_from_single_amount_1(
                pool_state.sqrt_price_x64,
                tick_math::get_sqrt_price_at_tick(tick_lower_index)?,
                tick_math::get_sqrt_price_at_tick(tick_upper_index)?,
                amount_1_max.checked_sub(amount_1_transfer_fee).unwrap(),
            ); */
            #[cfg(feature = "enable-log")]
            msg!(
                "liquidity: {}, amount_1_max:{}, amount_1_transfer_fee:{}",
                *liquidity,
                amount_1_max,
                amount_1_transfer_fee
            );
        }
    }
    assert!(*liquidity > 0);
    let liquidity_before = pool_state.liquidity;

    let clock = Clock::get()?;
    let (amount_0, amount_1) = modify_position(
        i128::try_from(*liquidity).unwrap(),
        pool_state,
        protocol_position,
        clock.unix_timestamp as u64,
    )?;

    require!(
        amount_0 > 0 || amount_1 > 0,
        ErrorCode::ForbidBothZeroForSupplyLiquidity
    );

    let mut amount_0_transfer_fee = 0;
    let mut amount_1_transfer_fee = 0;
    if vault_0_mint.is_some() {
        amount_0_transfer_fee =
            get_transfer_inverse_fee(vault_0_mint.clone().unwrap(), amount_0).unwrap();
    };
    if vault_1_mint.is_some() {
        amount_1_transfer_fee =
            get_transfer_inverse_fee(vault_1_mint.clone().unwrap(), amount_1).unwrap();
    }
    emit!(LiquidityCalculateEvent {
        pool_liquidity: liquidity_before,
        pool_sqrt_price_x64: pool_state.sqrt_price_x64,
        calc_amount_0: amount_0,
        calc_amount_1: amount_1,
        trade_fee_owed_0: 0,
        trade_fee_owed_1: 0,
        transfer_fee_0: amount_0_transfer_fee,
        transfer_fee_1: amount_1_transfer_fee,
    });
    #[cfg(feature = "enable-log")]
    msg!(
        "amount_0: {}, amount_0_transfer_fee: {}, amount_1: {}, amount_1_transfer_fee: {}",
        amount_0,
        amount_0_transfer_fee,
        amount_1,
        amount_1_transfer_fee
    );
    require_gte!(
        amount_0_max,
        amount_0 + amount_0_transfer_fee,
        ErrorCode::PriceSlippageCheck
    );
    require_gte!(
        amount_1_max,
        amount_1 + amount_1_transfer_fee,
        ErrorCode::PriceSlippageCheck
    );
    let mut token_2022_program_opt: Option<AccountInfo> = None;
    if token_program_2022.is_some() {
        token_2022_program_opt = Some(token_program_2022.clone().unwrap().to_account_info());
    }
    transfer_from_user_to_pool_vault(
        payer,
        token_account_0,
        token_vault_0,
        vault_0_mint,
        &token_program,
        token_2022_program_opt.clone(),
        amount_0 + amount_0_transfer_fee,
    )?;

    transfer_from_user_to_pool_vault(
        payer,
        token_account_1,
        token_vault_1,
        vault_1_mint,
        &token_program,
        token_2022_program_opt.clone(),
        amount_1 + amount_1_transfer_fee,
    )?;
    emit!(LiquidityChangeEvent {
        pool_state: pool_state.key(),
        liquidity_before: liquidity_before,
        liquidity_after: pool_state.liquidity,
    });
    Ok((
        amount_0,
        amount_1,
        amount_0_transfer_fee,
        amount_1_transfer_fee,
    ))
}

//TODO: update position modifying
pub fn modify_position(
    liquidity_delta: i128,
    pool_state: &mut RefMut<PoolState>,
    protocol_position_state: &mut ProtocolPositionState,
    timestamp: u64,
) -> Result<(u64, u64)> {
    /* let (flip_tick_lower, flip_tick_upper) = update_position(
        liquidity_delta,
        pool_state,
        protocol_position_state,
        timestamp,
    )?;
    */

    let mut amount_0 = 0;
    let mut amount_1 = 0;

    /* 
    if liquidity_delta != 0 {
        (amount_0, amount_1) = liquidity_math::get_delta_amounts_signed(
            pool_state.tick_current,
            pool_state.sqrt_price_x64,
            liquidity_delta,
        )?;
        if pool_state.tick_current >= tick_lower_state.tick
            && pool_state.tick_current < tick_upper_state.tick
        {
            pool_state.liquidity =
                liquidity_math::add_delta(pool_state.liquidity, liquidity_delta)?;
        }
    }
    */

    Ok((amount_0, amount_1))
}

/// Updates a position with the given liquidity delta and tick
pub fn update_position(
    liquidity_delta: i128,  
    pool_state: &mut RefMut<PoolState>,
    protocol_position_state: &mut ProtocolPositionState,
    timestamp: u64,
) -> Result<(bool, bool)> {
    let updated_reward_infos = pool_state.update_reward_infos(timestamp)?;

    let mut flipped_lower = false;
    let mut flipped_upper = false;

    // update the ticks if liquidity delta is non-zero
    if liquidity_delta != 0 {
        // Update tick state and find if tick is flipped
        #[cfg(feature = "enable-log")]
        msg!(
            "tick_upper.reward_growths_outside_x64:{:?}, tick_lower.reward_growths_outside_x64:{:?}",
            identity(tick_upper_state.reward_growths_outside_x64),
            identity(tick_lower_state.reward_growths_outside_x64)
        );
    }

    Ok((flipped_lower, flipped_upper))
}

fn mint_nft_and_remove_mint_authority<'info>(
    payer: &Signer<'info>,
    pool_state_loader: &AccountLoader<'info, PoolState>,
    personal_position: &Account<'info, PersonalPositionState>,
    position_nft_mint: &AccountInfo<'info>,
    position_nft_account: &AccountInfo<'info>,
    metadata_account: Option<&UncheckedAccount<'info>>,
    //metadata_program: Option<&Program<'info, Metadata>>,
    token_program: &Program<'info, Token>,
    token_program_2022: Option<&Program<'info, Token2022>>,
    system_program: &Program<'info, System>,
    rent: &Sysvar<'info, Rent>,
    with_metadata: bool,
    use_metadata_extension: bool,
) -> Result<()> {
    let pool_state_info = pool_state_loader.to_account_info();
    let position_nft_mint_info = position_nft_mint.to_account_info();
    let pool_state = pool_state_loader.load()?;
    let seeds = pool_state.seeds();

    let token_program_info = if position_nft_mint_info.owner == token_program.key {
        token_program.to_account_info()
    } else {
        token_program_2022.unwrap().to_account_info()
    };

    if with_metadata {
        let (name, symbol, uri) = get_metadata_data(personal_position.key());
        if use_metadata_extension {
            initialize_token_metadata_extension(
                payer,
                &position_nft_mint_info,
                &pool_state_info,
                &personal_position.to_account_info(),
                token_program_2022.unwrap(),
                name,
                symbol,
                uri,
                &[&seeds],
            )?;
        } else {
            initialize_metadata_account(
                payer,
                &pool_state_info,
                &position_nft_mint_info,
                metadata_account.unwrap(),
                //metadata_program.unwrap(),
                system_program,
                rent,
                name,
                symbol,
                uri,
                &[&seeds],
            )?;
        }
    }
    // Mint the NFT
    token_2022::mint_to(
        CpiContext::new_with_signer(
            token_program_info.to_account_info(),
            token_2022::MintTo {
                mint: position_nft_mint_info.clone(),
                to: position_nft_account.to_account_info(),
                authority: pool_state_info.clone(),
            },
            &[&seeds],
        ),
        1,
    )?;

    // Disable minting
    token_2022::set_authority(
        CpiContext::new_with_signer(
            token_program_info.to_account_info(),
            token_2022::SetAuthority {
                current_authority: pool_state_loader.to_account_info(),
                account_or_mint: position_nft_mint_info,
            },
            &[&seeds],
        ),
        AuthorityType::MintTokens,
        None,
    )
}

fn get_metadata_data(personal_position_id: Pubkey) -> (String, String, String) {
    return (
        String::from("Raydium Concentrated Liquidity"),
        String::from("RCL"),
        format!(
            "https://dynamic-ipfs.raydium.io/clmm/position?id={}",
            personal_position_id.to_string()
        ),
    );
}

fn initialize_metadata_account<'info>(
    payer: &Signer<'info>,
    authority: &AccountInfo<'info>,
    position_nft_mint: &AccountInfo<'info>,
    metadata_account: &UncheckedAccount<'info>,
    //metadata_program: &Program<'info, Metadata>,
    system_program: &Program<'info, System>,
    rent: &Sysvar<'info, Rent>,
    name: String,
    symbol: String,
    uri: String,
    signers_seeds: &[&[&[u8]]],
) -> Result<()> {
    let data_v2_instruction = DataV2 {
        name,
        symbol,
        uri,
        seller_fee_basis_points: 0,
        creators: Some(vec![Creator {
            address: authority.key(),
            verified: true,
            share: 100,
        }]),
        collection: None,
        uses: None,
    };

    let accounts = vec![
        AccountMeta::new(metadata_account.key(), false),
        AccountMeta::new_readonly(position_nft_mint.key(), false),
        AccountMeta::new_readonly(authority.key(), true),
        AccountMeta::new(payer.key(), true),
        AccountMeta::new_readonly(authority.key(), true),
        AccountMeta::new_readonly(system_program::ID, false),
    ];

    //No need to pass program_id, as its defined inside CreateMetadataAccountV3Builder
    let create_metadata_instruction =
        mpl_token_metadata::instructions::CreateMetadataAccountV3Builder::new()
            .metadata(metadata_account.key())
            .mint(position_nft_mint.key())
            .mint_authority(authority.key())
            .payer(payer.key())
            .update_authority(authority.key(), true)
            .system_program(system_program::ID)
            .rent(Some(rent.key()))
            .data(data_v2_instruction)
            .add_remaining_accounts(&accounts)
            .instruction();

    solana_program::program::invoke_signed(
        &create_metadata_instruction,
        &[
            metadata_account.to_account_info(),
            position_nft_mint.to_account_info(),
            payer.to_account_info(),
            authority.to_account_info(),
            system_program.to_account_info(),
            rent.to_account_info(),
        ],
        signers_seeds,
    )?;

    Ok(())
}

pub fn initialize_token_metadata_extension<'info>(
    payer: &Signer<'info>,
    position_nft_mint: &AccountInfo<'info>,
    mint_authority: &AccountInfo<'info>,
    metadata_update_authority: &AccountInfo<'info>,
    token_2022_program: &Program<'info, Token2022>,
    name: String,
    symbol: String,
    uri: String,
    signers_seeds: &[&[&[u8]]],
) -> Result<()> {
    let metadata = token_interface::spl_token_metadata_interface::state::TokenMetadata {
        name,
        symbol,
        uri,
        ..Default::default()
    };

    let mint_data = position_nft_mint.try_borrow_data()?;
    let mint_state_unpacked =
        StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_data)?;
    let new_account_len = mint_state_unpacked.try_get_account_len()?;
    let new_rent_exempt_lamports = Rent::get()?.minimum_balance(new_account_len);
    let additional_lamports = new_rent_exempt_lamports.saturating_sub(position_nft_mint.lamports());
    // CPI call will borrow the account data
    drop(mint_data);

    let cpi_context = CpiContext::new(
        token_2022_program.to_account_info(),
        Transfer {
            from: payer.to_account_info(),
            to: position_nft_mint.to_account_info(),
        },
    );
    transfer(cpi_context, additional_lamports)?;

    solana_program::program::invoke_signed(
        &spl_token_metadata_interface::instruction::initialize(
            token_2022_program.key,
            position_nft_mint.key,
            metadata_update_authority.key,
            position_nft_mint.key,
            &mint_authority.key(),
            metadata.name,
            metadata.symbol,
            metadata.uri,
        ),
        &[
            position_nft_mint.to_account_info(),
            mint_authority.to_account_info(),
            metadata_update_authority.to_account_info(),
            token_2022_program.to_account_info(),
        ],
        signers_seeds,
    )?;

    Ok(())
}

#[cfg(test)]
mod modify_position_test {
    use super::modify_position;
    use crate::error::ErrorCode;
    use crate::states::oracle::block_timestamp_mock;
    use crate::states::pool_test::build_pool;
    use crate::states::protocol_position::*;

    #[test]
    fn liquidity_delta_zero_empty_liquidity_not_allowed_test() {
        let pool_state_ref = build_pool( 1000, 10000);
        let pool_state = &mut pool_state_ref.borrow_mut();

        let result = modify_position(
            0,
            pool_state,
            &mut ProtocolPositionState::default(),
            block_timestamp_mock(),
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ErrorCode::InvaildLiquidity.into());
    }

    #[test]
    fn init_position_in_range_test() {
        let liquidity = 10000;
        let tick_current = 1;
        let pool_state_ref = build_pool(
            tick_current, //tick_math::get_sqrt_price_at_tick(tick_current).unwrap(), TODO: calculate this
            liquidity,
        );
        let pool_state = &mut pool_state_ref.borrow_mut();

        let liquidity_delta = 10000;
        let protocol_position = &mut ProtocolPositionState::default();
        let (amount_0_int, amount_1_int) = modify_position(
            liquidity_delta,
            pool_state,
            protocol_position,
            block_timestamp_mock(),
        )
        .unwrap();
        assert!(amount_0_int != 0);
        assert!(amount_1_int != 0);

        // check pool active liquidity
        let new_liquidity = pool_state.liquidity;
        assert_eq!(new_liquidity, liquidity + (liquidity_delta as u128));

        // check tick state
        // check protocol position
        let fee_growth_inside_0_last_x64 = pool_state.fee_growth_global_0_x64;
        let fee_growth_inside_1_last_x64 = pool_state.fee_growth_global_1_x64;
        assert!(protocol_position.fee_growth_inside_0_last_x64 == fee_growth_inside_0_last_x64);
        assert!(protocol_position.fee_growth_inside_1_last_x64 == fee_growth_inside_1_last_x64);
        assert!(protocol_position.token_fees_owed_0 == 0);
        assert!(protocol_position.token_fees_owed_1 == 0);

        // check protocol position state
    }
}
