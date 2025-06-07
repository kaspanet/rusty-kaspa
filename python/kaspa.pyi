from enum import Enum
from typing import Any, Callable, Iterator, Optional, TypedDict, Union

class Address:

    def __init__(self, address: str) -> None: ...

    @staticmethod
    def validate(address: str) -> bool: ...

    def to_string(self) -> str: ...

    @property
    def version(self) -> str: ...

    @property
    def prefix(self) -> str: ...

    @prefix.setter
    def set_prefix(self, prefix: str) -> None: ...

    def payload(self) -> str: ...

    def short(self, n: int) -> str: ...


class SighashType(Enum):
    All = 1
    'None' = 2
    Single = 3
    AllAnyOneCanPay = 4
    NoneAnyOneCanPay = 5
    SingleAnyOneCanPay = 6


class ScriptPublicKey:

    def __init__(self, version: int, script: Union[str, bytes, list[int]]) -> None: ...

    @property
    def script(self) -> str: ...


class Transaction:

    def is_coinbase(self) -> bool: ...

    def finalize(self) -> str: ...

    @property
    def id(self) -> str: ...

    def __init__(
        self, 
        version: int, 
        inputs: list[TransactionInput], 
        outputs: list[TransactionOutput], 
        lock_time: int, 
        subnetwork_id: Union[str, bytes, list[int]], 
        gas: int,
        payload: Union[str, bytes, list[int]],
        mass: int
    ) -> None: ...

    @property
    def inputs(self) -> list[TransactionInput]: ...

    @inputs.setter
    def inputs(self, v: list[TransactionInput]) -> None: ...

    def addresses(self, network_type: str) -> list[Address]: ...

    @property
    def outputs(self) -> list[TransactionOutput]: ...

    @outputs.setter
    def outputs(self, v: list[TransactionOutput]) -> None: ...

    @property
    def version(self) -> int: ...

    @version.setter
    def version(self, v: int) -> None: ...

    @property
    def lock_time(self) -> int: ...

    @lock_time.setter
    def lock_time(self, v: int) -> None: ...

    @property
    def gas(self) -> int: ...

    @gas.setter
    def gas(self, v: int) -> None: ...

    @property
    def subnetwork_id(self) -> str: ...

    @subnetwork_id.setter
    def subnetwork_id(self, v: str) -> None: ...

    @property
    def payload(self) -> str: ...

    @payload.setter
    def payload(self, v: Union[str, bytes, list[int]]) -> None: ...

    @property
    def mass(self) -> int: ...

    @property.setter
    def mass(self, v: int) -> None: ...

    def serialize_to_dict(self) -> dict: ...


class TransactionInput:

    def __init__(
        self,
        previous_outpoint: TransactionOutpoint,
        signature_script: Union[str, bytes, list[int]],
        sequence: int,
        sig_op_count: int,
        utxo: Optional[UtxoEntryReference] = None
    ) -> None: ...

    @property
    def previous_outpoint(self) -> TransactionOutpoint: ...

    @previous_outpoint.setter
    def previous_outpoint(self, outpoint: TransactionOutpoint) -> None: ...

    @property
    def signature_script(self) -> str: ...

    @signature_script.setter
    def signature_script(self, signature_script: Union[str, bytes, list[int]]) -> None: ...

    @property
    def sequence(self) -> int: ...

    @sequence.setter
    def sequence(self, sequence: int) -> None: ...

    @property
    def sig_op_count(self) -> int: ...

    @sig_op_count.setter
    def sig_op_count(self, sig_op_count: int) -> None: ...

    @property
    def utxo(self) -> Optional[UtxoEntryReference]: ...


class TransactionOutpoint:
    
    def __init__(self, transaction_id: str, index: int) -> None: ...

    def get_id(self) -> str: ...

    @property
    def transaction_id(self) -> str: ...

    @property
    def index(self) -> int: ...


class TransactionOutput:

    def __init__(self, value: int, script_public_key: ScriptPublicKey) -> None: ...

    @property
    def value(self) -> int: ...

    @value.setter
    def value(self, v: int) -> None: ...

    @property
    def script_public_key(self) -> int: ...

    @script_public_key.setter
    def script_public_key(self, v: ScriptPublicKey) -> None: ...


