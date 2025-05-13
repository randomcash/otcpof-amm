use anchor_lang::prelude::*;

use crate::libraries::{FifoQueue, Queue, ZCPubkey};

pub const POOL_FIFO_QUEUE_SEED: &[u8] = b"pool_fifo_queue";
pub const POOL_FIFO_QUEUE_SEED_SIDE_0: &[u8] = &[0b0];
pub const POOL_FIFO_QUEUE_SEED_SIDE_1: &[u8] = &[0b1];

#[account(zero_copy(unsafe))]
#[repr(C, packed)]
#[derive(Default)]
pub struct PoolFifoQueue {
    /// Bump to identify PDA
    pub bump: [u8; 1],

    /// The ID of the pool with which this token is connected
    pub pool_id: Pubkey,

    pub position_queue: FifoQueue<ZCPubkey>,
}

impl PoolFifoQueue {
    pub const LEN: usize = Self::DISCRIMINATOR.len() + std::mem::size_of::<Self>();

    pub fn seeds(&self) -> [&[u8]; 3] {
        [
            &POOL_FIFO_QUEUE_SEED,
            self.pool_id.as_ref(),
            self.bump.as_ref(),
        ]
    }

    pub fn initialize(&mut self, bump: u8) -> Result<()> {
        self.bump = [bump];
        Ok(())
    }

    pub fn push_order(&mut self, new_order: ZCPubkey) -> Result<()> {
        self.position_queue.push(new_order)
    }
}

#[cfg(test)]
pub mod pool_fifo_queue_test {
    use super::*;

    #[test]
    fn test_initialize() {
        let mut queue = PoolFifoQueue::default();
        let pool_id = Pubkey::new_unique();
        queue.pool_id = pool_id;

        let bump = 254;
        queue.initialize(bump).unwrap();

        assert_eq!(queue.bump[0], bump);
        assert_eq!(queue.pool_id, pool_id);
        assert!(queue.position_queue.is_empty());
    }

    #[test]
    fn test_push_order() {
        let mut queue = PoolFifoQueue::default();
        queue.initialize(1).unwrap();

        let order = Pubkey::new_unique();
        assert!(queue.position_queue.is_empty());

        queue.push_order(order.into()).unwrap();
        assert_eq!(queue.position_queue.len(), 1);
        assert_eq!(queue.position_queue.peek().unwrap().key(), order.key());
    }

    #[test]
    fn test_fifo_behavior() {
        let mut queue = PoolFifoQueue::default();
        queue.initialize(1).unwrap();

        let order1 = ZCPubkey::new_unique();
        let order2 = ZCPubkey::new_unique();

        queue.push_order(order1).unwrap();
        queue.push_order(order2).unwrap();

        let popped1 = queue.position_queue.pop().unwrap();
        let popped2 = queue.position_queue.pop().unwrap();

        assert_eq!(popped1.key(), order1.key());
        assert_eq!(popped2.key(), order2.key());
        assert!(queue.position_queue.is_empty());
    }

    #[test]
    fn test_seeds() {
        let mut queue = PoolFifoQueue::default();
        let pool_id = Pubkey::new_unique();
        queue.pool_id = pool_id;
        queue.initialize(7).unwrap();

        let seeds = queue.seeds();
        assert_eq!(seeds[0], POOL_FIFO_QUEUE_SEED);
        assert_eq!(seeds[1], pool_id.as_ref());
        assert_eq!(seeds[2], &[7u8]);
    }

    #[test]
    fn test_pool_fifo_queue_layout() {
        // Constants
        const BUFFER_CAPACITY: usize = 128;
        const ZCPUBKEY_SIZE: usize = 32;
        const ENTRY_SIZE: usize = 1 + 32 + (BUFFER_CAPACITY * (1 + ZCPUBKEY_SIZE)) + 3;

        #[repr(C)]
        #[derive(Default, Copy, Clone)]
        pub struct ZCPubkey {
            pub key: [u8; 32],
        }

        let bump = [0x42];
        let pool_id = Pubkey::new_unique();

        let head: u8 = 3;
        let tail: u8 = 7;
        let len: u8 = 4;

        // Build the buffer
        let mut buffer = [[0u8; 33]; BUFFER_CAPACITY]; // 1 byte tag + 32 bytes key
        for i in 0..len as usize {
            buffer[i][0] = 1; // Tag for Some
            buffer[i][1..].copy_from_slice(&[i as u8; 32]);
        }

        // Prepare bytes
        let mut bytes = [0u8; ENTRY_SIZE];
        let mut offset = 0;

        // Bump
        bytes[offset..offset + 1].copy_from_slice(&bump);
        offset += 1;

        // Pool ID
        bytes[offset..offset + 32].copy_from_slice(pool_id.as_ref());
        offset += 32;

        // Buffer entries
        for i in 0..BUFFER_CAPACITY {
            bytes[offset..offset + 33].copy_from_slice(&buffer[i]);
            offset += 33;
        }

        // Head, Tail, Len
        bytes[offset] = head;
        offset += 1;
        bytes[offset] = tail;
        offset += 1;
        bytes[offset] = len;
        offset += 1;

        assert_eq!(offset, bytes.len());

        // SAFETY: Must match #[repr(C, packed)] layout exactly
        let pool_fifo_queue: &PoolFifoQueue =
            unsafe { &*(&bytes as *const _ as *const PoolFifoQueue) };

        assert_eq!(pool_fifo_queue.bump, bump);
        assert_eq!(pool_fifo_queue.pool_id, pool_id);
        assert_eq!(pool_fifo_queue.position_queue.test_head(), head);
        assert_eq!(pool_fifo_queue.position_queue.test_tail(), tail);
        assert_eq!(pool_fifo_queue.position_queue.test_len(), len);

        for i in 0..len as usize {
            let item = pool_fifo_queue.position_queue.test_buffer()[i];
            assert!(item.is_some());
            assert_eq!(item.unwrap().key(), [i as u8; 32].into());
        }

        for i in len as usize..BUFFER_CAPACITY {
            assert!(pool_fifo_queue.position_queue.test_buffer()[i].is_none());
        }
    }
}
