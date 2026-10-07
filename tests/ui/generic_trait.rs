use facade_rs::facade;
use shaku::Interface;

#[facade(Repo)]
pub trait Repository<T>: Interface {
    fn find(&self, id: u64) -> Option<T>;
}

fn main() {}
