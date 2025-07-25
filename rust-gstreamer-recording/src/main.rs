use std::{fs, thread};
use std::iter::Iterator;
use gstreamer::prelude::*;
use gstreamer::MessageView;
use gstreamer::*;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use aws_config::{BehaviorVersion, Region};
use aws_sdk_s3::Client;
use ctrlc;
use gstreamer::MessageType::Element;
use gstreamer_app::AppSrc;
use gstreamer_sdp::{SDPConnection, SDPMedia, SDPMessage};
use tokio::runtime::Runtime;

async fn upload_file_to_s3(client: &Client, bucket: &str, file_path: &str, file_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let body = aws_sdk_s3::primitives::ByteStream::from_path(file_path).await?;
    client.put_object()
        .bucket(bucket)
        .key(file_name)
        .body(body)
        .send()
        .await?;
    Ok(())
}

fn upload_to_s3(bucket: String, directory: String, s3_client: Client) {

    loop {
        // Read the current list of files in the directory
        let current_files = fs::read_dir(&directory)
            .unwrap()
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .collect::<Vec<_>>();

        // Find new files that have not been uploaded yet
        let new_files = current_files.iter()
            .collect::<Vec<_>>();

        for file in new_files {
            let file_name = file.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown")
                .to_string();

            let bucket = bucket.clone();
            let file_path = file.to_str().unwrap().to_string();
            let s3_client = s3_client.clone();

            // Upload the file asynchronously
            tokio::task::spawn(async move {
                match upload_file_to_s3(&s3_client, &bucket, &file_path, &file_name).await {
                    Ok(_) => {
                        println!("Uploaded file: {}", file_name);

                        // Delete the file after successful upload
                        if let Err(err) = fs::remove_file(&file_path) {
                            eprintln!("Failed to delete file {}: {}", file_name, err);
                        } else {
                            println!("Deleted file: {}", file_name);
                        }
                    }
                    Err(err) => {
                        eprintln!("Failed to upload {}: {}", file_name, err);
                    }
                }
            });
        }

        // Sleep for a short duration before checking again
        thread::sleep(Duration::from_secs(5));
    }
}


