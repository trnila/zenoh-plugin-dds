use async_std::task;
use async_trait::async_trait;
use gstreamer::prelude::*;
use gstreamer as gst;
use gstreamer_video as gst_video;
use gstreamer_app as gst_app;
use std::time::{Duration, Instant};
use gstreamer::gst_element_error as element_error;
use std::u8;
use futures::channel::mpsc; 
use futures::join;
use crate::coders::{Coder, Writer};
use serde_derive::{Deserialize, Serialize};
use cdr::{CdrLe, Infinite};

#[derive(Serialize, PartialEq)]
struct Time {
    sec: i32,
    nanosec: u32,
}

#[derive(Serialize, PartialEq)]
struct Header {
    stamp: Time,
    frame_id: String,
}

#[derive(Serialize, PartialEq)]
struct Image {
    header: Header,
    height: u32,
    width: u32,
    encoding: String,
    is_bigendian: u8,
    step: u32,
    data: Vec<u8>,
}

pub struct GstCoder {
    pipeline: gstreamer::Bin,
    src: gst_app::AppSrc,
}

impl GstCoder {
    pub fn new(writer: Box<Writer + Send>, pipeline_description: &Vec<&str>, encoder: bool) -> Self {
        println!("Starting pipeline");
        gst::init().unwrap();

        let mut context = gst::ParseContext::new();
        let pipeline = gst::parse_launch_full(&pipeline_description.join(" ! "), Some(&mut context), gst::ParseFlags::empty()).unwrap();
        pipeline.set_state(gst::State::Playing).unwrap();
        let pipeline = pipeline.dynamic_cast::<gst::Bin>().unwrap();

        let src = pipeline.get_by_name("src").unwrap().dynamic_cast::<gst_app::AppSrc>().unwrap();
        //        src.set_caps(Some(&video_info.to_caps().unwrap()));

        let (mut tx, mut rx) = mpsc::channel::<gstreamer::Sample>(16);

        let sink = pipeline.get_by_name("sink").unwrap().dynamic_cast::<gst_app::AppSink>().unwrap();
        sink.set_callbacks(
            gst_app::AppSinkCallbacks::builder()
                .new_sample(move |appsink| {
                    let sample = appsink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    let buffer = sample.get_buffer().unwrap();
                    let map = buffer.map_readable().unwrap();

                    if(!encoder) {
                        let msg = Image{
                            header: Header{
                                frame_id: "base_link".to_string(),
                                stamp: Time{sec: 0, nanosec: 0},
                            },
                            height: 480,
                            width: 640,
                            encoding: "rgb8".to_string(),
                            is_bigendian: 0,
                            step: 640*3,
                            data: map.as_slice().to_vec(),
                        };

                        let encoded = cdr::serialize::<_, _, CdrLe>(&msg, Infinite).unwrap();
                        println!("{}", encoded.len());
                        writer.write(encoded.as_slice());
                    } else {
                        writer.write(map.as_slice());
                    }
                    println!("out");
                    Ok(gst::FlowSuccess::Ok)
                })
                .build()
        );

        GstCoder {
            pipeline,
            src,
        }
    }
}

impl Coder for GstCoder {
    fn encode(&self, data: Vec<u8>) {
        let mut buffer = gst::Buffer::with_size(data.len()).unwrap();
        {
            let buffer = buffer.get_mut().unwrap();
            buffer.copy_from_slice(0, &data);
        }

        self.src.push_buffer(buffer);
        println!("in");
    }

    fn decode(&self, data: Vec<u8>) {
        self.encode(data);
    }
}
