use laravel_facade::extends_facade;
use shaku::Interface;

#[extends_facade(Counter)]
pub trait CounterStore: Interface {
    fn increment(&mut self);
}

fn main() {}
