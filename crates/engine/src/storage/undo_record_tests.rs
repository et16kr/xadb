use super::*;
use crate::{
    api::{DataType, DatabaseId},
    storage::page::{FileId, PAGE_BYTES, PageLsn},
};
use foundation::identity::CommitSeq;

// Frozen preimplementation Python struct/hashlib vectors, not candidate output.
const LITERALS: [&str; 20] = [
    "585552310100000006000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "5855523101000100060000000000000064000000000000000102030405060708090a0b0c0d0e0f101100000000000000120000000000000013000000000000001400000000000000150000000000000001000000000000000100000000000000050000000000000006000000000000000201000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "5855523101000100060000000000000064000000000000000102030405060708090a0b0c0d0e0f10110000000000000012000000000000001300000000000000140000000000000015000000000000000100000000000000010000000000000005000000000000000600000000000000020100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001000000000000001600000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000102030405060708090a0b0c0d0e0f10110000000000000011000000000000001700000000000000180000000000000000000000010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "5855523101000100060000000000000064000000000000000102030405060708090a0b0c0d0e0f10110000000000000012000000000000001300000000000000140000000000000015000000000000000500000000000000010000000000000005000000000000000600000000000000020100000000000002000000010000000300000000000000040000000000000005000000000000000600000000000000630000000000000002000000010000000300000000000000040000000000000005000000000000000600000000000000630000000000000002000000000000000102030405060708090a0b0c0d0e0f1011000000000000001200000000000000130000000000000014000000000000000102030405060708090a0b0c0d0e0f1011000000000000001200000000000000130000000000000014000000000000001f0000000201010100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000500000000010102feffffffffffffff030300000000c3a9040200000000ff",
    "5855523101000100060000000000000064000000000000000102030405060708090a0b0c0d0e0f10110000000000000012000000000000001300000000000000140000000000000015000000000000000100000000000000010000000000000005000000000000000600000000000000020100000000000002000000010000000300000000000000040000000000000005000000000000000600000000000000630000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001000000000000001600000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000102030405060708090a0b0c0d0e0f101100000000000000110000000000000017000000000000001800000000000000010800000202010200000000000000000200000001000000280000000000000004000000000000000108000001000000bff27d226a616eb526eea24d35af880abab1456476cca73dd28b33205a832ddd0000000000000000000000000000000000000000000000000000000000000000",
    "5855523101000100060000000000000064000000000000000102030405060708090a0b0c0d0e0f1011000000000000001200000000000000130000000000000014000000000000001500000000000000010000000000000002010a000000000005000000000000000600000000000000020100000000000002000000010000000300000000000000040000000000000005000000000000000600000000000000630000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001010000000000000102",
    "5855523101000100060000000000000064000000000000000102030405060708090a0b0c0d0e0f1011000000000000001200000000000000130000000000000014000000000000001500000000000000010000000000000002010a000000000005000000000000000600000000000000020100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001000000000000001600000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000102030405060708090a0b0c0d0e0f1011000000000000001100000000000000170000000000000018000000000000000000000003000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001010000000000000102",
    "5855523101000100060000000000000064000000000000000102030405060708090a0b0c0d0e0f1011000000000000001200000000000000130000000000000014000000000000001500000000000000020000000000000002010a000000000005000000000000000600000000000000020100000000000002000000010000000300000000000000040000000000000005000000000000000600000000000000630000000000000002000000010000000300000000000000040000000000000005000000000000000600000000000000630000000000000002000000000000000102030405060708090a0b0c0d0e0f1011000000000000001200000000000000130000000000000014000000000000000102030405060708090a0b0c0d0e0f1011000000000000001200000000000000130000000000000014000000000000000000000004000101000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001010000000000000102",
    "5855523101000200060000000000000064000000000000000102030405060708090a0b0c0d0e0f10110000000000000012000000000000001300000000000000140000000000000015000000000000000000000000000000010000000000000005000000000000000600000000000000020100000000000002000000010000000300000000000000040000000000000005000000000000000600000000000000630000000000000002000000010000000300000000000000040000000000000005000000000000000600000000000000630000000000000002000000000000000102030405060708090a0b0c0d0e0f1011000000000000001200000000000000130000000000000014000000000000000102030405060708090a0b0c0d0e0f1011000000000000001200000000000000130000000000000014000000000000001f0000000201010100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000500000000010102feffffffffffffff030300000000c3a9040200000000ff",
    "5855523101000200060000000000000064000000000000000102030405060708090a0b0c0d0e0f10110000000000000012000000000000001300000000000000140000000000000015000000000000000000000000000000010000000000000005000000000000000600000000000000020100000000000002000000010000000300000000000000040000000000000005000000000000000600000000000000630000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002000000000000000102030405060708090a0b0c0d0e0f1011000000000000001200000000000000130000000000000014000000000000000102030405060708090a0b0c0d0e0f1011000000000000001200000000000000130000000000000014000000000000001f0000000201010301000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000500000000010102feffffffffffffff030300000000c3a9040200000000ff",
    "5855523101000200060000000000000064000000000000000102030405060708090a0b0c0d0e0f10110000000000000012000000000000001300000000000000140000000000000015000000000000000000000000000000010000000000000005000000000000000600000000000000020100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002000000000000000102030405060708090a0b0c0d0e0f1011000000000000001200000000000000130000000000000014000000000000000102030405060708090a0b0c0d0e0f1011000000000000001200000000000000130000000000000014000000000000001f0000000201030302000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000500000000010102feffffffffffffff030300000000c3a9040200000000ff",
    "5855523101000200060000000000000064000000000000000102030405060708090a0b0c0d0e0f10110000000000000012000000000000001300000000000000140000000000000015000000000000000000000000000000010000000000000005000000000000000600000000000000020100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000102030405060708090a0b0c0d0e0f1011000000000000001200000000000000130000000000000014000000000000001f0000000201030303000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000500000000010102feffffffffffffff030300000000c3a9040200000000ff",
    "5855523101000200060000000000000064000000000000000102030405060708090a0b0c0d0e0f10110000000000000012000000000000001300000000000000140000000000000015000000000000000000000000000000010000000000000005000000000000000600000000000000020100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000102030405060708090a0b0c0d0e0f101100000000000000110000000000000017000000000000001800000000000000bd3e0000020203030300000000000000020000000100000028000000000000000400000000000000bd3e00000200000072aea06c41ba244a7562dd0b7d133f6e0eca4cfc02e54e1b5ba9c785579f98e50200000001000000280000000000000004000000000000000000000000000000",
    "5855523101000200060000000000000064000000000000000102030405060708090a0b0c0d0e0f10110000000000000012000000000000001300000000000000140000000000000015000000000000000000000000000000010000000000000005000000000000000600000000000000020100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000102030405060708090a0b0c0d0e0f101100000000000000110000000000000017000000000000001800000000000000bd3e0000020203030300000001000000020000000100000028000000000000000400000000000000bd3e00000200000072aea06c41ba244a7562dd0b7d133f6e0eca4cfc02e54e1b5ba9c785579f98e50200000001000000290000000000000004000000000000000000000000000000",
    "5855523101000200060000000000000064000000000000000102030405060708090a0b0c0d0e0f10110000000000000012000000000000001300000000000000140000000000000015000000000000000000000000000000010000000000000005000000000000000600000000000000020100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000102030405060708090a0b0c0d0e0f101100000000000000110000000000000017000000000000001800000000000000bd3e0000020203030300000002000000020000000100000028000000000000000400000000000000bd3e00000200000072aea06c41ba244a7562dd0b7d133f6e0eca4cfc02e54e1b5ba9c785579f98e50000000000000000000000000000000000000000000000000000000000000000",
    "0200000001000000280000000000000004000000000000000108000001000000bff27d226a616eb526eea24d35af880abab1456476cca73dd28b33205a832ddd",
    "0100000000000000050000000000000006000000000000000201000000000000",
    "02010a000000000005000000000000000600000000000000020100000000000001010000000000000102",
    "0300000000000000050000000000000006000000000000000201000000000000",
    "58554331010000000100000001080000010800000000000002000000010000001e000000000000000400000000000000000000000000000006000000000000006400000000000000020000000100000028000000000000000400000000000000000000000000000000000000000000000000000000000000bff27d226a616eb526eea24d35af880abab1456476cca73dd28b33205a832ddd11000000000000000100000004f8070000031425364758697a8b9cadbecfe0f102132435465768798a9bacbdcedff00112233445566778899aabbccddeef00112233445566778899aabbccddeeff102132435465768798a9bacbdcedfe0f2031425364758697a8b9cadbecfd0e1f30415263748596a7b8c9daebfc0d1e2f405162738495a6b7c8d9eafb0c1d2e3f5061728394a5b6c7d8e9fa0b1c2d3e4f60718293a4b5c6d7e8f90a1b2c3d4e5f708192a3b4c5d6e7f8091a2b3c4d5e6f8091a2b3c4d5e6f708192a3b4c5d6e7f90a1b2c3d4e5f60718293a4b5c6d7e8fa0b1c2d3e4f5061728394a5b6c7d8e9fb0c1d2e3f405162738495a6b7c8d9eafc0d1e2f30415263748596a7b8c9daebfd0e1f2031425364758697a8b9cadbecfe0f102132435465768798a9bacbdcedff00112233445566778899aabbccddeef00112233445566778899aabbccddeeff102132435465768798a9bacbdcedfe0f2031425364758697a8b9cadbecfd0e1f30415263748596a7b8c9daebfc0d1e2f405162738495a6b7c8d9eafb0c1d2e3f5061728394a5b6c7d8e9fa0b1c2d3e4f60718293a4b5c6d7e8f90a1b2c3d4e5f708192a3b4c5d6e7f8091a2b3c4d5e6f8091a2b3c4d5e6f708192a3b4c5d6e7f90a1b2c3d4e5f60718293a4b5c6d7e8fa0b1c2d3e4f5061728394a5b6c7d8e9fb0c1d2e3f405162738495a6b7c8d9eafc0d1e2f30415263748596a7b8c9daebfd0e1f2031425364758697a8b9cadbecfe0f102132435465768798a9bacbdcedff00112233445566778899aabbccddeef00112233445566778899aabbccddeeff102132435465768798a9bacbdcedfe0f2031425364758697a8b9cadbecfd0e1f30415263748596a7b8c9daebfc0d1e2f405162738495a6b7c8d9eafb0c1d2e3f5061728394a5b6c7d8e9fa0b1c2d3e4f60718293a4b5c6d7e8f90a1b2c3d4e5f708192a3b4c5d6e7f8091a2b3c4d5e6f8091a2b3c4d5e6f708192a3b4c5d6e7f90a1b2c3d4e5f60718293a4b5c6d7e8fa0b1c2d3e4f5061728394a5b6c7d8e9fb0c1d2e3f405162738495a6b7c8d9eafc0d1e2f30415263748596a7b8c9daebfd0e1f2031425364758697a8b9cadbecfe0f102132435465768798a9bacbdcedff00112233445566778899aabbccddeef00112233445566778899aabbccddeeff102132435465768798a9bacbdcedfe0f2031425364758697a8b9cadbecfd0e1f30415263748596a7b8c9daebfc0d1e2f405162738495a6b7c8d9eafb0c1d2e3f5061728394a5b6c7d8e9fa0b1c2d3e4f60718293a4b5c6d7e8f90a1b2c3d4e5f708192a3b4c5d6e7f8091a2b3c4d5e6f8091a2b3c4d5e6f708192a3b4c5d6e7f90a1b2c3d4e5f60718293a4b5c6d7e8fa0b1c2d3e4f5061728394a5b6c7d8e9fb0c1d2e3f405162738495a6b7c8d9eafc0d1e2f30415263748596a7b8c9daebfd0e1f2031425364758697a8b9cadbecfe0f102132435465768798a9bacbdcedff00112233445566778899aabbccddeef00112233445566778899aabbccddeeff102132435465768798a9bacbdcedfe0f2031425364758697a8b9cadbecfd0e1f30415263748596a7b8c9daebfc0d1e2f405162738495a6b7c8d9eafb0c1d2e3f5061728394a5b6c7d8e9fa0b1c2d3e4f60718293a4b5c6d7e8f90a1b2c3d4e5f708192a3b4c5d6e7f8091a2b3c4d5e6f8091a2b3c4d5e6f708192a3b4c5d6e7f90a1b2c3d4e5f60718293a4b5c6d7e8fa0b1c2d3e4f5061728394a5b6c7d8e9fb0c1d2e3f405162738495a6b7c8d9eafc0d1e2f30415263748596a7b8c9daebfd0e1f2031425364758697a8b9cadbecfe0f102132435465768798a9bacbdcedff00112233445566778899aabbccddeef00112233445566778899aabbccddeeff102132435465768798a9bacbdcedfe0f2031425364758697a8b9cadbecfd0e1f30415263748596a7b8c9daebfc0d1e2f405162738495a6b7c8d9eafb0c1d2e3f5061728394a5b6c7d8e9fa0b1c2d3e4f60718293a4b5c6d7e8f90a1b2c3d4e5f708192a3b4c5d6e7f8091a2b3c4d5e6f8091a2b3c4d5e6f708192a3b4c5d6e7f90a1b2c3d4e5f60718293a4b5c6d7e8fa0b1c2d3e4f5061728394a5b6c7d8e9fb0c1d2e3f405162738495a6b7c8d9eafc0d1e2f30415263748596a7b8c9daebfd0e1f2031425364758697a8b9cadbecfe0f102132435465768798a9bacbdcedff00112233445566778899aabbccddeef00112233445566778899aabbccddeeff102132435465768798a9bacbdcedfe0f2031425364758697a8b9cadbecfd0e1f30415263748596a7b8c9daebfc0d1e2f405162738495a6b7c8d9eafb0c1d2e3f5061728394a5b6c7d8e9fa0b1c2d3e4f60718293a4b5c6d7e8f90a1b2c3d4e5f708192a3b4c5d6e7f8091a2b3c4d5e6f8091a2b3c4d5e6f708192a3b4c5d6e7f90a1b2c3d4e5f60718293a4b5c6d7e8fa0b1c2d3e4f5061728394a5b6c7d8e9fb0c1d2e3f405162738495a6b7c8d9eafc0d1e2f30415263748596a7b8c9daebfd0e1f2031425364758697a8b9cadbecfe0f102132435465768798a9bacbdcedff00112233445566778899aabbccddeef00112233445566778899aabbccddeeff102132435465768798a9bacbdcedfe0f2031425364758697a8b9cadbecfd0e1f30415263748596a7b8c9daebfc0d1e2f405162738495a6b7c8d9eafb0c1d2e3f5061728394a5b6c7d8e9fa0b1c2d3e4f60718293a4b5c6d7e8f90a1b2c3d4e5f708192a3b4c5d6e7f8091a2b3c4d5e6f8091a2b3c4d5e6f708192a3b4c5d6e7f90a1b2c3d4e5f60718293a4b5c6d7e8fa0b1c2d3e4f5061728394a5b6c7d8e9fb0c1d2e3f405162738495a6b7c8d9eafc0d1e2f30415263748596a",
];
const HASHES: [&str; 20] = [
    "f897e222e7033965c225fe973cec4af0db948bff5a342dce86d36bc286819be7",
    "a093939a406f521e17f54d4f54e9cbee936c85a4dc849d0955df1511cfb38c5a",
    "7cd841f7a463827643cc469798dcda7eb8abc01cdcf59b53caba3863459d7bb7",
    "c267a18f8ac192c9e4e9908ba54695c18854abe1a11e2fc9a7f88e589269f8be",
    "ddd0de056440803507bc055f3b7ccd81b91f880a772e23bd9c982b1d94cc2c2c",
    "d63139c656444b38c85b0ff4e9b205910395669799a168fa167f4d11fad19ded",
    "34ba5452c0add1f4b3b25cd49be611554d2c5347b2c7924e695d52b7f9f1c166",
    "84bea8378fa74d5830e29c3177c778da2acbc4ba54eaf34ab67a57e0a6e04c3d",
    "9b4d53d36320daca7f19edde090abad813d828deb5e3e3ea267e67a321ff5159",
    "9968132d93477fef4c84626e6b1d3f7ac058337a0f5277d622d49b22ad81a03b",
    "b4ff6ce855858e8e099b4c0891436767378e3035302c73b0535a45e541b9656a",
    "e27fbe50a89eba49508af52d4f9747e05077b241b786c19f5d73b67603afa286",
    "04628be9babed93325eed27f0f77f0e183d60679de90e6813f2f26423365f6da",
    "7bb87e5c9855548ae38462971a4d3b022cbf4b9c1f009b1a07dfe157ba90b468",
    "1f70511604c79dc5bd7faaf3b1ce147cb3ffe967c4b67e4083c3ea79b81f4ea6",
    "dc0f4edfa69b72d3d4fcc816b408f62ad9d7e4b3373949e65e80adaf9cfda523",
    "b0dead0f7d06b790a670262d3335a713eee94ceb5367a30014c52cd1fbe0ae00",
    "8fc4cf31a0929fc00ea8a7127bc8e65dc111ce0f7e64f6ffe095e4190c93651a",
    "09398427aff017090632f025e984446b067287a7920aa09a1c212c20c922d184",
    "2221bcbac40ed59d5849c11eeb1b05376747052b581af53bed838a49473ea77e",
];

