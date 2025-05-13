use anchor_lang::{prelude::*, ZeroCopy};
use crate::error::ErrorCode;

pub const MAX_ORDER_LIMIT: usize = 128;

pub trait Queue<T> {
    fn push(&mut self, p: T) -> Result<()>;
    fn pop(&mut self) -> Option<T>;
    fn peek(&self) -> Option<T>;
    fn has_next(&self) -> bool;
    fn is_empty(&self) -> bool;
    fn is_full(&self) -> bool;
    fn len(&self) -> u8;
}

#[account(zero_copy(unsafe))]
#[repr(C, packed)]
pub struct FifoQueue<T> where T: ZeroCopy {
    buffer: [Option<T>; MAX_ORDER_LIMIT],
    head: u8, // Points to the next pop, max size is MAX_ORDER_LIMIT 128
    tail: u8, // Points to the next push, max size is MAX_ORDER_LIMIT 128
    len: u8,  // Number of items in the queue, max size is MAX_ORDER_LIMIT 128
}

impl<T> Default for FifoQueue<T> where T: ZeroCopy {
    fn default() -> Self {
        Self {
            buffer: [None; MAX_ORDER_LIMIT],
            head: 0,
            tail: 0,
            len: 0,
        }
    }
}

impl<T> FifoQueue<T> where T: ZeroCopy {
    pub fn new() -> Self {
        Self {
            buffer: [Default::default(); MAX_ORDER_LIMIT],
            head: 0,
            tail: 0,
            len: 0,
        }
    }

    //Methods for testing only
    #[cfg(test)]
    pub fn test_head(&self) -> u8 {
        self.head
    }
    #[cfg(test)]
    pub fn test_tail(&self) -> u8 {
        self.tail
    }
    #[cfg(test)]
    pub fn test_len(&self) -> u8 {
        self.len
    }
    #[cfg(test)]
    pub fn test_buffer(&self) -> [Option<T>; MAX_ORDER_LIMIT] {
        self.buffer
    }
}

impl<T> Queue<T> for FifoQueue<T> where T: ZeroCopy {
    fn push(&mut self, p: T) -> Result<()> {
        if self.len as usize == MAX_ORDER_LIMIT {
            return Err(error!(ErrorCode::QueueFull));
        }

        self.buffer[self.tail as usize] = Some(p);
        self.tail = (self.tail + 1) % MAX_ORDER_LIMIT as u8;
        self.len += 1;
        Ok(())
    }

    fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }

        let personal_position = self.buffer[self.head as usize];
        self.buffer[self.head as usize] = None;
        self.head = (self.head + 1) % MAX_ORDER_LIMIT as u8;
        self.len -= 1;
        personal_position
    }

    fn has_next(&self) -> bool {
        let p =  self.buffer[self.head as usize];
        p.is_some()
    }

    fn peek(&self) -> Option<T> {
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
    use crate::libraries::ZCPubkey;

    use super::*;
    use anchor_lang::solana_program::pubkey::Pubkey;

    fn create_queue() -> FifoQueue<ZCPubkey> {
        FifoQueue::new()
    }

    #[test]
    fn test_push_and_pop_single() {
        let mut queue = create_queue();
        let order = Pubkey::new_unique();

        assert!(queue.push(order.into()).is_ok());
        assert_eq!(queue.len(), 1);
        queue.pop().unwrap();
        assert!(queue.is_empty());
    }

    #[test]
    fn test_fifo_order() {
        let mut queue = create_queue();

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
        let mut queue = create_queue();

        for _ in 0..MAX_ORDER_LIMIT {
            assert!(queue.push(Pubkey::new_unique().into()).is_ok());
        }

        assert_eq!(queue.len(), MAX_ORDER_LIMIT as u8);
        assert!(queue.is_full());
        assert!(queue.push(Pubkey::new_unique().into()).is_err());
    }

    #[test]
    fn test_empty_queue_pop_fails() {
        let mut queue = create_queue();
        assert!(!queue.has_next());
        assert!(queue.pop().is_none());
    }

    #[test]
    fn test_wrap_around_behavior() {
        let mut queue = create_queue();

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
        let mut queue = create_queue();
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
