use anyhow::{anyhow, Result};
use fst::{Map, MapBuilder};
use lazy_static::lazy_static;
use regex::Regex;
use serde::de::{Deserializer, SeqAccess, Visitor};
use serde::ser::{SerializeStruct, Serializer};
use serde::{Deserialize, Serialize};

pub extern crate fst;

#[derive(Debug)]
pub struct Dict {
    pub keys: Map<Vec<u8>>,
    pub fields: Vec<Vec<Field>>,
}
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct Field {
    pub ident: Option<String>,
    pub explanation: Explanation,
    pub examples: Vec<Example>,
}
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct Explanation {
    pub body: String,
    pub complements: Vec<Complement>,
}
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct Example {
    pub sentence: String,
    pub complements: Vec<Complement>,
}
#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct Complement {
    pub body: String,
}

impl Serialize for Dict {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_struct("Dict", 2)?;
        let keys_bytes = self.keys.clone().into_fst().into_inner();
        seq.serialize_field("keys", &keys_bytes)?;
        seq.serialize_field("fields", &self.fields)?;
        seq.end()
    }
}

impl<'de> Deserialize<'de> for Dict {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct DictVisitor;
        impl<'de> Visitor<'de> for DictVisitor {
            type Value = Dict;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("struct Dict")
            }
            fn visit_seq<V>(self, mut seq: V) -> Result<Dict, V::Error>
            where
                V: SeqAccess<'de>,
            {
                use serde::de::Error as de_err;
                let keys_bytes = seq
                    .next_element()?
                    .ok_or_else(|| de_err::invalid_length(0, &self))?;
                let keys = Map::new(keys_bytes).unwrap();
                let fields = seq
                    .next_element()?
                    .ok_or_else(|| de_err::invalid_length(1, &self))?;
                Ok(Dict { keys, fields })
            }
        }
        deserializer.deserialize_struct("Dict", &["keys", "fields"], DictVisitor)
    }
}

