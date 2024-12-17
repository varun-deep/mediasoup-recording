use std::error::Error;
use std::sync::{Arc, Mutex};
use gstreamer::{ClockTime, ElementFactory, PadProbeReturn, PadProbeType};
use gstreamer::prelude::{ElementExt, ElementExtManual, GstBinExtManual, GstObjectExt, ObjectExt, PadExt, PadExtManual};

fn main() -> Result<(), Box<dyn Error>>{
    println!("Hello, world!");
    std::env::set_var("GST_DEBUG", "3");
    gstreamer::init()?;
    let pipeline = gstreamer::Pipeline::with_name("Audio Pipeline");
    /* ------------------- Creation of the elements  ------------------- */
    let audio_caps = gstreamer::Caps::builder("application/x-rtp")
        .field("media", "audio")
        .field("clock-rate", 48000)
        .field("encoding-name", "OPUS")
        .field("payload", 111)
        // .field("ssrc", &"4278448574")
        // .field("cname", &"demo-1")
        .build();

    let rtp_port: i32 = 60000;
    let udpsrc_element = gstreamer::ElementFactory::make("udpsrc")
        .name("udpsrc").build()
        .expect("Failed to create udpsrc element");
    udpsrc_element.set_property("address", &"127.0.0.1");
    udpsrc_element.set_property("port", &rtp_port);
    udpsrc_element.set_property("caps", &audio_caps);
    let rtp_bin_element = gstreamer::ElementFactory::make("rtpbin")
        .name("rtpbin").build()
        .expect("Failed to create rtpbin element");
    let queue_element = ElementFactory::make("queue")
        .name("queue")
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
    let output_pattern = String::from("chunk_%05d.mp4");
    let split_mux_sink_element = ElementFactory::make("splitmuxsink")
        .name("splitmuxsink")
        .property("location", output_pattern)
        // .property("max-size-bytes", 1000000u64)
        .property("max-size-time", ClockTime::from_seconds(5).nseconds()) // Split every 10 seconds is the config but idk why it splits when the first chunk is 3 mins and then all the subsequent chunks get split at 129 seconds (2:09 mins)
        // .property("muxer-factory", "mp4mux")
        // .property("muxer", &muxer)
        .build()
        .expect("Error creating splitmuxsink element");
    /* ------------------- Add elements to the pipeline ------------------- */
    pipeline.add_many(&[&udpsrc_element,
        &rtp_bin_element,
        &queue_element,
        &rtp_opus_de_pay_element,
        &opus_dec_element,
        &opus_enc_element,
        &split_mux_sink_element]).expect("Failed to add elements to the pipeline");

    /* ------------------- Start Linking of pads / elements ------------------- */
    udpsrc_element.link(&rtp_bin_element).expect("Failed to link udpsrc and fakesink");
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
                .expect("rtpopusdepay should have a sink pad");
            println!("Fake Sink Pad {:#?}",queue_sink_pad.query_caps(None));
            // Link the rtpbin pad to the depayloader
            if pad.link(&queue_sink_pad).is_ok() {
                println!("Successfully linked rtpbin to rtpopusdepay");
            } else {
                println!("Failed to link rtpbin to rtpopusdepay {:#?}", pad.link(&queue_sink_pad).err().unwrap());
            }
        }
    });
    queue_element.link(&rtp_opus_de_pay_element).expect("Failed to link queue and rtp_opus_de_pay");
    rtp_opus_de_pay_element.link(&opus_dec_element).expect("Failed to link rtpopusdepay and opusdec");
    opus_dec_element.link(&opus_enc_element).expect("Failed to link opusdec and opusenc");
    let opus_enc_src_pad = opus_enc_element.static_pad("src")
        .expect("Failed to get sink pad from opusenc");
    let split_mux_sink_audio_pad = split_mux_sink_element.request_pad_simple("audio_%u")
        .expect("Failed to get audio pad from splitmuxsink");
    opus_enc_src_pad.link(&split_mux_sink_audio_pad).expect("Failed to link opusenc and splitmuxsink");
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
