#[macro_export]
macro_rules! from {
    // Converter with parameter capture
    ($name:ident : $from_type:ty, $to_type:ty, $body:block) => {
        impl From<$from_type> for $to_type {
            fn from($name: $from_type) -> Self {
                $body
            }
        }
    };

    // Parameter-less converter capture
    ($from_type:ty, $to_type:ty) => {
        impl From<$from_type> for $to_type {
            fn from(_: $from_type) -> Self {
                Self {}
            }
        }
    };
}

#[macro_export]
macro_rules! try_from {
    // Converter with parameter capture
    ($name:ident : $from_type:ty, $to_type:ty, $ctor:block) => {
        impl TryFrom<$from_type> for $to_type {
            type Error = RpcError;
            fn try_from($name: $from_type) -> kaspa_rpc_core::RpcResult<Self> {
                // This attribute allows unimplemented!() to be used as $ctor
                #[allow(unreachable_code)]
                Ok($ctor)
            }
        }
    };

    // Parameter-less converter capture
    ($from_type:ty, $to_type:ty) => {
        impl TryFrom<$from_type> for $to_type {
            type Error = RpcError;
            fn try_from(_: $from_type) -> kaspa_rpc_core::RpcResult<Self> {
                Ok(Self {})
            }
        }
    };
}
