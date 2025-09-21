pub type Result<T=(), E=std::io::Error> = std::result::Result<T,E>;

const INTERVAL_FLAG_INTEGER : u32 = 1<<2;
#[derive(Debug)] #[derive(Clone,Copy)] #[repr(C)] struct Interval { min: u32, max: u32, flags : u32 }
impl Default for Interval { fn default() -> Self { Self{min: 0, max: !0, flags: 0 }}}
impl From<u32> for Interval { fn  from(value: u32) -> Self { Self{min: value, max: value, flags: INTERVAL_FLAG_INTEGER} } }
impl From<Interval> for u32 { fn from(Interval{min, max, ..}: Interval) -> u32 { assert_eq!(min, max); return max; } }

#[derive(Default)] #[repr(C)] #[derive(Debug)]
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
//const INTERVAL_PERIOD_TIME : usize = 4;
const INTERVAL_PERIOD_SIZE : usize = 5;
//const INTERVAL_PERIOD_BYTES : usize = 5;
const INTERVAL_PERIODS : usize = 7;
//const INTERVAL_BUFFER_TIME : usize = 8;
const INTERVAL_BUFFER_SIZE : usize = 9;
//const INTERVAL_BUFFER_BYTES : usize = 10;

macro_rules! IO{($name:ident, $group:expr, $number:expr) => {
    fn $name(fd: impl std::os::fd::AsFd) -> rustix::io::Result<()> { unsafe{rustix::ioctl::ioctl(fd, rustix::ioctl::NoArg::<{rustix::ioctl::opcode::none($group, $number)}>::new())} } } }
macro_rules! IOW{($name:ident, $group:expr, $number:expr, $type:ty) => {
	fn $name(fd: impl std::os::fd::AsFd, data: &mut $type) -> rustix::io::Result<()> { unsafe{rustix::ioctl::ioctl(fd, rustix::ioctl::Updater::<{rustix::ioctl::opcode::write::<$type>($group, $number)}, $type>::new(data))} } } }
macro_rules! IOWR{($name:ident, $group:expr, $number:expr, $type:ty) => {
	fn $name(fd: impl std::os::fd::AsFd, data: &mut $type) -> rustix::io::Result<()> { unsafe{rustix::ioctl::ioctl(fd, rustix::ioctl::Updater::<{rustix::ioctl::opcode::read_write::<$type>($group, $number)}, $type>::new(data))} } } }

IOWR!{hw_refine, b'A', 0x10, HWParams}
IOWR!{hw_params, b'A', 0x11, HWParams}
IO!{prepare, b'A', 0x40}
IO!{start, b'A', 0x42}
//IO!{drop, b'A', 0x43}
IOW!{pause, b'A', 0x45, i32}

const  STATE_SETUP : u32 = 1;
const  STATE_PREPARED : u32 = 2;
const  STATE_RUNNING : u32 = 3;
const  STATE_XRUN : u32 = 4;
//const  STATE_DRAINING : u32 = 5;
const  STATE_PAUSED : u32 = 6;
#[repr(C)] struct Status { state: u32, pad: u32, hw_pointer: usize, sec: u64, nsec: u64, suspended_state: u32 }
#[repr(C)] struct Control { sw_pointer: usize, avail_min: u64 }

pub struct Output {
	fd: std::os::fd::OwnedFd,
	pub rate: u32,
	buffer: &'static mut [[i16; 2]],
	status: *const Status,
	control: *mut Control,
	pub t: usize, // like sw_pointer but isn't reset by driver when state changes
}

