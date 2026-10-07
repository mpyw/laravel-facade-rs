use laravel_facade::extends_facade;
use shaku::Interface;

#[extends_facade(Cache)]
pub trait CacheStore: Interface {
    fn swap(&self);
}

fn main() {}
