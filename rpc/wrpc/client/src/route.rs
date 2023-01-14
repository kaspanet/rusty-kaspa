macro_rules! route {
    ($fn:ident, $name:tt) => {
        paste::paste! {
            fn $fn<'life0, 'async_trait>(
                &'life0 self,
                request: [<$name Request>],
            ) -> ::core::pin::Pin<Box<dyn ::core::future::Future<Output = RpcResult<[<$name Response>]>> + ::core::marker::Send + 'async_trait>>
            where
                'life0: 'async_trait,
                Self: 'async_trait,
            {
                Box::pin(async move {
                    if let ::core::option::Option::Some(__ret) = ::core::option::Option::None::<RpcResult<[<$name Response>]>> {
                        return __ret;
                    }
                    let __self = self;
                    let request = request;
                    let __ret: RpcResult<[<$name Response>]> = {
                        let resp: Response<[<$name Response>]> = __self.rpc.call(RpcApiOps::$name, request).await;
                        Ok(resp.map_err(|e| e.to_string())?)
                    };
                    #[allow(unreachable_code)]
                    __ret
                })
            }
                }
    };
}

pub(crate) use route;
