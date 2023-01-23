use {fehler::throws, alsa::{Result, PCM, direct::pcm::{MmapPlayback, RawSamples}, pcm::{HwParams,Format,Access,Frames,State}, ValueOr}};
pub use alsa::Error;
//pub type Result<T=()> = alsa::Result<T>;

pub struct Output {
	pub device: PCM,
	pub output: MmapPlayback<i16>,
}

impl Output {
	pub fn new(rate: u32) -> Result<Self> {
		let device = PCM::new("hw:0,31", alsa::Direction::Playback, false)?;
		{let hardware_parameters = HwParams::any(&device)?;
			hardware_parameters.set_rate(rate, ValueOr::Nearest)?;
			hardware_parameters.set_channels(2)?;
			hardware_parameters.set_format(Format::s16())?;
			hardware_parameters.set_access(Access::MMapInterleaved)?;
			hardware_parameters.set_buffer_size(hardware_parameters.get_buffer_size_max()?)?;
			device.hw_params(&hardware_parameters)?;}
		{let software_parameters = device.sw_params_current()?;
			software_parameters.set_avail_min(device.hw_params_current()?.get_period_size()?)?;
			device.sw_params(&software_parameters)?;}
		let output = device.direct_mmap_playback::<i16>()?;
		Ok(Self{device, output})
	}
	pub fn try_write(&self, frames: &mut impl ExactSizeIterator<Item=(i16, i16)>) -> Result<usize> {
		assert!(!frames.is_empty());
		fn write(buffer: RawSamples<i16>, frames: &mut impl ExactSizeIterator<Item=(i16, i16)>) -> usize {
			let buffer = unsafe{std::slice::from_raw_parts_mut(buffer.ptr as *mut [i16; 2], buffer.frames as usize)};
			assert!(buffer.len() > 0);
			let target = buffer.into_iter().zip(frames);
			let len = target.len();
			for (target, frame) in target { unsafe{std::ptr::write_volatile(target as *mut [i16; 2], [frame.0, frame.1])}; }
			len
		}
		let (buffer, more_buffer) = self.output.data_ptr();
		let mut len = write(buffer, frames);
		len += if let Some(buffer) = more_buffer && !frames.is_empty() { write(buffer, frames) } else {0};
		self.output.commit(len as Frames);
		if self.device.state() == State::Prepared { self.device.start()?; }
		assert!({use State::*; matches!(self.device.state(), Running|Paused)});
		Ok(len)
	}
	pub fn playing(self: &Output) -> bool { self.device.state() == State::Running }
	pub fn toggle_play_pause(self: &Output) -> Result<()> { self.device.pause(self.playing()) }
}
impl Default for Output { fn default() -> Self { Output::new(48000).unwrap() }}

pub trait Write {
	fn write<'t>(self, _: impl IntoIterator<IntoIter:ExactSizeIterator<Item=(i16, i16)>>) -> Result<()>;
}
impl<MutexGuard: std::ops::Deref<Target=Output>, S: Fn() -> MutexGuard> Write for S {
#[throws] fn write(self, frames: impl IntoIterator<IntoIter:ExactSizeIterator<Item=(i16, i16)>>) {
	let mut frames = frames.into_iter();
	while !frames.is_empty() {
		let audio_lock = self();
		let ref fd = unsafe{std::os::fd::BorrowedFd::borrow_raw(alsa::PollDescriptors::get(&audio_lock.device)?[0].fd)}; // Only lock to get the device fd
		let ref mut fds = [rustix::io::PollFd::new(fd, rustix::io::PollFlags::OUT)];
		drop(audio_lock); // But do not stay locked while this audio thread is waiting for the device
		rustix::io::poll(fds, -1).unwrap();
		assert!(fds[0].revents().contains(rustix::io::PollFlags::OUT));
		self().try_write(&mut frames)?; // Waits for device. TODO: fade out and return on UI quit
	}
}}