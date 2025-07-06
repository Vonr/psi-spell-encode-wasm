use base64::Engine;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;
use zstd::dict::{DecoderDictionary, EncoderDictionary};

use std::{
    cell::LazyCell,
    collections::HashMap,
    fmt::Display,
    io::{BufRead, Cursor, Read},
};

use quartz_nbt::{io::Flavor, serde::deserialize_from_buffer};
use serde::{Deserialize, Serialize};

#[cfg(feature = "wasm")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[cfg(feature = "wasm")]
type Error = wasm_bindgen::JsError;
#[cfg(not(feature = "wasm"))]
type Error = anyhow::Error;

type Result<T> = std::result::Result<T, Error>;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub struct Spell {
    #[serde(rename = "modsRequired")]
    #[serde(default)]
    pub mods: Vec<Mod>,
    #[serde(rename = "spellList")]
    pub pieces: Vec<Piece>,
    #[serde(rename = "spellName")]
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub struct Mod {
    #[serde(rename = "modName")]
    pub name: String,
    #[serde(rename = "modVersion")]
    pub version: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub struct Piece {
    pub data: SpellData,
    pub x: u8,
    pub y: u8,
}

const BUILTIN_PARAMS: [&str; 43] = [
    "_target",
    "_number",
    "_number1",
    "_number2",
    "_number3",
    "_number4",
    "_vector1",
    "_vector2",
    "_vector3",
    "_vector4",
    "_position",
    "_min",
    "_max",
    "_power",
    "_x",
    "_y",
    "_z",
    "_radius",
    "_distance",
    "_time",
    "_base",
    "_ray",
    "_vector",
    "_axis",
    "_angle",
    "_pitch",
    "_instrument",
    "_volume",
    "_list1",
    "_list2",
    "_list",
    "_direction",
    "_from1",
    "_from2",
    "_to1",
    "_to2",
    "_root",
    "_toggle",
    "_mask",
    "_channel",
    "_slot",
    "_ray_end",
    "_ray_start",
];

#[cfg_attr(feature = "wasm", tsify::declare)]
pub type SpellParams = HashMap<String, u8>;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]
#[cfg_attr(feature = "wasm", tsify(into_wasm_abi, from_wasm_abi))]
pub struct SpellData {
    pub key: String,
    pub params: Option<SpellParams>,
    #[serde(rename = "constantValue")]
    pub constant: Option<String>,
    pub comment: Option<String>,
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum SpecialTag {
    Connector = 0,
    ConstantNumber = 1,
    VectorConstruct = 2,
    VectorSum = 3,
    VectorSub = 4,
    VectorMul = 5,
    VectorDiv = 6,
    Sum = 7,
    Sub = 8,
    Mul = 9,
    Div = 10,
    Mod = 11,
    VectorExtractX = 12,
    VectorExtractY = 13,
    VectorExtractZ = 14,
    EntityPosition = 15,
    EntityLook = 16,
    Die = 17,
    ErrSuppressor = 18,
    Caster = 19,
    None = 255,
}

impl From<&[u8]> for SpecialTag {
    fn from(value: &[u8]) -> Self {
        match value {
            b"connector" => SpecialTag::Connector,
            b"constant_number" => SpecialTag::ConstantNumber,
            b"operator_vector_construct" => SpecialTag::VectorConstruct,
            b"operator_vector_sum" => SpecialTag::VectorSum,
            b"operator_vector_subtract" => SpecialTag::VectorSub,
            b"operator_vector_multiply" => SpecialTag::VectorMul,
            b"operator_divide" => SpecialTag::VectorDiv,
            b"operator_sum" => SpecialTag::Sum,
            b"operator_subtract" => SpecialTag::Sub,
            b"operator_multiply" => SpecialTag::Mul,
            b"operator_vector_divide" => SpecialTag::Div,
            b"operator_modulus" => SpecialTag::Mod,
            b"operator_vector_extract_x" => SpecialTag::VectorExtractX,
            b"operator_vector_extract_y" => SpecialTag::VectorExtractY,
            b"operator_vector_extract_z" => SpecialTag::VectorExtractZ,
            b"operator_entity_position" => SpecialTag::EntityPosition,
            b"operator_entity_look" => SpecialTag::EntityLook,
            b"trick_die" => SpecialTag::Die,
            b"error_suppressor" => SpecialTag::ErrSuppressor,
            b"selector_caster" => SpecialTag::Caster,
            _ => SpecialTag::None,
        }
    }
}

impl SpecialTag {
    pub fn to_key<'a>(self) -> Option<&'a str> {
        Some(match self {
            SpecialTag::Connector => "psi:connector",
            SpecialTag::ConstantNumber => "psi:constant_number",
            SpecialTag::VectorConstruct => "psi:operator_vector_construct",
            SpecialTag::VectorSum => "psi:operator_vector_sum",
            SpecialTag::VectorSub => "psi:operator_vector_subtract",
            SpecialTag::VectorMul => "psi:operator_vector_multiply",
            SpecialTag::VectorDiv => "psi:operator_vector_divide",
            SpecialTag::Sum => "psi:operator_sum",
            SpecialTag::Sub => "psi:operator_subtract",
            SpecialTag::Mul => "psi:operator_multiply",
            SpecialTag::Div => "psi:operator_divide",
            SpecialTag::Mod => "psi:operator_modulus",
            SpecialTag::VectorExtractX => "psi:operator_vector_extract_x",
            SpecialTag::VectorExtractY => "psi:operator_vector_extract_y",
            SpecialTag::VectorExtractZ => "psi:operator_vector_extract_z",
            SpecialTag::EntityPosition => "psi:operator_entity_position",
            SpecialTag::EntityLook => "psi:operator_entity_look",
            SpecialTag::Die => "psi:trick_die",
            SpecialTag::ErrSuppressor => "psi:error_suppressor",
            SpecialTag::Caster => "psi:selector_caster",
            SpecialTag::None => return None,
        })
    }
}

#[derive(Debug)]
struct InvalidDiscriminantError;

impl Display for InvalidDiscriminantError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid discriminant for enum")
    }
}