const KEY: &[u8] = &[1, 1, 0, 0, 0, 0, 0, 0, 1, 2];
fn hex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
fn literal(i: usize) -> Vec<u8> {
    hex(LITERALS[i])
}
fn g(n: u64) -> Generation {
    Generation::new(n).unwrap()
}
fn oid(n: u64) -> ObjectId {
    ObjectId::new(n).unwrap()
}
fn page(n: u64) -> PageRef {
    PageRef {
        file: FileId::new(0x1_0000_0002).unwrap(),
        page_no: n,
        incarnation: g(4),
    }
}
fn reference() -> UndoRef {
    UndoRef::new(page(30), 0, g(6)).unwrap()
}
fn id() -> TxnId {
    TxnId {
        database: DatabaseId::new(std::array::from_fn(|i| i as u8 + 1)).unwrap(),
        timeline: 17,
        writer_epoch: 18,
        local: 19,
    }
}
fn ns() -> Namespace {
    Namespace {
        database: id().database,
        timeline: 17,
    }
}
fn target(kind: UndoTargetKind) -> UndoTargetView<'static> {
    UndoTargetView {
        kind,
        owner: oid(5),
        generation: g(6),
        entity: 258,
        arity: if kind == UndoTargetKind::Index { 1 } else { 0 },
        key: if kind == UndoTargetKind::Index {
            KEY
        } else {
            &[]
        },
    }
}
fn expected(kind: UndoTargetKind) -> UndoRecordExpectation<'static> {
    UndoRecordExpectation {
        namespace: ns(),
        reference: reference(),
        target: target(kind),
    }
}
fn identity(n: u64) -> PageIdentity {
    PageIdentity {
        database: id().database,
        page: page(n),
        owner: oid(9),
        owner_generation: g(10),
    }
}
fn header(n: u64) -> PageHeader {
    PageHeader {
        identity: identity(n),
        kind: PageKind::Undo,
        level: 0,
        lsn: PageLsn(25),
    }
}
fn link() -> UndoEdge {
    UndoEdge::Linked(UndoLink {
        reference: UndoRef::new(page(3), 5, g(6)).unwrap(),
        sequence: 99,
    })
}
fn edge(tag: u8) -> UndoEdge {
    match tag {
        0 => UndoEdge::Never,
        1 => link(),
        2 => UndoEdge::Cut,
        3 => UndoEdge::Released,
        _ => panic!(),
    }
}
fn column(ty: DataType, nullable: bool) -> Column {
    Column {
        name: "c".into(),
        data_type: ty,
        nullable,
    }
}
fn binary_schema() -> Vec<Column> {
    vec![column(DataType::Binary, false)]
}
fn inline_schema() -> Vec<Column> {
    vec![
        column(DataType::Bool, true),
        column(DataType::Bool, false),
        column(DataType::Int64, false),
        column(DataType::Utf8, false),
        column(DataType::Binary, false),
    ]
}
fn binary(payload: usize) -> Vec<u8> {
    let mut out = vec![1, 0, 0, 0, 4];
    out.extend_from_slice(&(payload as u32).to_le_bytes());
    out.extend((0..payload).map(|i| (17 * i + 3) as u8));
    out
}
struct Fixtures {
    inline: Vec<u8>,
    big: Vec<u8>,
    two: Vec<u8>,
}
impl Fixtures {
    fn new() -> Self {
        Self {
            inline: hex("0500000000010102feffffffffffffff030300000000c3a9040200000000ff"),
            big: binary(2040),
            two: binary(16052),
        }
    }
    fn head(&self, two: bool) -> UndoPayloadHead {
        let bytes = if two { &self.two } else { &self.big };
        UndoPayloadHead::new(page(40), bytes.len(), content_digest(bytes)).unwrap()
    }
    fn record(&self, i: usize) -> UndoRecord<'_> {
        let kind = if (5..8).contains(&i) {
            UndoTargetKind::Index
        } else {
            UndoTargetKind::Row
        };
        let before = match i {
            1 | 5 => UndoBeforeKind::Missing,
            2 => UndoBeforeKind::RowDeleted,
            6 => UndoBeforeKind::IndexAbsent,
            7 => UndoBeforeKind::IndexPresent,
            _ => UndoBeforeKind::RowValue,
        };
        let own = matches!(i, 3 | 7 | 8..=11);
        let stamp = if before == UndoBeforeKind::Missing {
            None
        } else if own {
            Some(VersionStamp::Transaction(TxnRef::new(id(), g(20)).unwrap()))
        } else {
            Some(VersionStamp::Committed(CommitSeq::new(22).unwrap()))
        };
        let logical = if before == UndoBeforeKind::Missing {
            None
        } else {
            Some(
                LogicalRowVersion::new(
                    if own {
                        id()
                    } else {
                        TxnId {
                            writer_epoch: 17,
                            local: 23,
                            ..id()
                        }
                    },
                    if own { 20 } else { 24 },
                )
                .unwrap(),
            )
        };
        let payload = match i {
            3 | 8..=11 => UndoPayload::Inline(&self.inline),
            4 => UndoPayload::Chunks(self.head(false)),
            12..=14 => UndoPayload::Chunks(self.head(true)),
            _ => UndoPayload::None,
        };
        let mut r = UndoRecord {
            creator: UndoCreator {
                id: id(),
                status_generation: g(20),
            },
            sequence: 100,
            mutation_sequence: 21,
            incoming: if i == 3 {
                5
            } else if i == 7 {
                2
            } else {
                1
            },
            target: target(kind),
            edges: match i {
                3 | 7 | 8..=11 => [link(), link()],
                4 | 12..=14 => [link(), UndoEdge::Cut],
                5 => [link(), UndoEdge::Never],
                _ => [UndoEdge::Never; 2],
            },
            before_kind: before,
            before_stamp: stamp,
            before_logical: logical,
            payload,
            release: UndoRelease {
                cursor: 0,
                chunks_released: 0,
                next: None,
            },
        };
        if (8..=14).contains(&i) {
            r.incoming = 0;
            r.release.cursor = if i <= 11 { (i - 8) as u8 } else { 3 };
            if r.release.cursor >= 1 {
                r.edges[1] = UndoEdge::Released;
            }
            if r.release.cursor >= 2 {
                r.edges[0] = UndoEdge::Released;
            }
            if r.release.cursor == 3 {
                r.before_stamp = None;
            }
            if i >= 12 {
                r.release.chunks_released = (i - 12) as u16;
                r.release.next = if i < 14 {
                    Some(page(40 + (i - 12) as u64))
                } else {
                    None
                };
            }
        }
        r
    }
    fn cell(&self, i: usize) -> UndoCell<'_> {
        if i == 0 {
            UndoCell::Free(g(6))
        } else if i >= 8 {
            UndoCell::Releasing(self.record(i))
        } else {
            UndoCell::Live(self.record(i))
        }
    }
    fn schema(&self, i: usize) -> Vec<Column> {
        if matches!(i, 3 | 8..=11) {
            inline_schema()
        } else {
            binary_schema()
        }
    }
    fn expect(&self, i: usize) -> UndoRecordExpectation<'static> {
        expected(if (5..8).contains(&i) || i == 17 {
            UndoTargetKind::Index
        } else if i == 18 {
            UndoTargetKind::Catalog
        } else {
            UndoTargetKind::Row
        })
    }
    fn context(&self) -> UndoChunkContext {
        let b = literal(4);
        let v = decode_undo_record(&b, self.expect(4), &binary_schema()).unwrap();
        UndoChunkContext::from_record(v, identity(30)).unwrap()
    }
    fn encode(&self, i: usize, out: &mut [u8]) -> Result<usize, UndoError> {
        match i {
            0..=14 => encode_undo_record(self.cell(i), self.expect(i), &self.schema(i), out),
            15 => self.head(false).encode(out),
            16..=18 => encode_undo_target(self.expect(i).target, self.expect(i).target, out),
            19 => encode_undo_chunk_cell(self.context(), page(40), 0, None, &self.big, out),
            _ => panic!(),
        }
    }
    fn decode(&self, i: usize, b: &[u8]) -> Result<(), UndoError> {
        match i {
            0..=14 => decode_undo_record(b, self.expect(i), &self.schema(i)).map(|_| ()),
            15 => UndoPayloadHead::decode(b).map(|_| ()),
            16..=18 => UndoTargetView::decode(b, self.expect(i).target).map(|_| ()),
            19 => decode_undo_chunk_cell(b, self.context(), page(40), 0).map(|_| ()),
            _ => panic!(),
        }
    }
}
fn encode_record(cell: UndoCell<'_>, e: UndoRecordExpectation<'_>, columns: &[Column]) -> Vec<u8> {
    let mut out = vec![0; undo_record_encoded_len(cell, e, columns).unwrap()];
    encode_undo_record(cell, e, columns, &mut out).unwrap();
    out
}
fn pack(cell: &[u8], n: u64) -> Vec<u8> {
    let mut out = vec![0; PAGE_BYTES];
    encode_page(header(n), &[cell], &mut out).unwrap();
    out
}

