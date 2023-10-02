/*! RustRadio Block implementation

Blocks are the main buildingblocks of rustradio. They each do one
thing, and you connect them together with streams to process the data.

*/

use anyhow::Result;

use crate::stream::{InputStreams, OutputStreams, StreamType, Streamp};
use crate::Error;

/// Get input stream `n`, cast into the requested type.
///
/// Panics if the type is wrong.
pub fn get_input<T>(r: &InputStreams, n: usize) -> Streamp<T>
where
    T: Copy,
    Streamp<T>: From<StreamType>,
{
    let ret: Streamp<T> = r.get(n).into();
    ret
}

/// Get output stream `n`, cast into the requested type.
///
/// Panics if the type is wrong.
pub fn get_output<T>(w: &mut OutputStreams, n: usize) -> Streamp<T>
where
    T: Copy,
    Streamp<T>: From<StreamType>,
{
    let output: Streamp<T> = w.get(n).into();
    output
}

/** Return type for all blocks.

This will let the scheduler know if more data could come out of this block, or if
it should just never bother calling it again.

TODO: Add state for "don't call me unless there's more input".
*/
pub enum BlockRet {
    /// The normal return. More data may or not be coming.
    Ok,

    /// Block indicates that it will never produce more input.
    ///
    /// Examples:
    /// * reading from file, without repeating, and file reached EOF.
    /// * Head block reached its max.
    EOF,
}

/**
Block trait, that must be implemented for all blocks.

Simpler blocks can use macros to avoid needing to implement `work()`.
*/
pub trait Block {
    /** Name of block

    Not name of *instance* of block. But it may include the
    type. E.g. `FileSource<Float>`.
     */
    fn block_name(&self) -> &'static str;

    /** Block work function

    # Args
    * `r`: Object representing all input streams to read from.
    * `w`: Object representing all output streams to write to.

    A pure Source block will not use `r`, and a pure Sink block won't
    use `w`.

    Consuming data from `r` involves first reading it, and then
    "consuming" from the stream. If a `consume()` (or `clear()`) is
    not called on the stream, the same data will continue to be read
    forever.

    Writing data to streams in `w` only involves calling `.write()` on
    the stream.
     */
    fn work(&mut self, r: &mut InputStreams, w: &mut OutputStreams) -> Result<BlockRet, Error>;
}

/** Macro to make it easier to write one-for-one blocks.

Output type must be the same as the input type.

The first argument is the block struct name. The second (and beyond)
are traits that T must match.

`process_one(&mut self, s: &T) -> T` must be implemented by the block.

E.g.:
* [Add][add] or multiply by some constant, or negate.
* Delay, `o[n] = o[n] - o[n-1]`, or [IIR filter][iir]. These require state,
  but can process only one sample at a time.

# Example

```
use rustradio::block::Block;
struct Noop<T>{t: T};
impl<T: Copy> Noop<T> {
  fn process_one(&self, a: &T) -> T { *a }
}
rustradio::map_block_macro_v2![Noop<T>, std::ops::Add<Output = T>];
```

[add]: ../src/rustradio/add_const.rs.html
[iir]: ../src/rustradio/single_pole_iir_filter.rs.html
*/
#[macro_export]
macro_rules! map_block_macro_v2 {
    ($name:path, $($tr:path), *) => {
        impl<T> $crate::block::Block for $name
        where
            T: Copy $(+$tr)*,
            $crate::stream::Streamp<T>: From<$crate::stream::StreamType>,
        {
            fn block_name(&self) -> &'static str {
                stringify!{$name}
            }
            fn work(
                &mut self,
                r: &mut $crate::stream::InputStreams,
                w: &mut $crate::stream::OutputStreams,
            ) -> Result<$crate::block::BlockRet, $crate::Error> {
                let i = $crate::block::get_input(r, 0);
                $crate::block::get_output(w, 0)
                    .borrow_mut()
                    .write(i
                           .borrow()
                           .iter()
                           .map(|x| self.process_one(x)));
                i.borrow_mut().clear();
                Ok($crate::block::BlockRet::Ok)
            }
        }
    };
}

/** Macro to make it easier to write converting blocks.

Output type will be different from input type.

`process_one(&mut self, s: Type1) -> Type2` must be implemented by the
block.

Both types are derived, so only the name of the block is needed at
macro call.

Example block using this: `FloatToU32`.
*/
#[macro_export]
macro_rules! map_block_convert_macro {
    ($name:path) => {
        impl $crate::block::Block for $name {
            fn block_name(&self) -> &'static str {
                stringify! {$name}
            }
            fn work(
                &mut self,
                r: &mut $crate::stream::InputStreams,
                w: &mut $crate::stream::OutputStreams,
            ) -> Result<$crate::block::BlockRet, $crate::Error> {
                let i = $crate::block::get_input(r, 0);
                $crate::block::get_output(w, 0)
                    .borrow_mut()
                    .write(i.borrow().iter().map(|x| self.process_one(*x)));
                i.borrow_mut().clear();
                Ok($crate::block::BlockRet::Ok)
            }
        }
    };
}
