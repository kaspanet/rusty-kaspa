macro_rules! opcode_serde {
    ($type:ty) => {
        #[allow(dead_code)]
        fn serialize(&self) -> Vec<u8> {
            let length = self.data.len() as $type;
            [
                length.to_le_bytes().as_slice(),
                self.data.as_slice()
            ].concat()
        }

        fn deserialize<'i, I: Iterator<Item = &'i u8>>(it: &mut I) -> Result<Self, TxScriptError> {
            match it.take(size_of::<$type>()).copied().collect::<Vec<u8>>().try_into() {
                Ok(bytes) => {
                    let length = <$type>::from_le_bytes(bytes) as usize;
                    let data: Vec<u8> = it.take(length).copied().collect();
                    if data.len() != length {
                        // TODO: real error
                        todo!();
                    }
                    Ok(Self {
                        data
                    })
                },
                Err(_) => {
                    todo!()
                }
            }
        }
    };
    ($length: literal) => {
            #[allow(dead_code)]
            fn serialize(&self) -> Vec<u8> {
                self.data.clone()
            }

            fn deserialize<'i, I: Iterator<Item = &'i u8>>(it: &mut I) -> Result<Self, TxScriptError> {
                // Static length includes the opcode itself
                let data: Vec<u8> = it.take($length-1).copied().collect();
                if data.len() != $length-1 {
                    // TODO: real error
                    todo!();
                }
                Ok(Self{
                    data,
                })
            }
    }
}

macro_rules! opcode {
    ($name: ident, $num: literal, $length: tt, $code: expr, $self:ident, $vm:ident ) => {
        type $name = OpCode<$num>;

        impl $name {
            opcode_serde!($length);
        }

        impl OpCodeImplementation for $name {
            #[allow(unused_variables)]
            fn execute(&$self, $vm: &mut TxScriptEngine) -> OpCodeResult {
                $code
            }

            fn value(&self) -> u8 {
                return $num;
            }
        }
    }
}

macro_rules! opcode_list {
    ( $( opcode $name:ident<$num:literal, $length:tt>($self:ident, $vm:ident) $code: expr ) *)  => {
        $(
            opcode!($name, $num, $length, $code, $self, $vm);
        )*

        pub fn deserialize<'i, I: Iterator<Item = &'i u8>>(opcode_num: u8, it: &mut I) -> Result<Box<dyn OpCodeImplementation>, TxScriptError> {
            match opcode_num {
                $(
                    $num => Ok(Box::new($name::deserialize(it)?)),
                )*
                // TODO: real error! (opcode not implemented)
                // In case programmer didn't implement all opcodes
                #[allow(unreachable_patterns)]
                _ => todo!(),
            }
        }
    };
}
