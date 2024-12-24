use std::error::Error;
use std::sync::{Arc, Mutex};
use chrono::Utc;
use gstreamer::{ClockTime, ElementFactory, PadProbeReturn, PadProbeType};
use gstreamer::prelude::{ElementExt, ElementExtManual, GstBinExtManual, GstObjectExt, ObjectExt, PadExt, PadExtManual};
use gstreamer_app::gst_base::ffi::{GstAggregatorStartTimeSelection, GST_AGGREGATOR_START_TIME_SELECTION_SET};

fn main() -> Result<(), Box<dyn Error>>{
    println!("Hello, world!");
    std::env::set_var("GST_DEBUG", "3");
    gstreamer::init()?;
    let pipeline = gstreamer::Pipeline::with_name("Audio Pipeline");
    /* ------------------- Creation of the Audio elements  ------------------- */
    let audio_caps = gstreamer::Caps::builder("application/x-rtp")
        .field("media", "audio")
        .field("clock-rate", 48000)
        .field("encoding-name", "OPUS")
        .field("payload", 111)
        // .field("ssrc", &"4278448574")
        // .field("cname", &"demo-1")
        .build();
    let audio_rtp_port: i32 = 60000;
    let udp_src_audio_element = gstreamer::ElementFactory::make("udpsrc")
        .name("udp_src_audio").build()
        .expect("Failed to create udpsrc element");
    udp_src_audio_element.set_property("address", &"127.0.0.1");
    udp_src_audio_element.set_property("port", &audio_rtp_port);
    udp_src_audio_element.set_property("caps", &audio_caps);
    let audio_rtp_bin_element = gstreamer::ElementFactory::make("rtpbin")
        .name("audio_rtp_bin").build()
        .expect("Failed to create rtpbin element");
    let audio_queue_element = ElementFactory::make("queue")
        .name("audio_queue")
        .build()
        .expect("Error creating queue_opus element");
    let rtp_opus_de_pay_element = gstreamer::ElementFactory::make("rtpopusdepay")
        .name("rtpopusdepay").build()
        .expect("Failed to create rtpopusdepay element");
    let opus_dec_element = gstreamer::ElementFactory::make("opusdec")
        .name("opusdec").build()
        .expect("Failed to create opusdec element");
    let opus_enc_element = gstreamer::ElementFactory::make("opusenc")
        .name("opusenc").build()
        .expect("Failed to create opusenc element");
    let audio_output_pattern = String::from("chunk_%05d.ts");
    // let timestamp = (Utc::now().timestamp() * 90 / 1_000_000_000) as u64;
    // println!("Start time is {}", timestamp);
    let audio_muxer = ElementFactory::make("mpegtsmux")
        .name("audio_mpeg_ts_mux")
        .property_from_str("start-time-selection", "0")
        .property("pcr-interval",&40u32)
        // .property("start-time", timestamp)
        .build()
        .expect("Error creating mpegtsmux element");
    /*let start_time_selection= muxer
        .property_value("start-time-selection");
    println!("Muxer property start_time_selection {:#?}", start_time_selection);
    let start_time= muxer
        .property_value("start-time");
    println!("Muxer property start_time {:#?}", start_time);*/
    let audio_split_mux_sink_element = ElementFactory::make("splitmuxsink")
        .name("audio_split_mux_sink")
        .property("location", audio_output_pattern)
        // .property("max-size-bytes", 1000000u64)
        .property("max-size-time", ClockTime::from_seconds(5).nseconds()) // Split every 10 seconds is the config but idk why it splits when the first chunk is 3 mins and then all the subsequent chunks get split at 129 seconds (2:09 mins)
        // .property("muxer-factory", "mp4mux")
        .property("muxer", &audio_muxer)
        .build()
        .expect("Error creating splitmuxsink element");
    /************************ Creation of the Video Elements ************************/
    let video_caps = gstreamer::Caps::builder("application/x-rtp")
        .field("media", "video")
        .field("clock-rate", 90000)
        .field("encoding-name", "H264")
        .field("payload", 125)
        // .field("packetization-mode", 1)
        // .field("profile-level-id", "42e01f")
        // .field("ssrc", &"4278448574")
        // .field("cname", &"demo-1")
        .build();

    let video_rtp_port: i32 = 50000;
    let udp_src_video_element = gstreamer::ElementFactory::make("udpsrc")
        .name("udpsrc").build()
        .expect("Failed to create udpsrc element");
    udp_src_video_element.set_property("address", &"127.0.0.1");
    udp_src_video_element.set_property("port", &video_rtp_port);
    udp_src_video_element.set_property("caps", &video_caps);
    let video_rtp_bin_element = gstreamer::ElementFactory::make("rtpbin")
        .name("video_rtp_bin").build()
        .expect("Failed to create rtpbin element");
    let video_queue_element = gstreamer::ElementFactory::make("queue")
        .name("video_queue")
        .build()
        .expect("Error creating queue_opus element");
    let rtp_h264_de_pay_element = gstreamer::ElementFactory::make("rtph264depay")
        .name("rtph264depay").build()
        .expect("Failed to create rtph264depay element");
    let h264_parse_element = gstreamer::ElementFactory::make("h264parse")
        .name("h264parse").build()
        .expect("Failed to create h264parse element");
    let video_output_pattern = String::from("video_chunk_%05d.ts");
    let video_muxer = ElementFactory::make("mpegtsmux")
        .name("video_mpeg_ts_mux")
        .property_from_str("start-time-selection", "0")
        .property("pcr-interval",&40u32)
        // .property_from_str("start-time-selection", "2")
        .build()
        .expect("Error creating mpegtsmux element");
    let video_split_mux_sink_element = ElementFactory::make("splitmuxsink")
        .name("video_split_mux_sink")
        .property("location", video_output_pattern)
        // .property("max-size-bytes", 1000000u64)
        .property("max-size-time", ClockTime::from_seconds(5).nseconds()) // Split every 10 seconds is the config but idk why it splits when the first chunk is 3 mins and then all the subsequent chunks get split at 129 seconds (2:09 mins)
        // .property("muxer-factory", "mpegtsmux")
        .property("muxer", &video_muxer)
        .build()
        .expect("Error creating splitmuxsink element");

    /* ------------------- Add elements to the pipeline ------------------- */
    pipeline.add_many(&[&udp_src_audio_element,
        &audio_rtp_bin_element,
        &audio_queue_element,
        &rtp_opus_de_pay_element,
        &opus_dec_element,
        &opus_enc_element,
        &audio_split_mux_sink_element,
        &udp_src_video_element,
        &video_rtp_bin_element,
        &video_queue_element,
        &rtp_h264_de_pay_element,
        &h264_parse_element,
        &video_split_mux_sink_element
    ]).expect("Failed to add elements to the pipeline");

    /* ------------------- Start Linking of pads / elements ------------------- */
    /************************ Audio ************************/
    udp_src_audio_element.link(&audio_rtp_bin_element).expect("Failed to link udpsrc and fakesink");
    let audio_queue_element_arc = Arc::new(Mutex::new(audio_queue_element.clone()));
    let audio_queue_element_clone = Arc::clone(&audio_queue_element_arc);
    audio_rtp_bin_element.connect_pad_added(move |rtpbin, pad| {
        println!("Pad added: {}", pad.name());
        println!("{:#?}", pad);
        if pad.name().starts_with("recv_rtp_src_") {
            println!("{:#?}",pad.query_caps(None));
            println!("New pad added: {}", pad.name());

            // Get the sink pad of rtpopusdepay
            let audio_queue_sink_pad = audio_queue_element_clone.lock().unwrap().static_pad("sink")
                .expect("rtpopusdepay should have a sink pad");
            println!("Fake Sink Pad {:#?}",audio_queue_sink_pad.query_caps(None));
            // Link the rtpbin pad to the depayloader
            if pad.link(&audio_queue_sink_pad).is_ok() {
                println!("Successfully linked rtpbin to rtpopusdepay");
            } else {
                println!("Failed to link rtpbin to rtpopusdepay {:#?}", pad.link(&audio_queue_sink_pad).err().unwrap());
            }
        }
    });
    audio_queue_element.link(&rtp_opus_de_pay_element).expect("Failed to link queue and rtp_opus_de_pay");
    rtp_opus_de_pay_element.link(&opus_dec_element).expect("Failed to link rtpopusdepay and opusdec");
    opus_dec_element.link(&opus_enc_element).expect("Failed to link opusdec and opusenc");
    let opus_enc_src_pad = opus_enc_element.static_pad("src")
        .expect("Failed to get sink pad from opusenc");
    let split_mux_sink_audio_pad = audio_split_mux_sink_element.request_pad_simple("audio_%u")
        .expect("Failed to get audio pad from splitmuxsink");
    opus_enc_src_pad.link(&split_mux_sink_audio_pad).expect("Failed to link opusenc and splitmuxsink");
    /************************ Video ************************/
    udp_src_video_element.link(&video_rtp_bin_element).expect("Failed to link udpsrc and fakesink");
    let element_arc = Arc::new(Mutex::new(video_queue_element.clone()));
    let element_clone = Arc::clone(&element_arc);
    video_rtp_bin_element.connect_pad_added(move |rtpbin, pad| {
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
    video_queue_element.link(&rtp_h264_de_pay_element).expect("Failed to link queue and rtp_h264_de_pay");
    rtp_h264_de_pay_element.link(&h264_parse_element).expect("Failed to link rtp_h264_de_pay and fakesink");
    let h264_parse_src_pad = h264_parse_element.static_pad("src").unwrap();
    let split_mux_sink_video_pad = video_split_mux_sink_element.request_pad_simple("video")
        .expect("Failed to get video pad from splitmuxsink");
    h264_parse_src_pad.link(&split_mux_sink_video_pad).expect("Failed to link h264_parse and split mux");
    /*---------------------------------------------------------------------*/

    /* ------------------- Debugging help like adding of probes ... ------------------- */
    /*---------------------------------------------------------------------*/
    /* Boiler Plate code for starting a pipeline */
    pipeline.set_state(gstreamer::State::Playing).expect("Failed to set the pipeline to the Playing state");
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
