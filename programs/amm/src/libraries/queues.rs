use anchor_lang::prelude::*;

pub const MAX_ORDER_LIMIT: usize = 128;

#[repr(u8)]
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum QueueType {
    #[default]
    Fifo = 0,
}

impl From<u8> for QueueType {
    fn from(value: u8) -> Self {
        match value {
            0 | _=> QueueType::Fifo,
        }
    }
}

//TODO {3}: might remove this, as it will probably go unused
pub trait Queue<T> {
    fn push(&mut self, p: T) -> Result<()>;
    fn pop(&mut self) -> Option<T>;
    fn peek(&self) -> Option<T>;
    fn has_next(&self) -> bool;
    fn is_empty(&self) -> bool;
    fn is_full(&self) -> bool;
    fn len(&self) -> u8;
}