use std::error::Error;
use std::sync::{Arc, Mutex};
use chrono::{Local, Utc};
use gstreamer::{glib, ClockTime, Element, ElementFactory, PadProbeReturn, PadProbeType};
use gstreamer::prelude::{ElementExt, ElementExtManual, GstBinExt, GstBinExtManual, GstObjectExt, ObjectExt, PadExt, PadExtManual, PipelineExt, PresetExt};
use gstreamer_app::gst;
use gstreamer_app::gst_base::ffi::{GstAggregatorStartTimeSelection, GST_AGGREGATOR_START_TIME_SELECTION_SET};

fn main() -> Result<(), Box<dyn Error>>{
    println!("Hello, world!");
    // std::env::set_var("GST_DEBUG", "rtpbin:5");
    // std::env::set_var("GST_DEBUG", "5");
    gstreamer::init().expect("Failed to initialize GStreamer");
    let pipeline = gstreamer::Pipeline::with_name("Video Pipeline");
    let shared_clock = gstreamer::SystemClock::obtain();
    // shared_clock.set_property("clock-type", &"realtime");
    pipeline.use_clock(Some(&shared_clock));
    /* ------------------- Creation of the elements  ------------------- */

    let video_caps = gstreamer::Caps::builder("application/x-rtp")
        .field("media", "video")
        .field("clock-rate", 90000)
        .field("encoding-name", "H264")
        .field("payload", 125)
        // .field("packetization-mode", 0)
        // .field("profile-level-id", "42e01f")
        // .field("ssrc", "241348592")
        // .field("level-asymmetry-allowed", 1)
        // .field("stream-format", &"byte-stream")
        // .field("alignment", &"nal")
        // .field("parsed", &true)
        // .field("x-google-start-bitrate", 4000)
        // .field("cname", &"demo-1")
        .build();

    let rtp_port: i32 = 50000;
    let udpsrc_element = gstreamer::ElementFactory::make("udpsrc")
        .name("udpsrc").build()
        .expect("Failed to create udpsrc element");
    udpsrc_element.set_property("address", &"127.0.0.1");
    udpsrc_element.set_property("port", &rtp_port);
    udpsrc_element.set_property("caps", &video_caps);
    let rtp_bin_element = gstreamer::ElementFactory::make("rtpbin")
        .name("rtpbin")
        .property("add-reference-timestamp-meta", &true)
        .property_from_str("ntp-time-source", "2")
        .property("use-pipeline-clock", &true)
        .build()
        .expect("Failed to create rtpbin element");
    let queue_element = gstreamer::ElementFactory::make("queue")
        .name("queue")
        .build()
        .expect("Error creating queue_opus element");
    let rtp_h264_de_pay_element = gstreamer::ElementFactory::make("rtph264depay")
        .name("rtph264depay")
        // .property("request-keyframe", &true)
        // .property("wait-for-keyframe", &true)
        .build()
        .expect("Failed to create rtph264depay element");
    let h264_parse_element = gstreamer::ElementFactory::make("h264parse")
        .name("h264parse")
        .property("config-interval", -1)
        .build()
        .expect("Failed to create h264parse element");
    let output_pattern = String::from("video_chunk_%05d.ts");
    let muxer = ElementFactory::make("mpegtsmux")
            .name("mpegtsmux")
        // .property_from_str("start-time-selection", "2")
        .build()
        .expect("Error creating mpegtsmux element");

    let split_mux_sink_element = ElementFactory::make("splitmuxsink")
        .name("splitmuxsink")
        .property("location", output_pattern)
        // .property("max-size-bytes", 0)
        .property("max-size-time", ClockTime::from_seconds(5).nseconds())
        .property("send-keyframe-requests", true)
        // .property("muxer-factory", "mpegtsmux")
        .property("muxer", &muxer)
        .build()
        .expect("Error creating splitmuxsink element");
    split_mux_sink_element.connect("format-location", false, |values| {
        // Generate the custom filename with a timestamp
        let timestamp_nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards")
            .as_nanos();
        let filename = format!("video_chunk_{}.ts", timestamp_nanos);

        // Return the filename
        Some(glib::Value::from(&filename))
    });
    let fake_sink_element = gstreamer::ElementFactory::make("fakesink")
        .name("fakesink").build()
        .expect("Failed to create fakesink element");

    /* ------------------- Add elements to the pipeline ------------------- */
    pipeline.add_many(&[
        &udpsrc_element,
        &rtp_bin_element,
        &queue_element,
        &rtp_h264_de_pay_element,
        &h264_parse_element,
        &split_mux_sink_element
    ]).expect("Failed to add elements to the pipeline");
    /*---------------------------------------------------------------------*/

    /* ------------------- Start Linking of pads / elements ------------------- */
    let rtp_bin_sink_pad = rtp_bin_element.request_pad_simple("recv_rtp_sink_0")
        .expect("Failed to get rtpbin sink pad");
    let udp_src_pad = udpsrc_element.static_pad("src").unwrap();
    udp_src_pad.link(&rtp_bin_sink_pad).expect("Failed to link udpsrc and fakesink");
    let element_arc = Arc::new(Mutex::new(queue_element.clone()));
    let element_clone = Arc::clone(&element_arc);
    rtp_bin_element.connect_pad_added(move |rtpbin, pad| {
        println!("Pad added: {}", pad.name());
        println!("{:#?}", pad);
        if pad.name().starts_with("recv_rtp_src_") {
            println!("{:#?}",pad.query_caps(None));
            println!("New pad added: {}", pad.name());

            // Get the sink pad of rtpopusdepay
            let queue_sink_pad = element_clone.lock().unwrap().static_pad("sink")
                .expect("queue should have a sink pad");
            println!("Fake Sink Pad {:#?}",queue_sink_pad.query_caps(None));
            // Link the rtpbin pad to the depayloader
            if pad.link(&queue_sink_pad).is_ok() {
                println!("Successfully linked rtpbin to queue");
            } else {
                println!("Failed to link rtpbin to queue {:#?}", pad.link(&queue_sink_pad).err().unwrap());
            }
        }
    });
    queue_element.link(&rtp_h264_de_pay_element).expect("Failed to link rtp_h264_de_pay and rtp_h264_de_pay_element");
    rtp_h264_de_pay_element.link(&h264_parse_element).expect("Failed to link rtp_h264_de_pay and fakesink");
    let h264_parse_src_pad = h264_parse_element.static_pad("src").unwrap();
    let split_mux_sink_video_pad = split_mux_sink_element.request_pad_simple("video")
        .expect("Failed to get video pad from splitmuxsink");
    h264_parse_src_pad.link(&split_mux_sink_video_pad).expect("Failed to link h264_parse and split mux");
    /*---------------------------------------------------------------------*/

    /* ------------------- Debugging help like adding of probes ... ------------------- */
    let fake_sink_pad = fake_sink_element.static_pad("sink").unwrap();
    fake_sink_pad.add_probe(PadProbeType::BUFFER, |pad, info| {
        let mut buffer = info.buffer().unwrap();
        if let Some(caps) = pad.current_caps() {
            println!("Caps on {}: {}", pad.name(), caps.to_string());
        }
        let size = buffer.size();
        println!("Received buffer with size: {}, pts : {:?}, duration: {:?}", size, buffer.pts(), buffer.duration());
        PadProbeReturn::Ok
    }).unwrap();
    /*---------------------------------------------------------------------*/

    /* Boiler Plate code for starting a pipeline */
    pipeline.set_state(gstreamer::State::Playing)?;
    let pipeline_arc = Arc::new(Mutex::new(pipeline.clone()));
    let pipeline_clone = Arc::clone(&pipeline_arc);
    ctrlc::set_handler(move || {
        println!("Received Ctrl+C, terminating GStreamer recording...");
        let pipeline = pipeline_clone.lock().unwrap();
        println!("Received Ctrl+C, sending Eos...");
        let _ = pipeline.send_event(gstreamer::event::Eos::new());
    })?;

    // Monitor the bus for EOS or errors
    let bus = pipeline.bus().unwrap();
    for msg in bus.iter_timed(gstreamer::ClockTime::NONE) {
        match msg.view() {
            gstreamer::MessageView::Eos(_) => {
                println!("EOS reached.");
                break;
            }
            gstreamer::MessageView::Error(err) => {
                eprintln!("Error: {}", err.error());
                break;
            }
            _ => (),
        }
    }

    // Clean up
    pipeline.set_state(gstreamer::State::Null)?;
    Ok(())
}
