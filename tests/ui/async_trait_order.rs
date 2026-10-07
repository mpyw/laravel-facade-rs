use async_trait::async_trait;
use facade_rs::facade;
use shaku::Interface;

#[async_trait]
#[facade(Http)]
pub trait HttpClient: Interface {
    async fn get(&self, url: &str) -> String;
}

fn main() {}
