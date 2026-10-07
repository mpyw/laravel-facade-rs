use facade_rs::facade;
use shaku::Interface;

#[facade(Cache)]
pub trait CacheStore: Interface {
    fn swap(&self);
}

fn main() {}