impl std::error::Error for InvalidDiscriminantError {}

#[derive(Debug)]
struct MissingParamError {
    x: u8,
    y: u8,
    piece: String,
    param: String,
}

impl Display for MissingParamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "missing parameter {} for piece {} at [{}, {}]",
            self.param, self.piece, self.x, self.y,
        )
    }
}

impl std::error::Error for MissingParamError {}

impl TryFrom<u8> for SpecialTag {
    type Error = InvalidDiscriminantError;

    fn try_from(value: u8) -> std::result::Result<Self, Self::Error> {
        Ok(match value {
            0 => SpecialTag::Connector,
            1 => SpecialTag::ConstantNumber,
            2 => SpecialTag::VectorConstruct,
            3 => SpecialTag::VectorSum,
            4 => SpecialTag::VectorSub,
            5 => SpecialTag::VectorMul,
            6 => SpecialTag::VectorDiv,
            7 => SpecialTag::Sum,
            8 => SpecialTag::Sub,
            9 => SpecialTag::Mul,
            10 => SpecialTag::Div,
            11 => SpecialTag::Mod,
            12 => SpecialTag::VectorExtractX,
            13 => SpecialTag::VectorExtractY,
            14 => SpecialTag::VectorExtractZ,
            15 => SpecialTag::EntityPosition,
            16 => SpecialTag::EntityLook,
            17 => SpecialTag::Die,
            18 => SpecialTag::ErrSuppressor,
            19 => SpecialTag::Caster,
            255 => SpecialTag::None,
            _ => return Err(InvalidDiscriminantError),
        })
    }
}

trait GetParam {
    fn get_param(&self, piece: &Piece, key: &str) -> Result<u8>;
}

impl GetParam for Option<SpellParams> {
    fn get_param(&self, piece: &Piece, key: &str) -> Result<u8> {
        let err = || MissingParamError {
            x: piece.x,
            y: piece.y,
            piece: piece.data.key.clone(),
            param: "_target".to_owned(),
        };

        Ok(self
            .as_ref()
            .ok_or_else(err)?
            .get(key)
            .copied()
            .ok_or_else(err)?)
    }
}

