use serde_wasm_bindgen::Serializer;
use tsify::{declare, Tsify};
use wasm_bindgen::prelude::*;

use std::{
    collections::HashMap,
    io::{BufRead, Cursor, Read},
};

use quartz_nbt::{io::Flavor, serde::deserialize_from_buffer};
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsError;

const SERIALIZER: Serializer = Serializer::new().serialize_maps_as_objects(true);

type JsResult<T> = Result<T, JsError>;

#[derive(Tsify, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Spell {
    #[serde(rename = "modsRequired")]
    #[serde(default)]
    pub mods: Vec<Mod>,
    #[serde(rename = "spellList")]
    pub pieces: Vec<Piece>,
    #[serde(rename = "spellName")]
    pub name: String,
}

#[derive(Tsify, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Mod {
    #[serde(rename = "modName")]
    pub name: String,
    #[serde(rename = "modVersion")]
    pub version: String,
}

#[derive(Tsify, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
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

#[declare]
pub type SpellParams = HashMap<String, u8>;

#[derive(Tsify, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SpellData {
    pub key: String,
    pub params: Option<SpellParams>,
    #[serde(rename = "constantValue")]
    pub constant: Option<String>,
    pub comment: Option<String>,
}

impl Spell {
    #[inline]
    pub fn bin(&self) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        {
            let name = self.name.as_bytes();
            out.extend_from_slice(name);
            out.push(0);
        }

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
            out.extend_from_slice(key);
            out.push(0);
            if let Some(comment) = comment {
                out.extend_from_slice(comment.as_bytes());
            }
            out.push(0);

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

        out
    }

    #[inline]
    pub fn decode(data: &[u8]) -> JsResult<Self> {
        #[inline]
        fn read_until<T>(cursor: &mut Cursor<T>, byte: u8) -> JsResult<Vec<u8>>
        where
            T: std::convert::AsRef<[u8]>,
        {
            let mut out = Vec::new();
            cursor.read_until(byte, &mut out)?;
            out.pop();
            Ok(out)
        }

        #[inline]
        fn read_until_nul<T>(cursor: &mut Cursor<T>) -> JsResult<Vec<u8>>
        where
            T: std::convert::AsRef<[u8]>,
        {
            read_until(cursor, 0)
        }

        #[inline]
        fn next<T>(cursor: &mut Cursor<T>) -> JsResult<u8>
        where
            T: std::convert::AsRef<[u8]>,
        {
            let mut a = [0];
            cursor.read_exact(&mut a)?;
            Ok(a[0])
        }

        #[inline]
        fn btos(b: Vec<u8>) -> JsResult<String> {
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
            let key = btos(key)?;

            let comment = btos(read_until_nul(&mut cursor)?)?;
            let comment = if comment.is_empty() {
                None
            } else {
                Some(comment)
            };

            let mut params = HashMap::new();
            let mut constant = None;

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
                        BUILTIN_PARAMS[type_or_pos as usize].to_string()
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

impl From<&Spell> for Vec<u8> {
    #[inline]
    fn from(value: &Spell) -> Self {
        value.bin()
    }
}

impl TryFrom<&Spell> for JsValue {
    type Error = JsError;

    #[inline]
    fn try_from(value: &Spell) -> Result<Self, Self::Error> {
        value.serialize(&SERIALIZER).map_err(Into::into)
    }
}

impl TryFrom<Spell> for JsValue {
    type Error = JsError;

    #[inline]
    fn try_from(value: Spell) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

#[wasm_bindgen(js_name = "snbtToSpell")]
pub fn snbt_to_spell(snbt: &str) -> JsResult<JsValue> {
    let snbt = quartz_nbt::snbt::parse(snbt)?;

    let mut bytes = Vec::new();
    quartz_nbt::io::write_nbt(&mut bytes, None, &snbt, Flavor::Uncompressed)?;

    let spell = deserialize_from_buffer::<Spell>(&bytes)?.0;

    spell.try_into()
}

#[wasm_bindgen(js_name = "bytesToSpell")]
pub fn bytes_to_spell(bytes: Vec<u8>) -> JsResult<JsValue> {
    let spell: Spell = Spell::decode(&bytes)?;
    Ok(spell.serialize(&SERIALIZER)?)
}

#[wasm_bindgen(js_name = "spellToBytes")]
pub fn spell_to_bytes(spell: JsValue) -> Result<Vec<u8>, JsError> {
    let spell: Spell = serde_wasm_bindgen::from_value(spell)?;
    Ok((&spell).into())
}

#[wasm_bindgen(js_name = "urlSafeToSpell")]
pub fn url_safe_to_spell(url_safe: String) -> JsResult<JsValue> {
    Spell::decode(&url_safe_to_bytes(url_safe)?)?.try_into()
}

#[wasm_bindgen(js_name = "spellToUrlSafe")]
pub fn spell_to_url_safe(spell: JsValue) -> JsResult<String> {
    bytes_to_url_safe(spell_to_bytes(spell)?)
}

const ZSTD_DICT: &[u8] = include_bytes!("./zstd_dict");

#[wasm_bindgen(js_name = "bytesToUrlSafe")]
pub fn bytes_to_url_safe(bytes: Vec<u8>) -> JsResult<String> {
    let bytes =
        zstd::bulk::Compressor::with_dictionary(22, ZSTD_DICT)?.compress(bytes.as_slice())?;

    Ok(base64_simd::URL_SAFE.encode_to_string(bytes))
}

#[wasm_bindgen(js_name = "urlSafeToBytes")]
pub fn url_safe_to_bytes(url_safe: String) -> JsResult<Vec<u8>> {
    let mut bytes = url_safe.into_bytes();
    let decoded = base64_simd::URL_SAFE.decode_inplace(&mut bytes)?.to_vec();

    let mut dest = Vec::new();
    let mut decoder = zstd::stream::Decoder::with_dictionary(decoded.as_slice(), ZSTD_DICT)?;
    std::io::copy(&mut decoder, &mut dest)?;

    Ok(dest)
}

#[wasm_bindgen(js_name = "spellToSnbt")]
pub fn spell_to_snbt(spell: JsValue) -> JsResult<String> {
    let spell: Spell = serde_wasm_bindgen::from_value(spell)?;
    let ser = quartz_nbt::serde::serialize(&spell, None, Flavor::Uncompressed).unwrap();
    quartz_nbt::io::read_nbt(&mut Cursor::new(ser), Flavor::Uncompressed)
        .map(|o| o.0.to_snbt())
        .map_err(JsError::from)
}

#[wasm_bindgen(start)]
pub fn main() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
}
