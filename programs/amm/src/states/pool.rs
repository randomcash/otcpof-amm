use crate::states::*;
use crate::util::get_recent_epoch;
use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;
#[cfg(feature = "enable-log")]
use std::convert::identity;
use std::ops::{BitAnd, BitOr, BitXor};

/// Seed to derive account address and signature
pub const POOL_SEED: &str = "pool";
pub const POOL_VAULT_SEED: &str = "pool_vault";

pub enum PoolStatusBitIndex {
    OpenPositionOrIncreaseLiquidity,
    DecreaseLiquidity,
    CollectFee,
    CollectReward,
    Swap,
}

#[derive(PartialEq, Eq)]
pub enum PoolStatusBitFlag {
    Enable,
    Disable,
}

/// The pool state
///
/// PDA of `[POOL_SEED, config, token_mint_0, token_mint_1]`
///
#[account(zero_copy(unsafe))]
#[repr(C, packed)]
#[derive(Default, Debug)]
pub struct PoolState {
    /// Bump to identify PDA
    pub bump: [u8; 1],
    // Which config the pool belongs
    pub amm_config: Pubkey,
    // Pool creator
    pub owner: Pubkey,

    /// Protocol position
    pub protocol_position: Pubkey,

    /// Token pair of the pool, where token_mint_0 address < token_mint_1 address
    pub token_mint_0: Pubkey,
    pub token_mint_1: Pubkey,

    /// Token pair vault
    pub token_vault_0: Pubkey,
    pub token_vault_1: Pubkey,

    // Token pair queue
    pub token_queue_0: Pubkey,
    pub token_queue_1: Pubkey,

    /// observation account key
    pub observation_key: Pubkey,

    /// mint0 and mint1 decimals
    pub mint_decimals_0: u8,
    pub mint_decimals_1: u8,

    /// Bitwise representation of the state of the pool
    /// bit0, 1: disable open position and increase liquidity, 0: normal
    /// bit1, 1: disable decrease liquidity, 0: normal
    /// bit2, 1: disable collect fee, 0: normal
    /// bit3, 1: disable collect reward, 0: normal
    /// bit4, 1: disable swap, 0: normal
    pub status: u8,

    // The timestamp allowed for swap in the pool.
    // Note: The open_time is disabled for now.
    pub open_time: u64,
    // account recent update epoch
    pub recent_epoch: u64,
}

impl PoolState {
    // Zero copy: DISCRIMINATOR + Size
    pub const LEN: usize = Self::DISCRIMINATOR.len() + std::mem::size_of::<Self>();

    pub fn seeds(&self) -> [&[u8]; 5] {
        [
            &POOL_SEED.as_bytes(),
            self.amm_config.as_ref(),
            self.token_mint_0.as_ref(),
            self.token_mint_1.as_ref(),
            self.bump.as_ref(),
        ]
    }

    pub fn key(&self) -> Pubkey {
        Pubkey::create_program_address(&self.seeds(), &crate::id()).unwrap()
    }

    pub fn initialize(
        &mut self,
        bump: u8,
        open_time: u64,
        pool_creator: Pubkey,
        token_vault_0: Pubkey,
        token_vault_1: Pubkey,
        amm_config: &Account<AmmConfig>,
        protocol_position: &AccountInfo,
        token_mint_0: &InterfaceAccount<Mint>,
        token_mint_1: &InterfaceAccount<Mint>,
        observation_state_key: Pubkey,
    ) -> Result<()> {
        self.bump = [bump];
        self.amm_config = amm_config.key();
        self.owner = pool_creator.key();
        self.protocol_position = protocol_position.key();
        self.token_mint_0 = token_mint_0.key();
        self.token_mint_1 = token_mint_1.key();
        self.mint_decimals_0 = token_mint_0.decimals;
        self.mint_decimals_1 = token_mint_1.decimals;
        self.token_vault_0 = token_vault_0;
        self.token_vault_1 = token_vault_1;
        self.status = 0;
        self.open_time = open_time;
        self.recent_epoch = get_recent_epoch()?;
        self.observation_key = observation_state_key;

        Ok(())
    }

    pub fn set_status(&mut self, status: u8) {
        self.status = status
    }

    pub fn set_status_by_bit(&mut self, bit: PoolStatusBitIndex, flag: PoolStatusBitFlag) {
        let s = u8::from(1) << (bit as u8);
        if flag == PoolStatusBitFlag::Disable {
            self.status = self.status.bitor(s);
        } else {
            let m = u8::from(255).bitxor(s);
            self.status = self.status.bitand(m);
        }
    }