fn start_recording_gstreamer() -> Result<Pipeline, Box<dyn std::error::Error>> {
    // let sdp_path = String::from("input-h264.sdp");
    let output_pattern = String::from("recording/chunk_%05d.mp4");

    gstreamer::init()?;
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
    audio_media.set_port_info(60000, 1);
    audio_media.set_proto("RTP/AVPF");
    audio_media.add_format("111");
    audio_media.add_attribute("rtpmap", Some("111 opus/48000/2"));
    audio_media.add_attribute("fmtp", Some("111 minptime=10;useinbandfec=1"));
    audio_media.add_attribute("rtcp", Some("60001"));
    sdp_message_audio.add_media(audio_media);
    let audio_caps = Caps::builder("application/x-rtp")
        .field("media", &"audio")
        .field("clock-rate", &48000)
        .field("encoding-name", &"opus")
        .field("payload", &111)
        .build();

    let mut sdp_message_video = SDPMessage::new();
    sdp_message_video.set_origin("-", "0", "0", "IN", "IP4", "127.0.0.1");
    sdp_message_video.set_session_name("-");
    let conn_info = SDPConnection::new("IN", "IP4", "127.0.0.1", 64, 0);
    sdp_message_video.set_connection(conn_info.nettype().unwrap(),
                                     conn_info.addrtype().unwrap(),
                                     conn_info.address().unwrap(),
                                     conn_info.ttl(),
                                     conn_info.addr_number());
    let mut video_media = SDPMedia::new();
    video_media.set_media("video");
    video_media.set_port_info(50000, 1);
    video_media.set_proto("RTP/AVPF");
    video_media.add_format("125");
    video_media.add_attribute("rtpmap", Some("125 H264/90000"));
    video_media.add_attribute("fmtp", Some("125 profile-level-id=42e01f;packetization-mode=1"));
    video_media.add_attribute("rtcp", Some("50001"));
    sdp_message_video.add_media(video_media);
    let video_caps = Caps::builder("application/x-rtp")
        .field("media", &"video")
        .field("clock-rate", &90000)
        .field("encoding-name", &"H264")
        .field("payload", &125)
        .build();

    let pipeline = Pipeline::with_name("gstreamer-pipeline");
    /*let file_src = ElementFactory::make("filesrc")
        .name("filesrc")
        .property("location", sdp_path)
        .build()
        .expect("Error creating file_src element");*/

    let appsrc_video = ElementFactory::make("appsrc")
        .name("appsrc_video")
        .property("is-live", &true)
        .property("do-timestamp", &true)
        .property("format", Format::Time)
        .build()
        .expect("Error creating appsrc element");
    appsrc_video.set_property("caps", Some(&video_caps));

    let appsrc_audio = ElementFactory::make("appsrc")
        .name("appsrc_audio")
        .property("is-live", &true)
        .property("do-timestamp", &true)
        .property("format", Format::Time)
        .build()
        .expect("Error creating appsrc element");
    appsrc_audio.set_property("caps", Some(&audio_caps));

    let sdp_audio_bytes = sdp_message_audio.as_text().expect("Failed to convert SDP message to text").as_bytes().to_vec();
    let sdp_video_bytes = sdp_message_video.as_text().expect("Failed to convert SDP message to text").as_bytes().to_vec();
    let appsrc_audio_downcast = appsrc_audio.clone().dynamic_cast::<AppSrc>().unwrap();
    let appsrc_video_downcast = appsrc_video.clone().dynamic_cast::<AppSrc>().unwrap();
    /*appsrc_downcast.set_callbacks(
        gstreamer_app::AppSrcCallbacks::builder()
            .need_data(move |src, _| {
                // Create a GStreamer buffer from the SDP data
                let buffer = Buffer::from_slice(sdp_bytes.clone());
                let _ = match src.push_buffer(buffer) {
                    Err(err) => {
                        eprintln!("Failed to push buffer: {}", err);
                        gstreamer::FlowReturn::Error
                    }
                    Ok(_) => gstreamer::FlowReturn::Ok,
                };
                if let Err(err) = src.end_of_stream() {
                    eprintln!("Failed to send EOS: {:?}", err);
                }
            })
            .build(),
    );*/

    println!("Building sdp-demux");
    let sdp_demux = ElementFactory::make("sdpdemux")
        .name("sdpdemux")
        .property("timeout", 0u64)
        .build()
        .expect("Error creating sdp_demux element");

    let splitmuxsink = ElementFactory::make("splitmuxsink")
        .name("splitmuxsink")
        .property("location", output_pattern)
        // .property("max-size-bytes", 1000000u64)
        .property("max-size-time", ClockTime::from_seconds(10).nseconds()) // Split every 10 seconds is the config but idk why it splits when the first chunk is 3 mins and then all the subsequent chunks get split at 129 seconds (2:09 mins)
        // .property("muxer-factory", "mp4mux")
        .build()
        .expect("Error creating splitmuxsink element");

    let queue_opus = ElementFactory::make("queue")
        .name("queue_opus")
        .build()
        .expect("Error creating queue_opus element");

    let jitter_buffer_audio = ElementFactory::make("rtpjitterbuffer")
        .name("jitter_buffer_audio")
        .build()
        .expect("Error creating jitter_buffer_audio element");
    jitter_buffer_audio.set_property("do-lost", true);

    let identity_audio = ElementFactory::make("identity")
        .name("identity_audio")
        .build()
        .expect("Error creating identity_audio element");
    identity_audio.set_property("sync", true);

    //jitter_buffer_audio.set_property("latency", 200);
    let rtp_opus_depay = ElementFactory::make("rtpopusdepay")
        .name("rtpopusdepay")
        .build()
        .expect("Error creating rtp_opus_depay element");

    let opus_parse = ElementFactory::make("opusparse")
        .name("opusparse")
        .build()
        .expect("Error creating opus_parse element");

    let queue_opus_split = ElementFactory::make("queue")
        .name("queue_opus_split")
        .build()
        .expect("Error creating queue_opus element");

    let queue_h264 = ElementFactory::make("queue")
        .name("queue_h264")
        .build()
        .expect("Error creating queue_h264 element");

    let jitter_buffer_video = ElementFactory::make("rtpjitterbuffer")
        .name("jitter_buffer_video")
        .build()
        .expect("Error creating jitter_buffer_video element");
    jitter_buffer_video.set_property("do-lost", true);

    let identity_video = ElementFactory::make("identity")
        .name("identity_video")
        .build()
        .expect("Error creating identity_video element");
    identity_video.set_property("sync", true);

    let rtp_h264_depay = ElementFactory::make("rtph264depay")
        .name("rtph264depay")
        .build()
        .expect("Error creating rtp_h264_depay element");

    let h264_parse = ElementFactory::make("h264parse")
        .name("h264parse")
        .build()
        .expect("Error creating h264_parse element");

    let queue_h264_split = ElementFactory::make("queue")
        .name("queue_h264_split")
        .build()
        .expect("Error creating queue_opus element");

    pipeline.add_many(&[
        // &file_src,
        &appsrc_audio, &appsrc_video,
        &sdp_demux, &jitter_buffer_audio, &jitter_buffer_video,
        &identity_audio, &queue_opus, &rtp_opus_depay, &opus_parse, &queue_opus_split,
        &identity_video, &queue_h264, &rtp_h264_depay, &h264_parse, &queue_h264_split,
        &splitmuxsink
    ]).expect("Failed to add elements to pipeline");

    appsrc_audio_downcast.set_callbacks(
        gstreamer_app::AppSrcCallbacks::builder()
            .need_data(move |src, _| {
                // Create a GStreamer buffer from the SDP data
                if let Err(err) = src.push_buffer(Buffer::from_slice(sdp_audio_bytes.clone())) {
                    println!("Failed to push buffer: {}", err);
                }
            })
            .build(),
    );
    appsrc_video_downcast.set_callbacks(
        gstreamer_app::AppSrcCallbacks::builder()
            .need_data(move |src, _| {
                // Create a GStreamer buffer from the SDP data
                if let Err(err) = src.push_buffer(Buffer::from_slice(sdp_video_bytes.clone())) {
                    println!("Failed to push buffer: {}", err);
                }
            })
            .build(),
    );
    appsrc_audio.link(&sdp_demux)?;
    // appsrc_video.link(&sdp_demux)?;

    // gstreamer::Element::link_many(&[&appsrc, &sdp_demux])
    //     .expect("Error linking file_src and sdp_demux elements");
    // gstreamer::Element::link_many(&[&file_src , &sdp_demux])
    //     .expect("Error linking file_src and sdp_demux elements");

    let audio_sink = splitmuxsink.request_pad_simple("audio_%u").unwrap();
    let video_sink = splitmuxsink.request_pad_simple("video").unwrap();
    let start_time = Instant::now();

    sdp_demux.connect_pad_added(move |_, src_pad| {
        println!("Pad added: {}", src_pad.name());
        println!("{:#?}", src_pad);

        let caps = src_pad.query_caps(None);
        println!("{:#?}", caps);

        let media_type = caps.structure(0)
            .and_then(|s| s.get::<&str>("media").ok())
            .expect("Error getting structure");

        println!("New pad added with media type: {}", media_type);
        src_pad.add_probe(gstreamer::PadProbeType::BUFFER, move |_pad, probe_info| {
            if let Some(mut buffer) = probe_info.buffer_mut() {
                if buffer.pts().is_none() {
                    let elapsed = ClockTime::from_nseconds(start_time.elapsed().as_nanos() as u64);
                    buffer.make_mut().set_pts(elapsed);
                    println!("Assigned PTS: {:?}", elapsed);
                }
            }
            PadProbeReturn::Ok
        });

        video_sink.add_probe(gstreamer::PadProbeType::BUFFER, move |_pad, probe_info| {
            if let Some(mut buffer) = probe_info.buffer_mut() {
                if buffer.pts().is_none() {
                    let elapsed = ClockTime::from_nseconds(start_time.elapsed().as_nanos() as u64);
                    buffer.make_mut().set_pts(elapsed);
                    println!("Assigned PTS: {:?}", elapsed);
                }
            }
            PadProbeReturn::Ok
        });

        audio_sink.add_probe(gstreamer::PadProbeType::BUFFER, move |_pad, probe_info| {
            if let Some(mut buffer) = probe_info.buffer_mut() {
                if buffer.pts().is_none() {
                    let elapsed = ClockTime::from_nseconds(start_time.elapsed().as_nanos() as u64);
                    buffer.make_mut().set_pts(elapsed);
                    println!("Assigned PTS: {:?}", elapsed);
                }
            }
            PadProbeReturn::Ok
        });

        if media_type == "audio" {
            let sink_pad = jitter_buffer_audio
                .static_pad("sink")
                .unwrap();

            match src_pad.link(&sink_pad) {
                Ok(_) => println!("Linked audio pad to queue_opus"),
                Err(err) => {
                    eprintln!("Failed to link audio pad: {}", err);
                    return;
                }
            }

            match gstreamer::Element::link_many(&[
                &jitter_buffer_audio,
                &identity_audio,
                &queue_opus,
                &rtp_opus_depay,
                &opus_parse,
                &queue_opus_split,
            ]) {
                Ok(_) => println!("Successfully linked Opus branch."),
                Err(err) => eprintln!("Failed to link Opus branch: {}", err),
            }

            let opus_src = queue_opus_split.static_pad("src").expect("Failed to get opus_parse src pad");
            opus_src.link(&audio_sink).expect("Failed to link opus to splitmuxsink");
        } else if media_type == "video" {
            let sink_pad = jitter_buffer_video
                .static_pad("sink")
                .unwrap();
            match src_pad.link(&sink_pad) {
                Ok(_) => println!("Linked video pad to queue_h264."),
                Err(err) => {
                    eprintln!("Failed to link video pad: {}", err);
                    return;
                }
            }

            match gstreamer::Element::link_many(&[
                &jitter_buffer_video,
                &identity_video,
                &queue_h264,
                &rtp_h264_depay,
                &h264_parse,
                &queue_h264_split,
            ]) {
                Ok(_) => println!("Successfully linked H264 branch."),
                Err(err) => eprintln!("Failed to link H264 branch: {}", err),
            }
            // video_aux_%u
            // video
            let h264_src = queue_h264_split.static_pad("src").unwrap();
            h264_src.link(&video_sink).expect("Failed to link h264 to splitmuxsink");
        }
    });
    // appsrc_downcast.push_buffer(Buffer::from_slice(sdp_bytes.clone()))?;

    Ok(pipeline)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::env::set_var("GST_DEBUG", "3");
    // start time
    let now = std::time::SystemTime::now();
    let now = now.duration_since(std::time::SystemTime::UNIX_EPOCH).unwrap();
    let now = now.as_secs();

    println!("Starting GStreamer recording...");

    let pipeline = start_recording_gstreamer()?;
    pipeline.set_state(State::Playing)?;
    // let s3_client = aws_sdk_s3::Client::new(&aws_config::load_defaults(BehaviorVersion::latest()).await);
    // upload_to_s3("dd-bucket-v1".to_string(), "recording".to_string(), s3_client);

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
    // end time
    let end = std::time::SystemTime::now();
    let end = end.duration_since(std::time::SystemTime::UNIX_EPOCH).unwrap();
    let end = end.as_secs();

    let duration = end - now;
    println!("Duration: {} seconds", duration);
    Ok(())
}
