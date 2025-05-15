use anchor_client::{Client, Cluster};
use anchor_lang::prelude::AccountMeta;
use anchor_spl::associated_token::spl_associated_token_account;
use anchor_spl::token::spl_token;
use anchor_spl::token_2022::spl_token_2022;
use anyhow::Result;
use solana_sdk::{
    instruction::Instruction, pubkey::Pubkey, signature::Signer, system_program, sysvar,
};

use raydium_amm_v3::accounts as raydium_accounts;
use raydium_amm_v3::instruction as raydium_instruction;
use raydium_amm_v3::states::{
    AMM_CONFIG_SEED, OBSERVATION_SEED, OPERATION_SEED, POOL_SEED, POOL_VAULT_SEED, POSITION_SEED,
};
use std::rc::Rc;

use super::super::{read_keypair_file, ClientConfig};

pub fn create_amm_config_instr(
    config: &ClientConfig,
    config_index: u16,
    trade_fee_rate: u32,
    protocol_fee_rate: u32,
    fund_fee_rate: u32,
    queue_type_0: u8,
    queue_type_1: u8
) -> Result<(Vec<Instruction>, Pubkey)> {
    let payer = read_keypair_file(&config.admin_path)?;
    let url = Cluster::Custom(config.http_url.clone(), config.ws_url.clone());
    // Client.
    let client = Client::new(url, Rc::new(payer));
    let program = client.program(config.raydium_v3_program)?;
    let (amm_config_key, __bump) = Pubkey::find_program_address(
        &[AMM_CONFIG_SEED.as_bytes(), &config_index.to_be_bytes()],
        &program.id(),
    );
    let instructions = program
        .request()
        .accounts(raydium_accounts::CreateAmmConfig {
            owner: program.payer(),
            amm_config: amm_config_key,
            system_program: system_program::id(),
        })
        .args(raydium_instruction::CreateAmmConfig {
            index: config_index,
            trade_fee_rate,
            protocol_fee_rate,
            fund_fee_rate,
            queue_type_0,
            queue_type_1,
        })
        .instructions()?;
    Ok((instructions, amm_config_key))
}

pub fn update_amm_config_instr(
    config: &ClientConfig,
    amm_config: Pubkey,
    remaining_accounts: Vec<AccountMeta>,
    param: u8,
    value: u32,
) -> Result<Vec<Instruction>> {
    let payer = read_keypair_file(&config.payer_path)?;
    let admin = read_keypair_file(&config.admin_path)?;
    let url = Cluster::Custom(config.http_url.clone(), config.ws_url.clone());
    // Client.
    let client = Client::new(url, Rc::new(payer));
    let program = client.program(config.raydium_v3_program)?;
    let instructions = program
        .request()
        .accounts(raydium_accounts::UpdateAmmConfig {
            owner: admin.pubkey(),
            amm_config,
        })
        .accounts(remaining_accounts)
        .args(raydium_instruction::UpdateAmmConfig { param, value })
        .instructions()?;
    Ok(instructions)
}

pub fn create_operation_account_instr(config: &ClientConfig) -> Result<Vec<Instruction>> {
    let payer = read_keypair_file(&config.admin_path)?;
    let url = Cluster::Custom(config.http_url.clone(), config.ws_url.clone());
    // Client.
    let client = Client::new(url, Rc::new(payer));
    let program = client.program(config.raydium_v3_program)?;
    let (operation_account_key, __bump) =
        Pubkey::find_program_address(&[OPERATION_SEED.as_bytes()], &program.id());
    let instructions = program
        .request()
        .accounts(raydium_accounts::CreateOperationAccount {
            owner: program.payer(),
            operation_state: operation_account_key,
            system_program: system_program::id(),
        })
        .args(raydium_instruction::CreateOperationAccount)
        .instructions()?;
    Ok(instructions)
}