    /// Get status by bit, if it is `noraml` status, return true
    pub fn get_status_by_bit(&self, bit: PoolStatusBitIndex) -> bool {
        let status = u8::from(1) << (bit as u8);
        self.status.bitand(status) == 0
    }
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

/// Emitted by when a swap is performed for a pool
#[event]
#[cfg_attr(feature = "client", derive(Debug))]
pub struct SwapEvent {
    /// The pool for which token_0 and token_1 were swapped
    pub pool_state: Pubkey,

    /// The address that initiated the swap call, and that received the callback
    pub sender: Pubkey,

    /// The payer token account in zero for one swaps, or the recipient token account
    /// in one for zero swaps
    pub token_account_0: Pubkey,

    /// The payer token account in one for zero swaps, or the recipient token account
    /// in zero for one swaps
    pub token_account_1: Pubkey,

    /// The real delta amount of the token_0 of the pool or user
    pub amount_0: u64,

    /// The transfer fee charged by the withheld_amount of the token_0
    pub transfer_fee_0: u64,

    /// The real delta of the token_1 of the pool or user
    pub amount_1: u64,

    /// The transfer fee charged by the withheld_amount of the token_1
    pub transfer_fee_1: u64,

    /// The sqrt(price) of the pool after the swap, as a Q64.64
    pub sqrt_price_x64: u128,

    /// The liquidity of token_0 the pool after the swap
    pub pool_liquidity_0: u128,

    /// The liquidity of token_1 the pool after the swap
    pub pool_liquidity_1: u128,
}

/// Emitted pool liquidity change when increase and decrease liquidity
#[event]
#[cfg_attr(feature = "client", derive(Debug))]
pub struct LiquidityChangeEvent {
    /// The pool for swap
    pub pool_state: Pubkey,

    /// The liquidity of the pool before liquidity change
    pub liquidity_before_0: u128,
    /// The liquidity of the pool before liquidity change
    pub liquidity_before_1: u128,

    /// The liquidity of the pool after liquidity change
    pub liquidity_after_0: u128,
    /// The liquidity of the pool after liquidity change
    pub liquidity_after_1: u128,
}

#[cfg(test)]
pub mod pool_test {
    use super::*;
    use std::cell::RefCell;

    pub fn build_pool() -> RefCell<PoolState> {
        let mut new_pool = PoolState::default();
        new_pool.token_mint_0 = Pubkey::new_unique();
        new_pool.token_mint_1 = Pubkey::new_unique();
        new_pool.amm_config = Pubkey::new_unique();
        // let mut random = rand::random<u128>();
        new_pool.bump = [Pubkey::find_program_address(
            &[
                &POOL_SEED.as_bytes(),
                new_pool.amm_config.as_ref(),
                new_pool.token_mint_0.as_ref(),
                new_pool.token_mint_1.as_ref(),
            ],
            &crate::id(),
        )
        .1];
        RefCell::new(new_pool)
    }

    mod pool_status_test {
        use super::*;

