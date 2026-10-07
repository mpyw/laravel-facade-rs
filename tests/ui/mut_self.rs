use laravel_facade::facade;
use shaku::Interface;

#[facade(Counter)]
pub trait CounterStore: Interface {
    fn increment(&mut self);
}

fn main() {}
