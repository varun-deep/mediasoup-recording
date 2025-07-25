use gstreamer as gst;
use gstreamer::prelude::*;
use std::error::Error;
use gstreamer::glib::Value;

fn main() -> Result<(), Box<dyn Error>> {
    // Initialize GStreamer
    gst::init()?;

    // Create the main pipeline
    let pipeline = gst::Pipeline::new();

    // Create elements
    let src1 = gst::ElementFactory::make("videotestsrc").build().unwrap();
    let src2 = gst::ElementFactory::make("videotestsrc").build().unwrap();
    let src3 = gst::ElementFactory::make("videotestsrc").build().unwrap();
    let src4 = gst::ElementFactory::make("videotestsrc").build().unwrap();
    let videobox1 = gst::ElementFactory::make("videobox").build().unwrap();
    videobox1.set_property("top", -3i32);
    videobox1.set_property("bottom", -3i32);
    videobox1.set_property("left", -3i32);
    videobox1.set_property("right", -3i32);
    videobox1.set_property_from_str("fill", &"green");
    let videobox2 = gst::ElementFactory::make("videobox").build().unwrap();
    videobox2.set_property("top", -3i32);
    videobox2.set_property("bottom", -3i32);
    videobox2.set_property("left", -3i32);
    videobox2.set_property("right", -3i32);
    videobox2.set_property_from_str("fill", &"green");
    let compositor = gst::ElementFactory::make("compositor").build().unwrap();
    let capsfilter = gst::ElementFactory::make("capsfilter").build().unwrap();
    let caps = gst::Caps::builder("video/x-raw")
        .field("width", 1280i32)
        .field("height", 720i32)
        .build();
    capsfilter.set_property("caps", &caps);
    let converter = gst::ElementFactory::make("videoconvert").build().unwrap();
    let x264_enc = gst::ElementFactory::make("x264enc").build().unwrap();
    let muxer = gst::ElementFactory::make("mpegtsmux").name(format!("mpegtsmux")).build().unwrap();
    let sink = gst::ElementFactory::make("splitmuxsink").build().unwrap();
    sink.set_property("muxer", &muxer);
    sink.set_property("location", "output%05d.mp4");
    sink.set_property("max-size-time", &gst::ClockTime::from_seconds(10));

    // Configure video test sources
    src1.set_property_from_str("pattern", &"red"); // SMPTE color bars
    src2.set_property_from_str("pattern", &"white"); // Snow pattern
    src3.set_property_from_str("pattern", &"black");
    src4.set_property_from_str("pattern", &"blue"); // Snow pattern
    let src1_pad = src1.static_pad("src").unwrap();
    let src2_pad = src2.static_pad("src").unwrap();
    let src3_pad = src3.static_pad("src").unwrap();
    let src4_pad = src4.static_pad("src").unwrap();
    // Set framerate and resolution for both sources
    let caps1 = gst::Caps::builder("video/x-raw")
        .field("width", 640i32)
        .field("height", 720i32)
        .field("framerate", gst::Fraction::new(30, 1))
        .build();
    let caps2 = gst::Caps::builder("video/x-raw")
        .field("width", 180i32)
        .field("height", 180i32)
        .field("framerate", gst::Fraction::new(30, 1))
        .build();

    // Add elements to pipeline
    pipeline.add_many(&[&src1, &src2, &src3, &src4, &videobox1, &videobox2, &compositor, &capsfilter, &converter, &x264_enc, &sink])?;

    // Link src1 to compositor
    let compositor_sink1 = compositor.request_pad_simple("sink_%u").unwrap();
    src1_pad.link(&compositor_sink1)?;

    // Configure first video position (left side)
    compositor_sink1.set_property("xpos", 0i32);
    compositor_sink1.set_property("ypos", 0i32);
    compositor_sink1.set_property("width", 640i32);
    compositor_sink1.set_property("height", 720i32);

    // Link src2 to compositor
    let compositor_sink2 = compositor.request_pad_simple("sink_%u").unwrap();
    src2_pad.link(&compositor_sink2)?;
    compositor_sink2.set_property("xpos", 640i32);  // position it next to first video
    compositor_sink2.set_property("ypos", 0i32);
    compositor_sink2.set_property("width", 640i32);
    compositor_sink2.set_property("height", 720i32);

    let compositor_sink3 = compositor.request_pad_simple("sink_%u").unwrap();
    src3_pad.link(&compositor_sink3)?;
    compositor_sink3.set_property("xpos", 10i32);
    compositor_sink3.set_property("ypos", 530i32);
    compositor_sink3.set_property("width", 180i32);
    compositor_sink3.set_property("height", 180i32);
    compositor_sink3.set_property("zorder", 1u32);

    let compositor_sink4 = compositor.request_pad_simple("sink_%u").unwrap();
    src4_pad.link(&compositor_sink4)?;
    compositor_sink4.set_property("xpos", 200i32); // 10 + 180 + 10
    compositor_sink4.set_property("ypos", 530i32);
    compositor_sink4.set_property("width", 180i32);
    compositor_sink4.set_property("height", 180i32);
    compositor_sink4.set_property("zorder", 1u32);

    // Link compositor to converter and sink
    compositor.link(&capsfilter)?;
    capsfilter.link(&converter)?;
    converter.link(&x264_enc)?;
    let x264_src_pad = x264_enc.static_pad("src").unwrap();
    let sink_video_pad = sink.request_pad_simple("video").unwrap();
    x264_src_pad.link(&sink_video_pad)?;

    // Set up caps filters for the sources
    let capsfilter1 = gst::ElementFactory::make("capsfilter").build().unwrap();
    let capsfilter2 = gst::ElementFactory::make("capsfilter").build().unwrap();
    let capsfilter3 = gst::ElementFactory::make("capsfilter").build().unwrap();
    let capsfilter4 = gst::ElementFactory::make("capsfilter").build().unwrap();

    capsfilter1.set_property("caps", &caps1);
    capsfilter2.set_property("caps", &caps1);
    capsfilter3.set_property("caps", &caps2);
    capsfilter4.set_property("caps", &caps2);

    // Insert caps filters into pipeline
    pipeline.add_many(&[&capsfilter1, &capsfilter2, &capsfilter3, &capsfilter4])?;

    // Relink with caps filters
    src1_pad.unlink(&compositor_sink1).expect("Failed to unlink src1 from compositor_sink1");
    src2_pad.unlink(&compositor_sink2).expect("Failed to unlink src2 from compositor_sink2");
    src3_pad.unlink(&compositor_sink3).expect("Failed to unlink src2 from compositor_sink2");
    src4_pad.unlink(&compositor_sink4).expect("Failed to unlink src2 from compositor_sink2");

    src1.link(&capsfilter1)?;
    capsfilter1.link_pads(Some("src"), &compositor, Some("sink_0"))?;
    src2.link(&capsfilter2)?;
    capsfilter2.link_pads(Some("src"), &compositor, Some("sink_1"))?;

    src3.link(&videobox1)?;
    videobox1.link(&capsfilter3)?;
    capsfilter3.link_pads(Some("src"), &compositor, Some("sink_2"))?;

    src4.link(&videobox2)?;
    videobox2.link(&capsfilter4)?;
    capsfilter4.link_pads(Some("src"), &compositor, Some("sink_3"))?;

    // Set compositor output size
    compositor.set_property_from_str("background", "black"); // Black background

    // Set pipeline to playing state
    pipeline.set_state(gst::State::Playing)?;

    // Wait for error or EOS
    let bus = pipeline.bus().unwrap();
    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        match msg.view() {
            gst::MessageView::Error(err) => {
                eprintln!("Error: {}", err.error());
                break;
            }
            gst::MessageView::Eos(..) => {
                println!("End of stream");
                break;
            }
            _ => {}
        }
    }

    // Clean up
    pipeline.set_state(gst::State::Null)?;

    Ok(())
}

// Add this to your Cargo.toml:
// [dependencies]
// gstreamer = "0.21"