pub fn update_operation_account_instr(
    config: &ClientConfig,
    param: u8,
    keys: Vec<Pubkey>,
) -> Result<Vec<Instruction>> {
    let payer = read_keypair_file(&config.admin_path)?;
    let url = Cluster::Custom(config.http_url.clone(), config.ws_url.clone());
    // Client.
    let client = Client::new(url, Rc::new(payer));
    let program = client.program(config.raydium_v3_program)?;
    let (operation_account_key, __bump) =
        Pubkey::find_program_address(&[OPERATION_SEED.as_bytes()], &program.id());
    let instructions = program
        .request()
        .accounts(raydium_accounts::UpdateOperationAccount {
            owner: program.payer(),
            operation_state: operation_account_key,
            system_program: system_program::id(),
        })
        .args(raydium_instruction::UpdateOperationAccount { param, keys })
        .instructions()?;
    Ok(instructions)
}

pub fn create_pool_instr(
    config: &ClientConfig,
    amm_config: Pubkey,
    token_mint_0: Pubkey,
    token_mint_1: Pubkey,
    token_program_0: Pubkey,
    token_program_1: Pubkey,
    sqrt_price_x64: u128,
    open_time: u64,
) -> Result<(Vec<Instruction>, Pubkey)> {
    let payer = read_keypair_file(&config.payer_path)?;
    let url = Cluster::Custom(config.http_url.clone(), config.ws_url.clone());
    // Client.
    let client = Client::new(url, Rc::new(payer));
    let program = client.program(config.raydium_v3_program)?;
    let (pool_account_key, __bump) = Pubkey::find_program_address(
        &[
            POOL_SEED.as_bytes(),
            amm_config.to_bytes().as_ref(),
            token_mint_0.to_bytes().as_ref(),
            token_mint_1.to_bytes().as_ref(),
        ],
        &program.id(),
    );
    let (protocol_position_key, __bump) = Pubkey::find_program_address(
        &[
            POSITION_SEED.as_bytes(),
            pool_account_key.as_ref(),
        ],
        &program.id(),
    );
    let (token_vault_0, __bump) = Pubkey::find_program_address(
        &[
            POOL_VAULT_SEED.as_bytes(),
            pool_account_key.to_bytes().as_ref(),
            token_mint_0.to_bytes().as_ref(),
        ],
        &program.id(),
    );
    let (token_vault_1, __bump) = Pubkey::find_program_address(
        &[
            POOL_VAULT_SEED.as_bytes(),
            pool_account_key.to_bytes().as_ref(),
            token_mint_1.to_bytes().as_ref(),
        ],
        &program.id(),
    );
    let (observation_key, __bump) = Pubkey::find_program_address(
        &[
            OBSERVATION_SEED.as_bytes(),
            pool_account_key.to_bytes().as_ref(),
        ],
        &program.id(),
    );
    let instructions = program
        .request()
        .accounts(raydium_accounts::CreatePool {
            pool_creator: program.payer(),
            amm_config,
            pool_state: pool_account_key,
            token_mint_0,
            token_mint_1,
            token_vault_0,
            token_vault_1,
            observation_state: observation_key,
            token_program_0,
            token_program_1,
            system_program: system_program::id(),
            rent: sysvar::rent::id(),
            protocol_position: protocol_position_key,
        })
        .args(raydium_instruction::CreatePool {
            sqrt_price_x64,
            open_time,
        })
        .instructions()?;
    Ok((instructions, pool_account_key))
}

