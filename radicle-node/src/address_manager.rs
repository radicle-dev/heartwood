use crate::address_book::Store;

#[derive(Debug)]
pub struct AddressManager<S> {
    store: S,
}

impl<S: Store> AddressManager<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }
}
