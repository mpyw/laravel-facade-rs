use laravel_facade::facade;
use shaku::Interface;

#[facade(Cache)]
pub trait CacheStore: Interface {
    fn swap(&self);
}

fn main() {}