impl Spell {
    pub fn extend_bin(&self, out: &mut Vec<u8>) -> Result<()> {
        let name = self.name.as_bytes();
        out.extend_from_slice(name);
        out.push(0);

        if !self.mods.is_empty() {
            for m in &self.mods {
                let name = m.name.as_bytes();
                let version = m.version.as_bytes();
                out.extend_from_slice(name);
                out.push(b',');
                out.extend_from_slice(version);
                out.push(b';');
            }
            let last = out.len() - 1;
            out[last] = b']';
        } else {
            out.push(b']');
        }

        for piece in &self.pieces {
            let data = &piece.data;
            let key = data.key.as_bytes();
            let key = if &key[0..4] == b"psi:" {
                &key[4..]
            } else {
                key
            };
            let params = &data.params;
            let constant = &data.constant;
            let comment = &data.comment;
            out.push(piece.x << 4 | (piece.y & 0b1111));

            let special_tag = match key {
                b"connector" => SpecialTag::Connector,
                b"constant_number" => SpecialTag::ConstantNumber,
                b"operator_vector_construct" => SpecialTag::VectorConstruct,
                _ => SpecialTag::None,
            };

            out.push(special_tag as u8);
            match special_tag {
                SpecialTag::Connector
                | SpecialTag::VectorExtractX
                | SpecialTag::VectorExtractY
                | SpecialTag::VectorExtractZ
                | SpecialTag::EntityPosition
                | SpecialTag::EntityLook
                | SpecialTag::Die => {
                    out.push(params.get_param(piece, "_target")?);
                }
                SpecialTag::ConstantNumber => {
                    if let Some(constant) = constant {
                        out.extend_from_slice(constant.as_bytes());
                    }
                    out.push(0);
                }
                SpecialTag::VectorConstruct => {
                    out.push(params.get_param(piece, "_x")?);
                    out.push(params.get_param(piece, "_y")?);
                    out.push(params.get_param(piece, "_z")?);
                }
                SpecialTag::None => {
                    out.extend_from_slice(key);
                    out.push(0);
                }
                SpecialTag::VectorSum
                | SpecialTag::VectorSub
                | SpecialTag::VectorMul
                | SpecialTag::VectorDiv => {
                    out.push(params.get_param(piece, "_vector1")?);
                    out.push(params.get_param(piece, "_vector2")?);
                    out.push(params.get_param(piece, "_vector3")?);
                }
                SpecialTag::Sum | SpecialTag::Sub | SpecialTag::Mul | SpecialTag::Div => {
                    out.push(params.get_param(piece, "_number1")?);
                    out.push(params.get_param(piece, "_number2")?);
                    out.push(params.get_param(piece, "_number3")?);
                }
                SpecialTag::Mod => {
                    out.push(params.get_param(piece, "_number1")?);
                    out.push(params.get_param(piece, "_number2")?);
                }
                SpecialTag::ErrSuppressor | SpecialTag::Caster => {}
            }

            if let Some(comment) = comment {
                out.extend_from_slice(comment.as_bytes());
            }
            out.push(0);

            if matches!(
                special_tag,
                SpecialTag::Connector | SpecialTag::ConstantNumber | SpecialTag::VectorConstruct
            ) {
                continue;
            }

            if let Some(params) = params {
                out.push(params.len() as u8);
                for (key, side) in params {
                    if let Some(pos) = BUILTIN_PARAMS.iter().position(|e| **e == *key) {
                        out.push(pos as u8);
                    } else {
                        out.push(255);
                        out.extend_from_slice(key.as_bytes());
                        out.push(0);
                    }
                    out.push(*side);
                }
            } else if let Some(constant) = constant {
                out.push(255);
                out.extend_from_slice(constant.as_bytes());
                out.push(0);
            } else {
                out.push(254);
            }
        }

        Ok(())
    }

    pub fn bin(&self) -> Result<Vec<u8>> {
        let mut out: Vec<u8> = Vec::new();
        self.extend_bin(&mut out)?;
        Ok(out)
    }

