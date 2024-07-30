


pub struct Lazy<T> {
    value: std::cell::UnsafeCell<Option<T>>,
    f: fn() -> T,
}

impl<T> Lazy<T> {
    pub const fn new(f: fn() -> T) -> Self {
        Self {
            value: std::cell::UnsafeCell::new(None),
            f,
        }
    }
    
    pub fn get(&self) -> &T {
        let value = unsafe { &mut *self.value.get() };
        if value.is_none() {
            *value = Some((self.f)());
        }
        value.as_ref().unwrap()
    }
}

unsafe impl<T> Sync for Lazy<T> {}

#[cfg(test)]
mod tests {
    use crate::utils::Lazy;

    #[test]
    fn lazy() {
        static LAZY: Lazy<Vec<u8>> = Lazy::new(|| b"hello".to_vec());
        assert_eq!(LAZY.get(), b"hello");
    }
}
