use gstreamer::prelude::*;
use gstreamer::MessageView;
use gstreamer::*;
use std::sync::{Arc, Mutex};
use std::thread;
use ctrlc;

fn start_recording_gstreamer() -> Result<Pipeline, Box<dyn std::error::Error>> {
    let sdp_path = String::from("input-h264.sdp");
    let mp4_temp_path = String::from("output.mp4.tmp");
    let mp4_path = String::from("output.mp4");

    gstreamer::init()?;

    let pipeline = Pipeline::with_name("gstreamer-pipeline");

    let file_src = ElementFactory::make("filesrc")
        .name("filesrc")
        .property("location", sdp_path)
        .build()
        .expect("Error creating file_src element");

    let sdp_demux = ElementFactory::make("sdpdemux")
        .name("demux")
        .property("timeout", 0u64)
        .build()
        .expect("Error creating sdp_demux element");

    let mp4mux = ElementFactory::make("mp4mux")
        .name("mp4mux")
        .property("faststart", &true)
        .property("faststart-file", mp4_temp_path)
        .build()
        .expect("Error creating mp4mux element");

    let file_sink = ElementFactory::make("filesink")
        .name("filesink")
        .property("location", mp4_path)
        .build()
        .expect("Error creating file_sink element");

    let queue_opus = ElementFactory::make("queue")
        .name("queue_opus")
        .build()
        .expect("Error creating queue_opus element");

    let rtp_opus_depay = ElementFactory::make("rtpopusdepay")
        .name("rtpopusdepay")
        .build()
        .expect("Error creating rtp_opus_depay element");

    let opus_parse = ElementFactory::make("opusparse")
        .name("opusparse")
        .build()
        .expect("Error creating opus_parse element");

    let queue_h264 = ElementFactory::make("queue")
        .name("queue_h264")
        .build()
        .expect("Error creating queue_h264 element");

    let rtp_h264_depay = ElementFactory::make("rtph264depay")
        .name("rtph264depay")
        .build()
        .expect("Error creating rtp_h264_depay element");

    let h264_parse = ElementFactory::make("h264parse")
        .name("h264parse")
        .build()
        .expect("Error creating h264_parse element");

    pipeline.add_many(&[
        &file_src, &sdp_demux, &file_sink,
        &queue_opus, &rtp_opus_depay, &opus_parse,
        &queue_h264, &rtp_h264_depay, &h264_parse,
        &mp4mux
    ]).expect("");

    gstreamer::Element::link_many(&[&file_src , &sdp_demux])
        .expect("Error linking file_src and sdp_demux elements");

    gstreamer::Element::link_many(&[&mp4mux, &file_sink])
        .expect("Error linking mp4_mux and file_sink elements");

    sdp_demux.connect_pad_added(move |_, src_pad| {
        println!("Pad added: {}", src_pad.name());
        println!("{:#?}", src_pad);

        let caps = src_pad.query_caps(None);
        println!("{:#?}", caps);

        let media_type = caps.structure(0).and_then(|s| s.get::<&str>("media").ok()).expect("Error getting structure");

        println!("New pad added with media type: {}", media_type);

        if media_type == "audio" {

            let sink_pad = queue_opus
                .static_pad("sink")
                .expect("Failed to get sink pad from queue_opus_clone.");

            match src_pad.link(&sink_pad) {
                Ok(_) => println!("Linked audio pad to queue_opus_clone."),
                Err(err) => {
                    eprintln!("Failed to link audio pad: {}", err);
                    return;
                }
            }

            match gstreamer::Element::link_many(&[
                &queue_opus,
                &rtp_opus_depay,
                &opus_parse,
                &mp4mux,
            ]) {
                Ok(_) => println!("Successfully linked Opus branch."),
                Err(err) => eprintln!("Failed to link Opus branch: {}", err),
            }

        } else if media_type == "video" {

            let sink_pad = queue_h264
                .static_pad("sink")
                .expect("Failed to get sink pad from queue_h264.");
            match src_pad.link(&sink_pad) {
                Ok(_) => println!("Linked video pad to queue_h264."),
                Err(err) => {
                    eprintln!("Failed to link video pad: {}", err);
                    return;
                }
            }

            match gstreamer::Element::link_many(&[
                &queue_h264,
                &rtp_h264_depay,
                &h264_parse,
                &mp4mux,
            ]) {
                Ok(_) => println!("Successfully linked H264 branch."),
                Err(err) => eprintln!("Failed to link H264 branch: {}", err),
            }
        }
    });

    Ok(pipeline)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting GStreamer recording...");

    let pipeline = start_recording_gstreamer()?;
    pipeline.set_state(State::Playing)?;

    let pipeline = Arc::new(Mutex::new(pipeline));
    let pipeline_clone = Arc::clone(&pipeline);

    ctrlc::set_handler(move || {
        println!("Received Ctrl+C, terminating GStreamer recording...");
        let pipeline = pipeline_clone.lock().unwrap();
        let _ = pipeline.send_event(gstreamer::event::Eos::new());
    })?;

    let bus = pipeline.lock().unwrap().bus().expect("Pipeline has no bus");

    for msg in bus.iter_timed(gstreamer::ClockTime::NONE) {
        match msg.view() {
            MessageView::Eos(..) => {
                println!("End of stream");
                break;
            }
            MessageView::Error(err) => {
                eprintln!(
                    "Error from {:?}: {} ({:?})",
                    err.src().map(|s| s.path_string()),
                    err.error(),
                    err.debug()
                );
                break;
            }
            _ => (),
        }
    }

    pipeline.lock().unwrap().set_state(State::Null)?;
    println!("Pipeline stopped");

    Ok(())
}

