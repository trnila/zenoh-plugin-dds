extern crate yaml_rust;

use zenoh::net::{RBuf, ResKey, Session};
use std::str;
use crate::gst_coder::GstCoder;
use async_std::task;
use std::sync::Arc;
use std::ffi::CString;
use cyclors::*;
use yaml_rust::YamlLoader;
use std::fs::File;
use std::io::prelude::*;


pub trait Writer {
    fn write(&self, buf: &[u8]);
}

pub struct ZenohWriter {
    key: ResKey,
    session: Arc<Session>,
}

impl ZenohWriter {
    pub fn new(session: Arc<Session>, key: ResKey) -> Self {
        ZenohWriter {
            key, session
        }
    }
}

impl Writer for ZenohWriter {
    fn write(&self, buf: &[u8]) {
        task::block_on(async {
            self.session.write(&self.key, RBuf::from(buf)).await.unwrap();
        });
    }
}

pub struct DDSWriter {
    pub dp: i32,
    pub ton: String,
    pub tyn: String,
    pub keyless: bool,
    pub wr: i32,
}

impl Writer for DDSWriter {
    fn write(&self, buf: &[u8]) {
        unsafe {
            // As per the Vec documentation (see https://doc.rust-lang.org/std/vec/struct.Vec.html#method.into_raw_parts)
            // the only way to correctly releasing it is to create a vec using from_raw_parts
            // and then have its destructor do the cleanup.
            // Thus, while tempting to just pass the raw pointer to cyclone and then free it from C,
            // that is not necessarily safe or guaranteed to be leak free.
            let (ptr, len, capacity) = buf.to_vec().into_raw_parts();
            let cton = CString::new(self.ton.clone()).unwrap().into_raw();
            let ctyn = CString::new(self.tyn.clone()).unwrap().into_raw();
            let st = cdds_create_blob_sertopic(
                self.dp,
                cton as *mut std::os::raw::c_char,
                ctyn as *mut std::os::raw::c_char,
                self.keyless,
            );
            drop(CString::from_raw(cton));
            drop(CString::from_raw(ctyn));
            let fwdp = cdds_ddsi_payload_create(
                st,
                ddsi_serdata_kind_SDK_DATA,
                ptr,
                len as u64,
            );
            dds_writecdr(self.wr, fwdp as *mut ddsi_serdata);
            drop(Vec::from_raw_parts(ptr, len, capacity));
            cdds_sertopic_unref(st);
        }
    }
}

pub struct Coders {
    coders: Vec<yaml_rust::Yaml>,  
}

impl Coders {
    pub fn new() -> Self {
        Coders {
            coders: vec![],
        }
    }

    pub fn from_config(config_path: &str) -> Self {
        let mut file = File::open(config_path).expect("Unable to open file");
        let mut contents = String::new();
        file.read_to_string(&mut contents).expect("Unable to read file");
        let docs = YamlLoader::load_from_str(&contents).unwrap();

        Coders {
            coders: docs[0].as_vec().unwrap().to_vec(),
        }
    }

    fn create_coder(&self, topic_name: &str, _type_name: &str, writer: Box<dyn Writer + Send>, encoder: bool) -> Box<dyn Coder + Send> {
        for pipe in &self.coders {
            let topics: Vec<&str> = pipe["topics"].as_vec().unwrap().iter().map(|y| y.as_str().unwrap()).collect();
            let matches = topics.contains(&topic_name);

            if matches {
                let pipe_description = match encoder {
                    true => &pipe["encoder"],
                    false => &pipe["decoder"],
                }.as_vec().unwrap().iter().map(|y| y.as_str().unwrap()).collect();

                log::error!("[coders] Selected {:?} coder for {}", pipe, topic_name);
                return Box::new(GstCoder::new(writer, &pipe_description, encoder));
            }

        }
        log::error!("[coders] Selected identity coder for {}", topic_name);
        Box::new(IdentityCoder{writer})
    }

    pub fn new_decoder(&self, topic_name: &str, type_name: &str, writer: Box<dyn Writer + Send>) -> Box<dyn Coder + Send> {
        return self.create_coder(topic_name, type_name, writer, false);
    }
    pub fn new_encoder(&self, topic_name: &str, type_name: &str, writer: Box<dyn Writer + Send>) -> Box<dyn Coder + Send> {
        return self.create_coder(topic_name, type_name, writer, true);
    }
}



pub trait Coder {
    fn encode(&self, data: Vec<u8>);
    fn decode(&self, data: Vec<u8>);
}

struct IdentityCoder {
    writer: Box<dyn Writer + Send>,
}

impl Coder for IdentityCoder {
    fn encode(&self, data: Vec<u8>) {
        self.writer.write(&data);
    }

    fn decode(&self, data: Vec<u8>) {
        self.writer.write(&data);
    }
}
