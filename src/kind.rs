// Tagged dispatch mechanism for resolving the behavior of `anyhow!($expr)`.
//
// When anyhow! is given a single expr argument to turn into anyhow::Error, we
// want the resulting Error to pick up the input's implementation of source()
// and backtrace() if it has a std::error::Error impl, otherwise require nothing
// more than Display and Debug.
//
// Expressed in terms of specialization, we want something like:
//
//     trait AnyhowNew {
//         fn new(self) -> Error;
//     }
//
//     impl<T> AnyhowNew for T
//     where
//         T: Display + Debug + Send + Sync + 'static,
//     {
//         default fn new(self) -> Error {
//             /* no std error impl */
//         }
//     }
//
//     impl<T> AnyhowNew for T
//     where
//         T: std::error::Error + Send + Sync + 'static,
//     {
//         fn new(self) -> Error {
//             /* use std error's source() and backtrace() */
//         }
//     }
//
// Since specialization is not stable yet, instead we rely on autoref behavior
// of method resolution to perform tagged dispatch. Here we have two traits
// AdhocKind and TraitKind that both have an anyhow_kind() method. AdhocKind is
// implemented whether or not the caller's type has a std error impl, while
// TraitKind is implemented only when a std error impl does exist. The ambiguity
// is resolved by AdhocKind requiring an extra autoref so that it has lower
// precedence.
//
// The anyhow! macro will set up the call in this form:
//
//     #[allow(unused_imports)]
//     use $crate::__private::{AdhocKind, TraitKind};
//     let error = $msg;
//     (&error).anyhow_kind().new(error)

use crate::Error;
use core::fmt::{Debug, Display};
#[cfg(track_caller)]
use core::panic::Location;

#[cfg(feature = "std")]
use crate::StdError;

pub struct Adhoc;

pub trait AdhocKind: Sized {
    #[inline]
    fn anyhow_kind(&self) -> Adhoc {
        Adhoc
    }
}

impl<T> AdhocKind for &T where T: ?Sized + Display + Debug + Send + Sync + 'static {}

impl Adhoc {
    #[cold]
    #[cfg_attr(track_caller, track_caller)]
    pub fn new<M>(self, message: M) -> Error
    where
        M: Display + Debug + Send + Sync + 'static,
    {
        Error::from_adhoc(
            message,
            backtrace!(),
            #[cfg(track_caller)]
            Location::caller(),
        )
    }
}

pub struct Trait;

pub trait TraitKind: Sized {
    #[inline]
    fn anyhow_kind(&self) -> Trait {
        Trait
    }
}

impl<E> TraitKind for E where E: Into<Error> {}

impl Trait {
    #[cold]
    #[cfg_attr(track_caller, track_caller)]
    pub fn new<E>(self, error: E) -> Error
    where
        E: Into<Error>,
    {
        #[allow(unused_mut)]
        let mut error: Error = error.into();

        // The direct Into conversion on the previous line loses caller location
        // because the generic `impl<T, U> Into<U> for T where U: From<T>` in
        // libcore is not annotated #[track_caller]. We can't just add that
        // attribute because of the widespread performance impact and code bloat
        // on all uses of the Into trait. We'll need something like a
        // #[rustc_conditional_track_caller] which is equivalent to track_caller
        // if and only if the function being called inside the function body is
        // already track_caller itself.
        #[cfg(track_caller)]
        error.set_location(Location::caller());

        error
    }
}

#[cfg(feature = "std")]
pub struct Boxed;

#[cfg(feature = "std")]
pub trait BoxedKind: Sized {
    #[inline]
    fn anyhow_kind(&self) -> Boxed {
        Boxed
    }
}

#[cfg(feature = "std")]
impl BoxedKind for Box<dyn StdError + Send + Sync> {}

#[cfg(feature = "std")]
impl Boxed {
    #[cold]
    #[cfg_attr(track_caller, track_caller)]
    pub fn new(self, error: Box<dyn StdError + Send + Sync>) -> Error {
        let backtrace = backtrace_if_absent!(&*error);
        Error::from_boxed(
            error,
            backtrace,
            #[cfg(track_caller)]
            Location::caller(),
        )
    }
}
