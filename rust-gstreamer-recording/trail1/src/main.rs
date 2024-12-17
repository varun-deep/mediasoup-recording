use gstreamer::prelude::*;
use std::error::Error;
use std::env;
use std::sync::{Arc, Mutex};
use gstreamer::{Buffer, Caps, ClockTime, ElementFactory, Format, PadProbeReturn, PadProbeType};
use gstreamer_app::AppSrc;
use gstreamer_app::glib::Value;
use gstreamer_sdp::{SDPConnection, SDPMedia, SDPMessage};

fn main() -> Result<(), Box<dyn Error>> {
    std::env::set_var("GST_DEBUG", "appsrc:5");
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

    let mut sdp_message_audio = SDPMessage::new();
    sdp_message_audio.set_origin("-", "0", "0", "IN", "IP4", "127.0.0.1");
    sdp_message_audio.set_session_name("-");
    let conn_info = SDPConnection::new("IN", "IP4", "127.0.0.1", 64, 0);
    sdp_message_audio.set_connection(conn_info.nettype().unwrap(),
                                     conn_info.addrtype().unwrap(),
                                     conn_info.address().unwrap(),
                                     conn_info.ttl(),
                                     conn_info.addr_number());
    // let mut audio_media = SDPMedia::new("audio", 60000, "RTP/AVPF", Some(vec![111]));
    let mut audio_media = SDPMedia::new();
    audio_media.set_media("audio");
    audio_media.set_port_info(60004, 1);
    audio_media.set_proto("RTP/AVPF");
    audio_media.add_format("111");
    audio_media.add_attribute("rtpmap", Some("111 opus/48000/2"));
    audio_media.add_attribute("fmtp", Some("111 minptime=10;useinbandfec=1"));
    audio_media.add_attribute("rtcp", Some("60005"));
    sdp_message_audio.add_media(audio_media);

    // Create the UDP source for receiving the RTP stream
    // let udpsrc = gstreamer::ElementFactory::make("udpsrc").name("udpsrc").build().expect("Failed to create udpsrc element");
    // udpsrc.set_property("address", &"127.0.0.1"); // Listen on all interfaces
    // udpsrc.set_property("port", &rtp_port); // RTP port from command line
    // Set the caps for the udpsrc (RTP)
    let audio_caps = gstreamer::Caps::builder("application/x-rtp")
        .field("media", &"audio")
        .field("encoding-name", &codec)
        .field("clock-rate", &clock_rate)
        .field("payload", &payload_type)
        .build();
    // udpsrc.set_property("caps", &audio_caps);

    // Create appsrc for rtp stream
    let appsrc_audio = ElementFactory::make("appsrc")
        .name("appsrc_audio")
        .property("is-live", &true)
        .property("do-timestamp", &true)
        .property("format", Format::Time)
        .build()
        .expect("Error creating appsrc element");
    appsrc_audio.set_property("caps", &audio_caps);
    // let sdp_audio_bytes = sdp_message_audio.as_text().expect("Failed to convert SDP message to text").as_bytes().to_vec();
    let sdp_message_audio_arc = Arc::new(Mutex::new(sdp_message_audio));
    let sdp_message_audio_clone = Arc::clone(&sdp_message_audio_arc);
    let appsrc = appsrc_audio.clone().dynamic_cast::<AppSrc>().unwrap();
    appsrc.set_callbacks(
        gstreamer_app::AppSrcCallbacks::builder()
            .need_data(move |appsrc, _| {
                let sdp_audio_bytes = sdp_message_audio_clone.lock().unwrap().as_text().expect("Failed to convert SDP message to text").as_bytes().to_vec();
                match appsrc.push_buffer(Buffer::from_slice(sdp_audio_bytes)) {
                    Ok(_) => {
                        println!("Pushed buffer of size")
                    },
                    Err(err) => {
                        println!("Failed to push buffer: {}", err)
                    }
                }
            }).build());
    // Create the RTP bin for handling RTP and RTCP
    let rtpbin = gstreamer::ElementFactory::make("rtpbin").name("rtpbin").build().expect("Failed to create rtpbin element");
    //appsrc_audio.link(&rtpbin).expect("Failed to link appsrc to rtpbin");
    rtpbin.connect_pad_added(move |rtpbin, pad| {
        // Check if the newly added pad is an RTP media pad
        println!("Pad added: {}", pad.name());
        println!("{:#?}", pad);
        if pad.name().starts_with("recv_rtp_src_") {
            println!("{:#?}",pad.query_caps(None));
            println!("New pad added: {}", pad.name());

            // Get the sink pad of rtpopusdepay
    /*        let rtpopusdepay_sink = rtpopusdepay_arc_clone.lock().unwrap().static_pad("sink")
                .expect("rtpopusdepay should have a sink pad");
            println!("Queue {:#?}",rtpopusdepay_sink.query_caps(None));
            // Link the rtpbin pad to the depayloader
            if pad.link(&rtpopusdepay_sink).is_ok() {
                println!("Successfully linked rtpbin to rtpopusdepay");
            } else {
                println!("Failed to link rtpbin to rtpopusdepay {:#?}", pad.link(&rtpopusdepay_sink).err().unwrap());
            }*/
        }
    });

    let rtpopusdepay = gstreamer::ElementFactory::make("rtpopusdepay").name("rtpopusdepay").build().expect("Failed to create rtpopusdepay element");

    let opusdec = gstreamer::ElementFactory::make("opusdec").name("opusdec").build().expect("Failed to create opusdec element");
    // let multi_file_sink = gstreamer::ElementFactory::make("filesink").name("filesink").build().expect("Failed to create filesink element");
    // multi_file_sink.set_property("location", &"audio_chunk_%05d.mp4");
    // multi_file_sink.set_property("max-file-duration", 1000000u64);
    let splitmuxsink = ElementFactory::make("splitmuxsink")
        .name("splitmuxsink")
        .property("location", &"audio_chunk_%05d.mp4")
        // .property("force-chunks", true)
        .property("max-size-time", ClockTime::from_seconds(10).nseconds()) // Split every 10 seconds is the config but idk why it splits when the first chunk is 3 mins and then all the subsequent chunks get split at 129 seconds (2:09 mins)
        .property("muxer-factory", "mp4mux")
        .build()
        .expect("Error creating splitmuxsink element");

    let fake_sink = gstreamer::ElementFactory::make("fakesink").name("fakesink").build().expect("Failed to create fakesink element");
    let rtpopusdepay_arc = Arc::new(Mutex::new(rtpopusdepay.clone()));
    let rtpopusdepay_arc_clone = Arc::clone(&rtpopusdepay_arc);

    let fakesink_pad = fake_sink.static_pad("sink").unwrap();
    appsrc_audio.link(&fake_sink).expect("Failed to link appsrc to rtpbin");
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
    pipeline.add_many(&[&appsrc_audio, &rtpbin, &rtpopusdepay, &opusdec, &splitmuxsink, &fake_sink])?;
    let appsrc_src_pad = appsrc.static_pad("src").unwrap();
    let rtpbin_sink = rtpbin.request_pad_simple("recv_rtp_sink_%u").unwrap();
    appsrc_src_pad.link(&rtpbin_sink)?;
    let rtpopusdepay_src = rtpopusdepay.static_pad("src").unwrap();
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
    rtpopusdepay_src.link(&fakesink_pad)?;

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