class UtxoEntries:

    @property
    def items(self) -> list[UtxoEntryReference]: ...

    @items.setter
    def items(self, v: list[UtxoEntryReference]): ...

    def sort(self) -> None: ...

    def amount(self) -> int: ...


class UtxoEntry:

    @property
    def address(self) -> Optional[Address]: ...

    @property
    def outpoint(self) -> TransactionOutpoint: ...

    @property
    def amount(self) -> int: ...

    @property
    def script_public_key(self) -> ScriptPublicKey: ...

    @property
    def block_daa_score(self) -> int: ...

    @property
    def is_coinbase(self) -> bool: ...


class UtxoEntryReference:

    @property
    def entry(self) -> UtxoEntry: ...

    @property
    def outpoint(self) -> TransactionOutpoint: ...

    @property
    def address(self) -> Optional[Address]: ...

    @property
    def amount(self) -> int: ...

    @property
    def is_coinbase(self) -> bool: ...

    @property
    def block_daa_score(self) -> int: ...

    @property
    def script_public_key(self) -> ScriptPublicKey: ...


def address_from_script_public_key(script_public_key: ScriptPublicKey, network: str) -> Address: ...

def pay_to_address_script(address: Address) -> ScriptPublicKey: ...

def pay_to_script_hash_script(redeem_script: Union[str, bytes, list[int]]) -> ScriptBuilder: ...

def pay_to_script_hash_signature_script(redeem_script: Union[str, bytes, list[int]], signature: Union[str, bytes, list[int]]) -> str: ...

def is_script_pay_to_pubkey(script: Union[str, bytes, list[int]]) -> bool: ...

def is_script_pay_to_pubkey_ecdsa(script: Union[str, bytes, list[int]]) -> bool: ...

def is_script_pay_to_script_hash(script: Union[str, bytes, list[int]]) -> bool: ...

class Hash:

    def __init__(self, hex_str: str) -> None: ...

    def to_string(self) -> str: ...


class Language(Enum):
    English: 1


class Mnemonic:

    def __init__(self, phrase: str, language: Optional[Language] = None) -> None: ...    

    @staticmethod
    def validate(phrase: str, language: Optional[Language] = None) -> bool: ...

    @property
    def entropy(self) -> str: ...

    @entropy.setter
    def entropy(self, entropy: str) -> None: ...

    @staticmethod
    def random(word_count: Optional[int] = None) -> Mnemonic: ...

    @property
    def phrase(self) -> str: ...

    @phrase.setter
    def phrase(self, phrase: str) -> None: ...

    def to_seed(self, password: Optional[str] = None) -> str: ...


class ScriptBuilder:

    def __init__(self) -> None: ...

    @staticmethod
    def from_script(script: Union[str, bytes, list[int]]) -> ScriptBuilder: ...

    def add_op(self, op: Union[Opcodes, int]) -> ScriptBuilder: ...

    def add_ops(self, opcodes: Union[list[Opcodes], list[int]]) -> ScriptBuilder: ...

    def add_data(self, data: Union[str, bytes, list[int]]) -> ScriptBuilder: ...

    def add_i64(self, value: int) -> ScriptBuilder: ...

    def add_lock_time(self, lock_time: int) -> ScriptBuilder: ...

    def add_sequence(self, sequence: int) -> ScriptBuilder: ...

    @staticmethod
    def canonical_data_size(data: Union[str, bytes, list[int]]) -> int: ...

    def to_string(self) -> str: ...

    def drain(self) -> str: ...

    def create_pay_to_script_hash_script(self) -> ScriptPublicKey: ...

    def encode_pay_to_script_hash_signature_script(self, signature: Union[str, bytes, list[int]]) -> str: ...


