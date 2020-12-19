mod drawfb;
use drawfb::{Framebuffer, FramebufferExt};
use flate2::read::ZlibDecoder;
use likemod::errors;
use std::{fmt, fs, io, io::Read, path::Path, process, result};

#[derive(Debug)]
pub enum ErrorKind {
    Io(io::Error),
    UnexpectedEof,
    ExpectedEof,
}

#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
}

pub type Result<T> = result::Result<T, Error>;

impl Into<Error> for ErrorKind {
    fn into(self) -> Error {
        Error { kind: self }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Error {
        if e.kind() == io::ErrorKind::UnexpectedEof {
            ErrorKind::UnexpectedEof.into()
        } else {
            ErrorKind::Io(e).into()
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.kind {
            ErrorKind::Io(ref e) => write!(f, "{}", e),
            ErrorKind::UnexpectedEof => write!(f, "Unexpected end-of-file"),
            ErrorKind::ExpectedEof => write!(f, "Expected end-of-file"),
        }
    }
}

fn read_u32<R: io::Read>(r: &mut R) -> Result<u32> {
    let buf = &mut [0, 0, 0, 0];
    r.read_exact(buf)?;
    Ok(
        ((buf[3] as u32) << 24)
            + ((buf[2] as u32) << 16)
            + ((buf[1] as u32) << 8)
            + (buf[0] as u32),
    )
}

fn read_frames(mut animdata: &'static [u8]) -> Result<(usize, usize, usize, Vec<u8>, usize)> {
    let nframes = read_u32(&mut animdata)? as usize;
    let height = read_u32(&mut animdata)? as usize;
    let width = read_u32(&mut animdata)? as usize;
    let bpp = read_u32(&mut animdata)? as usize;
    let frame_size = height * width * bpp;
    let mut frames = vec![0; nframes * frame_size];
    let mut decoder = ZlibDecoder::new(animdata);
    decoder.read_exact(&mut frames)?;
    if decoder.read(&mut [0])? != 0 {
        return Err(ErrorKind::ExpectedEof.into());
    }
    Ok((nframes, height, width, frames, frame_size))
}

macro_rules! unwrap_or_exit {
    ( $x:expr, $msg:expr ) => {
        match $x {
            Ok(v) => v,
            Err(e) => {
                eprintln!($msg, e);
                process::exit(1);
            }
        }
    };
}

fn load_modfile(modpath: &str) -> errors::Result<()> {
    // Get a file descriptor to the kernel module object.
    let fmod = std::fs::File::open(Path::new(modpath))?;

    // Assemble module parameters for loading.
    let mut params = likemod::ModParams::new();
    params.insert("bus_delay".to_string(), likemod::ModParamValue::Int(5));

    // Try to load the module.
    let loader = likemod::ModLoader::default().set_parameters(params);
    loader.load_module_file(&fmod)
}

fn main() {
    /*
    // Handle ctrl + c signal
    ctrlc::set_handler(move || {
        println!("{}{}", termion::clear::All, termion::cursor::Show);
        println!("Received Ctrl+C!");
        process::exit(1);
    })
    .expect("Error setting Ctrl-C handler");
    */

    if !Path::new("/android").exists() {
        loop {
            if Path::new("/sys/module/drm_kms_helper").exists() {
                // Load overlay kernel modules
                let mut kernel_release = match fs::read_to_string("/proc/sys/kernel/osrelease") {
                    Ok(ok_result) => ok_result,
                    Err(_) => {
                        process::exit(1);
                    }
                };

                kernel_release.pop(); // Remove newline char

                for module in ["connector/cn.ko", "video/fbdev/uvesafb.ko"].iter() {
                    let mod_path = format!(
                        "/system/lib/modules/{}/kernel/drivers/{}",
                        kernel_release, module
                    );
                    match load_modfile(&mod_path) {
                        Ok(_) => {}
                        Err(_) => {
                            println!("rusty-magisk: Failed to load overlay kernel modules");
                            process::exit(1);
                        }
                    }
                }

                break;
            }
        }
    }

    let animdata: &'static [u8] = include_bytes!("anim.bin");

    let (nframes, height, width, frames, frame_size) = unwrap_or_exit!(
        read_frames(animdata),
        "anim.bin is in the wrong format ({})"
    );

    let fb_dev = {
        if Path::new("/dev/fb0").exists() {
            "/dev/fb0"
        } else {
            "/dev/graphics/fb0"
        }
    };

    let mut fb = unwrap_or_exit!(
        Framebuffer::new(fb_dev),
        "Could not open framebuffer device ({})"
    );

    let mut i = 0;
    fb.write_loop(width, height, |writer| {
        match writer.write(&frames[i * frame_size..(i + 1) * frame_size]) {
            Ok(_) => {}
            Err(_) => {
                process::exit(1);
            }
        }
        i += 1;
        if i == nframes {
            i = 0;
        }
        None as Option<()>
    });
}
