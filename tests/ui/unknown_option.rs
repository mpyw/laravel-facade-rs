use facade_rs::facade;
use shaku::Interface;

#[facade(Cache, cache = false)]
pub trait CacheStore: Interface {
    fn get(&self, key: &str) -> Option<String>;
}

fn main() {}