class Opcodes(Enum):
    OpFalse = 0x00

    OpData1 = 0x01
    OpData2 = 0x02
    OpData3 = 0x03
    OpData4 = 0x04
    OpData5 = 0x05
    OpData6 = 0x06
    OpData7 = 0x07
    OpData8 = 0x08
    OpData9 = 0x09
    OpData10 = 0x0a
    OpData11 = 0x0b
    OpData12 = 0x0c
    OpData13 = 0x0d
    OpData14 = 0x0e
    OpData15 = 0x0f
    OpData16 = 0x10
    OpData17 = 0x11
    OpData18 = 0x12
    OpData19 = 0x13
    OpData20 = 0x14
    OpData21 = 0x15
    OpData22 = 0x16
    OpData23 = 0x17
    OpData24 = 0x18
    OpData25 = 0x19
    OpData26 = 0x1a
    OpData27 = 0x1b
    OpData28 = 0x1c
    OpData29 = 0x1d
    OpData30 = 0x1e
    OpData31 = 0x1f
    OpData32 = 0x20
    OpData33 = 0x21
    OpData34 = 0x22
    OpData35 = 0x23
    OpData36 = 0x24
    OpData37 = 0x25
    OpData38 = 0x26
    OpData39 = 0x27
    OpData40 = 0x28
    OpData41 = 0x29
    OpData42 = 0x2a
    OpData43 = 0x2b
    OpData44 = 0x2c
    OpData45 = 0x2d
    OpData46 = 0x2e
    OpData47 = 0x2f
    OpData48 = 0x30
    OpData49 = 0x31
    OpData50 = 0x32
    OpData51 = 0x33
    OpData52 = 0x34
    OpData53 = 0x35
    OpData54 = 0x36
    OpData55 = 0x37
    OpData56 = 0x38
    OpData57 = 0x39
    OpData58 = 0x3a
    OpData59 = 0x3b
    OpData60 = 0x3c
    OpData61 = 0x3d
    OpData62 = 0x3e
    OpData63 = 0x3f
    OpData64 = 0x40
    OpData65 = 0x41
    OpData66 = 0x42
    OpData67 = 0x43
    OpData68 = 0x44
    OpData69 = 0x45
    OpData70 = 0x46
    OpData71 = 0x47
    OpData72 = 0x48
    OpData73 = 0x49
    OpData74 = 0x4a
    OpData75 = 0x4b

    OpPushData1 = 0x4c
    OpPushData2 = 0x4d
    OpPushData4 = 0x4e

    Op1Negate = 0x4f

    OpReserved = 0x50

    OpTrue = 0x51

    Op2 = 0x52
    Op3 = 0x53
    Op4 = 0x54
    Op5 = 0x55
    Op6 = 0x56
    Op7 = 0x57
    Op8 = 0x58
    Op9 = 0x59
    Op10 = 0x5a
    Op11 = 0x5b
    Op12 = 0x5c
    Op13 = 0x5d
    Op14 = 0x5e
    Op15 = 0x5f
    Op16 = 0x60

    OpNop = 0x61
    OpVer = 0x62
    OpIf = 0x63
    OpNotIf = 0x64
    OpVerIf = 0x65
    OpVerNotIf = 0x66

    OpElse = 0x67
    OpEndIf = 0x68
    OpVerify = 0x69
    OpReturn = 0x6a
    OpToAltStack = 0x6b
    OpFromAltStack = 0x6c

    Op2Drop = 0x6d
    Op2Dup = 0x6e
    Op3Dup = 0x6f
    Op2Over = 0x70
    Op2Rot = 0x71
    Op2Swap = 0x72
    OpIfDup = 0x73
    OpDepth = 0x74
    OpDrop = 0x75
    OpDup = 0x76
    OpNip = 0x77
    OpOver = 0x78
    OpPick = 0x79

    OpRoll = 0x7a
    OpRot = 0x7b
    OpSwap = 0x7c
    OpTuck = 0x7d

    # Splice opcodes.
    OpCat = 0x7e
    OpSubStr = 0x7f
    OpLeft = 0x80
    OpRight = 0x81

    OpSize = 0x82

    # Bitwise logic opcodes.
    OpInvert = 0x83
    OpAnd = 0x84
    OpOr = 0x85
    OpXor = 0x86

    OpEqual = 0x87
    OpEqualVerify = 0x88

    OpReserved1 = 0x89
    OpReserved2 = 0x8a

    # Numeric related opcodes.
    Op1Add = 0x8b
    Op1Sub = 0x8c
    Op2Mul = 0x8d
    Op2Div = 0x8e
    OpNegate = 0x8f
    OpAbs = 0x90
    OpNot = 0x91
    Op0NotEqual = 0x92

    OpAdd = 0x93
    OpSub = 0x94
    OpMul = 0x95
    OpDiv = 0x96
    OpMod = 0x97
    OpLShift = 0x98
    OpRShift = 0x99

    OpBoolAnd = 0x9a
    OpBoolOr = 0x9b

    OpNumEqual = 0x9c
    OpNumEqualVerify = 0x9d
    OpNumNotEqual = 0x9e

    OpLessThan = 0x9f
    OpGreaterThan = 0xa0
    OpLessThanOrEqual = 0xa1
    OpGreaterThanOrEqual = 0xa2
    OpMin = 0xa3
    OpMax = 0xa4
    OpWithin = 0xa5

    # Undefined opcodes.
    OpUnknown166 = 0xa6
    OpUnknown167 = 0xa7

    # Crypto opcodes.
    OpSHA256 = 0xa8

    OpCheckMultiSigECDSA = 0xa9

    OpBlake2b = 0xaa
    OpCheckSigECDSA = 0xab
    OpCheckSig = 0xac
    OpCheckSigVerify = 0xad
    OpCheckMultiSig = 0xae
    OpCheckMultiSigVerify = 0xaf
    OpCheckLockTimeVerify = 0xb0
    OpCheckSequenceVerify = 0xb1

    # Undefined opcodes.
    OpUnknown178 = 0xb2
    OpUnknown179 = 0xb3
    OpUnknown180 = 0xb4
    OpUnknown181 = 0xb5
    OpUnknown182 = 0xb6
    OpUnknown183 = 0xb7
    OpUnknown184 = 0xb8
    OpUnknown185 = 0xb9
    OpUnknown186 = 0xba
    OpUnknown187 = 0xbb
    OpUnknown188 = 0xbc
    OpUnknown189 = 0xbd
    OpUnknown190 = 0xbe
    OpUnknown191 = 0xbf
    OpUnknown192 = 0xc0
    OpUnknown193 = 0xc1
    OpUnknown194 = 0xc2
    OpUnknown195 = 0xc3
    OpUnknown196 = 0xc4
    OpUnknown197 = 0xc5
    OpUnknown198 = 0xc6
    OpUnknown199 = 0xc7
    OpUnknown200 = 0xc8
    OpUnknown201 = 0xc9
    OpUnknown202 = 0xca
    OpUnknown203 = 0xcb
    OpUnknown204 = 0xcc
    OpUnknown205 = 0xcd
    OpUnknown206 = 0xce
    OpUnknown207 = 0xcf
    OpUnknown208 = 0xd0
    OpUnknown209 = 0xd1
    OpUnknown210 = 0xd2
    OpUnknown211 = 0xd3
    OpUnknown212 = 0xd4
    OpUnknown213 = 0xd5
    OpUnknown214 = 0xd6
    OpUnknown215 = 0xd7
    OpUnknown216 = 0xd8
    OpUnknown217 = 0xd9
    OpUnknown218 = 0xda
    OpUnknown219 = 0xdb
    OpUnknown220 = 0xdc
    OpUnknown221 = 0xdd
    OpUnknown222 = 0xde
    OpUnknown223 = 0xdf
    OpUnknown224 = 0xe0
    OpUnknown225 = 0xe1
    OpUnknown226 = 0xe2
    OpUnknown227 = 0xe3
    OpUnknown228 = 0xe4
    OpUnknown229 = 0xe5
    OpUnknown230 = 0xe6
    OpUnknown231 = 0xe7
    OpUnknown232 = 0xe8
    OpUnknown233 = 0xe9
    OpUnknown234 = 0xea
    OpUnknown235 = 0xeb
    OpUnknown236 = 0xec
    OpUnknown237 = 0xed
    OpUnknown238 = 0xee
    OpUnknown239 = 0xef
    OpUnknown240 = 0xf0
    OpUnknown241 = 0xf1
    OpUnknown242 = 0xf2
    OpUnknown243 = 0xf3
    OpUnknown244 = 0xf4
    OpUnknown245 = 0xf5
    OpUnknown246 = 0xf6
    OpUnknown247 = 0xf7
    OpUnknown248 = 0xf8
    OpUnknown249 = 0xf9

    OpSmallInteger = 0xfa
    OpPubKeys = 0xfb
    OpUnknown252 = 0xfc
    OpPubKeyHash = 0xfd
    OpPubKey = 0xfe
    OpInvalidOpCode = 0xff


