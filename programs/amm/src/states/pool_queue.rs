use anchor_lang::prelude::*;

use crate::libraries::{Queue, QueueType};
use crate::error::ErrorCode;

pub const MAX_ORDER_LIMIT: usize = 128;

pub const POOL_QUEUE_SEED: &[u8] = b"pool_queue";
pub const POOL_QUEUE_SEED_SIDE_0: &[u8] = &[0b0];
pub const POOL_QUEUE_SEED_SIDE_1: &[u8] = &[0b1];

#[account(zero_copy(unsafe))]
#[repr(C, packed)]
pub struct PoolQueue {
    pub bump: [u8; 1],
    /// The ID of the pool with which this token is connected
    pub pool_id: Pubkey,

    /// The logic implemented behing this queue
    pub queue_type: QueueType,

    /// Buffer containing elements from queue
    pub buffer: [Option<Pubkey>; MAX_ORDER_LIMIT],
    
    /// Points to the next pop, max size is MAX_ORDER_LIMIT
    pub head: u8, 
    /// Points to the next push, max size is MAX_ORDER_LIMIT
    pub tail: u8, 
    /// Number of items in the queue, max size is MAX_ORDER_LIMIT
    pub len: u8, 
}

impl PoolQueue {
    pub const LEN: usize = Self::DISCRIMINATOR.len() + std::mem::size_of::<Self>();

    pub fn seeds(&self, side: u8) -> [&[u8]; 3] {
        let side_seed = if side == 0 {
            POOL_QUEUE_SEED_SIDE_0
        } else {
            POOL_QUEUE_SEED_SIDE_1
        };
        [
            &POOL_QUEUE_SEED,
            side_seed,
            self.pool_id.as_ref(),
        ]
    }

    pub fn initialize(&mut self, bump: u8, pool_id: Pubkey, queue_type: QueueType) -> Result<()> {
        self.bump = [bump];
        self.pool_id = pool_id;
        self.queue_type = queue_type;
        self.buffer = [None; MAX_ORDER_LIMIT];
        self.head = 0;
        self.tail = 0;
        self.len = 0;
        Ok(())
    }
}

impl Default for PoolQueue {
    fn default() -> Self {
        Self {
            buffer: [None; MAX_ORDER_LIMIT],
            head: 0,
            tail: 0,
            len: 0,
            pool_id: Pubkey::default(),
            queue_type: QueueType::Fifo,
            bump: [0],
        }
    }
}

impl Queue<Pubkey> for PoolQueue {
    fn push(&mut self, p: Pubkey) -> Result<()> {
        match self.queue_type {
            QueueType::Fifo => {
                if self.len as usize == MAX_ORDER_LIMIT {
                    return Err(error!(ErrorCode::QueueFull));
                }
        
                self.buffer[self.tail as usize] = Some(p);
                self.tail = (self.tail + 1) % MAX_ORDER_LIMIT as u8;
                self.len += 1;
            },
        }
        Ok(())
    }

    fn pop(&mut self) -> Option<Pubkey> {
        if self.len == 0 {
            return None;
        }

        let personal_position = match self.queue_type {
            QueueType::Fifo => {
                let personal_position = self.buffer[self.head as usize];
                self.buffer[self.head as usize] = None;
                self.head = (self.head + 1) % MAX_ORDER_LIMIT as u8;
                self.len -= 1;
                personal_position
            },
        };

        personal_position
    }

    fn has_next(&self) -> bool {
        let p =  self.buffer[self.head as usize];
        p.is_some()
    }

    fn peek(&self) -> Option<Pubkey> {
        let p =  self.buffer[self.head as usize];
        p
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn is_full(&self) -> bool {
        self.len == MAX_ORDER_LIMIT as u8
    }

    fn len(&self) -> u8 {
        self.len
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::solana_program::pubkey::Pubkey;

    mod queue_logic_tests {
        use super::*;
        #[test]
        fn test_push_and_pop_single() {
            let mut queue = PoolQueue::default();
            let order = Pubkey::new_unique();
    
            assert!(queue.push(order.into()).is_ok());
            assert_eq!(queue.len(), 1);
            queue.pop().unwrap();
            assert!(queue.is_empty());
        }
    
        #[test]
        fn test_fifo_order() {
            let mut queue = PoolQueue::default();
    
            for _ in 0..5 {
                queue.push(Pubkey::new_unique().into()).unwrap();
            }
    
            for _ in 0..5 {
                queue.pop().unwrap();
            }
    
            assert!(queue.is_empty());
        }
    
        #[test]
        fn test_full_queue() {
            let mut queue = PoolQueue::default();
    
            for _ in 0..MAX_ORDER_LIMIT {
                assert!(queue.push(Pubkey::new_unique().into()).is_ok());
            }
    
            assert_eq!(queue.len(), MAX_ORDER_LIMIT as u8);
            assert!(queue.is_full());
            assert!(queue.push(Pubkey::new_unique().into()).is_err());
        }
    
        #[test]
        fn test_empty_queue_pop_fails() {
            let mut queue = PoolQueue::default();
            assert!(!queue.has_next());
            assert!(queue.pop().is_none());
        }
    
        #[test]
        fn test_wrap_around_behavior() {
            let mut queue = PoolQueue::default();
    
            for _ in 0..MAX_ORDER_LIMIT {
                let order = Pubkey::new_unique();
                queue.push(order.into()).unwrap();
            }
    
            for _ in 0..MAX_ORDER_LIMIT {
                let _ = queue.has_next();
                let _ = queue.pop().unwrap();
            }
    
            for _ in 0..MAX_ORDER_LIMIT {
                queue.push(Pubkey::new_unique().into()).unwrap();
            }
    
            for _ in 0..MAX_ORDER_LIMIT {
                queue.pop().unwrap();
            }
    
            assert!(queue.is_empty());
        }
    
        #[test]
        fn test_len_and_flags() {
            let mut queue = PoolQueue::default();
            assert!(queue.is_empty());
            assert!(queue.is_full() == false);
    
            for _ in 0..MAX_ORDER_LIMIT {
                queue.push(Pubkey::new_unique().into()).unwrap();
            }
    
            assert_eq!(queue.len(), MAX_ORDER_LIMIT as u8);
            assert!(queue.is_full());
    
            assert!(queue.push(Pubkey::new_unique().into()).is_err());
    
            queue.pop().unwrap();
            assert!(!queue.is_full());
            assert_eq!(queue.len(), MAX_ORDER_LIMIT as u8 - 1);
        }
    }
}
