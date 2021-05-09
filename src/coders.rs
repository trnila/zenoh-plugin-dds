use async_trait::async_trait;
use zenoh::net::{RBuf};
use std::future::Future;
use std::str;



#[async_trait]
pub trait Coder {
    async fn encode(&self, data: Vec<u8>) -> Vec<u8>;
    fn decode(&self, data: Vec<u8>) -> Vec<u8>;
}

pub fn new_encoder(topic_type: &str) -> Box<dyn Coder> {
    match topic_type {
        "std_msgs::msg::dds_::String_" => Box::new(UpperCoder{n: 10}),
        _ => Box::new(IdentityCoder{n: 10}),
    }
}

struct IdentityCoder {
    n: u32,
}

#[async_trait]
impl Coder for IdentityCoder {
    async fn encode(&self, data: Vec<u8>) -> Vec<u8> {
        data
    }

    fn decode(&self, data: Vec<u8>) -> Vec<u8> {
        data
    }
}

struct UpperCoder {
    n: u32,
}

#[async_trait]
impl Coder for UpperCoder {
    async fn encode(&self, data: Vec<u8>) -> Vec<u8> {
        str::from_utf8(&data).unwrap().to_uppercase().as_bytes().to_vec()
    }

    fn decode(&self, data: Vec<u8>) -> Vec<u8> {
        str::from_utf8(&data).unwrap().to_lowercase().as_bytes().to_vec()
    }
}

