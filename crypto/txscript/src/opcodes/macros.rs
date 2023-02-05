macro_rules! opcode_serde {
    ($type:ty) => {
        #[allow(dead_code)]
        fn serialize(&self) -> Vec<u8> {
            let length = self.data.len() as $type;
            [length.to_le_bytes().as_slice(), self.data.as_slice()].concat()
        }

        fn deserialize<'i, I: Iterator<Item = &'i u8>, T: VerifiableTransaction>(
            it: &mut I,
        ) -> Result<Box<dyn OpCodeImplementation<T>>, TxScriptError> {
            match it.take(size_of::<$type>()).copied().collect::<Vec<u8>>().try_into() {
                Ok(bytes) => {
                    let length = <$type>::from_le_bytes(bytes) as usize;
                    let data: Vec<u8> = it.take(length).copied().collect();
                    if data.len() != length {
                        return Err(TxScriptError::MalformedPush(length, data.len()));
                    }
                    Ok(Box::new(Self { data }))
                }
                Err(vec) => {
                    return Err(TxScriptError::MalformedPushSize(vec));
                }
            }
        }
    };
    ($length: literal) => {
        #[allow(dead_code)]
        fn serialize(&self) -> Vec<u8> {
            self.data.clone()
        }

        fn deserialize<'i, I: Iterator<Item = &'i u8>, T: VerifiableTransaction>(
            it: &mut I,
        ) -> Result<Box<dyn OpCodeImplementation<T>>, TxScriptError> {
            // Static length includes the opcode itself
            let data: Vec<u8> = it.take($length - 1).copied().collect();
            if data.len() != $length - 1 {
                return Err(TxScriptError::MalformedPush($length - 1, data.len()));
            }
            Ok(Box::new(Self { data }))
        }
    };
}

macro_rules! opcode_impl {
    ($name: ident, $num: literal, $length: tt, $code: expr, $self:ident, $vm:ident ) => {
        type $name = OpCode<$num>;

        impl $name {
            opcode_serde!($length);
        }

        impl<T: VerifiableTransaction> OpCodeExecution<T> for $name {
            fn empty() -> Box<dyn OpCodeImplementation<T>> {
                return Box::new(Self{data: vec![]})
            }

            fn new(data: Vec<u8>) -> Box<dyn OpCodeImplementation<T>> {
                return Box::new(Self{data})
            }

            #[allow(unused_variables)]
            fn execute(&$self, $vm: &mut TxScriptEngine<T>) -> OpCodeResult {
                $code
            }
        }

        impl<T :VerifiableTransaction> OpCodeImplementation<T> for $name {}
    }
}

macro_rules! opcode_list {
    ( $( opcode $name:ident<$num:literal, $length:tt>($self:ident, $vm:ident) $code: expr ) *)  => {
        pub(crate) mod codes {
            $(
                #[allow(non_upper_case_globals)]
                #[allow(dead_code)]
                pub(crate) const $name: u8 = $num;
            )*
        }

        $(
            opcode_impl!($name, $num, $length, $code, $self, $vm);
        )*

        pub fn deserialize<'i, I: Iterator<Item = &'i u8>, T: VerifiableTransaction>(opcode_num: u8, it: &mut I) -> Result<Box<dyn OpCodeImplementation<T>>, TxScriptError> {
            match opcode_num {
                $(
                    $num => $name::deserialize(it),
                )*
            }
        }
    };
}
