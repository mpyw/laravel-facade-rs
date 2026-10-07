use async_trait::async_trait;
use laravel_facade::extends_facade;
use shaku::Interface;

#[async_trait]
#[extends_facade(Http)]
pub trait HttpClient: Interface {
    async fn get(&self, url: &str) -> String;
}

fn main() {}
