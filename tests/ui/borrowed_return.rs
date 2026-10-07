use laravel_facade::facade;
use shaku::Interface;

#[facade(Config)]
pub trait ConfigRepository: Interface {
    fn name(&self) -> &str;
}

fn main() {}
