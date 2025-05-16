use crate::error::ErrorCode;
use crate::libraries::Queue;
use crate::states::*;
use crate::util::*;
use anchor_lang::prelude::*;
use anchor_lang::solana_program;
use anchor_lang::system_program;
use anchor_lang::system_program::{transfer, Transfer};
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::metadata::mpl_token_metadata::types::Creator;
use anchor_spl::token::Token;
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
#[cfg(feature = "enable-log")]
use std::convert::identity;

pub fn open_position<'a, 'b, 'c: 'info, 'info>(
    payer: &'b Signer<'info>,
    position_nft_owner: &'b UncheckedAccount<'info>,
    position_nft_mint: &'b AccountInfo<'info>,
    position_nft_account: &'b AccountInfo<'info>,
    metadata_account: Option<&'b UncheckedAccount<'info>>,
    pool_state_loader: &'b AccountLoader<'info, PoolState>,
    protocol_position: &'b mut AccountLoader<'info, ProtocolPositionState>,
    personal_position: &'b mut Box<Account<'info, PersonalPositionState>>,
    token_account_0: &'b AccountInfo<'info>,
    token_account_1: &'b AccountInfo<'info>,
    token_vault_0: &'b AccountInfo<'info>,
    token_vault_1: &'b AccountInfo<'info>,
    token_queue_0: &'b AccountLoader<'info, PoolQueue>,
    token_queue_1: &'b AccountLoader<'info, PoolQueue>,
    rent: &'b Sysvar<'info, Rent>,
    system_program: &'b Program<'info, System>,
    token_program: &'b Program<'info, Token>,
    _associated_token_program: &'b Program<'info, AssociatedToken>,
    //metadata_program: Option<&'b Program<'info, Metadata>>,
    token_program_2022: Option<&'b Program<'info, Token2022>>,
    vault_0_mint: Option<Box<InterfaceAccount<'info, token_interface::Mint>>>,
    vault_1_mint: Option<Box<InterfaceAccount<'info, token_interface::Mint>>>,
    protocol_position_bump: u8,
    personal_position_bump: u8,
    amount_0: u64,
    amount_1: u64,
    with_metadata: bool,
    use_metadata_extension: bool,
) -> Result<()> {
    let pool_state = pool_state_loader.load()?;
    if !pool_state.get_status_by_bit(PoolStatusBitIndex::OpenPositionOrIncreaseLiquidity) {
        return err!(ErrorCode::NotApproved);
    }

    // check if protocol position is initialized
    let mut protocol_position = protocol_position.load_mut()?;
    if protocol_position.pool_id == Pubkey::default() {
        protocol_position.initialize(protocol_position_bump, pool_state_loader.key())?;
    }

    let liquidity_before_0 = protocol_position.liquidity_0;
    let liquidity_before_1 = protocol_position.liquidity_1;

    // Move liquidity into pool
    let (amount_0, amount_1, amount_0_transfer_fee, amount_1_transfer_fee) = add_liquidity(
        payer,
        token_account_0,
        token_account_1,
        token_vault_0,
        token_vault_1,
        &mut protocol_position,
        token_program_2022,
        token_program,
        vault_0_mint,
        vault_1_mint,
        amount_0,
        amount_1,
    )?;
    emit!(LiquidityChangeEvent {
        pool_state: pool_state_loader.key(),
        liquidity_before_0,
        liquidity_before_1,
        liquidity_after_0: protocol_position.liquidity_0,
        liquidity_after_1: protocol_position.liquidity_1
    });

    // Mint position nft and remove authority
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
    )?;
    personal_position.initialize(
        personal_position_bump,
        position_nft_mint.key(),
        pool_state_loader.key(),
        amount_0,
        amount_1,
    )?;
    emit!(CreatePersonalPositionEvent {
        pool_id: pool_state_loader.key(),
        minter: payer.key(),
        nft_owner: position_nft_owner.key(),
        deposit_amount_0: amount_0,
        deposit_amount_1: amount_1,
        deposit_amount_0_transfer_fee: amount_0_transfer_fee,
        deposit_amount_1_transfer_fee: amount_1_transfer_fee
    });

    // Push position into pool queue
    if amount_0 > 0 {
        let mut token_queue_0 = token_queue_0.load_mut()?;
        token_queue_0.push(personal_position.key())?;
        emit!(PositionPushedToQueueEvent {
            pool_id: pool_state_loader.key(),
            minter: payer.key(),
            nft_owner: position_nft_owner.key(),
            position_address: personal_position.key(),
            queue_position: token_queue_0.len()
        });
    }
    if amount_1 > 0 {
        let mut token_queue_1 = token_queue_1.load_mut()?;
        token_queue_1.push(personal_position.key())?;
        emit!(PositionPushedToQueueEvent {
            pool_id: pool_state_loader.key(),
            minter: payer.key(),
            nft_owner: position_nft_owner.key(),
            position_address: personal_position.key(),
            queue_position: token_queue_1.len()
        });
    }

    Ok(())
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
    amount_0: u64,
    amount_1: u64,
) -> Result<(u64, u64, u64, u64)> {
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
    require_gte!(
        amount_0,
        amount_0 + amount_0_transfer_fee,
        ErrorCode::PriceSlippageCheck
    );
    require_gte!(
        amount_1,
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

    let delta_0 = i64::try_from(amount_0)?;
    let delta_1 = i64::try_from(amount_1)?;
    protocol_position.update(delta_0, delta_1, delta_0, delta_0)?;

    Ok((
        amount_0,
        amount_1,
        amount_0_transfer_fee,
        amount_1_transfer_fee,
    ))
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

//TODO: update this with protocol metadata
//TODO: move this to another proper file with protocol metadata
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