#[test]
fn undo_independent_literals_and_layouts() {
    let f = Fixtures::new();
    let mut sum = 0;
    for (i, hash) in HASHES.iter().enumerate() {
        let b = literal(i);
        assert_eq!(content_digest(&b).as_slice(), hex(hash));
        let mut out = vec![0; b.len()];
        assert_eq!(f.encode(i, &mut out), Ok(b.len()), "U{}", i + 1);
        assert_eq!(out, b);
        f.decode(i, &b).unwrap();
        if i < 15 {
            let view = decode_undo_record(&b, f.expect(i), &f.schema(i)).unwrap();
            assert_eq!(view.generation(), g(6));
            assert_eq!(
                view.state(),
                if i == 0 {
                    UndoRecordState::Free
                } else if i < 8 {
                    UndoRecordState::Live
                } else {
                    UndoRecordState::Releasing
                }
            );
            if i == 0 {
                assert_eq!(view.metadata(), Err(UndoError::StateMismatch));
                assert_eq!(view.live_before(), Err(UndoError::StateMismatch));
            } else {
                let expected = f.record(i);
                assert_eq!(
                    view.metadata().unwrap(),
                    UndoRecordMetadata {
                        creator: expected.creator,
                        sequence: 100,
                        mutation_sequence: 21,
                        incoming: expected.incoming,
                        target: expected.target,
                    }
                );
            }
            if i < 8 {
                assert_eq!(view.release_progress(), Err(UndoError::StateMismatch));
            } else {
                assert_eq!(
                    view.release_progress().unwrap(),
                    UndoReleaseProgress {
                        progress: f.record(i).release,
                        chunk_count: if i >= 12 { 2 } else { 0 },
                    }
                );
                assert_eq!(view.live_before(), Err(UndoError::StateMismatch));
            }
        }
        sum += b.len();
    }
    assert_eq!(sum, 9044);
    // Exact mutually exclusive maxima: row inline 2480, index root 1464.
    for (value, columns) in [
        (vec![1, 0, 0, 0, 0], vec![column(DataType::Bool, true)]),
        (binary(2039), binary_schema()),
    ] {
        let mut record = f.record(3);
        record.payload = UndoPayload::Inline(&value);
        let bytes = encode_record(UndoCell::Live(record), f.expect(3), &columns);
        assert_eq!(bytes.len(), 432 + value.len());
        decode_undo_record(&bytes, f.expect(3), &columns).unwrap();
    }
    let mut key = vec![3];
    key.extend([b'a'; 1021]);
    key.extend([0, 0]);
    key.extend(258_u64.to_be_bytes());
    assert_eq!(key.len(), 1032);
    let mut record = f.record(7);
    record.target.key = &key;
    let expectation = UndoRecordExpectation {
        target: record.target,
        ..f.expect(7)
    };
    let bytes = encode_record(UndoCell::Live(record), expectation, &binary_schema());
    assert_eq!(bytes.len(), 1464);
    decode_undo_record(&bytes, expectation, &binary_schema()).unwrap();
    for (i, n, digest) in [
        (
            3,
            30,
            "f3a4e11d13bda7c94167632071c86bc85fd41e8c8bc9342dc865b7a0f9ba7580",
        ),
        (
            19,
            40,
            "f733be862715750ff978a1b37108984caeed2b76c5e3607796fbeac00304e4ba",
        ),
    ] {
        let p = pack(&literal(i), n);
        assert_eq!(content_digest(&p).as_slice(), hex(digest));
        if i == 3 {
            let v = lookup_undo_record(&p, identity(30), f.expect(i), &f.schema(i)).unwrap();
            assert_eq!(v.cell, f.cell(i));
            assert_eq!(
                v.live_before().unwrap().payload,
                UndoPayload::Inline(&f.inline)
            );
        } else {
            let v = decode_undo_chunk_page(&p, f.context(), page(40), 0).unwrap();
            assert_eq!(v.payload(), f.big);
            let mut output = vec![0; PAGE_BYTES];
            encode_undo_chunk_page(header(40), f.context(), 0, &literal(19), &mut output).unwrap();
            assert_eq!(output, p);
        }
    }
}

