use super::*;

use crossbeam::atomic::AtomicCell;

// These are some low-level primitives.

/// A wrapper around an object, allowing it to be safely shared between threads.
#[derive(Default, Clone)]
pub struct SharedProtectedObject<T> {
    object: Arc<AtomicCell<T>>,
}
impl<T> fmt::Debug for SharedProtectedObject<T>
where
    T: std::fmt::Debug + Copy,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{:#?}", &self.object.load()) }
}

#[allow(dead_code)]
impl<T> SharedProtectedObject<T>
where
    T: Copy + Eq,
{
    #[inline]
    pub fn get(&self) -> T { self.object.load() }
    #[inline]
    pub fn set(&self, new: T) { self.object.store(new) }
    #[inline]
    pub fn compare_and_set(&self, current: T, new: T) -> T { self.object.compare_and_swap(current, new) }
}

/// ProtectedInner is a trait for providing access to an otherwise inaccessible object.
pub trait ProtectedInner<T>
where
    T: Copy + Eq,
{
    fn set(&self, new: T);
    fn compare_and_set(&self, old: T, new: T) -> T;
    fn get(&self) -> T;
}