    pub fn decode(data: &[u8]) -> Result<Self> {
        fn read_until<T>(cursor: &mut Cursor<T>, byte: u8) -> Result<Vec<u8>>
        where
            T: std::convert::AsRef<[u8]>,
        {
            let mut out = Vec::new();
            cursor.read_until(byte, &mut out)?;
            out.pop();
            Ok(out)
        }

        fn read_until_nul<T>(cursor: &mut Cursor<T>) -> Result<Vec<u8>>
        where
            T: std::convert::AsRef<[u8]>,
        {
            read_until(cursor, 0)
        }

        fn next<T>(cursor: &mut Cursor<T>) -> Result<u8>
        where
            T: std::convert::AsRef<[u8]>,
        {
            let mut a = [0];
            cursor.read_exact(&mut a)?;
            Ok(a[0])
        }

        fn btos(b: Vec<u8>) -> Result<String> {
            Ok(String::from_utf8(b)?)
        }

        let mut cursor = Cursor::new(data);
        let name = btos(read_until_nul(&mut cursor)?)?;
        let mut mods = Vec::new();
        let mut pieces = Vec::new();

        {
            let m = read_until(&mut cursor, b']')?;
            for m in m.split(|b| *b == b';') {
                let mut name = Vec::new();
                let mut version = Vec::new();
                let mut name_done = false;
                for b in m {
                    let b = *b;
                    if b == b',' || b == b';' {
                        name_done = true;
                        continue;
                    }
                    if !name_done {
                        name.push(b);
                    } else {
                        version.push(b);
                    }
                }
                mods.push(Mod {
                    name: btos(name)?,
                    version: btos(version)?,
                })
            }
        }

        while cursor.fill_buf().map(|b| !b.is_empty())? {
            let xy = next(&mut cursor)?;
            let x = xy >> 4;
            let y = xy & 0b1111;
            let special_tag: SpecialTag = next(&mut cursor)?.try_into()?;
            let key = match special_tag.to_key() {
                Some(key) => key.to_owned(),
                None => {
                    let mut key = read_until_nul(&mut cursor)?;
                    if !key.contains(&b':') {
                        key.reserve(4);
                        unsafe {
                            std::ptr::copy(key.as_ptr(), key.as_mut_ptr().add(4), key.len());
                            key.set_len(key.len() + 4);
                        }
                        key[0] = b'p';
                        key[1] = b's';
                        key[2] = b'i';
                        key[3] = b':';
                    }
                    btos(key)?
                }
            };

            let mut params = HashMap::new();
            let mut constant = None;

            match special_tag {
                SpecialTag::Connector
                | SpecialTag::VectorExtractX
                | SpecialTag::VectorExtractY
                | SpecialTag::VectorExtractZ
                | SpecialTag::EntityPosition
                | SpecialTag::EntityLook
                | SpecialTag::Die => {
                    params.insert("_target".to_owned(), next(&mut cursor)?);
                }
                SpecialTag::ConstantNumber => {
                    constant = Some(btos(read_until_nul(&mut cursor)?)?);
                }
                SpecialTag::VectorConstruct => {
                    params.insert("_x".to_owned(), next(&mut cursor)?);
                    params.insert("_y".to_owned(), next(&mut cursor)?);
                    params.insert("_z".to_owned(), next(&mut cursor)?);
                }
                SpecialTag::VectorSum
                | SpecialTag::VectorSub
                | SpecialTag::VectorMul
                | SpecialTag::VectorDiv => {
                    params.insert("_vector1".to_owned(), next(&mut cursor)?);
                    params.insert("_vector2".to_owned(), next(&mut cursor)?);
                    params.insert("_vector3".to_owned(), next(&mut cursor)?);
                }
                SpecialTag::Sum | SpecialTag::Sub | SpecialTag::Mul | SpecialTag::Div => {
                    params.insert("_number1".to_owned(), next(&mut cursor)?);
                    params.insert("_number2".to_owned(), next(&mut cursor)?);
                    params.insert("_number3".to_owned(), next(&mut cursor)?);
                }
                SpecialTag::Mod => {
                    params.insert("_number1".to_owned(), next(&mut cursor)?);
                    params.insert("_number2".to_owned(), next(&mut cursor)?);
                }
                SpecialTag::ErrSuppressor | SpecialTag::Caster => {}
                SpecialTag::None => {}
            }

            let comment = btos(read_until_nul(&mut cursor)?)?;
            let comment = if comment.is_empty() {
                None
            } else {
                Some(comment)
            };

            if special_tag != SpecialTag::None {
                pieces.push(Piece {
                    data: SpellData {
                        key,
                        params: if params.is_empty() {
                            None
                        } else {
                            Some(params)
                        },
                        constant,
                        comment,
                    },
                    x,
                    y,
                });
                continue;
            }

            let ty = next(&mut cursor)?;
            if ty == 255 {
                constant = Some(btos(read_until_nul(&mut cursor)?)?);
            } else if ty != 254 {
                let len = ty;
                for _ in 0..len {
                    let type_or_pos = next(&mut cursor)?;
                    let param_key = if type_or_pos == 255 {
                        btos(read_until_nul(&mut cursor)?)?
                    } else {
                        BUILTIN_PARAMS[type_or_pos as usize].to_owned()
                    };

                    let side = next(&mut cursor)?;
                    params.insert(param_key, side);
                }
            }

            let params = if params.is_empty() {
                None
            } else {
                Some(params)
            };

            let data = SpellData {
                key,
                params,
                constant,
                comment,
            };

            let piece = Piece { data, x, y };
            pieces.push(piece);
        }

        Ok(Self { name, mods, pieces })
    }
}