#[test]
fn undo_all_lengths_preserve_outputs() {
    let f = Fixtures::new();
    let (mut dec, mut enc, mut tails) = (0, 0, 0);
    for i in 0..20 {
        let b = literal(i);
        for n in 0..b.len() {
            assert!(f.decode(i, &b[..n]).is_err(), "U{} len{}", i + 1, n);
            dec += 1;
            let mut out = vec![0xa7; n];
            assert!(f.encode(i, &mut out).is_err());
            assert_eq!(out, vec![0xa7; n]);
            enc += 1;
        }
        let mut bad = b.clone();
        bad.push(0);
        assert!(f.decode(i, &bad).is_err());
        dec += 1;
        let mut out = vec![0xa7; b.len() + 7];
        assert_eq!(f.encode(i, &mut out), Ok(b.len()));
        assert_eq!(&out[..b.len()], b);
        assert_eq!(&out[b.len()..], &[0xa7; 7]);
        tails += 1;
    }
    assert_eq!((dec, enc, tails), (9064, 9044, 20));
    for n in [0, 16383, 16385] {
        let bytes = vec![0; n];
        assert!(lookup_undo_record(&bytes, identity(30), f.expect(3), &inline_schema()).is_err());
        assert!(decode_undo_chunk_page(&bytes, f.context(), page(40), 0).is_err());
    }
}

#[test]
fn undo_targets_bind_namespace_owner_and_key() {
    let f = Fixtures::new();
    let mut cases = 0;
    for i in [3, 7] {
        let p = pack(&literal(i), 30);
        for change in 0..12 {
            let mut e = f.expect(i);
            let mut owner = identity(30);
            match change {
                0 => e.namespace.database = DatabaseId::new([9; 16]).unwrap(),
                1 => e.namespace.timeline = 18,
                2 => owner.page.file = FileId::new(0x2_0000_0002).unwrap(),
                3 => owner.page.page_no += 1,
                4 => owner.page.incarnation = g(5),
                5 => owner.owner = oid(8),
                6 => owner.owner_generation = g(11),
                7 => e.reference = UndoRef::new(page(30), 0, g(7)).unwrap(),
                8 => e.target.owner = oid(8),
                9 => e.target.generation = g(7),
                10 => e.target.entity = 259,
                11 => {
                    e.target = target(if i == 3 {
                        UndoTargetKind::Index
                    } else {
                        UndoTargetKind::Row
                    })
                }
                _ => panic!(),
            }
            assert!(
                lookup_undo_record(&p, owner, e, &f.schema(i)).is_err(),
                "{i}/{change}"
            );
            cases += 1;
        }
    }
    assert_eq!(cases, 24);
    for change in 0..3 {
        let mut b = literal(7);
        match change {
            0 => b[89] = 2,
            1 => b[432] = 7,
            2 => b[441] = 3,
            _ => panic!(),
        };
        assert!(f.decode(7, &b).is_err());
    }
    UndoTargetView::decode(&literal(18), target(UndoTargetKind::Catalog)).unwrap();
}

#[test]
fn undo_catalog_legacy_malformed_bodies() {
    use crate::storage::catalog_record::before_tests::{Harness, ids};
    let f = Fixtures::new();
    let mut harness = Harness::new(ids("K", 1, 5));
    for (index, before) in [
        UndoBeforeKind::Missing,
        UndoBeforeKind::RowDeleted,
        UndoBeforeKind::RowValue,
        UndoBeforeKind::IndexAbsent,
        UndoBeforeKind::IndexPresent,
    ]
    .into_iter()
    .enumerate()
    {
        harness.case(&format!("CB-K{:02}", index + 1), || {
            let mut r = f.record(3);
            r.target = target(UndoTargetKind::Catalog);
            r.before_kind = before;
            assert_eq!(
                undo_record_encoded_len(
                    UndoCell::Live(r),
                    expected(UndoTargetKind::Catalog),
                    &inline_schema()
                ),
                Err(if index == 0 {
                    UndoError::StateMismatch
                } else {
                    UndoError::InvalidInput
                })
            );
        });
    }
    harness.finish();
}

