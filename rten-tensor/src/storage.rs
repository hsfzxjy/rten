use std::borrow::Cow;
use std::marker::PhantomData;
use std::ops::Range;

/// Trait for backing storage used by tensors and views.
///
/// Mutable tensors have storage which also implement [StorageMut].
///
/// This specifies a contiguous array of elements in memory, as a pointer and a
/// length. The storage may be owned or borrowed. For borrowed storage, there
/// may be other storage whose ranges overlap. This is necessary to support
/// mutable views of non-contiguous tensors (eg. independent columns of a
/// matrix, whose data is stored in row-major order).
///
/// # Safety
///
/// Since different storage objects can have memory ranges that overlap, it is
/// up to the caller to ensure that mutable tensors cannot logically overlap any
/// other tensors. In other words, whenever a mutable tensor is split or sliced
/// or iterated, it should not be possible to get duplicate mutable references
/// to the same elements from those views.
pub trait Storage {
    /// The element type.
    type Elem;

    /// Return the number of elements in the storage.
    fn len(&self) -> usize;

    /// Return true if the storage contains no elements.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Return a pointer to the first element in the storage.
    fn as_ptr(&self) -> *const Self::Elem;

    /// Return the element at a given offset, or None if `offset >= self.len()`.
    ///
    /// # Safety
    ///
    /// - The caller must ensure that no mutable references to the same element
    ///   can be created.
    unsafe fn get(&self, offset: usize) -> Option<&Self::Elem> {
        if offset < self.len() {
            Some(&*self.as_ptr().add(offset))
        } else {
            None
        }
    }

    /// Return a reference to the element at `offset`.
    ///
    /// # Safety
    ///
    /// This has the same safety requirements as [`get`](Storage::get) plus
    /// the caller must ensure that `offset < len`.
    unsafe fn get_unchecked(&self, offset: usize) -> &Self::Elem {
        debug_assert!(offset < self.len());
        &*self.as_ptr().add(offset)
    }

    /// Return a view of a sub-region of the storage.
    ///
    /// Panics if the range is out of bounds.
    fn slice(&self, range: Range<usize>) -> ViewData<Self::Elem> {
        assert!(range.end <= self.len());
        ViewData {
            // Safety: `range.start < range.end` and `range.end <= self.len())`,
            // so this is in-bounds.
            ptr: unsafe { self.as_ptr().add(range.start) },
            len: range.len(),
            _marker: PhantomData,
        }
    }

    /// Return an immutable view of this storage.
    fn view(&self) -> ViewData<Self::Elem> {
        self.slice(0..self.len())
    }

    /// Return the contents of the storage as a slice.
    ///
    /// # Safety
    ///
    /// The caller must ensure that no mutable references exist to any element
    /// in the storage.
    unsafe fn as_slice(&self) -> &[Self::Elem] {
        std::slice::from_raw_parts(self.as_ptr(), self.len())
    }
}

/// Trait for converting owned and borrowed element containers (`Vec<T>`, slices)
/// into their corresponding `Storage` type.
///
/// This is used by [`Tensor::from_data`](crate::TensorBase::from_data).
pub trait IntoStorage {
    type Output: Storage;

    fn into_storage(self) -> Self::Output;
}

impl<T> IntoStorage for Vec<T> {
    type Output = Self;

    fn into_storage(self) -> Self {
        self
    }
}

impl<'a, T> IntoStorage for &'a [T] {
    type Output = ViewData<'a, T>;

    fn into_storage(self) -> ViewData<'a, T> {
        ViewData {
            ptr: self.as_ptr(),
            len: self.len(),
            _marker: PhantomData,
        }
    }
}

impl<'a, T, const N: usize> IntoStorage for &'a [T; N] {
    type Output = ViewData<'a, T>;

    fn into_storage(self) -> ViewData<'a, T> {
        self.as_slice().into_storage()
    }
}

impl<'a, T> IntoStorage for &'a mut [T] {
    type Output = ViewMutData<'a, T>;