fn parse_complements(text: &str) -> Result<Vec<Complement>> {
    lazy_static! {
        static ref COMPLEMENT: Regex = Regex::new(r#"◆([^◆■]+)"#).unwrap();
    }
    COMPLEMENT
        .captures_iter(text)
        .map(|m| {
            Ok(Complement {
                body: m
                    .get(1)
                    .ok_or(anyhow!("Invalid complement format"))?
                    .as_str()
                    .to_string(),
            })
        })
        .collect()
}

fn parse_examples(text: &str) -> Result<Vec<Example>> {
    lazy_static! {
        static ref EXAMPLE: Regex = Regex::new(r#"■([^◆■]+)(?P<complements>(◆[^◆■]+)+)?"#).unwrap();
    }
    EXAMPLE
        .captures_iter(text)
        .map(|m| {
            Ok(Example {
                sentence: m
                    .get(1)
                    .ok_or(anyhow!("Invalid example format"))?
                    .as_str()
                    .to_string(),
                complements: m
                    .name("complements")
                    .map(|m| parse_complements(m.as_str()))
                    .unwrap_or(Ok(Vec::new()))?,
            })
        })
        .collect()
}

fn parse_field(text: &str) -> Result<(String, Field)> {
    lazy_static! {
        static ref RE: Regex = Regex::new(
            r#"■(?P<item>.+?)(?: +\{(?P<ident>.+)\})? : (?P<exp>[^◆■]*)(?P<complements>(?:◆[^◆■]+)*)(?P<examples>(■.+)*)"#
        )
        .unwrap();
    }
    let cap = RE.captures(text).ok_or(anyhow!("Invalid field format"))?;
    let key = cap["item"].to_string();
    Ok((
        key,
        Field {
            ident: cap.name("ident").map(|m| m.as_str().to_string()),
            explanation: {
                Explanation {
                    body: cap["exp"].to_string(),
                    complements: parse_complements(&cap["complements"])?,
                }
            },
            examples: parse_examples(&cap["examples"])?,
        },
    ))
}

pub fn parse(text: &str) -> Result<Dict> {
    let mut tmp = text
        .lines()
        .enumerate()
        .map(|(line_no, line)| {
            let (k, f) = parse_field(line).map_err(|e| anyhow!("line {}: {}", line_no, e))?;
            Ok((k, f, line_no))
        })
        .collect::<Result<Vec<_>>>()?;
    tmp.sort();

    let mut map = MapBuilder::memory();
    let mut prev_key: Option<String> = None;
    let mut fields = Vec::new();
    for (k, f, line_no) in tmp.into_iter() {
        let new_key = prev_key.as_ref().map(|p| p != &k).unwrap_or(true);
        if new_key {
            map.insert(&k, fields.len() as u64)
                .map_err(|e| anyhow!("[line {}]: {}", line_no, e))?;
            fields.push(Vec::new());
            prev_key = Some(k);
        }
        fields.last_mut().unwrap().push(f);
    }
    Ok(Dict {
        keys: map.into_map(),
        fields,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kv_vec(dict: &Dict) -> Vec<(String, &Vec<Field>)> {
        use fst::Streamer;
        let mut ret = Vec::new();
        let mut stream = dict.keys.stream();
        while let Some((k, idx)) = stream.next() {
            let k = std::str::from_utf8(k).unwrap();
            ret.push((k.to_string(), &dict.fields[idx as usize]));
        }
        ret
    }

    fn new_field<S: Into<String>>(
        ident: Option<S>,
        exp: S,
        exp_coms: Vec<S>,
        examples: Vec<(S, Vec<S>)>,
    ) -> Field {
        Field {
            ident: ident.map(|s| s.into()),
            explanation: Explanation {
                body: exp.into(),
                complements: exp_coms
                    .into_iter()
                    .map(|s| Complement { body: s.into() })
                    .collect(),
            },
            examples: examples
                .into_iter()
                .map(|(s, c)| Example {
                    sentence: s.into(),
                    complements: c
                        .into_iter()
                        .map(|c| Complement { body: c.into() })
                        .collect(),
                })
                .collect(),
        }
    }

    #[test]
    fn test_01() {
        let s = "■autocompletion {名} : 《コ》〔入力文字の〕自動補完、オートコンプリート◆【参考】autocomplete";
        let dict = crate::parse(s).unwrap();
        assert_eq!(
            kv_vec(&dict),
            vec![(
                "autocompletion".to_string(),
                &vec![new_field(
                    Some("名"),
                    "《コ》〔入力文字の〕自動補完、オートコンプリート",
                    vec!["【参考】autocomplete"],
                    vec![]
                )]
            )]
        )
    }

    #[test]
    fn test_02() {
        let s =
        "■selfie {名} : 〈話〉セルフィー、自撮り（の）写真◆自分で撮影した自分の写真◆【複】selfies";
        let dict = crate::parse(s).unwrap();
        assert_eq!(
            kv_vec(&dict),
            vec![(
                "selfie".to_string(),
                &vec![new_field(
                    Some("名"),
                    "〈話〉セルフィー、自撮り（の）写真",
                    vec!["自分で撮影した自分の写真", "【複】selfies"],
                    vec![]
                )]
            )]
        )
    }

    #[test]
    fn test_03() {
        let s = "■awkward silence {1} : 《an ～》気まずい［ぎこちない］沈黙◆「会話が不自然に途切れた気まずい時間」を指す。1回・2回と数えられるので可算。■・There was an awkward silence for a few seconds. 数秒間の気まずい沈黙がありました。■・There was an awkward silence for a moment. ちょっとの間、気まずい沈黙がありました。／一瞬、微妙な空気が流れた。";
        let dict = crate::parse(s).unwrap();
        assert_eq!(
        kv_vec(&dict),
        vec![(
            "awkward silence".to_string(),
            &vec![new_field(
                Some("1"),
                "《an ～》気まずい［ぎこちない］沈黙",
                vec!["「会話が不自然に途切れた気まずい時間」を指す。1回・2回と数えられるので可算。"],
                vec![
                    ("・There was an awkward silence for a few seconds. 数秒間の気まずい沈黙がありました。", vec![]),
                    ("・There was an awkward silence for a moment. ちょっとの間、気まずい沈黙がありました。／一瞬、微妙な空気が流れた。", vec![])
                ]
            )]
        )]
    )
    }

    #[test]
    fn test_04() {
        let s = "■awkward silence {2} : 気まずい沈黙状態◆「誰もしゃべらない状態」を表す。不可算。■・We stared at each other in awkward silence. 私たちは、気まずいムードで黙って顔を見合わせました。";
        let dict = crate::parse(s).unwrap();
        assert_eq!(
        kv_vec(&dict),
        vec![(
            "awkward silence".to_string(),
            &vec![new_field(
                Some("2"),
                "気まずい沈黙状態",
                vec!["「誰もしゃべらない状態」を表す。不可算。"],
                vec![("・We stared at each other in awkward silence. 私たちは、気まずいムードで黙って顔を見合わせました。", vec![])]
            )]
        )]
    )
    }

    #[test]
    fn test_05() {
        let s = "■xxx : aaa◆bbb◆ccc■ddd◆eee■fff";
        let dict = parse(s).unwrap();
        assert_eq!(
            kv_vec(&dict),
            vec![(
                "xxx".to_string(),
                &vec![new_field(
                    None,
                    "aaa",
                    vec!["bbb", "ccc"],
                    vec![("ddd", vec!["eee"]), ("fff", vec![])]
                )]
            )]
        )
    }

    #[test]
    fn serde() {
        let s = "■xxx : aaa◆bbb◆ccc■ddd◆eee■fff";
        let dict = parse(s).unwrap();
        let bytes = bincode::serialize(&dict).unwrap();
        let dict = bincode::deserialize(&bytes).unwrap();
        assert_eq!(
            kv_vec(&dict),
            vec![(
                "xxx".to_string(),
                &vec![new_field(
                    None,
                    "aaa",
                    vec!["bbb", "ccc"],
                    vec![("ddd", vec!["eee"]), ("fff", vec![])]
                )]
            )]
        )
    }
}
