// use js_sys::Function;

// #[derive(Clone, Eq, PartialEq)]
// pub struct Sink(pub Function);
// unsafe impl Send for Sink {}
// impl From<Sink> for Function {
//     fn from(f: Sink) -> Self {
//         f.0
//     }
// }
// impl From<Function> for Sink {
//     fn from(f: Function) -> Self {
//         Self(f)
//     }
// }

// // pub struct Callbacks<T> {
// //     map: AHashMap<T, Vec<Sink>>,
// // }