impl Output {
	pub fn new(path: impl AsRef<std::path::Path>, rate: u32) -> Result<Self> {
		let fd = rustix::fs::open(path.as_ref(), rustix::fs::OFlags::RDWR|rustix::fs::OFlags::NONBLOCK, rustix::fs::Mode::empty())?; //
		let mut params = HWParams::new();
  		params.masks[MASK_ACCESS][0] |= ACCESS_MMAP_INTERLEAVED;
		params.masks[MASK_FORMAT][0] |= FORMAT_S16_LE;
		params.masks[MASK_SUBFORMAT][0] |= SUBFORMAT_STANDARD;
		params.intervals[INTERVAL_CHANNELS] = 2.into();
		params.intervals[INTERVAL_SAMPLE_BITS] = 16.into();
		params.intervals[INTERVAL_FRAME_BITS] =  (16*2).into();
		params.intervals[INTERVAL_RATE] =  rate.into();
		params.intervals[INTERVAL_PERIODS] =  3.into();
		hw_refine(&fd, &mut params).unwrap();
		params.intervals[INTERVAL_PERIOD_SIZE] =  params.intervals[INTERVAL_PERIOD_SIZE].max.into();
		params.intervals[INTERVAL_BUFFER_SIZE] = (u32::from(params.intervals[INTERVAL_PERIODS]) * u32::from(params.intervals[INTERVAL_PERIOD_SIZE])).into();
		hw_params(&fd, &mut params).unwrap();
		let period_size = u32::from(params.intervals[INTERVAL_PERIOD_SIZE]);
		let buffer_size = u32::from(params.intervals[INTERVAL_BUFFER_SIZE]);
		fn map<T>(fd: impl std::os::fd::AsFd, offset: u64, len: u32, prot: rustix::mm::ProtFlags) -> *mut T {
			unsafe{rustix::mm::mmap(std::ptr::null_mut(), (len as usize*std::mem::size_of::<T>()).max(0x1000), prot, rustix::mm::MapFlags::SHARED, &fd, offset).unwrap() as *mut T}}
		use rustix::mm::ProtFlags;
		let buffer = unsafe{std::slice::from_raw_parts_mut(map(&fd, 0, buffer_size, ProtFlags::READ|ProtFlags::WRITE), buffer_size as usize)};
  		let status : *const Status = map(&fd, 0x80000000, 1, ProtFlags::READ);
  		let control : *mut Control = map(&fd, 0x81000000, 1, ProtFlags::READ|ProtFlags::WRITE);
  		unsafe{&mut *control}.avail_min = period_size as u64;
		assert_eq!(unsafe{&*status}.state, STATE_SETUP);
  		prepare(&fd)?;
		Ok(Self{fd, rate, buffer, status, control, t: 0})
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
		let start = unsafe{&*self.control}.sw_pointer%self.buffer.len();
		let sw_pointer = unsafe{&*self.control}.sw_pointer;
		let hw_pointer =  unsafe{&*self.status}.hw_pointer;
		let len = if sw_pointer >= hw_pointer {
			let available = self.buffer.len() - (sw_pointer - hw_pointer);
			let len = if start + available <= self.buffer.len() {
				let end = start+available;
				write(&mut self.buffer[start..end], frames)
			} else {
				let len = write(&mut self.buffer[start..], frames);
				if len < self.buffer.len()-start { len }
				else { len + write(&mut self.buffer[0..available-len], frames) }
			};
			assert!(len as u32 > 0);
			unsafe{&mut *self.control}.sw_pointer += len;
			self.t += len;
			len
		} else { assert_eq!(unsafe{&*self.status}.state, STATE_XRUN); 0 };
		match unsafe{&*self.status}.state {
			STATE_RUNNING => {},
			STATE_PREPARED => { self::start(&self.fd)?; },
			STATE_XRUN => { println!("xrun"); self::prepare(&self.fd)?; },
			state => panic!("{state}"),
		}
		Ok(len)
	}
	//fn running(&self) -> bool { unsafe{&*self.status}.state == STATE_RUNNING }
	//fn drop(&self) { drop(&self.fd).unwrap(); }
	//fn play(&self) { self.lock().audio.device.pause(false).unwrap(); }
	pub fn toggle_play_pause(&self) -> Result<()> {
		match unsafe{&*self.status}.state {
			STATE_RUNNING => pause(&self.fd, &mut 1)?,
			STATE_PAUSED => pause(&self.fd, &mut 0)?,
			state => panic!("{state}"),
		}
		Ok(())
	}
}

pub trait Write {
	fn write<'t>(self, _: impl IntoIterator<IntoIter:ExactSizeIterator<Item=[i16; 2]>>) -> Result<()>;
}

impl<MutexGuard: std::ops::DerefMut<Target=[Output; N]>, S: FnMut() -> MutexGuard, const N: usize> Write for S {
	fn write(mut self, frames: impl IntoIterator<IntoIter:ExactSizeIterator<Item=[i16; 2]>>) -> Result {
		let frames = Box::from_iter(frames.into_iter());
		let ref mut frames = [(); N].map(|_| frames.iter().copied());
		while !frames.iter().all(|frames| frames.is_empty()) {
			let audio_lock = self(); // Need to lock to get the device fd
			fn map<T, U>(iter: impl IntoIterator<Item=T>, f: impl Fn(T) -> U) -> Box<[U]> { Box::from_iter(iter.into_iter().map(f)) }
			unsafe fn erase<'t>(fd: &impl std::os::fd::AsRawFd) -> std::os::fd::BorrowedFd<'t> { unsafe{std::os::fd::BorrowedFd::borrow_raw(fd.as_raw_fd())} }
			let ref fds = map(audio_lock.deref(), |pcm| unsafe{erase(&pcm.fd)}); // Erase lifetime to drop lock while waiting
			drop(audio_lock); // But do not stay locked while this audio thread is waiting for the device
			let ref mut fds = Vec::from_iter(fds.into_iter().map(|fd| rustix::event::PollFd::new(fd, rustix::event::PollFlags::OUT)));
			rustix::event::poll(fds, None).unwrap();
			assert!(fds.into_iter().any(|fd| fd.revents().contains(rustix::event::PollFlags::OUT)));
			for ((pcm, frames), fd) in (*self()).iter_mut().zip(frames.iter_mut()).zip(fds) { if !frames.is_empty() && fd.revents().contains(rustix::event::PollFlags::OUT) { pcm.try_write(frames)?; } } // Waits for device. TODO: fade out and return on UI quit
		}
		Ok(())
	}
}