def sign_message(message: str, private_key: PrivateKey, no_aux_rand: Optional[bool] = False) -> str: ...

def verify_message(message: str, signature: str, public_key: PublicKey) -> bool: ...

def sign_transaction(tx: Transaction, signer: list[PrivateKey], verify_sig: bool) -> Transaction: ...

def create_input_signature(tx: Transaction, input_index: int, private_key: PrivateKey, sighash_type: Optional[SighashType] = None) -> str: ...

def sign_script_hash(script_hash: str, privkey: PrivateKey) -> str: ...

class Generator:

    def __init__(
        self,
        network_id: str,
        entries: list[Union[UtxoEntryReference, dict]],
        change_address: Address,
        outputs: Optional[list[Union[PaymentOutput, dict]]] = None,
        payload: Optional[Union[str, bytes, list[int]]] = None,
        priority_fee: Optional[int] = None,
        priority_entries: Optional[list[Union[UtxoEntryReference, dict]]] = None,
        sig_op_count: Optional[int] = None,
        minimun_signatures: Optional[int] = None
    ) -> None: ...

    def estimate(
        self,
        network_id: str,
        entries: list[dict],
        outputs: list[dict],
        change_address: Address,
        payload: Optional[str],
        priority_fee: Optional[str],
        priority_entries: Optional[list[dict]],
        sig_op_count: Optional[int],
        minimun_signatures: Optional[int]
    ) -> GeneratorSummary: ...

    def summary(self) -> GeneratorSummary: ...

    def __iter__(self) -> Iterator[PendingTransaction]: ...

    def __next__(self) -> PendingTransaction: ...


