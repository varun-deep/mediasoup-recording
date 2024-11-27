use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use ctrlc;

fn start_recording_gstreamer() -> Result<std::process::Child, Box<dyn std::error::Error>> {
    
    /*
    gst-launch-1.0 --eos-on-shutdown 
    filesrc location=./input-h264.sdp 
    ! sdpdemux timeout=0 name=demux 
    mp4mux faststart=true faststart-file=./output.mp4.tmp name=mux 
    ! filesink location=./output.mp4 
    demux. 
    ! queue       
    ! rtpopusdepay       
    ! opusparse       
    ! mux. 
    demux. 
    ! queue         
    ! rtph264depay         
    ! h264parse         
    ! mux.
    */

    let gst_launch_args = [
        "--eos-on-shutdown",
        "filesrc", "location=/Users/varundeepsaini/RustroverProjects/rust-gstreamer-recording/input-h264.sdp",
        "!", "sdpdemux", "timeout=0", "name=demux",
        "mp4mux", "faststart=true", "faststart-file=/Users/varundeepsaini/RustroverProjects/rust-gstreamer-recording/output.mp4.tmp", "name=mux",
        "!", "filesink", "location=/Users/varundeepsaini/RustroverProjects/rust-gstreamer-recording/output.mp4",
        "demux.",
        "!", "queue",
        "!", "rtpopusdepay",
        "!", "opusparse",
        "!", "mux.",
        "demux.",
        "!", "queue",
        "!", "rtph264depay",
        "!", "h264parse",
        "!", "mux.",
    ];

    
    let mut cmd = Command::new("gst-launch-1.0");
    cmd.args(&gst_launch_args)
        .stdout(Stdio::piped()) 
        .stderr(Stdio::piped()) 
        .env("GST_DEBUG", "3"); 

    let mut child = cmd.spawn()?;

    if let Some(stdout) = child.stdout.take() {
        let tx = setup_logger("STDOUT".to_string());
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                if let Ok(line) = line {
                    tx.send(line).expect("Failed to send stdout line");
                }
            }
        });
    }

    if let Some(stderr) = child.stderr.take() {
        let tx = setup_logger("STDERR".to_string());
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                if let Ok(line) = line {
                    tx.send(line).expect("Failed to send stderr line");
                }
            }
        });
    }

    Ok(child)
}

fn setup_logger(prefix: String) -> std::sync::mpsc::Sender<String> {
    let (tx, rx) = std::sync::mpsc::channel();

    thread::spawn(move || {
        for line in rx {
            println!("[{}] {}", prefix, line);
        }
    });

    tx
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting GStreamer recording...");

    let child = start_recording_gstreamer()?;
    let child = Arc::new(Mutex::new(child));

    let child_clone = Arc::clone(&child);

    ctrlc::set_handler(move || {
        println!("Received Ctrl+C, terminating GStreamer recording...");

        let mut child = child_clone.lock().unwrap();

        #[cfg(unix)]{
            let pid = child.id() as i32;
            if let Err(e) = kill(Pid::from_raw(pid), Signal::SIGTERM) {
                eprintln!("Failed to send SIGTERM to process {}: {}", pid, e);
            } else {
                println!("SIGTERM sent to process {}", pid);
            }
        }
    })?;

    let child_for_wait = Arc::clone(&child);
    let handle = thread::spawn(move || {
        let mut child = child_for_wait.lock().unwrap();
        match child.wait() {
            Ok(status) => {
                if status.success() {
                    println!("GStreamer recording finished successfully.");
                } else {
                    println!(
                        "GStreamer recording exited with status: {}",
                        status
                    );
                }
            }
            Err(e) => {
                println!("Failed to wait on child process: {}", e);
            }
        }
    });

    handle.join().expect("Failed to join recording thread");

    println!("Exiting application.");

    Ok(())
}
