#![allow(non_snake_case)]
fn collect<T>(target: &mut [T], iter: &mut impl Iterator<Item=T>) -> usize { target.iter_mut().zip(iter).fold(0, |len, (slot, item)| { *slot = item; len + 1 }) }
#[track_caller] fn exhaust<T>(target: &mut [T], iter: impl IntoIterator<IntoIter:ExactSizeIterator,Item=T>) -> usize {
    let ref mut iter = iter.into_iter();
    assert_eq!(target.len(), iter.len());
    let len = collect(target, iter);
    assert!(iter.next().is_none() && len==target.len());
    len
}
use realfft::num_traits::Zero;
fn zero<T:Zero>(len: usize) -> Box<[T]> { Box::<[T]>::from_iter((0..len).map(|_| T::zero())) }

mod filter {
    pub fn list<T>(iter: impl std::iter::IntoIterator<Item=T>) -> Box<[T]> { iter.into_iter().collect() }
    pub fn map<T,U>(iter: impl std::iter::IntoIterator<Item=T>, f: impl Fn(T)->U) -> Box<[U]> { list(iter.into_iter().map(f)) }

    use {std::f64::consts::PI as π, num::sq};
    fn blackman_harris2(n: f64, N: f64) -> f64 {
        #[allow(non_upper_case_globals)] const a : [f64; 4] = [0.35875, 0.48829, 0.14128, 0.01168];
        let x = π * n / N;
        let cos = |x:f64| x.cos();
        sq(a[0] - a[1] * cos(2.*x) + a[2] * cos(4.*x) - a[3]*cos(6.*x))
    }
    fn sinc(x: f64) -> f64 { if x == 0. { 1. } else { (π*x).sin() / (π*x) } }
    pub fn sinc_filter(len: usize, cutoff: f64) -> Box<[f32]> {
        let N = len as f64;
        let sinc = map((0..len).map(|n| n as f64), |n| blackman_harris2(n, N) * sinc(cutoff*(n - N/2.)));
        let sum = sinc.iter().sum::<f64>();
        sinc.iter().map(|y| (y/(sum*2.*N)) as f32).chain((0..len).map(|_| 0.)).collect()
    }
}
use filter::sinc_filter;
use {std::{cmp::max, sync::Arc}, realfft::{num_complex::Complex, ComplexToReal, RealFftPlanner, RealToComplex}};
struct Resampler {
	forward: usize,
	fft: Arc<dyn RealToComplex<f32>>,
    filter: Box<[Complex<f32>]>,
    frequency_domain: Box<[Complex<f32>]>,
    inverse: usize,
	ifft: Arc<dyn ComplexToReal<f32>>,
	scratch: Box<[Complex<f32>]>,
}
impl Resampler {
pub fn new(forward: usize, inverse: usize) -> Self {
	assert!(forward < inverse); // Upsampling
	let cutoff = 0.4f64.powf(16. / forward as f64); //?
	let mut planner = RealFftPlanner::<f32>::new();
	let fft = planner.plan_fft_forward(2 * forward);
	let mut filter = zero(forward+1);
	fft.process(&mut sinc_filter(forward, cutoff), &mut filter).unwrap();
	let ifft = planner.plan_fft_inverse(2 * inverse);
	let scratch = zero(fft.get_scratch_len().max(ifft.get_scratch_len()));
	Self{forward, fft, frequency_domain: zero(max(forward,inverse)+1), filter, inverse, ifft, scratch}
}
fn resample<'t>(&mut self, mut time_domain: &'t mut [f32], previous_time_domain: &'t [f32]) -> impl ExactSizeIterator<Item=f32>+'t {
    let mut input = &mut time_domain[..self.forward*2];
    input[self.forward..].fill(0.);
    self.fft.process_with_scratch(&mut input, &mut self.frequency_domain[..self.forward+1], &mut self.scratch).unwrap();
	assert!(self.forward < self.inverse); // Upsampling
	for (X, C) in self.frequency_domain[..self.forward+1].iter_mut().zip(&*self.filter) { *X *= C }
	self.frequency_domain[self.forward+1..].fill(Complex::zero());
    self.ifft.process_with_scratch(&mut self.frequency_domain, &mut time_domain, &mut self.scratch).unwrap();
    time_domain[..self.inverse].iter().zip(&previous_time_domain[self.inverse..]).map(|(output, overlap)| output+overlap)
}
}
pub struct MultiResampler {
    resampler: Resampler,
    previous_time_domain0: Box<[f32]>, previous_time_domain1: Box<[f32]>,
    time_domain0: Box<[f32]>, time_domain1: Box<[f32]>,
    overflow: usize,
}
impl MultiResampler {
pub fn new(input: u32, output: u32) -> Option<Self> {
    (input != output).then(|| {
        fn gcd(mut a: u32, mut b: u32) -> u32 { while b != 0 { (a,b) = (b, a % b) } a }
        let gcd = gcd(input, output);
        let [forward, inverse] = [(input/gcd) as usize, (output/gcd) as usize];
        let [previous_time_domain0, previous_time_domain1] = [(); 2].map(|_| zero(max(forward,inverse)*2));
        let [time_domain0, time_domain1] = [(); 2].map(|_| zero(max(forward,inverse)*2));
        Self{resampler: Resampler::new(forward, inverse), previous_time_domain0, previous_time_domain1, time_domain0, time_domain1, overflow: 0}
    })
}
pub fn resample<'b, 'i, P: Iterator, T, B: 'b, I:ExactSizeIterator<Item=f32>+'i>(&mut self,
    ref mut packets: &mut P,
    //convert: impl Fn(&P::Item)->[impl ExactSizeIterator<Item=f32>; 2]
    mut decode: impl FnMut(&P::Item) -> T,
    upcast: impl Fn(&T) -> B,
    convert: impl for<'bo> Fn(&'bo B) -> [I; 2] 
) -> Option<[impl ExactSizeIterator<Item=f32>+'_; 2]> {
    std::mem::swap(&mut self.previous_time_domain0, &mut self.time_domain0); std::mem::swap(&mut self.previous_time_domain1, &mut self.time_domain1);
    let mut filled = self.overflow; // previous time domain (for overlap-add) might also be used to store the remainder of the input packet
    while filled < self.resampler.forward {
        let packet = packets.next()?;
        let buffer = decode(&packet);
        let buffer = upcast(&buffer);
        let mut buffer = convert(&buffer);
        filled += collect(&mut self.time_domain0[filled..], &mut buffer[0]); collect(&mut self.time_domain1[filled..], &mut buffer[1]);
        self.overflow = collect(&mut self.previous_time_domain0, &mut buffer[0]); collect(&mut self.previous_time_domain1, &mut buffer[1]);
        assert!(self.overflow < self.resampler.inverse) // previous_time_domain[inverse..] is already used to store overlap to be added for overlap-add
        // previous_time_domain will become time_domain next time
    }
    Some([self.resampler.resample(&mut self.time_domain0, &self.previous_time_domain0), self.resampler.resample(&mut self.time_domain1, &self.previous_time_domain1)])
}
}