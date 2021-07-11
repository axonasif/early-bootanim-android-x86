use flate2::read::ZlibDecoder;
pub use framebuffer::Framebuffer;
use likemod::errors;
use std::{
    io::{self, Read, Seek, Write},
    os::unix::process::CommandExt,
    *,
};
use sysinfo::*;

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

pub fn read_u32<R: io::Read>(r: &mut R) -> Result<u32> {
    let buf = &mut [0, 0, 0, 0];
    r.read_exact(buf)?;
    Ok(
        ((buf[3] as u32) << 24)
            + ((buf[2] as u32) << 16)
            + ((buf[1] as u32) << 8)
            + (buf[0] as u32),
    )
}

pub fn read_frames(mut animdata: &'static [u8]) -> Result<(usize, usize, usize, Vec<u8>, usize)> {
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

pub trait FramebufferExt {
    fn writer(&mut self, width: usize, height: usize) -> FbWriter;
    fn write(&mut self, offset: usize, data: &[u8]) -> io::Result<()>;
    fn write_loop<R, F>(&mut self, width: usize, height: usize, render_frame: F) -> Option<R>
    where
        F: FnMut(&mut FbWriter) -> Option<R>;
}

pub struct FbWriter<'a> {
    fb: &'a mut framebuffer::Framebuffer,
    offset: usize,
    width: usize,
    height: usize,
}

impl FramebufferExt for Framebuffer {
    fn writer(&mut self, width: usize, height: usize) -> FbWriter {
        let x = (self.var_screen_info.xres as usize - width) / 2;
        let y = (self.var_screen_info.yres as usize - height) / 2;
        let bytes_per_pixel = self.var_screen_info.bits_per_pixel as usize / 8;
        let offset = (y + self.var_screen_info.yoffset as usize)
            * self.fix_screen_info.line_length as usize
            + (x + self.var_screen_info.xoffset as usize) * bytes_per_pixel;
        FbWriter {
            fb: self,
            offset: offset,
            width: width * bytes_per_pixel,
            height: height,
        }
    }
    fn write_loop<R, F>(&mut self, width: usize, height: usize, mut render_frame: F) -> Option<R>
    where
        F: FnMut(&mut FbWriter) -> Option<R>,
    {
        // Exit on existence of android bootanimation
        let _handle_one = thread::spawn(|| loop {
//             if !path::Path::new("/android").exists() {
                for (_pid, process) in System::new_all().get_processes() {
                    if process.name() == "bootanimation" || process.name() == "gearinit" {
                        thread::sleep(time::Duration::from_secs(6));
                        std::process::exit(0);
                    }
                }
//             } else {
//                 break;
//             }
        });

        let mut writer = self.writer(width, height);
        let dur = time::Duration::from_millis(1000 / 30);
        /*
        println!("{}", termion::cursor::Hide);
        println!("{}", termion::clear::All);
        */

        loop {
            let next = time::Instant::now() + dur;
            match render_frame(&mut writer) {
                Some(r) => return Some(r),
                None => (),
            };
            let now = time::Instant::now();
            if now < next {
                thread::sleep(next - now);
            }
        }
    }
    fn write(&mut self, offset: usize, data: &[u8]) -> io::Result<()> {
        self.device.seek(io::SeekFrom::Start(offset as u64))?;
        self.device.write_all(data)
    }
}

impl<'a> FbWriter<'a> {
    pub fn write(&mut self, frame: &[u8]) -> io::Result<()> {
        let mut offset = self.offset;
        let mut input = 0;
        for _ in 0..self.height {
            self.fb.write(offset, &frame[input..input + self.width])?;
            input += self.width;
            offset += self.fb.fix_screen_info.line_length as usize;
        }
        Ok(())
    }
}

fn load_modfile(modpath: &str) -> errors::Result<()> {
    // Get a file descriptor to the kernel module object.
    let fmod = std::fs::File::open(path::Path::new(modpath))?;

    // Assemble module parameters for loading.
    let mut params = likemod::ModParams::new();
    params.insert("bus_delay".to_string(), likemod::ModParamValue::Int(5));

    // Try to load the module.
    let loader = likemod::ModLoader::default().set_parameters(params);
    loader.load_module_file(&fmod)
}

pub fn playanim() {
    if !path::Path::new("/android").exists() {
        loop {
            if path::Path::new("/sys/module/drm_kms_helper").exists() {
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
                    if let Err(_) = load_modfile(&mod_path) {
                        {
                            println!("early-bootanim: Failed to load overlay kernel modules");
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
        if path::Path::new("/dev/fb0").exists() {
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
        if let Err(_) = writer.write(&frames[i * frame_size..(i + 1) * frame_size]) {
            {
                if path::Path::new("/anim").exists() && path::Path::new("/android").exists() {
                    thread::sleep(time::Duration::from_secs(2));
                    process::Command::new("/anim").exec();
                } else {
                    process::exit(1);
                }
            }
        }
        i += 1;
        if i == nframes {
            i = 0;
        }
        None as Option<()>
    });
}
