// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Shared LLVM bitcode block IDs and record codes used by both the reader and
//! writer.

// Block IDs
pub(crate) const MODULE_BLOCK_ID: u32 = 8;
pub(crate) const PARAMATTR_BLOCK_ID: u32 = 9;
pub(crate) const PARAMATTR_GROUP_BLOCK_ID: u32 = 10;
pub(crate) const CONSTANTS_BLOCK_ID: u32 = 11;
pub(crate) const FUNCTION_BLOCK_ID: u32 = 12;
pub(crate) const IDENTIFICATION_BLOCK_ID: u32 = 13;
pub(crate) const VALUE_SYMTAB_BLOCK_ID: u32 = 14;
pub(crate) const METADATA_BLOCK_ID: u32 = 15;
pub(crate) const TYPE_BLOCK_ID_NEW: u32 = 17;
pub(crate) const STRTAB_BLOCK_ID: u32 = 23;

// Module codes
pub(crate) const MODULE_CODE_VERSION: u32 = 1;
pub(crate) const MODULE_CODE_TRIPLE: u32 = 2;
pub(crate) const MODULE_CODE_DATALAYOUT: u32 = 3;
pub(crate) const MODULE_CODE_GLOBALVAR: u32 = 7;
pub(crate) const MODULE_CODE_FUNCTION: u32 = 8;
pub(crate) const MODULE_CODE_VSTOFFSET: u32 = 13;
pub(crate) const MODULE_CODE_SOURCE_FILENAME: u32 = 16;

// Type codes
pub(crate) const TYPE_CODE_NUMENTRY: u32 = 1;
pub(crate) const TYPE_CODE_VOID: u32 = 2;
pub(crate) const TYPE_CODE_FLOAT: u32 = 3;
pub(crate) const TYPE_CODE_DOUBLE: u32 = 4;
pub(crate) const TYPE_CODE_LABEL: u32 = 5;
pub(crate) const TYPE_CODE_OPAQUE: u32 = 6;
pub(crate) const TYPE_CODE_INTEGER: u32 = 7;
pub(crate) const TYPE_CODE_HALF: u32 = 10;
pub(crate) const TYPE_CODE_ARRAY: u32 = 11;
pub(crate) const TYPE_CODE_POINTER: u32 = 16;
pub(crate) const TYPE_CODE_STRUCT_NAME: u32 = 19;
pub(crate) const TYPE_CODE_FUNCTION_TYPE: u32 = 21;
pub(crate) const TYPE_CODE_OPAQUE_POINTER: u32 = 25;

// Constant codes
pub(crate) const CST_CODE_SETTYPE: u32 = 1;
pub(crate) const CST_CODE_NULL: u32 = 2;
pub(crate) const CST_CODE_INTEGER: u32 = 4;
pub(crate) const CST_CODE_FLOAT: u32 = 6;
pub(crate) const CST_CODE_CSTRING: u32 = 9;
pub(crate) const CST_CODE_CE_CAST: u32 = 11;
pub(crate) const CST_CODE_CE_INBOUNDS_GEP: u32 = 20;

// Function instruction codes
pub(crate) const FUNC_CODE_DECLAREBLOCKS: u32 = 1;
pub(crate) const FUNC_CODE_INST_BINOP: u32 = 2;
pub(crate) const FUNC_CODE_INST_CAST: u32 = 3;
pub(crate) const FUNC_CODE_INST_SELECT: u32 = 5;
pub(crate) const FUNC_CODE_INST_RET: u32 = 10;
pub(crate) const FUNC_CODE_INST_BR: u32 = 11;
pub(crate) const FUNC_CODE_INST_SWITCH: u32 = 12;
pub(crate) const FUNC_CODE_INST_UNREACHABLE: u32 = 15;
pub(crate) const FUNC_CODE_INST_PHI: u32 = 16;
pub(crate) const FUNC_CODE_INST_ALLOCA: u32 = 19;
pub(crate) const FUNC_CODE_INST_LOAD: u32 = 20;
pub(crate) const FUNC_CODE_INST_CMP2: u32 = 28;
pub(crate) const FUNC_CODE_INST_CALL: u32 = 34;
pub(crate) const FUNC_CODE_INST_GEP: u32 = 43;
pub(crate) const FUNC_CODE_INST_STORE: u32 = 44;

// Packed CALL cc-info flags
pub(crate) const CALL_EXPLICIT_TYPE_FLAG: u64 = 1_u64 << 15;

// Value symbol table codes
pub(crate) const VST_CODE_ENTRY: u32 = 1;
pub(crate) const VST_CODE_BBENTRY: u32 = 2;
pub(crate) const VST_CODE_FNENTRY: u32 = 3;

// String table codes
pub(crate) const STRTAB_BLOB: u32 = 1;

// Metadata record codes
pub(crate) const METADATA_STRING_OLD: u32 = 1;
pub(crate) const METADATA_VALUE: u32 = 2;
pub(crate) const METADATA_NODE: u32 = 3;
pub(crate) const METADATA_NAME: u32 = 4;
pub(crate) const METADATA_NAMED_NODE: u32 = 10;
