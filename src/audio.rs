use fehler::throws;
//pub use alsa::Error;
//pub type Result<T=()> = alsa::Result<T>;
pub use std::io::Error;
pub type Result<T=(), E=Error> = std::result::Result<T,E>;

const INTERVAL_FLAG_INTEGER : u32 = 1<<2;
#[derive(Default,Clone,Copy)] struct Interval { min: u32, max: u32, flags : u32 }
impl From<u32> for Interval { fn  from(value: u32) -> Self { Self{min: value, max: value, flags: INTERVAL_FLAG_INTEGER} } }
impl From<Interval> for u32 { fn from(Interval{min, max, ..}: Interval) -> u32 { assert_eq!(min, max); return max; } }

#[derive(Default)]
struct HWParams {
    flags: f32, // = NoResample;
    masks: [[u32; 8]; 3],
    mres: [[u32; 8]; 5],
    intervals: [Interval; 12],
    ires: [Interval;  9],
    rmask: u32, cmask: u32, info: u32, msbits: u32, rate_num: u32, rate_den: u32, //=~0, cmask=0, info=0, msbits=0, rate_num=0, rate_den=0;
    fifo_size: u64, //=0;
    reserved: [u32; 16],
}
impl HWParams { fn new() -> Self { Self{rmask: !0, ..Default::default()}}}

const MASK_ACCESS: usize = 0;
const MASK_FORMAT: usize = 1;
const MASK_SUBFORMAT: usize = 2;

const ACCESS_MMAP_INTERLEAVED : u32 = 1<<0;
const FORMAT_S16_LE : u32 = 1<<2;
const SUBFORMAT_STANDARD : u32 = 1<<0;

const INTERVAL_SAMPLE_BITS : usize = 0;
const INTERVAL_FRAME_BITS : usize = 1;
const INTERVAL_CHANNELS : usize = 2;
const INTERVAL_RATE : usize = 3;
const INTERVAL_PERIOD_SIZE : usize = 5;
const INTERVAL_PERIODS : usize = 7;

macro_rules! IO{($name:ident, $ioty:expr, $nr:expr) => {
    pub fn $name(fd: impl std::os::fd::AsFd) -> rustix::io::Result<()> { unsafe{rustix::ioctl::ioctl(fd, rustix::ioctl::NoArg::<rustix::ioctl::NoneOpcode<$ioty, $nr, ()>>::new())} } } }
macro_rules! IOWR{($name:ident, $ioty:expr, $nr:expr, $ty:ty) => {
	pub fn $name(fd: impl std::os::fd::AsFd, data: &mut $ty) -> rustix::io::Result<()> { unsafe{rustix::ioctl::ioctl(fd, rustix::ioctl::Updater::<rustix::ioctl::ReadWriteOpcode<$ioty, $nr, $ty>, $ty>::new(data))} } } }

IOWR!{hw_params, b'A', 0x11, HWParams}
IO!{start, b'A', 0x42}

const  STATE_PREPARED : u32 = 2;
#[repr(C)] struct Status { state: u32, pad: u32, hw_pointer: *const u16, sec: u64, nsec: u64, suspended_state: u32 }
#[repr(C)] struct Control { sw_pointer: usize, avail_min: u64 }

pub struct PCM {
	fd: std::os::fd::OwnedFd,
	pub rate: u32,
	buffer: &'static mut [[i16; 2]],
	status: *const Status,
	control: *mut Control,
	period_size: u32,
}

