#![allow(dead_code)]
use anchor_client::solana_account_decoder::{
    parse_token::{TokenAccountType, UiAccountState},
    UiAccountData, UiAccountEncoding,
};
use anchor_client::solana_client::{
    rpc_client::RpcClient,
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig, RpcTransactionConfig},
    rpc_filter::{Memcmp, RpcFilterType},
    rpc_request::TokenAccountsFilter,
};
use anchor_client::{Client, Cluster};
use anchor_lang::prelude::AccountMeta;
use anchor_spl::{associated_token::spl_associated_token_account, token::spl_token};
use anyhow::{format_err, Result};
use arrayref::array_ref;
use borsh::{to_vec, BorshSerialize};
use chrono::Utc;
use clap::Parser;
use configparser::ini::Ini;
use raydium_amm_v3::{libraries::Queue, ACCOUNT_DATA_LEN, PYTH_PROGRAM_ID};
use solana_sdk::{
    clock, commitment_config::CommitmentConfig, compute_budget::ComputeBudgetInstruction, instruction::Instruction, program_pack::Pack, pubkey::Pubkey, signature::{Keypair, Signature, Signer}, system_instruction, sysvar::Sysvar, transaction::Transaction
};
use solana_transaction_status::UiTransactionEncoding;
use spl_token_client::{spl_token_2022, token::ExtensionInitializationParams};
use std::mem::size_of;
use std::path::Path;
use std::rc::Rc;
use std::str::FromStr;

mod instructions;
use instructions::amm_instructions::*;
use instructions::events_instructions_parse::*;
use instructions::rpc::*;
use instructions::token_instructions::*;
use instructions::utils::*;
use spl_token_client::spl_token_2022::{
    extension::StateWithExtensions,
    state::Mint,
    state::{Account, AccountState},
};

#[derive(Clone, Debug, PartialEq)]
pub struct ClientConfig {
    http_url: String,
    ws_url: String,
    payer_path: String,
    admin_path: String,
    raydium_v3_program: Pubkey,
    slippage: f64,
    amm_config_key: Pubkey,