impl TryFrom<&Spell> for Vec<u8> {
    type Error = Error;

    fn try_from(value: &Spell) -> std::result::Result<Self, Self::Error> {
        value.bin()
    }
}

#[cfg_attr(feature = "wasm", wasm_bindgen(js_name = "snbtToSpell"))]
pub fn snbt_to_spell(snbt: &str) -> Result<Spell> {
    let snbt = quartz_nbt::snbt::parse(snbt)?;

    let mut bytes = Vec::new();
    quartz_nbt::io::write_nbt(&mut bytes, None, &snbt, Flavor::Uncompressed)?;

    let spell = deserialize_from_buffer::<Spell>(&bytes)?.0;

    Ok(spell)
}

#[cfg_attr(feature = "wasm", wasm_bindgen(js_name = "bytesToSpell"))]
pub fn bytes_to_spell(bytes: Vec<u8>) -> Result<Spell> {
    byte_slice_to_spell(&bytes)
}

pub fn byte_slice_to_spell(bytes: &[u8]) -> Result<Spell> {
    let spell: Spell = Spell::decode(bytes)?;
    Ok(spell)
}

#[cfg_attr(feature = "wasm", wasm_bindgen(js_name = "spellToBytes"))]
pub fn spell_to_bytes(spell: Spell) -> Result<Vec<u8>> {
    (&spell).try_into()
}

#[cfg_attr(feature = "wasm", wasm_bindgen(js_name = "urlSafeToSpell"))]
pub fn url_safe_to_spell(url_safe: String) -> Result<Spell> {
    Spell::decode(&url_safe_to_bytes(url_safe)?)
}

#[cfg_attr(feature = "wasm", wasm_bindgen(js_name = "spellToUrlSafe"))]
pub fn spell_to_url_safe(spell: Spell) -> Result<String> {
    bytes_to_url_safe(spell_to_bytes(spell)?)
}

const ZSTD_DICT_RAW: &[u8] = include_bytes!("./zstd_dict");

thread_local! {
    static ZSTD_CDICT: LazyCell<&'static EncoderDictionary> = const { LazyCell::new(|| Box::leak(Box::new(EncoderDictionary::new(ZSTD_DICT_RAW, 22)))) };
    static ZSTD_DDICT: LazyCell<&'static DecoderDictionary> = const { LazyCell::new(|| Box::leak(Box::new(DecoderDictionary::new(ZSTD_DICT_RAW)))) };
}

#[cfg_attr(feature = "wasm", wasm_bindgen(js_name = "bytesToUrlSafe"))]
pub fn bytes_to_url_safe(bytes: Vec<u8>) -> Result<String> {
    byte_slice_to_url_safe(&bytes)
}

pub fn byte_slice_to_url_safe(bytes: &[u8]) -> Result<String> {
    let bytes = ZSTD_CDICT
        .with(|d| zstd::bulk::Compressor::with_prepared_dictionary(d)?.compress(bytes))?;

    Ok(base64::prelude::BASE64_URL_SAFE_NO_PAD.encode(bytes))
}

#[cfg_attr(feature = "wasm", wasm_bindgen(js_name = "urlSafeToBytes"))]
pub fn url_safe_to_bytes(url_safe: String) -> Result<Vec<u8>> {
    let mut bytes = url_safe.into_bytes();
    let decoded = base64::prelude::BASE64_URL_SAFE_NO_PAD.decode(&mut bytes)?;

    let mut decoder = ZSTD_DDICT.with(|d| zstd::bulk::Decompressor::with_prepared_dictionary(d))?;
    let dest = decoder.decompress(&decoded, 2 << 20)?;

    Ok(dest)
}

#[cfg_attr(feature = "wasm", wasm_bindgen(js_name = "spellToSnbt"))]
pub fn spell_to_snbt(spell: Spell) -> Result<String> {
    let ser = quartz_nbt::serde::serialize(&spell, None, Flavor::Uncompressed)?;
    quartz_nbt::io::read_nbt(&mut Cursor::new(ser), Flavor::Uncompressed)
        .map(|o| o.0.to_snbt())
        .map_err(Error::from)
}

#[cfg(feature = "wasm")]
#[wasm_bindgen(start)]
pub fn main() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
}
