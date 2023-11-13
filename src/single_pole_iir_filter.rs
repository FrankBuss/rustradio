//! Infinite Impulse Response (IIR) filter.
use anyhow::Result;

use crate::stream::{new_streamp2, Streamp2};
use crate::{map_block_macro_v2, Float};

struct SinglePoleIIR<Tout> {
    alpha: Float, // TODO: GNURadio uses double
    one_minus_alpha: Float,
    prev_output: Tout,
}

impl<Tout> SinglePoleIIR<Tout>
where
    Tout: Copy + Default + std::ops::Mul<Float, Output = Tout> + std::ops::Add<Output = Tout>,
{
    fn new(alpha: Float) -> Option<Self> {
        let mut r = Self {
            alpha: Float::default(),
            one_minus_alpha: Float::default(),
            prev_output: Tout::default(),
        };
        r.set_taps(alpha)?;
        Some(r)
    }
    fn filter<Tin>(&mut self, sample: Tin) -> Tout
    where
        Tin: Copy + std::ops::Mul<Float, Output = Tin> + std::ops::Add<Tout, Output = Tout>,
    {
        let o: Tout = sample * self.alpha + self.prev_output * self.one_minus_alpha;
        self.prev_output = o;
        o
    }
    fn set_taps(&mut self, alpha: Float) -> Option<()> {
        if !(0.0..=1.0).contains(&alpha) {
            return None;
        }
        self.alpha = alpha;
        self.one_minus_alpha = 1.0 - alpha;
        Some(())
    }
}

/// Infinite Impulse Response (IIR) filter.
pub struct SinglePoleIIRFilter<T>
where
    T: Copy + Default + std::ops::Mul<T, Output = T> + std::ops::Add<T, Output = T>,
{
    iir: SinglePoleIIR<T>,
    src: Streamp2<T>,
    dst: Streamp2<T>,
}

impl<T> SinglePoleIIRFilter<T>
where
    T: Copy
        + Default
        + std::ops::Mul<Float, Output = T>
        + std::ops::Mul<T, Output = T>
        + std::ops::Add<T, Output = T>,
{
    /// Create new IIR filter.
    pub fn new(src: Streamp2<T>, alpha: Float) -> Option<Self> {
        Some(Self {
            src,
            dst: new_streamp2(),
            iir: SinglePoleIIR::<T>::new(alpha)?,
        })
    }
    fn process_one(&mut self, a: &T) -> T {
        self.iir.filter(*a)
    }
}

map_block_macro_v2![
    SinglePoleIIRFilter<T>,
    Default,
    std::ops::Add<Output = T>,
    std::ops::Mul<T, Output = T>,
    std::ops::Mul<Float, Output = T>,
    std::ops::Add<T, Output = T>
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::Block;
    use crate::stream::streamp2_from_slice;
    use crate::{Complex, Error};

    #[test]
    fn iir_ff() -> Result<()> {
        // TODO: create an actual test.
        let src = streamp2_from_slice(&[0.1, 0.2]);
        let mut iir = SinglePoleIIRFilter::new(src, 0.2).ok_or(Error::new("alpha out of range"))?;
        iir.work()?;
        Ok(())
    }

    #[test]
    fn iir_cc() -> Result<()> {
        // TODO: create an actual test.
        let src = streamp2_from_slice(&[Complex::new(1.0, 0.1), Complex::default()]);
        let mut iir = SinglePoleIIRFilter::new(src, 0.2).ok_or(Error::new("alpha out of range"))?;
        iir.work()?;
        Ok(())
    }

    #[test]
    fn reject_bad_alpha() -> Result<()> {
        let src = streamp2_from_slice(&[0.1, 0.2]);
        SinglePoleIIRFilter::new(src.clone(), 0.0).ok_or(Error::new("should accept 0.0"))?;
        SinglePoleIIRFilter::new(src.clone(), 0.1).ok_or(Error::new("should accept 0.1"))?;
        SinglePoleIIRFilter::new(src.clone(), 1.0).ok_or(Error::new("should accept 1.0"))?;
        if SinglePoleIIRFilter::new(src.clone(), -0.1).is_some() {
            return Err(Error::new("should not accept -0.1").into());
        }
        if SinglePoleIIRFilter::new(src, 1.1).is_some() {
            return Err(Error::new("should not accept 1.1").into());
        }
        Ok(())
    }
}
