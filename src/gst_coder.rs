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


pub struct GstCoder {
    pipeline: gstreamer::Bin,
    src: gst_app::AppSrc,
}

impl GstCoder {
    pub fn new(writer: Box<Writer + Send>, pipeline_description: &Vec<&str>) -> Self {
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
                    writer.write(map.as_slice());
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

    fn decode(&self, data: Vec<u8>) -> Vec<u8> {
        data
    }
}