        #[test]
        fn get_set_status_by_bit() {
            let mut pool_state = PoolState::default();
            pool_state.set_status(17); // 00010001
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::Swap),
                false
            );
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::OpenPositionOrIncreaseLiquidity),
                false
            );
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::DecreaseLiquidity),
                true
            );
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::CollectFee),
                true
            );
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::CollectReward),
                true
            );

            // disable -> disable, nothing to change
            pool_state.set_status_by_bit(PoolStatusBitIndex::Swap, PoolStatusBitFlag::Disable);
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::Swap),
                false
            );

            // disable -> enable
            pool_state.set_status_by_bit(PoolStatusBitIndex::Swap, PoolStatusBitFlag::Enable);
            assert_eq!(pool_state.get_status_by_bit(PoolStatusBitIndex::Swap), true);

            // enable -> enable, nothing to change
            pool_state.set_status_by_bit(
                PoolStatusBitIndex::DecreaseLiquidity,
                PoolStatusBitFlag::Enable,
            );
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::DecreaseLiquidity),
                true
            );
            // enable -> disable
            pool_state.set_status_by_bit(
                PoolStatusBitIndex::DecreaseLiquidity,
                PoolStatusBitFlag::Disable,
            );
            assert_eq!(
                pool_state.get_status_by_bit(PoolStatusBitIndex::DecreaseLiquidity),
                false
            );
        }
    }

    mod pool_layout_test {
        use super::*;
        use anchor_lang::Discriminator;
        #[test]
        fn test_pool_layout() {
            let bump: u8 = 0x12;
            let amm_config = Pubkey::new_unique();
            let owner = Pubkey::new_unique();
            let token_mint_0 = Pubkey::new_unique();
            let token_mint_1 = Pubkey::new_unique();
            let token_vault_0 = Pubkey::new_unique();
            let token_vault_1 = Pubkey::new_unique();
            let token_queue_0 = Pubkey::new_unique();
            let token_queue_1 = Pubkey::new_unique();
            let observation_key = Pubkey::new_unique();
            let mint_decimals_0: u8 = 0x13;
            let mint_decimals_1: u8 = 0x14;
            let liquidity: u128 = 0x11002233445566778899aabbccddeeff;
            let sqrt_price_x64: u128 = 0x11220033445566778899aabbccddeeff;
            let padding3: u16 = 0x1718;
            let padding4: u16 = 0x191a;
            let fee_growth_global_0_x64: u128 = 0x11223300445566778899aabbccddeeff;
            let fee_growth_global_1_x64: u128 = 0x11223344005566778899aabbccddeeff;
            let protocol_fees_token_0: u64 = 0x123456789abcdef0;
            let protocol_fees_token_1: u64 = 0x123456789abcde0f;
            let swap_in_amount_token_0: u128 = 0x11223344550066778899aabbccddeeff;
            let swap_out_amount_token_1: u128 = 0x11223344556600778899aabbccddeeff;
            let swap_in_amount_token_1: u128 = 0x11223344556677008899aabbccddeeff;
            let swap_out_amount_token_0: u128 = 0x11223344556677880099aabbccddeeff;
            let status: u8 = 0x1b;
            let padding: [u8; 7] = [0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18];

            let total_fees_token_0: u64 = 0x1234567809abcdef;
            let total_fees_token_1: u64 = 0x1234567089abcdef;
            let total_fees_claimed_token_0: u64 = 0x1234560789abcdef;
            let total_fees_claimed_token_1: u64 = 0x1234506789abcdef;
            let fund_fees_token_0: u64 = 0x1234056789abcdef;
            let fund_fees_token_1: u64 = 0x1230456789abcdef;
            let pool_open_time: u64 = 0x1203456789abcdef;
            let recent_epoch: u64 = 0x1023456789abcdef;
            let mut padding1: [u64; 24] = [0u64; 24];
            let mut padding1_data = [0u8; 8 * 24];
            let mut offset = 0;
            for i in 0..24 {
                padding1[i] = u64::MAX - i as u64;
                padding1_data[offset..offset + 8].copy_from_slice(&padding1[i].to_le_bytes());
                offset += 8;
            }
            let mut padding2: [u64; 32] = [0u64; 32];
            let mut padding2_data = [0u8; 8 * 32];
            let mut offset = 0;
            for i in 24..(24 + 32) {
                padding2[i - 24] = u64::MAX - i as u64;
                padding2_data[offset..offset + 8].copy_from_slice(&padding2[i - 24].to_le_bytes());
                offset += 8;
            }
            // serialize original data
            let mut pool_data = [0u8; PoolState::LEN];
            let mut offset = 0;
            pool_data[offset..offset + 8].copy_from_slice(&PoolState::DISCRIMINATOR);
            offset += 8;
            pool_data[offset..offset + 1].copy_from_slice(&bump.to_le_bytes());
            offset += 1;
            pool_data[offset..offset + 32].copy_from_slice(&amm_config.to_bytes());
            offset += 32;
            pool_data[offset..offset + 32].copy_from_slice(&owner.to_bytes());
            offset += 32;
            pool_data[offset..offset + 32].copy_from_slice(&token_mint_0.to_bytes());
            offset += 32;
            pool_data[offset..offset + 32].copy_from_slice(&token_mint_1.to_bytes());
            offset += 32;
            pool_data[offset..offset + 32].copy_from_slice(&token_vault_0.to_bytes());
            offset += 32;
            pool_data[offset..offset + 32].copy_from_slice(&token_vault_1.to_bytes());
            offset += 32;
            pool_data[offset..offset + 32].copy_from_slice(&token_queue_0.to_bytes());
            offset += 32;
            pool_data[offset..offset + 32].copy_from_slice(&token_queue_1.to_bytes());
            offset += 32;
            pool_data[offset..offset + 32].copy_from_slice(&observation_key.to_bytes());
            offset += 32;
            pool_data[offset..offset + 1].copy_from_slice(&mint_decimals_0.to_le_bytes());
            offset += 1;
            pool_data[offset..offset + 1].copy_from_slice(&mint_decimals_1.to_le_bytes());
            offset += 1;
            pool_data[offset..offset + 16].copy_from_slice(&liquidity.to_le_bytes());
            offset += 16;
            pool_data[offset..offset + 16].copy_from_slice(&sqrt_price_x64.to_le_bytes());
            offset += 16;
            pool_data[offset..offset + 2].copy_from_slice(&padding3.to_le_bytes());
            offset += 2;
            pool_data[offset..offset + 2].copy_from_slice(&padding4.to_le_bytes());
            offset += 2;
            pool_data[offset..offset + 16].copy_from_slice(&fee_growth_global_0_x64.to_le_bytes());
            offset += 16;
            pool_data[offset..offset + 16].copy_from_slice(&fee_growth_global_1_x64.to_le_bytes());
            offset += 16;
            pool_data[offset..offset + 8].copy_from_slice(&protocol_fees_token_0.to_le_bytes());
            offset += 8;
            pool_data[offset..offset + 8].copy_from_slice(&protocol_fees_token_1.to_le_bytes());
            offset += 8;
            pool_data[offset..offset + 16].copy_from_slice(&swap_in_amount_token_0.to_le_bytes());
            offset += 16;
            pool_data[offset..offset + 16].copy_from_slice(&swap_out_amount_token_1.to_le_bytes());
            offset += 16;
            pool_data[offset..offset + 16].copy_from_slice(&swap_in_amount_token_1.to_le_bytes());
            offset += 16;
            pool_data[offset..offset + 16].copy_from_slice(&swap_out_amount_token_0.to_le_bytes());
            offset += 16;
            pool_data[offset..offset + 1].copy_from_slice(&status.to_le_bytes());
            offset += 1;
            pool_data[offset..offset + 7].copy_from_slice(&padding);
            offset += 7;
            pool_data[offset..offset + 8].copy_from_slice(&total_fees_token_0.to_le_bytes());
            offset += 8;
            pool_data[offset..offset + 8]
                .copy_from_slice(&total_fees_claimed_token_0.to_le_bytes());
            offset += 8;
            pool_data[offset..offset + 8].copy_from_slice(&total_fees_token_1.to_le_bytes());
            offset += 8;
            pool_data[offset..offset + 8]
                .copy_from_slice(&total_fees_claimed_token_1.to_le_bytes());
            offset += 8;
            pool_data[offset..offset + 8].copy_from_slice(&fund_fees_token_0.to_le_bytes());
            offset += 8;
            pool_data[offset..offset + 8].copy_from_slice(&fund_fees_token_1.to_le_bytes());
            offset += 8;
            pool_data[offset..offset + 8].copy_from_slice(&pool_open_time.to_le_bytes());
            offset += 8;
            pool_data[offset..offset + 8].copy_from_slice(&recent_epoch.to_le_bytes());
            offset += 8;
            pool_data[offset..offset + 8 * 24].copy_from_slice(&padding1_data);
            offset += 8 * 24;
            pool_data[offset..offset + 8 * 32].copy_from_slice(&padding2_data);
            offset += 8 * 32;

            // len check
            assert_eq!(offset, pool_data.len());
            assert_eq!(pool_data.len(), core::mem::size_of::<PoolState>() + 8);

            // deserialize original data
            let unpack_data: &PoolState =
                bytemuck::from_bytes(&pool_data[8..core::mem::size_of::<PoolState>() + 8]);

            // data check
            let unpack_bump = unpack_data.bump[0];
            assert_eq!(unpack_bump, bump);
            let unpack_amm_config = unpack_data.amm_config;
            assert_eq!(unpack_amm_config, amm_config);
            let unpack_owner = unpack_data.owner;
            assert_eq!(unpack_owner, owner);
            let unpack_token_mint_0 = unpack_data.token_mint_0;
            assert_eq!(unpack_token_mint_0, token_mint_0);
            let unpack_token_mint_1 = unpack_data.token_mint_1;
            assert_eq!(unpack_token_mint_1, token_mint_1);
            let unpack_token_vault_0 = unpack_data.token_vault_0;
            assert_eq!(unpack_token_vault_0, token_vault_0);
            let unpack_token_vault_1 = unpack_data.token_vault_1;
            assert_eq!(unpack_token_vault_1, token_vault_1);
            let unpack_token_vault_0 = unpack_data.token_queue_0;
            assert_eq!(unpack_token_vault_0, token_queue_0);
            let unpack_token_vault_1 = unpack_data.token_queue_1;
            assert_eq!(unpack_token_vault_1, token_queue_1);
            let unpack_observation_key = unpack_data.observation_key;
            assert_eq!(unpack_observation_key, observation_key);
            let unpack_mint_decimals_0 = unpack_data.mint_decimals_0;
            assert_eq!(unpack_mint_decimals_0, mint_decimals_0);
            let unpack_mint_decimals_1 = unpack_data.mint_decimals_1;
            assert_eq!(unpack_mint_decimals_1, mint_decimals_1);
            let unpack_status = unpack_data.status;
            assert_eq!(unpack_status, status);

            let unpack_open_time = unpack_data.open_time;
            assert_eq!(unpack_open_time, pool_open_time);
            let unpack_recent_epoch = unpack_data.recent_epoch;
            assert_eq!(unpack_recent_epoch, recent_epoch);
        }
    }
}