    mint0: Option<Pubkey>,
    mint1: Option<Pubkey>,
    pool_id_account: Option<Pubkey>,
    amm_config_index: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PoolAccounts {
    pool_id: Option<Pubkey>,
    pool_config: Option<Pubkey>,
    pool_observation: Option<Pubkey>,
    pool_protocol_positions: Vec<Pubkey>,
    pool_personal_positions: Vec<Pubkey>,
}
#[repr(u32)]
enum PythInstruction {
    /// This isn't official; we mock an example layout
    UpdatePrice = 0,
}

/// Mock instruction data
#[derive(BorshSerialize)]
struct UpdatePriceData {
    instruction: u32, // PythInstruction::UpdatePrice
    price: i64,       // mock price
    confidence: u64,  // confidence interval
    expo: i32,        // exponent (e.g., -6 for 0.000001)
    publish_time: i64,
}

fn load_cfg(client_config: &String) -> Result<ClientConfig> {
    let mut config = Ini::new();
    let _map = config.load(client_config).unwrap();
    let http_url = config.get("Global", "http_url").unwrap();
    if http_url.is_empty() {
        panic!("http_url must not be empty");
    }
    let ws_url = config.get("Global", "ws_url").unwrap();
    if ws_url.is_empty() {
        panic!("ws_url must not be empty");
    }
    let payer_path = config.get("Global", "payer_path").unwrap();
    if payer_path.is_empty() {
        panic!("payer_path must not be empty");
    }
    let admin_path = config.get("Global", "admin_path").unwrap();
    if admin_path.is_empty() {
        panic!("admin_path must not be empty");
    }

    let raydium_v3_program_str = config.get("Global", "raydium_v3_program").unwrap();
    if raydium_v3_program_str.is_empty() {
        panic!("raydium_v3_program must not be empty");
    }
    let raydium_v3_program = Pubkey::from_str(&raydium_v3_program_str).unwrap();
    let slippage = config.getfloat("Global", "slippage").unwrap().unwrap();

    let mut mint0 = None;
    let mint0_str = config.get("Pool", "mint0").unwrap();
    if !mint0_str.is_empty() {
        mint0 = Some(Pubkey::from_str(&mint0_str).unwrap());
    }
    let mut mint1 = None;
    let mint1_str = config.get("Pool", "mint1").unwrap();
    if !mint1_str.is_empty() {
        mint1 = Some(Pubkey::from_str(&mint1_str).unwrap());
    }
    let amm_config_index = config.getuint("Pool", "amm_config_index").unwrap().unwrap() as u16;

    let (amm_config_key, __bump) = Pubkey::find_program_address(
        &[
            raydium_amm_v3::states::AMM_CONFIG_SEED.as_bytes(),
            &amm_config_index.to_be_bytes(),
        ],
        &raydium_v3_program,
    );

    let pool_id_account = if mint0 != None && mint1 != None {
        if mint0.unwrap() > mint1.unwrap() {
            let temp_mint = mint0;
            mint0 = mint1;
            mint1 = temp_mint;
        }
        Some(
            Pubkey::find_program_address(
                &[
                    raydium_amm_v3::states::POOL_SEED.as_bytes(),
                    amm_config_key.to_bytes().as_ref(),
                    mint0.unwrap().to_bytes().as_ref(),
                    mint1.unwrap().to_bytes().as_ref(),
                ],
                &raydium_v3_program,
            )
            .0,
        )
    } else {
        None
    };

    Ok(ClientConfig {
        http_url,
        ws_url,
        payer_path,
        admin_path,
        raydium_v3_program,
        slippage,
        amm_config_key,
        mint0,
        mint1,
        pool_id_account,
        amm_config_index,
    })
}
fn read_keypair_file(s: &str) -> Result<Keypair> {
    solana_sdk::signature::read_keypair_file(s)
        .map_err(|_| format_err!("failed to read keypair from {}", s))
}
fn write_keypair_file(keypair: &Keypair, outfile: &str) -> Result<String> {
    solana_sdk::signature::write_keypair_file(keypair, outfile)
        .map_err(|_| format_err!("failed to write keypair to {}", outfile))
}
fn path_is_exist(path: &str) -> bool {
    Path::new(path).exists()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PositionNftTokenInfo {
    key: Pubkey,
    program: Pubkey,
    position: Pubkey,
    mint: Pubkey,
    amount: u64,
    decimals: u8,
}
fn get_all_nft_and_position_by_owner(
    client: &RpcClient,
    owner: &Pubkey,
    raydium_amm_v3_program: &Pubkey,
) -> Vec<PositionNftTokenInfo> {
    let mut spl_nfts = get_nft_account_and_position_by_owner(
        client,
        owner,
        spl_token::id(),
        raydium_amm_v3_program,
    );
    let spl_2022_nfts = get_nft_account_and_position_by_owner(
        client,
        owner,
        spl_token_2022::id(),
        raydium_amm_v3_program,
    );
    spl_nfts.extend(spl_2022_nfts);
    spl_nfts
}
fn get_nft_account_and_position_by_owner(
    client: &RpcClient,
    owner: &Pubkey,
    token_program: Pubkey,
    raydium_amm_v3_program: &Pubkey,
) -> Vec<PositionNftTokenInfo> {
    let all_tokens = client
        .get_token_accounts_by_owner(owner, TokenAccountsFilter::ProgramId(token_program))
        .unwrap();
    let mut position_nft_accounts = Vec::new();
    for keyed_account in all_tokens {
        if let UiAccountData::Json(parsed_account) = keyed_account.account.data {
            if parsed_account.program == "spl-token" || parsed_account.program == "spl-token-2022" {
                if let Ok(TokenAccountType::Account(ui_token_account)) =
                    serde_json::from_value(parsed_account.parsed)
                {
                    let _frozen = ui_token_account.state == UiAccountState::Frozen;

                    let token = ui_token_account
                        .mint
                        .parse::<Pubkey>()
                        .unwrap_or_else(|err| panic!("Invalid mint: {}", err));
                    let token_account = keyed_account
                        .pubkey
                        .parse::<Pubkey>()
                        .unwrap_or_else(|err| panic!("Invalid token account: {}", err));
                    let token_amount = ui_token_account
                        .token_amount
                        .amount
                        .parse::<u64>()
                        .unwrap_or_else(|err| panic!("Invalid token amount: {}", err));

                    let _close_authority = ui_token_account.close_authority.map_or(*owner, |s| {
                        s.parse::<Pubkey>()
                            .unwrap_or_else(|err| panic!("Invalid close authority: {}", err))
                    });

                    if ui_token_account.token_amount.decimals == 0 && token_amount == 1 {
                        let (position_pda, _) = Pubkey::find_program_address(
                            &[
                                raydium_amm_v3::states::POSITION_SEED.as_bytes(),
                                token.to_bytes().as_ref(),
                            ],
                            &raydium_amm_v3_program,
                        );
                        position_nft_accounts.push(PositionNftTokenInfo {
                            key: token_account,
                            program: token_program,
                            position: position_pda,
                            mint: token,
                            amount: token_amount,
                            decimals: ui_token_account.token_amount.decimals,
                        });
                    }
                }
            }
        }
    }
    position_nft_accounts
}

#[derive(Debug, Parser)]
pub struct Opts {
    #[clap(subcommand)]
    pub command: CommandsName,
}
#[derive(Debug, Parser)]
pub enum CommandsName {
    NewMint {
        #[arg(short, long)]
        decimals: u8,
        authority: Option<Pubkey>,
        #[arg(short, long)]
        token_2022: bool,
        #[arg(short, long)]
        freezable: bool,
        #[arg(short, long)]
        closable: bool,
        #[arg(short, long)]
        non_transferable: bool,
        #[arg(short, long)]
        permanent_delegate: bool,
        rate_bps: Option<i16>,
        default_account_state: Option<String>,
        transfer_fee: Option<Vec<u64>>,
        #[arg(last(true))]
        confidential_transfer_auto_approve: Option<bool>,
    },
    NewToken {
        mint: Pubkey,
        authority: Pubkey,
        #[arg(short, long)]
        not_ata: bool,
    },
    MintTo {
        mint: Pubkey,
        to_token: Pubkey,
        amount: u64,
    },
    WrapSol {
        amount: u64,
    },
    UnWrapSol {
        wrap_sol_account: Pubkey,
    },
    CreateConfig {
        config_index: u16,
        trade_fee_rate: u32,
        protocol_fee_rate: u32,
        fund_fee_rate: u32,
        queue_type_0: u8,
        queue_type_1: u8,
        price_feed_max_age: u64,
    },
    UpdateConfig {
        config_index: u16,
        param: u8,
        value: u32,
        remaining: Option<Pubkey>,
    },
    CreateOperation,
    UpdateOperation {
        param: u8,
        keys: Vec<Pubkey>,
    },
    CreatePriceFeed {
        /// 1.200000 price
        #[arg(long, default_value = "1200000")] 
        price: i64,
        /// 1.000000 confidence
        #[arg(long, default_value = "1000000")] 
        confidence: u64,
        /// Exponent (e.g. -6 for 10^-6)
        #[arg(long, default_value = "-6")]
        expo: i32,
    },
    CreatePool {
        config_index: u16,
        price: f64,
        mint0: Pubkey,
        mint1: Pubkey,
        price_feed: Pubkey,
        #[arg(short, long, default_value_t = 0)]
        open_time: u64,
    },
    OpenPosition {
        pool_address: Pubkey,
        amount_0: u64,
        amount_1: u64,
        #[arg(short, long)]
        with_metadata: bool,
    },
    Swap {
        pool_address: Pubkey,
        #[arg(short, long)]
        simulate: bool,
    },
    PPositionByOwner {
        user_wallet: Pubkey,
    },
    CompareKey {
        key0: Pubkey,
        key1: Pubkey,
    },
    PMint {
        mint: Pubkey,
    },
    PToken {
        token: Pubkey,
    },
    POperation,
    PObservation,
    PConfig {
        config_index: u16,
    },
    PPersonalPositionByPool {
        pool_id: Option<Pubkey>,
    },
    PProtocolPositionByPool {
        pool_id: Option<Pubkey>,
    },
    PPool {
        pool_id: Option<Pubkey>,
    },
    PProtocol {
        protocol_id: Pubkey,
    },
    PPersonal {
        personal_id: Pubkey,
    },
    DecodeInstruction {
        instr_hex_data: String,
    },
    DecodeEvent {
        log_event: String,
    },
    DecodeTxLog {
        tx_id: String,
    },
}
// #[cfg(not(feature = "async"))]
fn main() -> Result<()> {
    println!("Starting...");
    let client_config = "client_config.ini";
    let pool_config = load_cfg(&client_config.to_string()).unwrap();
    // Admin and cluster params.
    let payer = read_keypair_file(&pool_config.payer_path)?;
    let admin = read_keypair_file(&pool_config.admin_path)?;
    // solana rpc client
    let rpc_client = RpcClient::new(pool_config.http_url.to_string());

    // anchor client.
    let anchor_config = pool_config.clone();
    let url = Cluster::Custom(anchor_config.http_url, anchor_config.ws_url);
    let wallet = read_keypair_file(&pool_config.payer_path)?;
    let anchor_client = Client::new(url, Rc::new(wallet));
    let program = anchor_client.program(pool_config.raydium_v3_program)?;

    let opts = Opts::parse();
    match opts.command {
        CommandsName::NewMint {
            authority,
            decimals,
            token_2022,
            freezable: enable_freeze,
            closable: enable_close,
            non_transferable: enable_non_transferable,
            permanent_delegate: enable_permanent_delegate,
            rate_bps,
            default_account_state,
            transfer_fee,
            confidential_transfer_auto_approve,
        } => {
            let token_program = if token_2022 {
                spl_token_2022::id()
            } else {
                spl_token::id()
            };
            let authority = if let Some(key) = authority {
                key
            } else {
                payer.pubkey()
            };
            let freeze_authority = if enable_freeze { Some(authority) } else { None };
            let mut extensions = vec![];
            if enable_close {
                extensions.push(ExtensionInitializationParams::MintCloseAuthority {
                    close_authority: Some(authority),
                });
            }
            if enable_permanent_delegate {
                extensions.push(ExtensionInitializationParams::PermanentDelegate {
                    delegate: authority,
                });
            }
            if let Some(rate_bps) = rate_bps {
                extensions.push(ExtensionInitializationParams::InterestBearingConfig {
                    rate_authority: Some(authority),
                    rate: rate_bps,
                })
            }
            if let Some(state) = default_account_state {
                assert!(
                    enable_freeze,
                    "Token requires a freeze authority to default to frozen accounts"
                );
                let account_state;
                match state.as_str() {
                    "Uninitialized" => account_state = AccountState::Uninitialized,
                    "Initialized" => account_state = AccountState::Initialized,
                    "Frozen" => account_state = AccountState::Frozen,
                    _ => panic!("error default_account_state[Uninitialized, Initialized, Frozen]"),
                }
                extensions.push(ExtensionInitializationParams::DefaultAccountState {
                    state: account_state,
                })
            }
            if let Some(transfer_fee_value) = transfer_fee {
                let transfer_fee_basis_points = transfer_fee_value[0] as u16;
                let maximum_fee = transfer_fee_value[1];
                extensions.push(ExtensionInitializationParams::TransferFeeConfig {
                    transfer_fee_config_authority: Some(authority),
                    withdraw_withheld_authority: Some(authority),
                    transfer_fee_basis_points,
                    maximum_fee,
                });
            }
            if enable_non_transferable {
                extensions.push(ExtensionInitializationParams::NonTransferable);
            }
            if let Some(auto_approve) = confidential_transfer_auto_approve {
                extensions.push(ExtensionInitializationParams::ConfidentialTransferMint {
                    authority: Some(authority),
                    auto_approve_new_accounts: auto_approve,
                    auditor_elgamal_pubkey: None,
                });
            }

            let mint = Keypair::new();
            let create_and_init_instr = create_and_init_mint_instr(
                &pool_config.clone(),
                token_program,
                &mint.pubkey(),
                &authority,
                freeze_authority.as_ref(),
                extensions,
                decimals as u8,
            )?;
            // send
            let signers = vec![&payer, &mint];
            let recent_hash = rpc_client.get_latest_blockhash()?;
            let txn = Transaction::new_signed_with_payer(
                &create_and_init_instr,
                Some(&payer.pubkey()),
                &signers,
                recent_hash,
            );
            let signature = send_txn(&rpc_client, &txn, true)?;

            println!("mint: {}", mint.pubkey());
            println!("signature: {}", signature);
        }
        CommandsName::NewToken {
            mint,
            authority,
            not_ata,
        } => {
            let mut signers = vec![&payer];
            let auxiliary_token_keypair = Keypair::new();
            let create_ata_instr = if not_ata {
                signers.push(&auxiliary_token_keypair);
                create_and_init_auxiliary_token(
                    &pool_config.clone(),
                    &auxiliary_token_keypair.pubkey(),
                    &mint,
                    &authority,
                )?
            } else {
                let mint_account = rpc_client.get_account(&mint)?;
                create_ata_token_account_instr(
                    &pool_config.clone(),
                    mint_account.owner,
                    &mint,
                    &authority,
                )?
            };
            // send
            let recent_hash = rpc_client.get_latest_blockhash()?;
            let txn = Transaction::new_signed_with_payer(
                &create_ata_instr,
                Some(&payer.pubkey()),
                &signers,
                recent_hash,
            );
            let signature = send_txn(&rpc_client, &txn, true)?;
            println!("token: {:?}", auxiliary_token_keypair.pubkey());
            println!("signature: {}", signature);
        }
        CommandsName::MintTo {
            mint,
            to_token,
            amount,
        } => {
            let mint_account = rpc_client.get_account(&mint)?;
            let mint_to_instr = spl_token_mint_to_instr(
                &pool_config.clone(),
                mint_account.owner,
                &mint,
                &to_token,
                amount,
                &payer,
            )?;
            // send
            let signers = vec![&payer];
            let recent_hash = rpc_client.get_latest_blockhash()?;
            let txn = Transaction::new_signed_with_payer(
                &mint_to_instr,
                Some(&payer.pubkey()),
                &signers,
                recent_hash,
            );
            let signature = send_txn(&rpc_client, &txn, true)?;
            println!("signature: {}", signature);
        }
        CommandsName::WrapSol { amount } => {
            let wrap_sol_instr = wrap_sol_instr(&pool_config, amount)?;
            // send
            let signers = vec![&payer];
            let recent_hash = rpc_client.get_latest_blockhash()?;
            let txn = Transaction::new_signed_with_payer(
                &wrap_sol_instr,
                Some(&payer.pubkey()),
                &signers,
                recent_hash,
            );
            let signature = send_txn(&rpc_client, &txn, true)?;
            println!("signature: {}", signature);
        }
        CommandsName::UnWrapSol { wrap_sol_account } => {
            let unwrap_sol_instr =
                close_token_account(&pool_config, &wrap_sol_account, &payer.pubkey(), &payer)?;
            // send
            let signers = vec![&payer];
            let recent_hash = rpc_client.get_latest_blockhash()?;
            let txn = Transaction::new_signed_with_payer(
                &unwrap_sol_instr,
                Some(&payer.pubkey()),
                &signers,
                recent_hash,
            );
            let signature = send_txn(&rpc_client, &txn, true)?;
            println!("signature: {}", signature);
        }
        CommandsName::CreateConfig {
            config_index,
            trade_fee_rate,
            protocol_fee_rate,
            fund_fee_rate,
            queue_type_0,
            queue_type_1,
            price_feed_max_age,
        } => {
            let (create_instr, amm_config_key) = create_amm_config_instr(
                &pool_config.clone(),
                config_index,
                trade_fee_rate,
                protocol_fee_rate,
                fund_fee_rate,
                queue_type_0,
                queue_type_1,
                price_feed_max_age,
            )?;
            // send
            let signers = vec![&payer, &admin];
            let recent_hash = rpc_client.get_latest_blockhash()?;
            let txn = Transaction::new_signed_with_payer(
                &create_instr,
                Some(&payer.pubkey()),
                &signers,
                recent_hash,
            );
            let signature = send_txn(&rpc_client, &txn, true)?;
            println!("amm_config_key: {}", amm_config_key);
            println!("signature: {}", signature);
        }
        CommandsName::UpdateConfig {
            config_index,
            param,
            value,
            remaining,
        } => {
            let mut remaing_accounts = Vec::new();
            let mut update_value = 0;
            let match_param = Some(param);
            match match_param {
                Some(0) => update_value = value,
                Some(1) => update_value = value,
                Some(2) => update_value = value,
                Some(3) => {
                    let remaining_key = remaining.unwrap();
                    remaing_accounts.push(AccountMeta::new_readonly(remaining_key, false));
                }
                Some(4) => {
                    let remaining_key = remaining.unwrap();
                    remaing_accounts.push(AccountMeta::new_readonly(remaining_key, false));
                }
                Some(5) => {
                    todo!("updating queue type not supported yet");
                }
                _ => panic!("error input"),
            }
            let (amm_config_key, __bump) = Pubkey::find_program_address(
                &[
                    raydium_amm_v3::states::AMM_CONFIG_SEED.as_bytes(),
                    &config_index.to_be_bytes(),
                ],
                &pool_config.raydium_v3_program,
            );
            let update_amm_config_instr = update_amm_config_instr(
                &pool_config.clone(),
                amm_config_key,
                remaing_accounts,
                param,
                update_value,
            )?;
            // send
            let signers = vec![&payer, &admin];
            let recent_hash = rpc_client.get_latest_blockhash()?;
            let txn = Transaction::new_signed_with_payer(
                &update_amm_config_instr,
                Some(&payer.pubkey()),
                &signers,
                recent_hash,
            );
            let signature = send_txn(&rpc_client, &txn, true)?;
            println!("signature: {}", signature);
        }
        CommandsName::CreateOperation => {
            let create_instr = create_operation_account_instr(&pool_config.clone())?;
            // send
            let signers = vec![&payer, &admin];
            let recent_hash = rpc_client.get_latest_blockhash()?;
            let txn = Transaction::new_signed_with_payer(
                &create_instr,
                Some(&payer.pubkey()),
                &signers,
                recent_hash,
            );
            let signature = send_txn(&rpc_client, &txn, true)?;
            println!("signature: {}", signature);
        }
        CommandsName::UpdateOperation { param, keys } => {
            let create_instr = update_operation_account_instr(&pool_config.clone(), param, keys)?;
            // send
            let signers = vec![&payer, &admin];
            let recent_hash = rpc_client.get_latest_blockhash()?;
            let txn = Transaction::new_signed_with_payer(
                &create_instr,
                Some(&payer.pubkey()),
                &signers,
                recent_hash,
            );
            let signature = send_txn(&rpc_client, &txn, true)?;
            println!("signature: {}", signature);
        }
        CommandsName::CreatePriceFeed { price, confidence, expo } => {
            let price_feed = Keypair::new();
            let lamports = rpc_client
                .get_minimum_balance_for_rent_exemption(ACCOUNT_DATA_LEN.try_into().unwrap())?;

            let create_ix = system_instruction::create_account(
                &payer.pubkey(),
                &price_feed.pubkey(),
                lamports,
                ACCOUNT_DATA_LEN,
                &PYTH_PROGRAM_ID,
            );

            let tx = Transaction::new_signed_with_payer(
                &[create_ix],
                Some(&payer.pubkey()),
                &[&payer, &price_feed],
                rpc_client.get_latest_blockhash()?,
            );

            rpc_client.send_and_confirm_transaction(&tx)?;

            let data = UpdatePriceData {
                instruction: PythInstruction::UpdatePrice as u32,
                price,
                confidence,
                expo,
                publish_time: Utc::now().timestamp(),
            };

            let ix = Instruction {
                program_id: PYTH_PROGRAM_ID,
                accounts: vec![
                    AccountMeta::new(price_feed.pubkey(), false),
                    AccountMeta::new_readonly(payer.pubkey(), true),
                ],
                data: to_vec(&data).unwrap(),
            };

            let tx = Transaction::new_signed_with_payer(
                &[ix],
                Some(&payer.pubkey()),
                &[&payer],
                rpc_client.get_latest_blockhash().unwrap(),
            );

            rpc_client.send_and_confirm_transaction(&tx).expect("Failed to send price update");

            println!("price_feed: {}", price_feed.pubkey());
            println!("Price: {}", price);
            println!("Confidence: {}", confidence);
        }
        CommandsName::CreatePool {
            config_index,
            price,
            mint0,
            mint1,
            price_feed,
            open_time,
        } => {
            let mut price = price;
            let mut mint0 = mint0;
            let mut mint1 = mint1;
            if mint0 > mint1 {
                std::mem::swap(&mut mint0, &mut mint1);
                price = 1.0 / price;
            }
            println!("mint0:{}, mint1:{}, price:{}", mint0, mint1, price);
            let load_pubkeys = vec![mint0, mint1];
            let rsps = rpc_client.get_multiple_accounts(&load_pubkeys)?;
            let mint0_owner = rsps[0].clone().unwrap().owner;
            let mint1_owner = rsps[1].clone().unwrap().owner;
            let mint0_account =
                spl_token::state::Mint::unpack(&rsps[0].as_ref().unwrap().data).unwrap();
            let mint1_account =
                spl_token::state::Mint::unpack(&rsps[1].as_ref().unwrap().data).unwrap();
            let sqrt_price_x64 =
                price_to_sqrt_price_x64(price, mint0_account.decimals, mint1_account.decimals);
            let (amm_config_key, __bump) = Pubkey::find_program_address(
                &[
                    raydium_amm_v3::states::AMM_CONFIG_SEED.as_bytes(),
                    &config_index.to_be_bytes(),
                ],
                &pool_config.raydium_v3_program,
            );
            let (create_pool_instr, pool_account_address) = create_pool_instr(
                &pool_config.clone(),
                amm_config_key,
                price_feed,
                mint0,
                mint1,
                mint0_owner,
                mint1_owner,
                sqrt_price_x64,
                open_time,
            )?;

            // send
            let signers = vec![&payer];
            let recent_hash = rpc_client.get_latest_blockhash()?;
            let txn = Transaction::new_signed_with_payer(
                &create_pool_instr,
                Some(&payer.pubkey()),
                &signers,
                recent_hash,
            );
            let signature = send_txn(&rpc_client, &txn, true)?;
            println!("pool_account_address: {}", pool_account_address);
            println!("signature: {}", signature);
        }
        CommandsName::OpenPosition {
            pool_address,
            amount_0,
            amount_1,
            with_metadata,
        } => {
            // load pool to get observation
            let pool: raydium_amm_v3::states::PoolState = program.account(pool_address)?;
            println!("amount_0:{}, amount_1:{}", amount_0, amount_1);
            // calc with slippage
            let amount_0_with_slippage =
                amount_with_slippage(amount_0 as u64, pool_config.slippage, true);
            let amount_1_with_slippage =
                amount_with_slippage(amount_1 as u64, pool_config.slippage, true);
            // calc with transfer_fee
            let transfer_fee = get_pool_mints_inverse_fee(
                &rpc_client,
                pool.token_mint_0,
                pool.token_mint_1,
                amount_0_with_slippage,
                amount_1_with_slippage,
            );
            println!(
                "transfer_fee_0:{}, transfer_fee_1:{}",
                transfer_fee.0.transfer_fee, transfer_fee.1.transfer_fee
            );
            let amount_0_max = (amount_0_with_slippage as u64)
                .checked_add(transfer_fee.0.transfer_fee)
                .unwrap();
            let amount_1_max = (amount_1_with_slippage as u64)
                .checked_add(transfer_fee.1.transfer_fee)
                .unwrap();

            // load position
            let position_nft_infos = get_all_nft_and_position_by_owner(
                &rpc_client,
                &payer.pubkey(),
                &pool_config.raydium_v3_program,
            );
            let positions: Vec<Pubkey> = position_nft_infos
                .iter()
                .map(|item| item.position)
                .collect();
            let rsps = rpc_client.get_multiple_accounts(&positions)?;
            let mut user_positions = Vec::new();
            for rsp in rsps {
                match rsp {
                    None => continue,
                    Some(rsp) => {
                        let position = deserialize_anchor_account::<
                            raydium_amm_v3::states::PersonalPositionState,
                        >(&rsp)?;
                        user_positions.push(position);
                    }
                }
            }

            /*let mut find_position = raydium_amm_v3::states::PersonalPositionState::default();
            for position in user_positions {
                if position.pool_id == pool_address {
                    find_position = position.clone();
                }
            }*/

            // personal position not exist
            // new nft mint
            let nft_mint = Keypair::new();
            let remaining_accounts = Vec::new();

            let mut instructions = Vec::new();
            let request_inits_instr = ComputeBudgetInstruction::set_compute_unit_limit(1400_000u32);
            instructions.push(request_inits_instr);

            let user_token_account_0 =
                spl_associated_token_account::get_associated_token_address_with_program_id(
                    &payer.pubkey(),
                    &pool.token_mint_0,
                    &transfer_fee.0.owner,
                );
            let user_token_account_1 =
                spl_associated_token_account::get_associated_token_address_with_program_id(
                    &payer.pubkey(),
                    &pool.token_mint_1,
                    &transfer_fee.1.owner,
                );

            let (open_position_instr, nft_mint_key, nft_ata_token_account) =
                open_position_with_token22_nft_instr(
                    &pool_config.clone(),
                    pool_address,
                    pool.token_vault_0,
                    pool.token_vault_1,
                    pool.token_mint_0,
                    pool.token_mint_1,
                    nft_mint.pubkey(),
                    payer.pubkey(),
                    user_token_account_0,
                    user_token_account_1,
                    remaining_accounts,
                    amount_0_max,
                    amount_1_max,
                    with_metadata,
                )?;
            instructions.extend(open_position_instr);
            // send
            let signers = vec![&payer, &nft_mint];
            let recent_hash = rpc_client.get_latest_blockhash()?;
            let txn = Transaction::new_signed_with_payer(
                &instructions,
                Some(&payer.pubkey()),
                &signers,
                recent_hash,
            );
            let signature = send_txn(&rpc_client, &txn, true)?;
            println!("nft_mint_key: {}", nft_mint_key);
            println!("nft_ata_token_account: {}", nft_ata_token_account);
            println!("signature: {}", signature);
        }
        CommandsName::Swap {
            pool_address,
            simulate,
        } => {
            let pool_state_account = rpc_client.get_account(&pool_address)?;
            let pool_state = deserialize_anchor_account::<raydium_amm_v3::states::PoolState>(
                &pool_state_account,
            )?;

            let token_queue_accounts = vec![pool_state.token_queue_0, pool_state.token_queue_1];
            let rsps = rpc_client.get_multiple_accounts(&token_queue_accounts)?;
            let [token_queue_0_account, token_queue_1_account] = array_ref![rsps, 0, 2];
            let token_queue_0 = deserialize_anchor_account::<raydium_amm_v3::states::PoolQueue>(
                &token_queue_0_account.as_ref().unwrap(),
            )?;
            let token_queue_1 = deserialize_anchor_account::<raydium_amm_v3::states::PoolQueue>(
                &token_queue_1_account.as_ref().unwrap(),
            )?;

            let mut remaining_accounts = Vec::new();
            let mut accounts = Vec::new();
            remaining_accounts.append(&mut accounts);
            let mut instructions = Vec::new();
            let request_inits_instr = ComputeBudgetInstruction::set_compute_unit_limit(1400_000u32);
            instructions.push(request_inits_instr);

            let swap_instr = swap_instr(
                &pool_config.clone(),
                pool_state.amm_config,
                pool_address,
                pool_state.protocol_position,
                pool_state.token_queue_0,
                pool_state.token_queue_1,
                token_queue_0.peek().expect("token_queue_0 head"),
                token_queue_1.peek().expect("token_queue_1 head"),
                pool_state.price_feed,
                remaining_accounts,
            )?;
            instructions.extend(swap_instr);
            // send
            let signers = vec![&payer];
            let recent_hash = rpc_client.get_latest_blockhash()?;
            let txn = Transaction::new_signed_with_payer(
                &instructions,
                Some(&payer.pubkey()),
                &signers,
                recent_hash,
            );
            if simulate {
                let ret =
                    simulate_transaction(&rpc_client, &txn, true, CommitmentConfig::confirmed())?;
                println!("{:#?}", ret);
            } else {
                let signature = send_txn(&rpc_client, &txn, true)?;
                println!("{}", signature);
            }
        }
        CommandsName::PPositionByOwner { user_wallet } => {
            // load position
            let position_nft_infos = get_all_nft_and_position_by_owner(
                &rpc_client,
                &user_wallet,
                &pool_config.raydium_v3_program,
            );
            let positions: Vec<Pubkey> = position_nft_infos
                .iter()
                .map(|item| item.position)
                .collect();
            let rsps = rpc_client.get_multiple_accounts(&positions)?;
            let mut user_positions = Vec::new();
            for rsp in rsps {
                match rsp {
                    None => continue,
                    Some(rsp) => {
                        let position = deserialize_anchor_account::<
                            raydium_amm_v3::states::PersonalPositionState,
                        >(&rsp)?;
                        let (personal_position_key, __bump) = Pubkey::find_program_address(
                            &[
                                raydium_amm_v3::states::POSITION_SEED.as_bytes(),
                                position.nft_mint.to_bytes().as_ref(),
                            ],
                            &program.id(),
                        );
                        println!(
                            "id:{}, amount_0:{}, amount_1:{}",
                            personal_position_key, position.amount_0, position.amount_1
                        );
                        user_positions.push(position);
                    }
                }
            }
        }
        CommandsName::CompareKey { key0, key1 } => {
            let mut token_mint_0 = key0;
            let mut token_mint_1 = key1;
            if token_mint_0 > token_mint_1 {
                std::mem::swap(&mut token_mint_0, &mut token_mint_1);
            }
            println!("mint0:{}, mint1:{}", token_mint_0, token_mint_1);
        }
        CommandsName::PMint { mint } => {
            let mint_data = &rpc_client.get_account_data(&mint)?;
            let mint_state = StateWithExtensions::<Mint>::unpack(mint_data)?;
            println!("mint_state:{:?}", mint_state);
            let extensions = get_account_extensions(&mint_state);
            println!("mint_extensions:{:#?}", extensions);
        }
        CommandsName::PToken { token } => {
            let token_data = &rpc_client.get_account_data(&token)?;
            let token_state = StateWithExtensions::<Account>::unpack(token_data)?;
            println!("token_state:{:?}", token_state);
            let extensions = get_account_extensions(&token_state);
            println!("token_extensions:{:#?}", extensions);
        }
        CommandsName::POperation => {
            let (operation_account_key, __bump) = Pubkey::find_program_address(
                &[raydium_amm_v3::states::OPERATION_SEED.as_bytes()],
                &program.id(),
            );
            println!("{}", operation_account_key);
            let operation_account: raydium_amm_v3::states::OperationState =
                program.account(operation_account_key)?;
            println!("{:#?}", operation_account);
        }
        CommandsName::PObservation => {
            let pool: raydium_amm_v3::states::PoolState =
                program.account(pool_config.pool_id_account.unwrap())?;
            println!("{}", pool.observation_key);
            let observation_account: raydium_amm_v3::states::ObservationState =
                program.account(pool.observation_key)?;
            println!("{:#?}", observation_account);
        }
        CommandsName::PConfig { config_index } => {
            let (amm_config_key, __bump) = Pubkey::find_program_address(
                &[
                    raydium_amm_v3::states::AMM_CONFIG_SEED.as_bytes(),
                    &config_index.to_be_bytes(),
                ],
                &program.id(),
            );
            println!("{}", amm_config_key);
            let amm_config_account: raydium_amm_v3::states::AmmConfig =
                program.account(amm_config_key)?;
            println!("{:#?}", amm_config_account);
        }
        CommandsName::PPersonalPositionByPool { pool_id } => {
            let pool_id = if let Some(pool_id) = pool_id {
                pool_id
            } else {
                pool_config.pool_id_account.unwrap()
            };
            println!("pool_id:{}", pool_id);
            let position_accounts_by_pool = rpc_client.get_program_accounts_with_config(
                &pool_config.raydium_v3_program,
                RpcProgramAccountsConfig {
                    filters: Some(vec![
                        RpcFilterType::Memcmp(Memcmp::new_base58_encoded(
                            8 + 1 + size_of::<Pubkey>(),
                            &pool_id.to_bytes(),
                        )),
                        RpcFilterType::DataSize(
                            raydium_amm_v3::states::PersonalPositionState::LEN as u64,
                        ),
                    ]),
                    account_config: RpcAccountInfoConfig {
                        encoding: Some(UiAccountEncoding::Base64),
                        ..RpcAccountInfoConfig::default()
                    },
                    with_context: Some(false),
                    sort_results: Some(false),
                },
            )?;

            for position in position_accounts_by_pool {
                let personal_position = deserialize_anchor_account::<
                    raydium_amm_v3::states::PersonalPositionState,
                >(&position.1)?;
                if personal_position.pool_id == pool_id {
                    println!(
                        "personal_position:{}, amount_0:{}, amount_1:{}",
                        position.0, personal_position.amount_0, personal_position.amount_1
                    );
                }
            }
        }
        CommandsName::PProtocolPositionByPool { pool_id } => {
            let pool_id = if let Some(pool_id) = pool_id {
                pool_id
            } else {
                pool_config.pool_id_account.unwrap()
            };
            println!("pool_id:{}", pool_id);
            let position_accounts_by_pool = rpc_client.get_program_accounts_with_config(
                &pool_config.raydium_v3_program,
                RpcProgramAccountsConfig {
                    filters: Some(vec![
                        RpcFilterType::Memcmp(Memcmp::new_base58_encoded(
                            8 + 1,
                            &pool_id.to_bytes(),
                        )),
                        RpcFilterType::DataSize(
                            raydium_amm_v3::states::ProtocolPositionState::LEN as u64,
                        ),
                    ]),
                    account_config: RpcAccountInfoConfig {
                        encoding: Some(UiAccountEncoding::Base64Zstd),
                        ..RpcAccountInfoConfig::default()
                    },
                    with_context: Some(false),
                    sort_results: Some(false),
                },
            )?;

            for position in position_accounts_by_pool {
                let protocol_position = deserialize_anchor_account::<
                    raydium_amm_v3::states::ProtocolPositionState,
                >(&position.1)?;
                let liquidity_0 = protocol_position.liquidity_0;
                let liquidity_1 = protocol_position.liquidity_1;
                if protocol_position.pool_id == pool_id {
                    println!(
                        "protocol_position:{}, liquidity_0:{}, liquidity_1: {}",
                        position.0, liquidity_0, liquidity_1
                    );
                }
            }
        }
        CommandsName::PPool { pool_id } => {
            let pool_id = if let Some(pool_id) = pool_id {
                pool_id
            } else {
                pool_config.pool_id_account.unwrap()
            };
            println!("pool_id:{}", pool_id);
            let pool_account: raydium_amm_v3::states::PoolState = program.account(pool_id)?;
            println!("{:#?}", pool_account);
        }
        CommandsName::PProtocol { protocol_id } => {
            let protocol_account: raydium_amm_v3::states::ProtocolPositionState =
                program.account(protocol_id)?;
            println!("{:#?}", protocol_account);
        }
        CommandsName::PPersonal { personal_id } => {
            let personal_account: raydium_amm_v3::states::PersonalPositionState =
                program.account(personal_id)?;
            println!("{:#?}", personal_account);
        }
        CommandsName::DecodeInstruction { instr_hex_data } => {
            handle_program_instruction(&instr_hex_data, InstructionDecodeType::BaseHex)?;
        }
        CommandsName::DecodeEvent { log_event } => {
            handle_program_log(
                &pool_config.raydium_v3_program.to_string(),
                &log_event,
                false,
            )?;
        }
        CommandsName::DecodeTxLog { tx_id } => {
            let signature = Signature::from_str(&tx_id)?;
            let tx = rpc_client.get_transaction_with_config(
                &signature,
                RpcTransactionConfig {
                    encoding: Some(UiTransactionEncoding::Json),
                    commitment: Some(CommitmentConfig::confirmed()),
                    max_supported_transaction_version: Some(0),
                },
            )?;
            let transaction = tx.transaction;
            // get meta
            let meta = if transaction.meta.is_some() {
                transaction.meta
            } else {
                None
            };
            // get encoded_transaction
            let encoded_transaction = transaction.transaction;
            // decode instruction data
            parse_program_instruction(
                &pool_config.raydium_v3_program.to_string(),
                encoded_transaction,
                meta.clone(),
            )?;
            // decode logs
            parse_program_event(&pool_config.raydium_v3_program.to_string(), meta.clone())?;
        }
    }

    Ok(())
}
