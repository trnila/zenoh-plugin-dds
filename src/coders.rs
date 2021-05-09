use async_trait::async_trait;
use zenoh::net::{RBuf, ResKey, Session};
use std::future::Future;
use std::str;
use crate::gst_coder::GstCoder;
use async_std::task;
use std::sync::Arc;
use std::ffi::CString;
use cyclors::*;


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
            self.session.write(&self.key, RBuf::from(buf)).await;
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

#[async_trait]
pub trait Coder {
    fn encode(&self, data: Vec<u8>);
    fn decode(&self, data: Vec<u8>) -> Vec<u8>;
}

pub fn new_encoder(topic_type: &str, writer: Box<Writer + Send>, encoder: bool) -> Box<dyn Coder + Send> {

    match topic_type {
        "std_msgs::msg::dds_::String_" => Box::new(UpperCoder{writer}),
        "sensor_msgs::msg::dds_::Image_" => {
            let pipe_description = match encoder {
                true => vec![
                        "appsrc name=src format=time is-live=true caps=video/x-raw,width=640,height=480,format=RGB,framerate=15/1",
                        "queue",
                        "videoconvert",
                        "nvvidconv",
                        "video/x-raw(memory:NVMM),format=(string)I420",
                        "nvv4l2h264enc insert-sps-pps=1",
                        "h264parse",
                        "queue",
                        "appsink name=sink emit-signals=1"
                ],
                false => panic!("not implemented"),
            };

            Box::new(GstCoder::new(writer, &pipe_description))
        },
        _ => Box::new(IdentityCoder{writer}),
    }
}

struct IdentityCoder {
    writer: Box<Writer + Send>,
}

impl Coder for IdentityCoder {
    fn encode(&self, data: Vec<u8>) {
        self.writer.write(&data);
    }

    fn decode(&self, data: Vec<u8>) -> Vec<u8> {
        data
    }
}

struct UpperCoder {
    writer: Box<Writer + Send>,
}

impl Coder for UpperCoder {
    fn encode(&self, data: Vec<u8>) {
        self.writer.write(str::from_utf8(&data).unwrap().to_uppercase().as_bytes());
    }

    fn decode(&self, data: Vec<u8>) -> Vec<u8> {
        str::from_utf8(&data).unwrap().to_lowercase().as_bytes().to_vec()
    }
}
