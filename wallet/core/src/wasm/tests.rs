use crate::imports::*;
// use wasm_bindgen::prelude::*;
// use wasm_bindgen::{prelude::*, JsCast, JsValue};
// use workflow_wasm::object::*;

// struct Iterable<A> {
//     value: js_sys::IntoIter,
//     phantom: PhantomData<A>,
// }

// impl<A> Iterable<A> {
//     #[allow(dead_code)]
//     fn unchecked_new(value: &JsValue) -> Self {
//         Self { value: js_sys::try_iter(value).unwrap().unwrap(), phantom: PhantomData }
//     }
// }

// impl<A> Iterator for Iterable<A>
// where
//     A: JsCast,
// {
//     type Item = A;

//     #[inline]
//     fn next(&mut self) -> Option<Self::Item> {
//         self.value.next().map(|x| JsCast::unchecked_from_js(x.unwrap()))
//     }
// }

// #[wasm_bindgen(inline_js = "
//     export function foo(obj) {
//         obj[Symbol.iterator] = function () {
//             return this;
//         };
//     }
// ")]
// extern "C" {
//     fn foo(obj: &Object);
// }

// // #[wasm_bindgen(start)]
// pub fn test_iter(object: &Object) -> Result<(), JsValue> {
//     // ...
//     // This works, but I couldn't figure out how to get the prototype of an object without instantiating a copy first
//     foo(&Object::get_prototype_of(&object.into()));
//     Ok(())
// }

// // obj[Symbol.iterator] = function () {
// //     return this;
// // };

#[wasm_bindgen]
pub fn get_async_iter() -> JsValue {
    let iter = stream::iter(0..30);
    AsyncStream::new(iter).into()
}