pub fn open_position_with_token22_nft_instr(
    config: &ClientConfig,
    pool_account_key: Pubkey,
    token_vault_0: Pubkey,
    token_vault_1: Pubkey,
    token_mint_0: Pubkey,
    token_mint_1: Pubkey,
    nft_mint_key: Pubkey,
    nft_to_owner: Pubkey,
    user_token_account_0: Pubkey,
    user_token_account_1: Pubkey,
    remaining_accounts: Vec<AccountMeta>,
    amount_0: u64,
    amount_1: u64,
    with_metadata: bool,
) -> Result<Vec<Instruction>> {
    let payer = read_keypair_file(&config.payer_path)?;
    let url = Cluster::Custom(config.http_url.clone(), config.ws_url.clone());
    // Client.
    let client = Client::new(url, Rc::new(payer));
    let program = client.program(config.raydium_v3_program)?;
    let nft_ata_token_account =
        spl_associated_token_account::get_associated_token_address_with_program_id(
            &program.payer(),
            &nft_mint_key,
            &spl_token_2022::id(),
        );
    let (protocol_position_key, __bump) = Pubkey::find_program_address(
        &[
            POSITION_SEED.as_bytes(),
            pool_account_key.to_bytes().as_ref(),
        ],
        &program.id(),
    );
    let (personal_position_key, __bump) = Pubkey::find_program_address(
        &[POSITION_SEED.as_bytes(), nft_mint_key.to_bytes().as_ref()],
        &program.id(),
    );
    let instructions = program
        .request()
        .accounts(raydium_accounts::OpenPositionWithToken22Nft {
            payer: program.payer(),
            position_nft_owner: nft_to_owner,
            position_nft_mint: nft_mint_key,
            position_nft_account: nft_ata_token_account,
            pool_state: pool_account_key,
            protocol_position: protocol_position_key,
            personal_position: personal_position_key,
            token_account_0: user_token_account_0,
            token_account_1: user_token_account_1,
            token_vault_0,
            token_vault_1,
            rent: sysvar::rent::id(),
            system_program: system_program::id(),
            token_program: spl_token::id(),
            associated_token_program: spl_associated_token_account::id(),
            token_program_2022: spl_token_2022::id(),
            vault_0_mint: token_mint_0,
            vault_1_mint: token_mint_1,
        })
        .accounts(remaining_accounts)
        .args(raydium_instruction::OpenPositionWithToken22Nft {
            amount_0,
            amount_1,
            with_metadata,
        })
        .instructions()?;
    Ok(instructions)
}

pub fn close_personal_position_instr(
    config: &ClientConfig,
    nft_mint_key: Pubkey,
    nft_token_key: Pubkey,
    nft_token_program: Pubkey,
) -> Result<Vec<Instruction>> {
    let payer = read_keypair_file(&config.payer_path)?;
    let url = Cluster::Custom(config.http_url.clone(), config.ws_url.clone());
    // Client.
    let client = Client::new(url, Rc::new(payer));
    let program = client.program(config.raydium_v3_program)?;
    let (personal_position_key, __bump) = Pubkey::find_program_address(
        &[POSITION_SEED.as_bytes(), nft_mint_key.to_bytes().as_ref()],
        &program.id(),
    );
    let instructions = program
        .request()
        .accounts(raydium_accounts::ClosePosition {
            nft_owner: program.payer(),
            position_nft_mint: nft_mint_key,
            position_nft_account: nft_token_key,
            personal_position: personal_position_key,
            system_program: system_program::id(),
            token_program: nft_token_program,
        })
        .args(raydium_instruction::ClosePosition)
        .instructions()?;
    Ok(instructions)
}

pub fn swap_instr(
    config: &ClientConfig,
    amm_config: Pubkey,
    pool_account_key: Pubkey,
    input_vault: Pubkey,
    output_vault: Pubkey,
    observation_state: Pubkey,
    user_input_token: Pubkey,
    user_out_put_token: Pubkey,
    remaining_accounts: Vec<AccountMeta>,
    amount: u64,
    other_amount_threshold: u64,
    sqrt_price_limit_x64: Option<u128>,
    is_base_input: bool,
) -> Result<Vec<Instruction>> {
    let payer = read_keypair_file(&config.payer_path)?;
    let url = Cluster::Custom(config.http_url.clone(), config.ws_url.clone());
    // Client.
    let client = Client::new(url, Rc::new(payer));
    let program = client.program(config.raydium_v3_program)?;
    let instructions = program
        .request()
        .accounts(raydium_accounts::SwapSingle {
            payer: program.payer(),
            amm_config,
            pool_state: pool_account_key,
            input_token_account: user_input_token,
            output_token_account: user_out_put_token,
            input_vault,
            output_vault,
            observation_state,
            token_program: spl_token::id(),
        })
        .accounts(remaining_accounts)
        .args(raydium_instruction::Swap {
            amount,
            other_amount_threshold,
            sqrt_price_limit_x64: sqrt_price_limit_x64.unwrap_or(0u128),
            is_base_input,
        })
        .instructions()?;
    Ok(instructions)
}
