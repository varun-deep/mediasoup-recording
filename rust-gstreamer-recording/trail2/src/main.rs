use gstreamer::prelude::*;
use std::error::Error;
use std::env;
use std::sync::{Arc, Mutex};
use gstreamer::{ClockTime, ElementFactory, PadProbeReturn, PadProbeType};
use gstreamer_app::glib::Value;

fn main() -> Result<(), Box<dyn Error>> {
    std::env::set_var("GST_DEBUG", "udpsrc:5");
    // std::env::set_var("GST_PROBE", "5");
    // Initialize GStreamer
    gstreamer::init()?;

    let rtp_port: i32 = 60000;
    let rtcp_port: i32 = 60001;

    // Hardcoded SDP information
    let codec = "OPUS"; // Hardcoded codec
    let payload_type = 111; // Hardcoded payload type
    let clock_rate = 48000; // Hardcoded clock rate

    // Create the pipeline
    let pipeline = gstreamer::Pipeline::with_name("Audio Pipeline");

    // Create the UDP source for receiving the RTP stream
    let udpsrc = gstreamer::ElementFactory::make("udpsrc").name("udpsrc").build().expect("Failed to create udpsrc element");
    udpsrc.set_property("address", &"127.0.0.1"); // Listen on all interfaces
    udpsrc.set_property("port", &rtp_port); // RTP port from command line
    // Set the caps for the udpsrc (RTP)
    let audio_caps = gstreamer::Caps::builder("application/x-rtp")
        .field("media", "audio")
        .field("encoding-name", codec)
        .field("clock-rate", clock_rate)
        .field("payload", payload_type)
        .build();
    udpsrc.set_property("caps", &audio_caps);

    // Create the RTP bin for handling RTP and RTCP
    let rtpbin = gstreamer::ElementFactory::make("rtpbin").name("rtpbin").build().expect("Failed to create rtpbin element");
    let rtpopusdepay = gstreamer::ElementFactory::make("rtpopusdepay").name("rtpopusdepay").build().expect("Failed to create rtpopusdepay element");
    let opusdec = gstreamer::ElementFactory::make("opusdec").name("opusdec").build().expect("Failed to create opusdec element");
    let opusenc = gstreamer::ElementFactory::make("opusenc").name("opusenc").build().expect("Failed to create opusenc element");
    let fake_sink = gstreamer::ElementFactory::make("fakesink").name("fakesink").build().expect("Failed to create fakesink element");
    let output_pattern = String::from("chunk_%05d.mp4");
    let muxer = gstreamer::ElementFactory::make("oggmux").name("muxer").build().expect("Failed to create qtmux element");
    let splitmuxsink = ElementFactory::make("splitmuxsink")
        .name("splitmuxsink")
        .property("location", output_pattern)
        // .property("max-size-bytes", 1000000u64)
        .property("max-size-time", ClockTime::from_seconds(1).nseconds()) // Split every 10 seconds is the config but idk why it splits when the first chunk is 3 mins and then all the subsequent chunks get split at 129 seconds (2:09 mins)
        // .property("muxer-factory", "mp4mux")
        .property("muxer", &muxer)
        .build()
        .expect("Error creating splitmuxsink element");
    let queue = ElementFactory::make("queue")
        .name("queue_opus")
        .build()
        .expect("Error creating queue_opus element");
    // queue.set_property("max-size-bytes", 0u32);
    // queue.set_property("max-size-buffers", 500u32);
    // queue.set_property("max-size-time", gstreamer::ClockTime::from_seconds(10));
    let queue_sink = queue.static_pad("sink").unwrap();
    let queue_src = queue.static_pad("src").unwrap();
    let audio_sink = splitmuxsink.request_pad_simple("audio_%u").unwrap();
    audio_sink.add_probe(PadProbeType::BUFFER, |pad, info| {
        // Get the buffer passing through the pad
        let buffer = info.buffer().unwrap();

        // Inspect the buffer (for example, print the buffer size)
        let size = buffer.size();
        println!("Received buffer with size: {}", size);

        // You can also inspect other properties of the buffer, such as:
        // - Buffer timestamp: buffer.pts()
        // - Buffer duration: buffer.duration()

        // Returning PadProbeReturn::Ok allows the data to continue in the pipeline
        PadProbeReturn::Ok
    }).unwrap();
    let fakesink_pad = fake_sink.static_pad("sink").unwrap();
    let rtpopusdepay_arc = Arc::new(Mutex::new(rtpopusdepay.clone()));
    let rtpopusdepay_arc_clone = Arc::clone(&rtpopusdepay_arc);
    let fakesink_arc = Arc::new(Mutex::new(fake_sink.clone()));
    let fakesink_arc_clone = Arc::clone(&fakesink_arc);
    let queue_arc = Arc::new(Mutex::new(queue.clone()));
    let queue_clone = Arc::clone(&queue_arc);
    rtpbin.connect_pad_added(move |rtpbin, pad| {
        // Check if the newly added pad is an RTP media pad
        println!("Pad added: {}", pad.name());
        println!("{:#?}", pad);
        if pad.name().starts_with("recv_rtp_src_") {
            println!("{:#?}",pad.query_caps(None));
            println!("New pad added: {}", pad.name());

            // Get the sink pad of rtpopusdepay
            let queue_sink = queue_clone.lock().unwrap().static_pad("sink")
                .expect("rtpopusdepay should have a sink pad");
            println!("Queue {:#?}",queue_sink.query_caps(None));
            // Link the rtpbin pad to the depayloader
            if pad.link(&queue_sink).is_ok() {
                println!("Successfully linked rtpbin to rtpopusdepay");
            } else {
                println!("Failed to link rtpbin to rtpopusdepay {:#?}", pad.link(&queue_sink).err().unwrap());
            }
        }
    });

    fakesink_pad.add_probe(PadProbeType::BUFFER, |pad, info| {
        // Get the buffer passing through the pad
        let buffer = info.buffer().unwrap();

        // Inspect the buffer (for example, print the buffer size)
        let size = buffer.size();
        println!("Received buffer with size: {}", size);

        // You can also inspect other properties of the buffer, such as:
        // - Buffer timestamp: buffer.pts()
        // - Buffer duration: buffer.duration()

        // Returning PadProbeReturn::Ok allows the data to continue in the pipeline
        PadProbeReturn::Ok
    }).unwrap();
    pipeline.add_many(&[&udpsrc, &rtpbin, &rtpopusdepay, &opusdec, &opusenc, &fake_sink, &queue, &splitmuxsink])?;
    udpsrc.link(&fake_sink)?;
    // queue.link(&fake_sink)?;

    /*queue.link(&rtpopusdepay)?;
    rtpopusdepay.link(&opusdec)?;
    opusdec.link(&opusenc)?;
    opusenc.link(&fake_sink)?;*/
    // queue.link(&rtpbin)?;
    // let rtpopusdepay_src = rtpopusdepay.static_pad("src").unwrap();
    // rtpopusdepay_src.link(&queue_sink)?;
    // queue_src.link(&audio_sink)?;

    pipeline.set_state(gstreamer::State::Playing)?;
    let pipeline_arc = Arc::new(Mutex::new(pipeline.clone()));
    let pipeline_clone = Arc::clone(&pipeline_arc);
    /*ctrlc::set_handler(move || {
        println!("Received Ctrl+C, terminating GStreamer recording...");
        let pipeline = pipeline_clone.lock().unwrap();
        println!("Received Ctrl+C, sending Eos...");
        let _ = pipeline.send_event(gstreamer::event::Eos::new());
    })?;*/

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