#[test]
fn undo_before_state_matrix() {
    let f = Fixtures::new();
    let mut good = 0;
    let mut bad = 0;
    for kind in [UndoTargetKind::Row, UndoTargetKind::Index] {
        for before in 0..5 {
            for storage in 0..3 {
                for stamp in 0..3 {
                    let mut r = f.record(3);
                    r.target = target(kind);
                    r.edges = [UndoEdge::Never; 2];
                    r.before_kind = match before {
                        0 => UndoBeforeKind::Missing,
                        1 => UndoBeforeKind::RowDeleted,
                        2 => UndoBeforeKind::RowValue,
                        3 => UndoBeforeKind::IndexAbsent,
                        _ => UndoBeforeKind::IndexPresent,
                    };
                    r.payload = match storage {
                        0 => UndoPayload::None,
                        1 => UndoPayload::Inline(&f.inline),
                        _ => UndoPayload::Chunks(f.head(false)),
                    };
                    if stamp == 0 {
                        r.before_stamp = None;
                        r.before_logical = None;
                    } else if stamp == 2 {
                        r.before_stamp = f.record(2).before_stamp;
                        r.before_logical = f.record(2).before_logical;
                    }
                    let valid = if before == 0 {
                        storage == 0 && stamp == 0
                    } else {
                        stamp != 0
                            && match kind {
                                UndoTargetKind::Row => {
                                    (before == 1 && storage == 0) || (before == 2 && storage != 0)
                                }
                                UndoTargetKind::Index => {
                                    (before == 3 || before == 4) && storage == 0
                                }
                                _ => false,
                            }
                    };
                    let mut out = vec![0xa7; 2480];
                    let result = encode_undo_record(
                        UndoCell::Live(r),
                        expected(kind),
                        &inline_schema(),
                        &mut out,
                    );
                    if valid {
                        good += 1;
                        let len = result.unwrap();
                        decode_undo_record(&out[..len], expected(kind), &inline_schema()).unwrap();
                    } else {
                        bad += 1;
                        assert!(result.is_err(), "{kind:?}/{before}/{storage}/{stamp}");
                        assert_eq!(out, vec![0xa7; 2480]);
                    }
                }
            }
        }
    }
    assert_eq!((good, bad), (12, 78));
    for change in 0..5 {
        let mut b = literal(3);
        match change {
            0 => b[264] = 21,
            1 => b[256] = 20,
            2 => {
                b[216..272].fill(0);
                b[216] = 1;
            }
            3 => b[312] = 21,
            4 => b[312] = 22,
            _ => panic!(),
        };
        assert!(f.decode(3, &b).is_err());
    }
}

#[test]
fn undo_edges_keep_roles_and_multiplicity() {
    let f = Fixtures::new();
    let (mut good, mut bad) = (0, 0);
    for a in 0..4 {
        for b in 0..4 {
            let mut r = f.record(3);
            r.edges = [edge(a), edge(b)];
            let result = undo_record_encoded_len(UndoCell::Live(r), f.expect(3), &inline_schema());
            if a < 3 && b < 3 {
                result.unwrap();
                good += 1;
            } else {
                assert!(result.is_err());
                bad += 1;
            }
        }
    }
    assert_eq!((good, bad), (9, 7));
    for duplicate in [true, false] {
        let mut r = f.record(3);
        if !duplicate {
            r.edges[1] = UndoEdge::Linked(UndoLink {
                reference: UndoRef::new(page(4), 5, g(6)).unwrap(),
                sequence: 98,
            });
        }
        let bytes = encode_record(UndoCell::Live(r), f.expect(3), &inline_schema());
        let v = decode_undo_record(&bytes, f.expect(3), &inline_schema()).unwrap();
        assert_eq!(v.linked_edges().iter().flatten().count(), 2);
        assert_eq!(
            v.linked_edges()[0].unwrap().1 == v.linked_edges()[1].unwrap().1,
            duplicate
        );
        assert!(v.before_status_ref().is_some());
    }
    // Four independent loaded histories; actual target fields come from typed inputs.
    for case in 0..4 {
        let role = if case < 2 {
            UndoEdgeRole::Transaction
        } else {
            UndoEdgeRole::Version
        };
        let mut src = f.record(3);
        let mut dst = f.record(1);
        dst.sequence = 99;
        dst.mutation_sequence = 20;
        if case == 1 {
            dst.target.entity = 259;
        }
        if case == 3 {
            dst.creator.id = TxnId {
                writer_epoch: 17,
                local: 23,
                ..id()
            };
            dst.mutation_sequence = 24;
            src.before_stamp = Some(VersionStamp::Committed(CommitSeq::new(22).unwrap()));
            src.before_logical = Some(LogicalRowVersion::new(dst.creator.id, 24).unwrap());
        }
        let e = UndoRecordExpectation {
            reference: UndoRef::new(page(3), 5, g(6)).unwrap(),
            target: dst.target,
            ..f.expect(1)
        };
        let a = encode_record(UndoCell::Live(src), f.expect(3), &inline_schema());
        let b = encode_record(UndoCell::Live(dst), e, &binary_schema());
        let av = decode_undo_record(&a, f.expect(3), &inline_schema()).unwrap();
        let bv = decode_undo_record(&b, e, &binary_schema()).unwrap();
        validate_loaded_edge(av, role, bv).unwrap();
    }
    for change in 0..8 {
        let src_bytes = literal(3);
        let src = decode_undo_record(&src_bytes, f.expect(3), &inline_schema()).unwrap();
        let mut dst = f.record(1);
        dst.sequence = 99;
        dst.mutation_sequence = 20;
        let mut e = UndoRecordExpectation {
            reference: UndoRef::new(page(3), 5, g(6)).unwrap(),
            ..f.expect(1)
        };
        match change {
            0 => e.reference = UndoRef::new(page(3), 5, g(7)).unwrap(),
            1 => {
                e.reference = UndoRef::new(
                    PageRef {
                        incarnation: g(5),
                        ..page(3)
                    },
                    5,
                    g(6),
                )
                .unwrap()
            }
            2 => dst.sequence = 98,
            3 => dst.sequence = 0,
            4 => dst.sequence = 100,
            5 => dst.sequence = 101,
            6 => dst.creator.id.local += 1,
            7 => {
                dst.target.entity = 259;
                e.target = dst.target;
            }
            _ => panic!(),
        }
        if change == 3 {
            assert!(undo_record_encoded_len(UndoCell::Live(dst), e, &binary_schema()).is_err());
            continue;
        }
        let bytes = encode_record(UndoCell::Live(dst), e, &binary_schema());
        let view = decode_undo_record(&bytes, e, &binary_schema()).unwrap();
        assert!(
            validate_loaded_edge(
                src,
                if change == 7 {
                    UndoEdgeRole::Version
                } else {
                    UndoEdgeRole::Transaction
                },
                view
            )
            .is_err(),
            "{change}"
        );
    }
}

#[test]
fn undo_release_cursor_state_matrix() {
    let f = Fixtures::new();
    let (mut good, mut bad) = (0, 0);
    for cursor in 0..5 {
        for a in 0..4 {
            for b in 0..4 {
                let mut r = f.record(8);
                r.release.cursor = cursor;
                r.edges = [edge(a), edge(b)];
                if cursor == 3 {
                    r.before_stamp = None;
                }
                let valid = match cursor {
                    0 => a < 3 && b < 3,
                    1 => a < 3 && b == 3,
                    2 | 3 => a == 3 && b == 3,
                    _ => false,
                };
                let result =
                    undo_record_encoded_len(UndoCell::Releasing(r), f.expect(3), &inline_schema());
                if valid {
                    good += 1;
                    result.unwrap();
                    let bytes =
                        encode_record(UndoCell::Releasing(r), f.expect(3), &inline_schema());
                    let view = decode_undo_record(&bytes, f.expect(3), &inline_schema()).unwrap();
                    assert!(view.live_before().is_err());
                    assert_eq!(view.before_status_ref().is_none(), cursor == 3);
                } else {
                    bad += 1;
                    assert!(result.is_err(), "{cursor}/{a}/{b}");
                }
            }
        }
    }
    assert_eq!((good, bad), (14, 66));
    for count in [1, u64::MAX] {
        let mut r = f.record(8);
        r.incoming = count;
        assert!(
            undo_record_encoded_len(UndoCell::Releasing(r), f.expect(3), &inline_schema()).is_err()
        );
    }
    for cursor in 1..4 {
        let mut r = f.record(3);
        r.release.cursor = cursor;
        assert!(undo_record_encoded_len(UndoCell::Live(r), f.expect(3), &inline_schema()).is_err());
    }
    let (mut good, mut bad) = (0, 0);
    for released in 0..4 {
        for next in [None, Some(page(40)), Some(page(41))] {
            let mut r = f.record(12);
            r.release.chunks_released = released;
            r.release.next = next;
            let valid = matches!(
                (released, next.map(|p| p.page_no)),
                (0, Some(40)) | (1, Some(41)) | (2, None)
            );
            let result =
                undo_record_encoded_len(UndoCell::Releasing(r), f.expect(12), &binary_schema());
            assert_eq!(result.is_ok(), valid);
            if valid { good += 1 } else { bad += 1 }
        }
    }
    assert_eq!((good, bad), (3, 9));
}