    fn into_storage(self) -> ViewMutData<'a, T> {
        ViewMutData {
            ptr: self.as_mut_ptr(),
            len: self.len(),
            _marker: PhantomData,
        }
    }
}

/// Trait for backing storage used by mutable tensors and views.
///
/// This extends [Storage] with methods to get mutable pointers and references
/// to elements in the storage.
pub trait StorageMut: Storage {
    /// Return a mutable pointer to the first element in storage.
    fn as_mut_ptr(&mut self) -> *mut Self::Elem;

    /// Mutable version of [Storage::get].
    ///
    /// # Safety
    ///
    /// This has the same safety requirements as [`get`](Storage::get).
    unsafe fn get_mut(&mut self, offset: usize) -> Option<&mut Self::Elem> {
        if offset < self.len() {
            Some(&mut *self.as_mut_ptr().add(offset))
        } else {
            None
        }
    }

    /// Mutable version of [Storage::get_unchecked].
    ///
    /// # Safety
    ///
    /// This has the same requirement as [`get_mut`](StorageMut::get_mut) plus
    /// the caller must ensure that `offset < self.len()`.
    unsafe fn get_unchecked_mut(&mut self, offset: usize) -> &mut Self::Elem {
        debug_assert!(offset < self.len());
        &mut *self.as_mut_ptr().add(offset)
    }

    /// Return a slice of this storage.
    fn slice_mut(&mut self, range: Range<usize>) -> ViewMutData<Self::Elem> {
        assert!(range.end <= self.len());
        ViewMutData {
            // Safety: `range.start <= self.len()`
            ptr: unsafe { self.as_mut_ptr().add(range.start) },
            len: range.len(),
            _marker: PhantomData,
        }
    }

    /// Return a mutable view of this storage.
    fn view_mut(&mut self) -> ViewMutData<Self::Elem> {
        self.slice_mut(0..self.len())
    }

    /// Return the stored elements as a mutable slice.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the storage is contiguous (ie. no unused
    /// elements) and that there are no references to any elements in the
    /// storage.
    unsafe fn as_slice_mut(&mut self) -> &mut [Self::Elem] {
        std::slice::from_raw_parts_mut(self.as_mut_ptr(), self.len())
    }
}

impl<T> Storage for Vec<T> {
    type Elem = T;

    fn len(&self) -> usize {
        self.len()
    }

    fn as_ptr(&self) -> *const T {
        self.as_ptr()
    }
}

impl<T> StorageMut for Vec<T> {
    fn as_mut_ptr(&mut self) -> *mut T {
        self.as_mut_ptr()
    }
}

/// Storage for an immutable tensor view.
///
/// This has the same representation in memory as a slice: a pointer and a
/// length. Unlike a slice it allows for other mutable storage to reference
/// memory ranges that overlap with this one. It is up to APIs built on top of
/// this to ensure uniqueness of mutable element references.
#[derive(Debug)]
pub struct ViewData<'a, T> {
    ptr: *const T,
    len: usize,
    _marker: PhantomData<&'a T>,
}

// Safety: `ViewData` does not provide mutable access to its elements, so it
// is `Send` and `Sync`.
unsafe impl<'a, T> Send for ViewData<'a, T> {}
unsafe impl<'a, T> Sync for ViewData<'a, T> {}

impl<'a, T> Clone for ViewData<'a, T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<'a, T> Copy for ViewData<'a, T> {}

