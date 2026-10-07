use laravel_facade::extends_facade;
use shaku::Interface;

#[extends_facade(Cache, cache = false)]
pub trait CacheStore: Interface {
    fn get(&self, key: &str) -> Option<String>;
}

fn main() {}
