macro_rules! impl_id_gen {
    ($name:ty) => {
        #[allow(unused)]
        impl $name {
            fn counter() -> &'static ::std::sync::atomic::AtomicUsize {
                static COUNTER: ::std::sync::atomic::AtomicUsize =
                    ::std::sync::atomic::AtomicUsize::new(0);
                &COUNTER
            }

            pub fn generate() -> Self {
                Self(Self::counter().fetch_add(1, ::std::sync::atomic::Ordering::SeqCst))
            }

            pub fn restore(val: usize) -> Self {
                Self::counter().fetch_max(val + 1, ::std::sync::atomic::Ordering::SeqCst);

                Self(val)
            }

            pub fn new(val: usize) -> Self {
                Self(val)
            }

            pub fn get(&self) -> usize {
                self.0
            }
        }

        impl<'de> serde::de::Deserialize<'de> for $name {
            fn deserialize<D: serde::de::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<Self, D::Error> {
                let v = usize::deserialize(deserializer)?;

                Ok(<$name>::restore(v))
            }
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub struct NodeId(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
pub struct PortId(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub struct LinkId(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub struct DeviceId(usize);

impl_id_gen!(NodeId);
impl_id_gen!(PortId);
impl_id_gen!(LinkId);
impl_id_gen!(DeviceId);
