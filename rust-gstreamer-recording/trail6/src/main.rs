use gstreamer::Pipeline;
use std::error::Error;
use std::sync::{Arc, Mutex};
use chrono::Utc;
use gstreamer::{ClockTime, ElementFactory, PadProbeReturn, PadProbeType};
use gstreamer::prelude::{ElementExt, ElementExtManual, GstBinExtManual, GstObjectExt, ObjectExt, PadExt, PadExtManual};

fn main() {
    let pipeline = Pipeline::with_name("Vp8 Video Pipeline");
    //pipeline.use_clock(Some(&clock));
    /* ------------------- Creation of the elements  ------------------- */
    let video_caps = gstreamer::Caps::builder("application/x-rtp")
        .field("media", "video")
        .field("clock-rate", 90000)
        .field("encoding-name", "VP8")
        .field("payload", 96)
        .build();
    let udp_src_rtp_element = gstreamer::ElementFactory::make("udpsrc")
        .name("udpsrc").build()
        .expect("Failed to create udpsrc element");
    udp_src_rtp_element.set_property("address", &"127.0.0.1");
    udp_src_rtp_element.set_property("port", &video_port);
    udp_src_rtp_element.set_property("caps", &video_caps);
    let udp_rtcp_src_element = gstreamer::ElementFactory::make("udpsrc")
        .name("udpsrc_rtcp").build()
        .expect("Failed to create udpsrc element");
    udp_rtcp_src_element.set_property("address", &"127.0.0.1");
    udp_rtcp_src_element.set_property("port", &video_port + 1);
    let udp_rtcp_sink_element = gstreamer::ElementFactory::make("udpsink")
        .name("udpsink_rtcp").build()
        .expect("Failed to create udpsink element");
    udp_rtcp_sink_element.set_property("host", &"127.0.0.1");
    udp_rtcp_sink_element.set_property("port", &video_port + 1);
    udp_rtcp_sink_element.set_property("async", &false);
    udp_rtcp_sink_element.set_property("sync", &false);
    let rtp_bin_element = gstreamer::ElementFactory::make("rtpbin")
        .name("rtpbin").build()
        .expect("Failed to create rtpbin element");
    let queue_element = gstreamer::ElementFactory::make("queue")
        .name("queue")
        .build()
        .expect("Error creating queue_opus element");
    let rtp_v8_de_pay_element = gstreamer::ElementFactory::make("rtpvp8depay")
        .name("rtpvp8depay").build()
        .expect("Failed to create rtpvp8depay element");
    let output_pattern = String::from(format!("{}/video_chunk_%05d.ts", meeting_id));
    let muxer = ElementFactory::make("mpegtsmux")
        .name("mpegtsmux")
        .build()
        .expect("Error creating mpegtsmux element");
    let split_mux_sink_element = ElementFactory::make("splitmuxsink")
        .name("splitmuxsink")
        .property("location", output_pattern)
        // .property("max-size-bytes", 1000000u64)
        .property("max-size-time", ClockTime::from_seconds(5).nseconds())
        // .property("send-keyframe-requests", true)
        // .property("muxer-factory", "mp4mux")
        .property("muxer", &muxer)
        .build()
        .expect("Error creating splitmuxsink element");
    let meeting_id_clone = meeting_id.clone();
    let participant_id_clone = participant_id.clone();
    let plain_transport_id_clone = transport_id.clone();
    split_mux_sink_element.connect("format-location", false, move |values| {
        // Generate the custom filename with a timestamp
        let timestamp_nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards")
            .as_nanos();
        let filename = format!("{}/{}/{}/video_chunk_{}.ts", meeting_id_clone, participant_id_clone, plain_transport_id_clone, timestamp_nanos);

        // Return the filename
        Some(glib::Value::from(&filename))
    });
    /* ------------------- Add elements to the pipeline ------------------- */
    pipeline.add_many(&[
        &udp_src_rtp_element,
        &udp_rtcp_src_element,
        &udp_rtcp_sink_element,
        &rtp_bin_element,
        &queue_element,
        &rtp_v8_de_pay_element,
        &split_mux_sink_element
    ]).expect("Failed to add elements to the pipeline");
    /*---------------------------------------------------------------------*/

    /* ------------------- Start Linking of pads / elements ------------------- */
    udp_src_rtp_element.link(&rtp_bin_element).expect("Failed to link udpsrc and fakesink");
    let udp_rtcp_src_pad = udp_rtcp_src_element.static_pad("src")
        .expect("Failed to get src pad from udpsrc");
    let rtp_bin_rtcp_sink_pad = rtp_bin_element.request_pad_simple("recv_rtcp_sink_%u");
    udp_rtcp_src_pad.link(&rtp_bin_rtcp_sink_pad.unwrap()).expect("Failed to link udpsrc and rtpbin");
    let queue_element_arc = Arc::new(Mutex::new(queue_element.clone()));
    let queue_element_clone = Arc::clone(&queue_element_arc);
    rtp_bin_element.connect_pad_added(move |rtpbin, pad| {
        println!("Pad added: {}", pad.name());
        println!("{:#?}", pad);
        if pad.name().starts_with("recv_rtp_src_") {
            println!("{:#?}",pad.query_caps(None));
            println!("New pad added: {}", pad.name());

            // Get the sink pad of rtpopusdepay
            let queue_sink_pad = queue_element_clone.lock().unwrap().static_pad("sink")
                .expect("queue should have a sink pad");
            println!("Fake Sink Pad {:#?}",queue_sink_pad.query_caps(None));
            // Link the rtpbin pad to the depayloader
            if pad.link(&queue_sink_pad).is_ok() {
                println!("Successfully linked rtpbin to queue");
            } else {
                println!("Failed to link rtpbin to queue {:#?}", pad.link(&queue_sink_pad).err().unwrap());
            }
        } else if (pad.name().starts_with("recv_rtcp_src_")) {
            println!("{:#?}",pad.query_caps(None));
            println!("New pad added: {}", pad.name());

            // Get the sink pad of udp sink
            let udp_rtcp_sink_sink_pad = udp_rtcp_sink_element.static_pad("sink")
                .expect("udpsink should have a sink pad");
            println!("Udp Sink Pad {:#?}",udp_rtcp_sink_sink_pad.query_caps(None));
            pad.link(&udp_rtcp_sink_sink_pad).expect("Failed to link rtpbin to udpsink");
        }
    });
    if queue_element.link(&rtp_v8_de_pay_element).is_ok() {
        println!("Successfully linked queue and rtp_v8_de_pay");
    } else {
        println!("Failed to link queue and rtp_v8_de_pay");
        return Err("Failed to link queue and rtp_v8_de_pay".to_string());
    }
    let rtp_v8_de_pay_src_pad = rtp_v8_de_pay_element.static_pad("src")
        .expect("Failed to get src pad from rtp_v8_de_pay");
    let split_mux_sink_video_pad = match split_mux_sink_element.request_pad_simple("video") {
        Some(pad) => pad,
        None => {
            println!("Failed to get video pad from splitmuxsink");
            return Err("Failed to get video pad from splitmuxsink".to_string());
        }
    };
    if rtp_v8_de_pay_src_pad.link(&split_mux_sink_video_pad).is_ok() {
        println!("Successfully linked rtp_v8_de_pay and split mux");
    } else {
        println!("Failed to link rtp_v8_de_pay and split mux");
        return Err("Failed to link rtp_v8_de_pay and split mux".to_string());
    }
    /*---------------------------------------------------------------------*/

    /* ------------------- Debugging help like adding of probes ... ------------------- */

    /*---------------------------------------------------------------------*/
    Ok(pipeline)
}