class PendingTransaction:

    @property
    def id(self) -> str: ...

    @property
    def payment_amount(self) -> Optional[int]: ...

    @property
    def change_amount(self) -> int: ...

    @property
    def fee_amount(self) -> int: ...

    @property
    def mass(self) -> int: ...

    @property
    def minimum_signatures(self) -> int: ...

    @property
    def aggregate_input_amount(self) -> int: ...

    @property
    def aggregate_output_amount(self) -> int: ...

    @property
    def transaction_type(self) -> str: ...

    def addresses(self) -> list[Address]: ...

    def get_utxo_entries(self) -> list[UtxoEntryReference]: ...

    def create_input_signature(self, input_index: int, private_key: PrivateKey, sighash_type: Optional[SighashType] = None) -> str: ...

    def fill_input(self, input_index: int, signature_script: Union[str, bytes, list[int]]) -> None: ...

    def sign_input(self, input_index: int, private_key: PrivateKey, sighash_type: Optional[SighashType] = None) -> None: ...

    def sign(self, private_keys: list[PrivateKey], check_fully_signed: Optional[bool] = None) -> None: ...

    def submit(self, rpc_client: RpcClient) -> str: ...

    @property
    def transaction(self) -> Transaction: ...


class GeneratorSummary:

    @property
    def network_type(self) -> str: ...

    @property
    def utxos(self) -> int: ...

    @property
    def fees(self) -> int: ...

    @property
    def transactions(self) -> int: ...

    @property
    def final_amount(self) -> Optional[int]: ...

    @property
    def final_transaction_id(self) -> Optional[str]: ...

def maximum_standard_transaction_mass() -> int: ...

def calculate_transaction_fee(network_id: str, tx: Transaction, minimum_signatures: Optional[int] = None) -> Optional[int]: ...

def calculate_transaction_mass(network_id: str, tx: Transaction, minimum_signatures: Optional[int] = None) -> int: ...

def update_transaction_mass(network_id: str, tx: Transaction, minimum_signatures: Optional[int] = None) -> bool: ...

def calculate_storage_mass(network_id: str, input_values: list[int], output_values: list[int]) -> int: ...

def create_transaction(
    utxo_entry_source: list[dict],
    outputs: list[dict],
    priority_fee: int,
    payload: Optional[list[int]] = None,
    sig_op_count: Optional[int] = None
) -> Transaction: ...

class CreateTransactionsDict(TypedDict):
    transactions: list[PendingTransaction]
    summary: GeneratorSummary

