macro_rules! route {
    ($fn:ident, $name:tt) => {
        paste::paste! {
            #[allow(
                clippy::let_unit_value,
                clippy::no_effect_underscore_binding,
                clippy::shadow_same,
                clippy::type_complexity,
                clippy::type_repetition_in_bounds,
                clippy::used_underscore_binding
            )]
            fn $fn<'life0>(
                &'life0 self,
                _connection : core::option::Option<Self::RpcConnection>,
                request: [<$name Request>],
            ) -> impl ::core::future::Future<Output = RpcResult<[<$name Response>]>> + ::core::marker::Send
            {
                Box::pin(async move {
                    if let ::core::option::Option::Some(__ret) = ::core::option::Option::None::<RpcResult<[<$name Response>]>> {
                        return __ret;
                    }
                    let __self = self;
                    let __ret: RpcResult<[<$name Response>]> =
                        { __self.inner.call(KaspadPayloadOps::$name, request).await?.as_ref().try_into() };
                    #[allow(unreachable_code)]
                    __ret
                })
            }
        }
    };
}
