use laravel_facade::extends_facade;
use shaku::Interface;

#[extends_facade(Repo)]
pub trait Repository<T>: Interface {
    fn find(&self, id: u64) -> Option<T>;
}

fn main() {}