impl<'a, T> ViewData<'a, T> {
    /// Variant of [Storage::get] which preserves lifetimes.
    ///
    /// # Safety
    ///
    /// See [Storage::get].
    pub unsafe fn get(&self, offset: usize) -> Option<&'a T> {
        if offset < self.len {
            Some(unsafe { &*self.ptr.add(offset) })
        } else {
            None
        }
    }

    /// Variant of [Storage::get_unchecked] which preserves lifetimes.
    ///
    /// # Safety
    ///
    /// See [Storage::get_unchecked].
    pub unsafe fn get_unchecked(&self, offset: usize) -> &'a T {
        debug_assert!(offset < self.len);
        &*self.ptr.add(offset)
    }

    /// Variant of [Storage::slice] which preserves lifetimes.
    pub fn slice(&self, range: Range<usize>) -> ViewData<'a, T> {
        assert!(range.end <= self.len());
        ViewData {
            // Safety: `range.start < range.end` and `range.end <= self.len())`,
            // so this is in-bounds.
            ptr: unsafe { self.as_ptr().add(range.start) },
            len: range.len(),
            _marker: PhantomData,
        }
    }

    /// Variant of [Storage::view] which preserves lifetimes.
    pub fn view(&self) -> ViewData<'a, T> {
        self.slice(0..self.len())
    }

    /// Return the contents of the storage as a slice.
    ///
    /// # Safety
    ///
    /// The caller must ensure that no mutable references exist to any element
    /// in the storage.
    pub unsafe fn as_slice(&self) -> &'a [T] {
        std::slice::from_raw_parts(self.ptr, self.len)
    }
}

impl<'a, T> Storage for ViewData<'a, T> {
    type Elem = T;

    fn len(&self) -> usize {
        self.len
    }

    fn as_ptr(&self) -> *const T {
        self.ptr
    }
}

/// Storage for a mutable tensor view.
///
/// This has the same representation in memory as a mutable slice: a pointer
/// and a length. Unlike a slice it allows for other storage objects to
/// reference memory ranges that overlap with this one. It is up to
/// APIs built on top of this to ensure uniqueness of mutable references.
#[derive(Debug)]
pub struct ViewMutData<'a, T> {
    ptr: *mut T,
    len: usize,
    _marker: PhantomData<&'a mut T>,
}
unsafe impl<'a, T> Send for ViewMutData<'a, T> {}

impl<'a, T> ViewMutData<'a, T> {
    /// Variant of [StorageMut::as_slice_mut] which preserves the underlying
    /// lifetime in the result.
    ///
    /// # Safety
    ///
    /// See [StorageMut::as_slice_mut].
    pub unsafe fn to_slice_mut(mut self) -> &'a mut [T] {
        std::slice::from_raw_parts_mut(self.as_mut_ptr(), self.len())
    }
}

impl<'a, T> Storage for ViewMutData<'a, T> {
    type Elem = T;

    fn len(&self) -> usize {
        self.len
    }

    fn as_ptr(&self) -> *const T {
        self.ptr
    }
}

impl<'a, T> StorageMut for ViewMutData<'a, T> {
    fn as_mut_ptr(&mut self) -> *mut T {
        self.ptr
    }
}

impl<'a, T> Storage for Cow<'a, [T]>
where
    [T]: ToOwned,
{
    type Elem = T;

    fn len(&self) -> usize {
        self.as_ref().len()
    }

    fn as_ptr(&self) -> *const T {
        self.as_ref().as_ptr()
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::{IntoStorage, Storage, ViewData, ViewMutData};

    fn test_storage_impl<S: Storage<Elem = i32>>(s: S, expected: &[i32]) {
        // Test `len`, `get`.
        assert_eq!(s.len(), expected.len());
        for i in 0..s.len() {
            assert_eq!(unsafe { s.get(i) }, expected.get(i));
        }
        assert_eq!(unsafe { s.get(s.len()) }, None);

        // Test slicing storage.
        let range = 1..s.len() - 1;
        let slice = s.slice(range.clone());
        assert_eq!(slice.len(), range.len());
        for i in 0..slice.len() {
            assert_eq!(unsafe { slice.get(i) }, expected[range.clone()].get(i));
        }

        // Test restoring a slice.
        assert_eq!(unsafe { s.as_slice() }, expected);
    }

    #[test]
    fn test_storage() {
        let data = &mut [1, 2, 3, 4];

        let owned = data.to_vec();
        test_storage_impl(owned, data);

        let view: ViewData<i32> = data.as_slice().into_storage();
        test_storage_impl(view, data);

        let cow_view = Cow::Borrowed(data.as_slice());
        test_storage_impl(cow_view, data);

        let mut_view: ViewMutData<i32> = data.as_mut_slice().into_storage();
        test_storage_impl(mut_view, &[1, 2, 3, 4]);
    }
}