def create_transactions(
    network_id: str,
    entries: list[dict],
    change_address: Address,
    outputs: Optional[list[dict]] = None,
    payload: Optional[str] = None,
    priority_fee: Optional[int] = None,
    priority_entries: Optional[list[dict]] = None,
    sig_op_count: Optional[int] = None,
    minimum_signatures: Optional[int] = None
) -> CreateTransactionsDict: ...

def estimate_transactions(
    network_id: str,
    entries: list[dict],
    change_address: Address,
    outputs: Optional[list[dict]] = None,
    payload: Optional[str] = None,
    priority_fee: Optional[int] = None,
    priority_entries: Optional[list[dict]] = None,
    sig_op_count: Optional[int] = None,
    minimum_signatures: Optional[int] = None
) -> GeneratorSummary: ...

def kaspa_to_sompi(kaspa: float) -> int: ...

def sompi_to_kaspa(sompi: int) -> float: ...

def sompi_to_kaspa_string_with_suffix(sompi: int, network: str) -> str: ...

class AccountKind:

    def __init__(self, kind: str) -> None: ...

    def __str__(self) -> str: ...

    def to_string(self) -> str: ...


def create_multisig_address(
    minimum_signatures: int,
    keys: list[PublicKey],
    network_type: str,
    ecdsa: Optional[bool] = None,
    account_kind: Optional[str] = None,
) -> Address: ...

class PaymentOutput:

    def __init__(self, address: Address, amount: int) -> None: ...


class DerivationPath:

    def __init__(self, path: str) -> None: ...

    def is_empty(self) -> bool: ...

    def length(self) -> int: ...

    def parent(self) -> Optional[DerivationPath]: ...

    def push(self, child_number: int, hardened: Optional[bool] = None) -> None: ...

    def to_string(self) -> str: ...


class Keypair:
    def __init__(self, secret_key: str, public_key: str, xonly_public_key: str) -> None: ...

    @property
    def xonly_public_key(self) -> str: ...

    @property
    def public_key(self) -> str: ...

    @property
    def private_key(self) -> str: ...
        
    def to_address(self, network: str) -> Address: ...

    def to_address_ecdsa(self, network: str) -> Address: ...

    @staticmethod
    def random() -> Keypair: ...

    @staticmethod
    def from_private_key(secret_key: PrivateKey) -> Keypair: ...


class PrivateKey:

    def __init__(self, secret_key: str) -> None: ...

    def to_string(self) -> str: ...

    def to_public_key(self) -> PublicKey: ...

    def to_address(self, network: str) -> Address: ...

    def to_address_ecdsa(self, network: str) -> Address: ...

    def to_keypair(self) -> Keypair: ...


class PrivateKeyGenerator:

    def __init__(self, xprv: str, is_multisig: bool,
                 account_index: int, cosigner_index: Optional[int] = None) -> str: ...

    def receive_key(self, index: int) -> PrivateKey: ...

    def change_key(self, index: int) -> PrivateKey: ...


class PublicKey:

    def __init__(self, key: str) -> None: ...

    def to_string(self) -> str: ...

    def to_address(self, network: str) -> Address: ...

    def to_address_ecdsa(self, network: str) -> Address: ...

    def to_x_only_public_key(self) -> XOnlyPublicKey: ...


class PublicKeyGenerator:

    @staticmethod
    def from_xpub(kpub: str, cosigner_index: Optional[int] = None) ->PublicKeyGenerator: ...

    @staticmethod
    def from_master_xprv(xprv: str, is_multisig: bool, account_index: int, cosigner_index: Optional[int] = None) -> PublicKeyGenerator: ...

    def receive_pubkeys(self, start: int, end: int) -> list[PublicKey]: ...

    def receive_pubkey(self, index: int) -> list[PublicKey]: ...

    def receive_pubkeys_as_strings(self, start: int, end: int) -> list[str]: ...

    def receive_pubkey_as_string(self, index: int) -> str: ...

    def receive_addresses(self, network_type: str, start: int, end: int) -> list[Address]: ...

    def receive_address(self, network_type: str, index: int) -> Address: ...

    def receive_addresses_as_strings(self, network_type: str, start: int, end: int) -> list[str]: ...

    def receive_address_as_string(self, network_type: str, index: int) -> str: ...

    def change_pubkeys(self, start: int, end: int) -> list[PublicKey]: ...

    def change_pubkey(self, index: int) -> list[PublicKey]: ...

    def change_pubkeys_as_strings(self, start: int, end: int) -> list[str]: ...

    def change_pubkey_as_string(self, index: int) -> str: ...

    def change_addresses(self, network_type: str, start: int, end: int) -> list[Address]: ...

    def change_address(self, network_type: str, index: int) -> Address: ...

    def change_addresses_as_strings(self, network_type: str, start: int, end: int) -> list[str]: ...

    def change_address_as_string(self, network_type: str, index: int) -> str: ...

    def to_string(self) -> str: ...