#[test]
fn undo_rejects_checksummed_damage() {
    let f = Fixtures::new();
    for n in 1..=48 {
        let index = if (41..=46).contains(&n) {
            4
        } else if n == 47 {
            11
        } else {
            3
        };
        let mut b = literal(index);
        match n {
            1 => b[0] = 0,
            2 => b[4] = 2,
            3 => b[6] = 3,
            4 => b[7] = 1,
            5 => b[8..16].fill(0),
            6 => b[16..24].fill(0),
            7 => b[48..52].fill(0),
            8 => b[52] = 1,
            9 => b[56..64].fill(0),
            10 => b[64..72].fill(0),
            11 => b[72..80].fill(0),
            12 => b[88] = 0,
            13 => b[89] = 1,
            14 => b[90] = 1,
            15 => b[92] = 1,
            16 => b[96..104].fill(0),
            17 => b[104..112].fill(0),
            18 => b[112..120].fill(0),
            19 => b[326] = 4,
            20 => b[327] = 4,
            21 => b[120..160].fill(0),
            22 => b[160..168].fill(0),
            23 => put64(&mut b, 160, 100),
            24 => put64(&mut b, 160, 101),
            25 => b[146] = 1,
            26 | 27 => {
                reference().encode(&mut b[120..160]).unwrap();
                if n == 27 {
                    put64(&mut b, 152, 7);
                }
            }
            28 => b[326] = 0,
            29 => b[326] = 2,
            30 => {
                b[326] = 3;
                b[120..168].fill(0);
            }
            31 => put64(&mut b, 256, 20),
            32 => put64(&mut b, 264, 21),
            33 => put64(&mut b, 304, 20),
            34 => put64(&mut b, 312, 21),
            35 => put64(&mut b, 312, 22),
            36 => b[325] = 0,
            37 => put32(&mut b, 320, 2049),
            38 => b[436] = 5,
            39 => b[329] = 1,
            40 => b[334] = 1,
            41 => b[364] = 2,
            42 => b[366] = 1,
            43 => put32(&mut b, 360, 2048),
            44 => page(30).encode(&mut b[336..360]).unwrap(),
            45 => page(40).encode(&mut b[400..424]).unwrap(),
            46 => b[332] = 1,
            47 => VersionStamp::Transaction(TxnRef::new(id(), g(20)).unwrap())
                .encode(&mut b[216..272])
                .unwrap(),
            48 => b[424] = 1,
            _ => panic!(),
        }
        let p = pack(&b, 30);
        assert!(
            lookup_undo_record(&p, identity(30), f.expect(index), &f.schema(index)).is_err(),
            "N{n:02}"
        );
    }
    for n in 1..=16 {
        let mut b = literal(19);
        match n {
            1 => b[0] = 0,
            2 => b[4] = 2,
            3 => b[6] = 1,
            4 => b[8] = 2,
            5 => b[10] = 1,
            6 => put32(&mut b, 12, 2050),
            7 => put32(&mut b, 16, 2048),
            8 => b[20] = 1,
            9 => put64(&mut b, 32, 31),
            10 => put64(&mut b, 64, 99),
            11 => put64(&mut b, 80, 41),
            12 => page(41).encode(&mut b[96..120]).unwrap(),
            13 => b[120] ^= 1,
            14 => put64(&mut b, 152, 18),
            15 => {
                b.pop();
            }
            16 => b.push(0),
            _ => panic!(),
        }
        let p = pack(&b, 40);
        assert!(
            decode_undo_chunk_page(&p, f.context(), page(40), 0).is_err(),
            "C{n:02}"
        );
    }
}

#[test]
fn undo_slot_lookup_checks_generation_and_kind() {
    let f = Fixtures::new();
    let minimal = literal(1);
    let mut p = vec![0; PAGE_BYTES];
    encode_page(header(30), &vec![minimal.as_slice(); 37], &mut p).unwrap();
    let e = UndoRecordExpectation {
        reference: UndoRef::new(page(30), 36, g(6)).unwrap(),
        ..f.expect(1)
    };
    lookup_undo_record(&p, identity(30), e, &binary_schema()).unwrap();
    for change in 0..12 {
        let mut e = f.expect(1);
        let mut ident = identity(30);
        let mut bytes = pack(&minimal, 30);
        match change {
            0 => ident.database = DatabaseId::new([9; 16]).unwrap(),
            1 => ident.page.file = FileId::new(0x2_0000_0002).unwrap(),
            2 => ident.page.page_no = 31,
            3 => ident.page.incarnation = g(5),
            4 => ident.owner = oid(8),
            5 => ident.owner_generation = g(11),
            6 => {
                let mut h = header(30);
                h.kind = PageKind::TransactionStatus;
                encode_page(h, &[&minimal], &mut bytes).unwrap();
            }
            7 => e.reference = UndoRef::new(page(30), 37, g(6)).unwrap(),
            8 => e.reference = UndoRef::new(page(30), 1, g(6)).unwrap(),
            9 => e.reference = UndoRef::new(page(30), 0, g(7)).unwrap(),
            10 => bytes = pack(&literal(0), 30),
            11 => bytes = pack(&literal(19), 30),
            _ => panic!(),
        }
        let result = lookup_undo_record(&bytes, ident, e, &binary_schema());
        assert!(result.is_err(), "slot {change}");
        if change == 10 {
            assert_eq!(result.unwrap_err(), UndoError::StateMismatch);
        }
    }
    assert_eq!(next_slot_generation(UndoCell::Free(g(1))), Ok(g(2)));
    assert_eq!(
        next_slot_generation(UndoCell::Free(g(u64::MAX - 1))),
        Ok(g(u64::MAX))
    );
    assert_eq!(
        next_slot_generation(UndoCell::Free(g(u64::MAX))),
        Err(UndoError::Exhausted)
    );
    assert!(next_slot_generation(f.cell(1)).is_err());
}

fn canonical_total(total: usize, n: usize) -> Vec<u8> {
    let a = 65536.min(total - 14);
    let b = total - 14 - a;
    let mut out = vec![2, 0, 0, 0];
    for size in [a, b] {
        out.push(4);
        out.extend_from_slice(&(size as u32).to_le_bytes());
        out.extend((0..size).map(|i| (31 * i + n) as u8));
    }
    assert_eq!(out.len(), total);
    out
}
fn chunk_context(value: &[u8]) -> UndoChunkContext {
    let f = Fixtures::new();
    let mut r = f.record(4);
    r.payload = UndoPayload::Chunks(
        UndoPayloadHead::new(page(40), value.len(), content_digest(value)).unwrap(),
    );
    let b = encode_record(UndoCell::Live(r), f.expect(4), &binary_schema());
    let v = decode_undo_record(&b, f.expect(4), &binary_schema()).unwrap();
    UndoChunkContext::from_record(v, identity(30)).unwrap()
}
fn chunk_bytes(
    ctx: UndoChunkContext,
    current: PageRef,
    index: usize,
    next: Option<PageRef>,
    payload: &[u8],
) -> Vec<u8> {
    let mut out = vec![0; 160 + payload.len()];
    encode_undo_chunk_cell(ctx, current, index, next, payload, &mut out).unwrap();
    out
}
fn chain(value: &[u8]) -> (UndoChunkContext, Vec<Vec<u8>>) {
    let ctx = chunk_context(value);
    let pages = value
        .chunks(UNDO_CHUNK_PAYLOAD)
        .enumerate()
        .map(|(i, payload)| {
            let current = page(40 + i as u64);
            let next = if i + 1 < ctx.head.count() {
                Some(page(41 + i as u64))
            } else {
                None
            };
            let cell = chunk_bytes(ctx, current, i, next, payload);
            let mut out = vec![0; PAGE_BYTES];
            encode_undo_chunk_page(header(current.page_no), ctx, i, &cell, &mut out).unwrap();
            out
        })
        .collect();
    (ctx, pages)
}
#[test]
fn undo_chunk_boundaries_and_verifier() {
    let mut chunks = 0;
    for total in [2049, 16060, 16061, 32120, 32121, 131072] {
        let value = canonical_total(total, 7);
        let (ctx, pages) = chain(&value);
        let mut v = UndoChunkVerifier::new(ctx);
        assert_eq!(v.finish(), Err(UndoError::Incomplete));
        if pages.len() > 1 {
            let skip = decode_undo_chunk_page(&pages[1], ctx, page(41), 1).unwrap();
            assert!(v.push(skip).is_err());
        }
        for (i, p) in pages.iter().enumerate() {
            let view = decode_undo_chunk_page(p, ctx, page(40 + i as u64), i).unwrap();
            if i + 1 == pages.len() {
                let mut bad = p.clone();
                let offset = usize::from(u16_at(&bad, 128)) + 160;
                bad[offset] ^= 1;
                let digest = content_digest(&bad[..16352]);
                bad[16352..].copy_from_slice(&digest);
                let corrupt = decode_undo_chunk_page(&bad, ctx, page(40 + i as u64), i).unwrap();
                assert_eq!(v.push(corrupt), Err(UndoError::Corrupt));
                assert!(v.finish().is_err());
            }
            v.push(view).unwrap();
            assert!(v.push(view).is_err());
            chunks += 1;
        }
        v.finish().unwrap();
        assert_eq!(ctx.head.count(), pages.len());
        let f = Fixtures::new();
        let mut r = f.record(4);
        r.payload = UndoPayload::Chunks(ctx.head);
        let rb = encode_record(UndoCell::Live(r), f.expect(4), &binary_schema());
        let rv = decode_undo_record(&rb, f.expect(4), &binary_schema()).unwrap();
        let cols = vec![column(DataType::Binary, false); 2];
        decode_materialized_before(rv, &value, &cols).unwrap();
        assert!(decode_materialized_before(rv, &value, &binary_schema()).is_err());
        assert!(decode_materialized_before(rv, &value[..value.len() - 1], &cols).is_err());
    }
    assert_eq!(chunks, 18);
    // A late non-root repeated address remains invalid even with a new incarnation.
    for incarnation in [4, 5] {
        let value = canonical_total(48181, 8);
        let ctx = chunk_context(&value);
        let addresses = [
            page(40),
            page(41),
            page(42),
            PageRef {
                incarnation: g(incarnation),
                ..page(41)
            },
        ];
        let mut verifier = UndoChunkVerifier::new(ctx);
        for i in 0..4 {
            let payload = &value[i * 16060..value.len().min((i + 1) * 16060)];
            let bytes = chunk_bytes(ctx, addresses[i], i, addresses.get(i + 1).copied(), payload);
            let view = decode_undo_chunk_cell(&bytes, ctx, addresses[i], i).unwrap();
            if i == 3 {
                assert!(verifier.push(view).is_err());
            } else {
                verifier.push(view).unwrap();
            }
        }
    }
}

