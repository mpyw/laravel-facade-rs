use facade_rs::facade;
use shaku::Interface;

#[facade(Counter)]
pub trait CounterStore: Interface {
    fn increment(&mut self);
}

fn main() {}
