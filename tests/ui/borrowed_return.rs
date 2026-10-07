use laravel_facade::extends_facade;
use shaku::Interface;

#[extends_facade(Config)]
pub trait ConfigRepository: Interface {
    fn name(&self) -> &str;
}

fn main() {}