#[test]
fn undo_chunk_ownership_and_release_resume() {
    let f = Fixtures::new();
    let original = pack(&literal(19), 40);
    for change in 0..8 {
        let mut b = original.clone();
        let at = usize::from(u16_at(&b, 128));
        match change {
            0 => b[8] ^= 1,
            1 => put64(&mut b, 48, 5),
            2 => put64(&mut b, 56, 6),
            3 => put64(&mut b, at + 32, 31),
            4 => put16(&mut b, at + 48, 1),
            5 => put64(&mut b, at + 56, 7),
            6 => put64(&mut b, at + 64, 99),
            7 => {
                b[6] = 7;
                b[at..at + 4].copy_from_slice(b"XOV1");
            }
            _ => panic!(),
        };
        let digest = content_digest(&b[..16352]);
        b[16352..].copy_from_slice(&digest);
        assert!(
            decode_undo_chunk_page(&b, f.context(), page(40), 0).is_err(),
            "owner {change}"
        );
    }
    for released in 0..3 {
        let bytes = literal(12 + released);
        let view = decode_undo_record(&bytes, f.expect(12 + released), &binary_schema()).unwrap();
        assert!(view.live_before().is_err());
        let ctx = UndoChunkContext::from_record(view, identity(30)).unwrap();
        if released == 2 {
            assert_eq!(
                match view.cell {
                    UndoCell::Releasing(r) => r.release.next,
                    _ => panic!(),
                },
                None
            );
            continue;
        }
        let payload = &f.two[released * 16060..f.two.len().min((released + 1) * 16060)];
        let cell = chunk_bytes(
            ctx,
            page(40 + released as u64),
            released,
            if released == 0 { Some(page(41)) } else { None },
            payload,
        );
        let chunk =
            decode_undo_chunk_cell(&cell, ctx, page(40 + released as u64), released).unwrap();
        validate_release_chunk(view, chunk).unwrap();
        // Only the durable next page is supplied; no released-prefix load occurs.
        let live_bytes = literal(4);
        let live = decode_undo_record(&live_bytes, f.expect(4), &binary_schema()).unwrap();
        assert!(validate_release_chunk(live, chunk).is_err());
    }
}

#[test]
fn undo_failed_shapes_have_no_effects() {
    let f = Fixtures::new();
    let mut count = 0;
    for change in 0..6 {
        let mut r = f.record(3);
        match change {
            0 => r.creator.id.local = 0,
            1 => r.creator.id.timeline = 18,
            2 => {
                r.target = target(UndoTargetKind::Index);
                r.target.key = &[1, 1, 0, 0, 0, 0, 0, 0, 1, 3];
            }
            3 => {
                r.before_stamp = Some(VersionStamp::Transaction(
                    TxnRef::new(TxnId { local: 20, ..id() }, g(20)).unwrap(),
                ))
            }
            4 => {
                r.edges[0] = UndoEdge::Linked(UndoLink {
                    sequence: 100,
                    ..match link() {
                        UndoEdge::Linked(l) => l,
                        _ => panic!(),
                    }
                })
            }
            5 => r.release.cursor = 1,
            _ => panic!(),
        };
        let mut out = vec![0xa7; 2480];
        assert!(
            encode_undo_record(UndoCell::Live(r), f.expect(3), &inline_schema(), &mut out).is_err()
        );
        assert_eq!(out, vec![0xa7; 2480]);
        count += 1;
    }
    let long_key = vec![0; 1033];
    let invalid = UndoTargetView {
        key: &long_key,
        ..target(UndoTargetKind::Index)
    };
    let mut out = vec![0xa7; 2500];
    assert!(encode_undo_target(invalid, invalid, &mut out).is_err());
    assert_eq!(out, vec![0xa7; 2500]);
    count += 1;
    assert_eq!(
        UndoPayloadHead::new(page(40), 131073, [0; 32]),
        Err(UndoError::InvalidInput)
    );
    assert_eq!(out, vec![0xa7; 2500]);
    count += 1;
    for change in 0..3 {
        let (index, next, payload) = match change {
            0 => (1, None, f.big.as_slice()),
            1 => (0, Some(page(41)), f.big.as_slice()),
            _ => (0, None, &f.big[..2048]),
        };
        assert!(
            encode_undo_chunk_cell(f.context(), page(40), index, next, payload, &mut out).is_err()
        );
        assert_eq!(out, vec![0xa7; 2500]);
        count += 1;
    }
    let mut short = [0xa7; 431];
    assert!(encode_undo_record(f.cell(1), f.expect(1), &binary_schema(), &mut short).is_err());
    assert_eq!(short, [0xa7; 431]);
    count += 1;
    assert_eq!(count, 12);
    assert_eq!(incoming_decrement(0, 1), Err(UndoError::StateMismatch));
    assert_eq!(incoming_increment(u64::MAX, 1), Err(UndoError::Exhausted));
    let count = incoming_increment(5, 2).unwrap();
    assert_eq!(count, 7);
    assert_eq!(
        incoming_decrement(incoming_decrement(count, 1).unwrap(), 1),
        Ok(5)
    );
}