class XOnlyPublicKey:

    def __init__(self, key: str) -> None: ...

    def to_string(self) -> str: ...

    def to_address(self, network: str) -> Address: ...

    def to_address_ecdsa(self, network: str) -> Address: ...

    @staticmethod
    def from_address(address: Address) -> XOnlyPublicKey: ...


class XPrv:

    def __init__(self, seed: str) -> None: ...

    @staticmethod
    def from_xprv(xprv: str) -> XPrv: ...

    def derive_child(self, child_number: int, hardened: Optional[bool] = None) -> XPrv: ...

    def derive_path(self, path: Union[str, DerivationPath]) -> XPrv: ...

    def into_string(self, prefix: str) -> str: ...

    def to_string(self) -> str: ...

    def to_xpub(self) -> XPub: ...

    def to_private_key(self) -> PrivateKey: ...

    @property
    def xprv(self) -> str: ...

    @property
    def private_key(self) -> str: ...

    @property
    def depth(self) -> int: ...

    @property
    def parent_fingerprint(self) -> int: ...

    @property
    def child_number(self) -> int: ...

    @property
    def chain_code(self) -> str: ...


class XPub:

    def __init__(self, xpub: str) -> None: ...

    def derive_child(self, child_number: int, hardened: Optional[bool] = None) -> XPub: ...

    def derive_path(self, path: str) -> XPub: ...

    def to_str(self, prefix: str) -> str: ...

    def to_public_key(self) -> PublicKey: ...

    @property
    def xpub(self) -> str: ...

    @property
    def depth(self) -> int: ...

    @property
    def parent_fingerprint(self) -> str: ...

    @property
    def child_number(self) -> int: ...

    @property
    def chain_code(self) -> str: ...


class Resolver:

    def __init__(self, urls: Optional[list[str]] = None, tls: Optional[int] = None) -> None: ...

    def urls(self) -> list[str]: ...

    def get_node(self, encoding: str, network_id: str) -> dict: ...

    def get_url(self, encoding: str, network_id: str) -> str: ...

    def connect(self, encoding: str, network_id: str) -> RpcClient: ...


