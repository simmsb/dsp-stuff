macro_rules! impl_id_gen {
    ($name:ty) => {
        impl $name {
            pub fn generate() -> Self {
                static COUNTER: ::std::sync::atomic::AtomicUsize =
                    ::std::sync::atomic::AtomicUsize::new(0);

                Self(COUNTER.fetch_add(1, ::std::sync::atomic::Ordering::SeqCst))
            }
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PortId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LinkId(pub usize);

impl_id_gen!(NodeId);
impl_id_gen!(PortId);
impl_id_gen!(LinkId);
