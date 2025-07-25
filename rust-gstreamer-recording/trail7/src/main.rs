use cairo::{Context, Format, ImageSurface};
use gstreamer::prelude::*;
use gstreamer_app::AppSrc;
use std::{thread, time::Duration};

const WIDTH: i32 = 1280;
const HEIGHT: i32 = 720;
const FPS: u64 = 30;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize GStreamer
    gstreamer::init()?;

    // Create the GStreamer pipeline
    let pipeline = gstreamer::parse::launch(
        "appsrc name=src ! videoconvert ! x264enc tune=zerolatency ! mp4mux ! filesink location=whiteboard.mp4"
    )?;
    let pipeline = pipeline.downcast::<gstreamer::Pipeline>().unwrap();

    let appsrc = pipeline
        .by_name("src")
        .unwrap()
        .downcast::<AppSrc>()
        .unwrap();

    // Correct caps: BGRx = 32-bit, matches Cairo's ARgb32 layout
    let caps = gstreamer::Caps::builder("video/x-raw")
        .field("format", "BGRx")
        .field("width", WIDTH)
        .field("height", HEIGHT)
        .field("framerate", gstreamer::Fraction::new(FPS as i32, 1))
        .build();
    appsrc.set_caps(Some(&caps));
    appsrc.set_property("format", &gstreamer::Format::Time);

    // Start pipeline
    pipeline.set_state(gstreamer::State::Playing)?;

    let duration = gstreamer::ClockTime::SECOND / FPS;
    println!("Starting recording to whiteboard.mp4 at {} FPS", FPS);

    for frame_number in 0..150 {
        //println!("Rendering frame {}", frame_number);

        // Cairo surface with ARgb32 format (matches BGRx layout)
        let mut surface = ImageSurface::create(Format::ARgb32, WIDTH, HEIGHT)?;
        {
            let cr = Context::new(&surface)?;

            // Background
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.paint()?;

            // Red stroke
            cr.set_source_rgb(1.0, 0.0, 0.0);
            cr.set_line_width(5.0);
            cr.move_to(100.0, 100.0);
            cr.line_to(100.0 + frame_number as f64 * 5.0, 500.0);

            cr.stroke()?;
        }

        surface.flush(); // Ensure pixels are committed

        // Copy data safely from surface
        let data = {
            let surface_data = surface.data()?; // borrow temporarily
            surface_data.as_ref().to_vec()      // make owned copy
        };

        // Push to GStreamer
        let pts = duration * frame_number;
        let mut gst_buffer = gstreamer::Buffer::with_size(data.len())?;
        {
            let buffer_mut = gst_buffer.get_mut().unwrap();
            let mut map = buffer_mut.map_writable()?;
            map.copy_from_slice(&data);
        }

        {
            let buffer_mut = gst_buffer.get_mut().unwrap();
            buffer_mut.set_pts(pts);
            buffer_mut.set_duration(duration);
        }

        let flow = appsrc.push_buffer(gst_buffer);
        if flow != Ok(gstreamer::FlowSuccess::Ok) {
            eprintln!("Error pushing buffer: {:?}", flow);
            break;
        }

        thread::sleep(Duration::from_millis(1000 / FPS));
    }

    // End the stream
    appsrc.end_of_stream()?;

    // Wait for EOS
    pipeline
        .bus()
        .unwrap()
        .timed_pop_filtered(
            gstreamer::ClockTime::NONE,
            &[gstreamer::MessageType::Eos, gstreamer::MessageType::Error],
        );

    pipeline.set_state(gstreamer::State::Null)?;
    println!("Recording complete: whiteboard.mp4");
    Ok(())
}