class RpcClient:

    def __init__(self, resolver: Optional[Resolver] = None, url: Optional[str] = None, encoding: Optional[str] = None, network_id: Optional[str] = None) -> None: ...

    @property
    def url(self) -> str: ...

    @property
    def resolver(self) -> Optional[Resolver]: ...

    def set_resolver(self, Resolver) -> None: ...

    def set_network_id(self, network_id: str) -> None: ...

    @property
    def is_connected(self) -> bool: ...

    @property
    def encoding(self) -> str: ...

    @property
    def node_id(self) -> str: ...

    async def connect(self, block_async_connect: Optional[bool] = None, strategy: Optional[str] = None, url: Optional[str] = None, timeout_duration: Optional[int] = None, retry_interval: Optional[int] = None) -> None: ...

    async def disconnect(self) -> None: ...

    async def start(self) -> None: ...

    # def trigger_abort(self) -> None: ...

    def add_event_listener(self, event: str, callback: Callable[..., Any], *args: Any, **kwargs: Optional[Any]) -> None: ...

    def remove_event_listener(self, event: str, callback: Callable[..., Any] = None) -> None: ...

    def remove_all_event_listeners(self) -> None: ...

    # @staticmethod
    # def default_port(encoding: str, network: str) -> int: ...

    # @staticmethod
    # def parse_url(url: str, encoding: str, network: str) -> str: ...

    async def subscribe_utxos_changed(self, addresses: list[Address]) -> None: ...
    
    async def unsubscribe_utxos_changed(self, addresses: list[Address]) -> None: ...

    async def subscribe_virtual_chain_changed(self, include_accepted_transaction_ids: bool) -> None: ...
    
    async def unsubscribe_virtual_chain_changed(self, include_accepted_transaction_ids: bool) -> None: ...

    async def subscribe_block_added(self) -> None: ...
    
    async def unsubscribe_block_added(self) -> None: ...

    async def subscribe_finality_conflict(self) -> None: ...
    
    async def unsubscribe_finality_conflict(self) -> None: ...

    async def subscribe_finality_conflict_resolved(self) -> None: ...
    
    async def unsubscribe_finality_conflict_resolved(self) -> None: ...

    async def subscribe_new_block_template(self) -> None: ...
    
    async def unsubscribe_new_block_template(self) -> None: ...

    async def subscribe_pruning_point_utxo_set_override(self) -> None: ...
    
    async def unsubscribe_pruning_point_utxo_set_override(self) -> None: ...
    
    async def subscribe_sink_blue_score_changed(self) -> None: ...
    
    async def unsubscribe_sink_blue_score_changed(self) -> None: ...
    
    async def subscribe_virtual_daa_score_changed(self) -> None: ...
    
    async def unsubscribe_virtual_daa_score_changed(self) -> None: ...

    async def get_block_count(self, request: Optional[dict] = None) -> dict: ...
    
    async def get_block_dag_info(self, request: Optional[dict] = None) -> dict: ...
    
    async def get_coin_supply(self, request: Optional[dict] = None) -> dict: ...
    
    async def get_connected_peer_info(self, request: Optional[dict] = None) -> dict: ...
    
    async def get_info(self, request: Optional[dict] = None) -> dict: ...
    
    async def get_peer_addresses(self, request: Optional[dict] = None) -> dict: ...
            
    async def get_sink(self, request: Optional[dict] = None) -> dict: ...
    
    async def get_sink_blue_score(self, request: Optional[dict] = None) -> dict: ...
    
    async def ping(self, request: Optional[dict] = None) -> dict: ...
    
    async def shutdown(self, request: Optional[dict] = None) -> dict: ...
    
    async def get_server_info(self, request: Optional[dict] = None) -> dict: ...
    
    async def get_sync_status(self, request: Optional[dict] = None) -> dict: ...
    
    async def add_peer(self, request: dict) -> dict: ...
    
    async def ban(self, request: dict) -> dict: ...
    
    async def estimate_network_hashes_per_second(self, request: dict) -> dict: ...
    
    async def get_balance_by_address(self, request: dict) -> dict: ...
    
    async def get_balances_by_addresses(self, request: dict) -> dict: ...
    
    async def get_block(self, request: dict) -> dict: ...
    
    async def get_blocks(self, request: dict) -> dict: ...
    
    async def get_block_template(self, request: dict) -> dict: ...
    
    async def get_connections(self, request: dict) -> dict: ...

    async def get_current_block_color(self, request: dict) -> dict: ...
    
    async def get_daa_score_timestamp_estimate(self, request: dict) -> dict: ...
    
    async def get_fee_estimate(self, request: dict) -> dict: ...
    
    async def get_fee_estimate_experimental(self, request: dict) -> dict: ...
    
    async def get_current_network(self, request: dict) -> dict: ...
    
    async def get_headers(self, request: dict) -> dict: ...
    
    async def get_mempool_entries(self, request: dict) -> dict: ...
    
    async def get_mempool_entries_by_addresses(self, request: dict) -> dict: ...
    
    async def get_mempool_entry(self, request: dict) -> dict: ...
    
    async def get_metrics(self, request: dict) -> dict: ...

    async def get_subnetwork(self, request: dict) -> dict: ...
    
    async def get_utxos_by_addresses(self, request: dict) -> dict: ...

    async def get_utxo_return_address(self, request: dict) -> dict: ...

    async def get_virtual_chain_from_block(self, request: dict) -> dict: ...
    
    async def resolve_finality_conflict(self, request: dict) -> dict: ...
    
    # async def submit_block(self, request: dict) -> dict: ...
    
    async def submit_transaction(self, request: dict) -> dict: ...
    
    async def submit_transaction_replacement(self, request: dict) -> dict: ...

    async def unban(self, request: dict) -> dict: ...