#[test]
fn undo_generated_profiles_match_independent_fields() {
    let f = Fixtures::new();
    let mut count = 0;
    for seed in 0..64 {
        for sample in 0..128 {
            let n = 128 * seed + sample;
            let variant = n % 7;
            let kind = if variant >= 4 {
                UndoTargetKind::Index
            } else {
                UndoTargetKind::Row
            };
            let missing = variant == 0 || variant == 4;
            let own = n & 8 == 0;
            let creator = TxnId {
                local: 19 + n as u64,
                ..id()
            };
            let mutation = 10 + n as u64;
            let mut key = vec![1, (n & 1) as u8];
            key.extend_from_slice(&(258 + n as u64).to_be_bytes());
            let target = UndoTargetView {
                entity: 258 + n as u64,
                key: if kind == UndoTargetKind::Index {
                    &key
                } else {
                    &[]
                },
                ..target(kind)
            };
            let reference = UndoRef::new(page(100 + 2 * n as u64), 0, g(6)).unwrap();
            let e = UndoRecordExpectation {
                namespace: ns(),
                reference,
                target,
            };
            let prev = UndoEdge::Linked(UndoLink {
                reference: UndoRef::new(page(99 + 2 * n as u64), 5, g(6)).unwrap(),
                sequence: 999 + n as u64,
            });
            let inline = {
                let len = n % 97;
                let mut b = vec![1, 0, 0, 0, 4];
                b.extend_from_slice(&(len as u32).to_le_bytes());
                b.extend((0..len).map(|i| (31 * i + n) as u8));
                b
            };
            let large = if variant == 3 {
                canonical_total(2049 + (7919 * n % 129024), n)
            } else {
                vec![]
            };
            let before = match variant {
                0 | 4 => UndoBeforeKind::Missing,
                1 => UndoBeforeKind::RowDeleted,
                2 | 3 => UndoBeforeKind::RowValue,
                5 => UndoBeforeKind::IndexAbsent,
                _ => UndoBeforeKind::IndexPresent,
            };
            let r = UndoRecord {
                creator: UndoCreator {
                    id: creator,
                    status_generation: g(20),
                },
                sequence: 1000 + n as u64,
                mutation_sequence: mutation,
                incoming: (n % 6) as u64,
                target,
                edges: [
                    if n & 16 != 0 { prev } else { UndoEdge::Never },
                    if missing {
                        UndoEdge::Never
                    } else if n & 32 != 0 {
                        prev
                    } else {
                        UndoEdge::Cut
                    },
                ],
                before_kind: before,
                before_stamp: if missing {
                    None
                } else if own {
                    Some(VersionStamp::Transaction(
                        TxnRef::new(creator, g(20)).unwrap(),
                    ))
                } else {
                    Some(VersionStamp::Committed(CommitSeq::new(22).unwrap()))
                },
                before_logical: if missing {
                    None
                } else {
                    Some(
                        LogicalRowVersion::new(
                            if own {
                                creator
                            } else {
                                TxnId {
                                    writer_epoch: 17,
                                    local: 10000 + n as u64,
                                    ..id()
                                }
                            },
                            if own { mutation - 1 } else { 1 },
                        )
                        .unwrap(),
                    )
                },
                payload: match variant {
                    2 => UndoPayload::Inline(&inline),
                    3 => UndoPayload::Chunks(
                        UndoPayloadHead::new(
                            page(1000000 + 9 * n as u64),
                            large.len(),
                            content_digest(&large),
                        )
                        .unwrap(),
                    ),
                    _ => UndoPayload::None,
                },
                release: UndoRelease {
                    cursor: 0,
                    chunks_released: 0,
                    next: None,
                },
            };
            let bytes = encode_record(UndoCell::Live(r), e, &binary_schema());
            // Direct independent expected fields, plus candidate roundtrip separately.
            assert_eq!(&bytes[..8], b"XUR1\x01\x00\x01\x00");
            assert_eq!(u64_at(&bytes, 8), 6);
            assert_eq!(u64_at(&bytes, 16), 1000 + n as u64);
            assert_eq!(&bytes[24..40], &id().database.bytes());
            assert_eq!(u64_at(&bytes, 40), 17);
            assert_eq!(u32_at(&bytes, 48), 18);
            assert_eq!(u64_at(&bytes, 56), 19 + n as u64);
            assert_eq!(u64_at(&bytes, 64), 20);
            assert_eq!(u64_at(&bytes, 72), mutation);
            assert_eq!(u64_at(&bytes, 80), (n % 6) as u64);
            assert_eq!(bytes[88], kind as u8);
            assert_eq!(u64_at(&bytes, 96), 5);
            assert_eq!(u64_at(&bytes, 104), 6);
            assert_eq!(u64_at(&bytes, 112), 258 + n as u64);
            assert_eq!(bytes[326], if n & 16 != 0 { 1 } else { 0 });
            assert_eq!(
                bytes[327],
                if missing {
                    0
                } else if n & 32 != 0 {
                    1
                } else {
                    2
                }
            );
            assert_eq!(bytes[324], before as u8);
            assert_eq!(
                bytes[325],
                if variant == 2 {
                    1
                } else if variant == 3 {
                    2
                } else {
                    0
                }
            );
            assert_eq!(
                u32_at(&bytes, 320) as usize,
                if variant == 2 {
                    9 + n % 97
                } else {
                    large.len()
                }
            );
            assert!(zero(&bytes[328..336]));
            assert!(zero(&bytes[400..432]));
            let k = if kind == UndoTargetKind::Index { 10 } else { 0 };
            assert_eq!(u16_at(&bytes, 90) as usize, k);
            assert_eq!(
                bytes.len(),
                432 + k + if variant == 2 { 9 + n % 97 } else { 0 }
            );
            assert_eq!(
                &bytes[432..432 + k],
                if k > 0 { key.as_slice() } else { &[] }
            );
            // Full independent offset oracle. No candidate encoder/decoder is
            // used to assemble identity, edge, stamp, head, or reserved bytes.
            let mut raw = vec![0; 432 + k + if variant == 2 { inline.len() } else { 0 }];
            raw[..8].copy_from_slice(b"XUR1\x01\x00\x01\x00");
            let raw64 = |raw: &mut [u8], at: usize, value: u64| {
                raw[at..at + 8].copy_from_slice(&value.to_le_bytes());
            };
            let raw_txn = |raw: &mut [u8], at: usize, epoch: u32, local: u64| {
                raw[at..at + 16].copy_from_slice(&std::array::from_fn::<_, 16, _>(|i| i as u8 + 1));
                raw[at + 16..at + 24].copy_from_slice(&17_u64.to_le_bytes());
                raw[at + 24..at + 28].copy_from_slice(&epoch.to_le_bytes());
                raw[at + 32..at + 40].copy_from_slice(&local.to_le_bytes());
            };
            for (at, value) in [
                (8, 6),
                (16, 1000 + n as u64),
                (64, 20),
                (72, mutation),
                (80, (n % 6) as u64),
                (96, 5),
                (104, 6),
                (112, 258 + n as u64),
            ] {
                raw64(&mut raw, at, value);
            }
            raw_txn(&mut raw, 24, 18, 19 + n as u64);
            raw[88] = if variant >= 4 { 2 } else { 1 };
            raw[89] = u8::from(variant >= 4);
            raw[90..92].copy_from_slice(&(k as u16).to_le_bytes());
            for (index, linked) in [n & 16 != 0, !missing && n & 32 != 0]
                .into_iter()
                .enumerate()
            {
                raw[326 + index] = if linked {
                    1
                } else if index == 1 && !missing {
                    2
                } else {
                    0
                };
                if linked {
                    let at = 120 + index * 48;
                    for (delta, value) in [
                        (0, 0x1_0000_0002),
                        (8, 99 + 2 * n as u64),
                        (16, 4),
                        (32, 6),
                        (40, 999 + n as u64),
                    ] {
                        raw64(&mut raw, at + delta, value);
                    }
                    raw[at + 24..at + 26].copy_from_slice(&5_u16.to_le_bytes());
                }
            }
            if !missing {
                raw[216] = if own { 2 } else { 1 };
                if own {
                    raw_txn(&mut raw, 224, 18, 19 + n as u64);
                    raw64(&mut raw, 264, 20);
                } else {
                    raw64(&mut raw, 224, 22);
                }
                raw_txn(
                    &mut raw,
                    272,
                    if own { 18 } else { 17 },
                    if own { 19 + n as u64 } else { 10000 + n as u64 },
                );
                raw64(&mut raw, 312, if own { mutation - 1 } else { 1 });
            }
            raw[324] = [0, 1, 2, 2, 0, 3, 4][variant];
            raw[325] = if variant == 2 {
                1
            } else if variant == 3 {
                2
            } else {
                0
            };
            let total = if variant == 2 {
                inline.len()
            } else {
                large.len()
            };
            raw[320..324].copy_from_slice(&(total as u32).to_le_bytes());
            if variant == 3 {
                raw64(&mut raw, 336, 0x1_0000_0002);
                raw64(&mut raw, 344, 1000000 + 9 * n as u64);
                raw64(&mut raw, 352, 4);
                raw[360..364].copy_from_slice(&(total as u32).to_le_bytes());
                raw[364..366].copy_from_slice(&(total.div_ceil(16060) as u16).to_le_bytes());
                raw[368..400].copy_from_slice(&content_digest(&large));
            }
            if k > 0 {
                raw[432..432 + k].copy_from_slice(&key);
            }
            if variant == 2 {
                raw[432 + k..].copy_from_slice(&inline);
            }
            assert_eq!(bytes, raw, "seed {seed} sample {sample}");
            let view = decode_undo_record(&bytes, e, &binary_schema()).unwrap();
            assert_eq!(view.cell, UndoCell::Live(r));
            assert_eq!(view.reference(), reference);
            if variant == 3 {
                let columns = vec![column(DataType::Binary, false); 2];
                decode_materialized_before(view, &large, &columns).unwrap();
            }
            count += 1;
        }
    }
    assert_eq!(count, 8192);
    assert_eq!(f.inline.len(), 31);
}

#[path = "undo_free_page_tests.rs"]
mod free_page;