impl PCM {
	pub fn new(rate: u32) -> Result<Self> {
		let fd = rustix::fs::open("/dev/snd/pcmC0D31p", rustix::fs::OFlags::RDWR|rustix::fs::OFlags::NONBLOCK, rustix::fs::Mode::empty())?;
		let mut hparams = HWParams::new();
  		hparams.masks[MASK_ACCESS][0] |= ACCESS_MMAP_INTERLEAVED;
		hparams.masks[MASK_FORMAT][0] |= FORMAT_S16_LE;
		hparams.masks[MASK_SUBFORMAT][0] |= SUBFORMAT_STANDARD;
		hparams.intervals[INTERVAL_CHANNELS] = 2.into();
		hparams.intervals[INTERVAL_SAMPLE_BITS] = 16.into();
		hparams.intervals[INTERVAL_FRAME_BITS] =  (16*2).into();
		hparams.intervals[INTERVAL_RATE] =  rate.into();
		hw_params(&fd, &mut hparams);
		let period_size = u32::from(hparams.intervals[INTERVAL_PERIOD_SIZE]);
  		let buffer_size = u32::from(hparams.intervals[INTERVAL_PERIODS]) * period_size;
		//let map = |addr, len, prot| unsafe{std::slice::from_raw_parts_mut(mmap(std::ptr::from_exposed_addr(addr), len*std::mem::size_of::<T>(), prot, SHARED, &fd, 0).unwrap() as *mut T, len)};
		fn map<T>(fd: impl std::os::fd::AsFd, addr: usize, len: u32, prot: rustix::mm::ProtFlags) -> *mut T {
			unsafe{rustix::mm::mmap(addr as *mut _, len as usize*std::mem::size_of::<T>(), prot, rustix::mm::MapFlags::SHARED, &fd, 0).unwrap() as *mut T}}
		use rustix::mm::ProtFlags;
		let buffer = unsafe{std::slice::from_raw_parts_mut(map(&fd, 0, buffer_size, ProtFlags::READ|ProtFlags::WRITE), buffer_size as usize)};
  		let status = map(&fd, 0x80000000, 1, ProtFlags::READ);
  		let control : *mut Control = map(&fd, 0x81000000, 1, ProtFlags::READ|ProtFlags::WRITE);
  		unsafe{&mut *control}.avail_min = period_size as u64;
		Ok(Self{fd, rate, buffer, status, control, period_size})
	}
	pub fn try_write(&mut self, frames: &mut impl ExactSizeIterator<Item=[i16; 2]>) -> Result<usize> {
		assert!(!frames.is_empty());
		fn write(buffer: &mut [[i16; 2]], frames: &mut impl ExactSizeIterator<Item=[i16; 2]>) -> usize {
			assert!(buffer.len() > 0);
			let target = buffer.into_iter().zip(frames);
			let len = target.len();
			for (target, frame) in target { unsafe{std::ptr::write_volatile(target as *mut [i16; 2], frame)}; }
			len
		}
		let len = self.buffer.len();
		let len = write(&mut self.buffer[unsafe{&mut *self.control}.sw_pointer%len..(unsafe{&mut *self.control}.sw_pointer+self.period_size as usize)%len], frames);
		unsafe{&mut *self.control}.sw_pointer += len;
		if unsafe{&*self.status}.state == STATE_PREPARED { start(&self.fd)?; }
		Ok(len)
	}
	//pub fn playing(&self) -> bool { self.device.state() == State::Running }
	//pub fn toggle_play_pause(&self) -> Result<()> { self.device.pause(self.playing()) }
}
impl Default for PCM { fn default() -> Self { Self::new(48000).unwrap() }}

pub trait Write {
	fn write<'t>(self, _: impl IntoIterator<IntoIter:ExactSizeIterator<Item=[i16; 2]>>) -> Result<()>;
}
impl<MutexGuard: std::ops::DerefMut<Target=PCM>, S: FnMut() -> MutexGuard> Write for S {
#[throws] fn write(mut self, frames: impl IntoIterator<IntoIter:ExactSizeIterator<Item=[i16; 2]>>) {
	let mut frames = frames.into_iter();
	while !frames.is_empty() {
		let audio_lock = self(); // Only lock to get the device fd
		let ref fd = unsafe{std::os::fd::BorrowedFd::borrow_raw(rustix::fd::AsRawFd::as_raw_fd(&audio_lock.fd))}; // Downcast to drop lock while waiting
		let ref mut fds = [rustix::event::PollFd::new(fd, rustix::event::PollFlags::OUT)];
		drop(audio_lock); // But do not stay locked while this audio thread is waiting for the device
		rustix::event::poll(fds, -1).unwrap();
		assert!(fds[0].revents().contains(rustix::event::PollFlags::OUT));
		self().try_write(&mut frames)?; // Waits for device. TODO: fade out and return on UI quit
	}
}}
