macro_rules! opcode_serde {
    ($type:ty) => {
        #[allow(dead_code)]
        fn serialize(&self) -> Vec<u8> {
            let length = self.data.len() as $type;
            [length.to_le_bytes().as_slice(), self.data.as_slice()].concat()
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
                    Ok(Self { data })
                }
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
            let data: Vec<u8> = it.take($length - 1).copied().collect();
            if data.len() != $length - 1 {
                // TODO: real error
                todo!();
            }
            Ok(Self { data })
        }
    };
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

            fn len(&self) -> usize {
                self.data.len()
            }

            // TODO: add it to opcode specification
            fn is_conditional(&self) -> bool {
                self.value() >= 0x63 && self.value() >= 0x68
            }

            fn check_minimal_data_push(&self) -> Result<(), TxScriptError> {
                let data_len = self.len();
                let opcode = self.value();

                if data_len == 0 {
                    if opcode != codes::OpFalse {
                        return Err(TxScriptError::NotMinimalData(
                            format!("zero length data push is encoded with \
                                opcode {:?} instead of OpFalse", self)
                        ));
                    }
                } else if data_len == 1 &&  self.data[0] >= 1 && self.data[0] <= 16 {
                    if opcode != codes::OpTrue + self.data[0]-1 {
                        return Err(TxScriptError::NotMinimalData(
                            format!("zero length data push is encoded with \
                                opcode {:?} instead of Op_{}", self, self.data[0])
                        ));
                    }
                } else if data_len == 1 && self.data[0] == 0x81 {
                    if opcode != codes::Op1Negate {
                        return Err(TxScriptError::NotMinimalData(
                            format!("data push of the value -1 encoded \
				                with opcode {:?} instead of OP_1NEGATE", self)
                        ));
                    }
                } else if data_len <= 75 {
                    if opcode as usize != data_len {
                        return Err(TxScriptError::NotMinimalData(
                            format!("data push of {} bytes encoded \
                                with opcode {:?} instead of OP_DATA_{}", data_len, self, data_len)
                        ));
                    }
                } else if data_len <= 255 {
                    if opcode != codes::OpPushData1 {
                        return Err(TxScriptError::NotMinimalData(
                            format!("data push of {} bytes encoded \
				                with opcode {:?} instead of OP_PUSHDATA1", data_len, self)
                        ));
		            }
                } else if data_len < 65535 {
                    if opcode != codes::OpPushData2 {
                        return Err(TxScriptError::NotMinimalData(
                            format!("data push of {} bytes encoded \
				                with opcode {:?} instead of OP_PUSHDATA2", data_len, self)
                        ));
		            }
                }
                Ok(())
            }
        }
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
